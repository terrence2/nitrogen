// This file is part of Nitrogen.
//
// Nitrogen is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Nitrogen is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Nitrogen.  If not, see <http://www.gnu.org/licenses/>.

// Data Sets:
//   NASA's Shuttle Radar Topography Map (SRTM); height data
//
// Desired Data Sets:
//   NASA's Blue Marble Next Generation (BMNG); diffuse color information
//   JAXA's Advanced Land Observing Satellite "DAICHI" (ALOS); height data
//   Something cartesian polar north and south
//
// Tiles are 512x512 with a one pixel overlap with other tiles to enable linear filtering. Data is
//   stored row-major with low indexed rows to the south, going north and low index.
//
// Tile cache design:
//   Upload one mega-texture(s) for each dataset.
//   The index is a fixed, large texture:
//     * SRTM has 1' resolution, but tiles have at minimum 510' of content.
//     * We need a (360|180 * 60 * 60 / 510) pixels wide|high texture => 2541.17 x 1270.59
//     * 2560 * 1280 px index texture.
//     * Open Question: do we have data sets with higher resolution that we want to support? Will
//       those inherently load in larger blocks to support the above index scheme? Or do we need
//       mulitple layers of indexing?
//     * Open Question: one index per dataset or shared globally and we assume the same resolution
//       choice for all datasets? I think we'll need higher resolution color and normal data than
//       height?
//   Tile Updates:
//     * The patch tree "votes" on what resolution it wants.
//       * Q: can we compute the index in O(1) instead of walking the tree?
//     * We select a handful of the most needed that are not present to upload and create copy ops.
//       * Q: how do we determine globally what the most needed changes are?
//     * We update the index texture with a compute shader that overwrites if the scale is smaller.
//       * Q: are there optimizations we can make knowing that it is a quadtree?

use crate::{
    tile::{
        spherical_tile_set::{
            SphericalColorTileSet, SphericalHeightTileSet, SphericalNormalsTileSet,
        },
        DataSetCoordinates, DataSetDataKind,
    },
    GpuDetail, VisiblePatch,
};
use anyhow::{anyhow, bail, Result};
use camera::Camera;
use catalog::{from_utf8_string, Catalog};
use global_data::GlobalParametersBuffer;
use gpu::{Gpu, UploadTracker};
use parking_lot::RwLock;
use rayon::prelude::*;
use std::{any::Any, fmt::Debug, sync::Arc};
use tokio::runtime::Runtime;

#[derive(Clone, Copy, Debug)]
pub struct TileSetHandle(usize);

pub trait TileSet: Debug + Send + Sync + 'static {
    // Allow downcast back into concrete types so we can stream data.
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;

    // Maintain runtime visibility tracking based on VisiblePatch notifications derived
    // from the global geometry calculations.
    fn begin_visibility_update(&mut self);
    fn note_required(&mut self, visible_patch: &VisiblePatch);
    fn finish_visibility_update(&mut self, camera: &Camera, catalog: Arc<RwLock<Catalog>>);
    fn ensure_uploaded(&mut self, gpu: &Gpu, tracker: &mut UploadTracker);

    // Indicate that the current index should be written to the debug file.
    fn snapshot_index(&mut self, async_rt: &Runtime, gpu: &mut Gpu);

    // Per-frame opportunity to update the index based on any visibility updates pushed above.
    fn paint_atlas_index(&self, encoder: &mut wgpu::CommandEncoder);

    // The Terrain engine produces an optimal, gpu-tesselated mesh at the start of every frame
    // based on the current view. This mesh is at the terrain surface. This callback gives each
    // tile-set an opportunity to displace the terrain based on a shader and the height data we
    // uploaded as part of finish_update and paint_atlas_index.
    //
    // The terrain may already have been updated by other passes. The implementor should be
    // sure to coordinate to make sure that all passes sum together nicely.
    fn displace_height<'a>(
        &'a self,
        vertex_count: u32,
        mesh_bind_group: &'a wgpu::BindGroup,
        cpass: wgpu::ComputePass<'a>,
    ) -> Result<wgpu::ComputePass<'a>>;

    // Implementors should read from the provided screen space world position coordinates and
    // accumulate into the provided normal accumulation buffer. Accumulation buffers will be
    // automatically cleared at the start of each frame. TileSets should pre-arrange with each
    // other the relative weight of their contributions.
    fn accumulate_normals<'a>(
        &'a self,
        extent: &wgpu::Extent3d,
        globals_buffer: &'a GlobalParametersBuffer,
        accumulate_common_bind_group: &'a wgpu::BindGroup,
        cpass: wgpu::ComputePass<'a>,
    ) -> Result<wgpu::ComputePass<'a>>;

    // Implementors should read from the provided screen space world position coordinates and
    // accumulate into the provided color accumulation buffer. Accumulation buffers will be
    // automatically cleared at the start of each frame. TileSets should pre-arrange with each
    // other the relative weight of their contributions.
    fn accumulate_colors<'a>(
        &'a self,
        extent: &wgpu::Extent3d,
        globals_buffer: &'a GlobalParametersBuffer,
        accumulate_common_bind_group: &'a wgpu::BindGroup,
        cpass: wgpu::ComputePass<'a>,
    ) -> Result<wgpu::ComputePass<'a>>;
}

// A collection of TileSet, potentially more than one per kind.
#[derive(Debug)]
pub(crate) struct TileManager {
    tile_sets: Vec<Box<dyn TileSet>>,
    take_index_snapshot: bool,
}

impl TileManager {
    pub(crate) fn new(
        displace_height_bind_group_layout: &wgpu::BindGroupLayout,
        accumulate_common_bind_group_layout: &wgpu::BindGroupLayout,
        catalog: &Catalog,
        globals_buffer: &GlobalParametersBuffer,
        gpu_detail: &GpuDetail,
        gpu: &Gpu,
    ) -> Result<Self> {
        // TODO: figure out a way to track a single tree with multiple tile-sets under it; we're
        //       wasting a ton of time recomputing the same visibility info for heights and normals
        //       and not taking advantage of the different required resolutions for each. It would
        //       be even more efficient to always load at the highest granularity and use the same
        //       tree for all spherical tiles
        // Scan catalog for all tile sets.
        let tile_sets = catalog
            .find_glob_with_extension("*-index.json", Some("json"))?
            .par_iter()
            .map(|&index_fid| {
                // Parse the index to figure out what sort of TileSet to create.
                let index_data = from_utf8_string(catalog.read_sync(index_fid)?)?;
                let index_json = json::parse(&index_data)?;
                let prefix = index_json["prefix"]
                    .as_str()
                    .ok_or_else(|| anyhow!("no prefix listed in index"))?;
                let kind = DataSetDataKind::from_name(
                    index_json["kind"]
                        .as_str()
                        .ok_or_else(|| anyhow!("no kind listed in index"))?,
                )?;
                let coordinates = DataSetCoordinates::from_name(
                    index_json["coordinates"]
                        .as_str()
                        .ok_or_else(|| anyhow!("no coordinates listed in index"))?,
                )?;

                Ok(match coordinates {
                    DataSetCoordinates::Spherical => match kind {
                        DataSetDataKind::Height => Box::new(SphericalHeightTileSet::new(
                            displace_height_bind_group_layout,
                            catalog,
                            prefix,
                            gpu_detail,
                            gpu,
                        )?) as Box<dyn TileSet>,
                        DataSetDataKind::Color => Box::new(SphericalColorTileSet::new(
                            accumulate_common_bind_group_layout,
                            catalog,
                            prefix,
                            globals_buffer,
                            gpu_detail,
                            gpu,
                        )?) as Box<dyn TileSet>,
                        DataSetDataKind::Normal => Box::new(SphericalNormalsTileSet::new(
                            accumulate_common_bind_group_layout,
                            catalog,
                            prefix,
                            globals_buffer,
                            gpu_detail,
                            gpu,
                        )?) as Box<dyn TileSet>,
                    },
                    DataSetCoordinates::CartesianPolar => {
                        bail!("unimplemented polar tiles")
                    }
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            tile_sets,
            take_index_snapshot: false,
        })
    }

    pub fn add_tile_set(&mut self, tile_set: Box<dyn TileSet>) -> TileSetHandle {
        let index = self.tile_sets.len();
        self.tile_sets.push(tile_set);
        TileSetHandle(index)
    }

    pub fn tile_set(&self, handle: TileSetHandle) -> &dyn TileSet {
        self.tile_sets[handle.0].as_ref()
    }

    pub fn tile_set_mut(&mut self, handle: TileSetHandle) -> &mut dyn TileSet {
        self.tile_sets[handle.0].as_mut()
    }

    pub fn begin_visibility_update(&mut self) {
        for ts in self.tile_sets.iter_mut() {
            ts.begin_visibility_update();
        }
    }

    pub fn note_required(&mut self, visible_patch: &VisiblePatch) {
        for ts in self.tile_sets.iter_mut() {
            ts.note_required(visible_patch);
        }
    }

    pub fn finish_visibility_update(&mut self, camera: &Camera, catalog: Arc<RwLock<Catalog>>) {
        for ts in self.tile_sets.iter_mut() {
            ts.finish_visibility_update(camera, catalog.clone());
        }
    }

    pub fn ensure_uploaded(
        &mut self,
        async_rt: &Runtime,
        gpu: &mut Gpu,
        tracker: &mut UploadTracker,
    ) {
        for ts in self.tile_sets.iter_mut() {
            ts.ensure_uploaded(gpu, tracker);
        }

        if self.take_index_snapshot {
            for ts in self.tile_sets.iter_mut() {
                ts.snapshot_index(async_rt, gpu);
            }
            self.take_index_snapshot = false;
        }
    }

    pub fn snapshot_index(&mut self) {
        self.take_index_snapshot = true;
    }

    pub fn paint_atlas_indices(
        &self,
        mut encoder: wgpu::CommandEncoder,
    ) -> Result<wgpu::CommandEncoder> {
        for ts in self.tile_sets.iter() {
            ts.paint_atlas_index(&mut encoder);
        }
        Ok(encoder)
    }

    pub fn displace_height<'a>(
        &'a self,
        vertex_count: u32,
        mesh_bind_group: &'a wgpu::BindGroup,
        mut cpass: wgpu::ComputePass<'a>,
    ) -> Result<wgpu::ComputePass<'a>> {
        for ts in self.tile_sets.iter() {
            cpass = ts.displace_height(vertex_count, mesh_bind_group, cpass)?;
        }
        Ok(cpass)
    }

    pub fn accumulate_normals<'a>(
        &'a self,
        mut cpass: wgpu::ComputePass<'a>,
        extent: &wgpu::Extent3d,
        globals_buffer: &'a GlobalParametersBuffer,
        accumulate_common_bind_group: &'a wgpu::BindGroup,
    ) -> Result<wgpu::ComputePass<'a>> {
        for ts in self.tile_sets.iter() {
            cpass =
                ts.accumulate_normals(extent, globals_buffer, accumulate_common_bind_group, cpass)?;
        }
        Ok(cpass)
    }

    pub fn accumulate_colors<'a>(
        &'a self,
        mut cpass: wgpu::ComputePass<'a>,
        extent: &wgpu::Extent3d,
        globals_buffer: &'a GlobalParametersBuffer,
        accumulate_common_bind_group: &'a wgpu::BindGroup,
    ) -> Result<wgpu::ComputePass<'a>> {
        for ts in self.tile_sets.iter() {
            cpass =
                ts.accumulate_colors(extent, globals_buffer, accumulate_common_bind_group, cpass)?;
        }
        Ok(cpass)
    }
}
