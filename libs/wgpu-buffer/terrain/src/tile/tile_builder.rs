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
    VisiblePatch,
};
use anyhow::{anyhow, Result};
use camera::ScreenCamera;
use catalog::{from_utf8_string, Catalog};
use global_data::GlobalParametersBuffer;
use gpu::Gpu;
use nitrous::make_symbol;
use rayon::prelude::*;
use runtime::Runtime;
use std::{any::Any, fmt::Debug};

pub trait TileSet: Debug + Send + Sync + 'static {
    // Allow downcast back into concrete types so we can stream data.
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;

    // Maintain runtime visibility tracking based on VisiblePatch notifications derived
    // from the global geometry calculations.
    fn begin_visibility_update(&mut self);
    fn note_required(&mut self, visible_patch: &VisiblePatch);
    fn finish_visibility_update(&mut self, camera: &ScreenCamera, catalog: &mut Catalog);
    fn encode_uploads(&mut self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder);

    // Indicate that the current index should be written to the debug file.
    fn snapshot_index(&mut self, gpu: &mut Gpu);

    // Per-frame opportunity to update the index based on any visibility updates pushed above.
    fn paint_atlas_index(&self, encoder: &mut wgpu::CommandEncoder);

    // Safe any ongoing activity before the system starts dropping parts of itself.
    fn shutdown_safely(&mut self);
}

pub trait HeightsTileSet: TileSet {
    // The Terrain engine produces an optimal, gpu-tesselated mesh at the start of every frame
    // based on the current view. This mesh is at the terrain surface. This callback gives each
    // tile-set an opportunity to displace the terrain based on a shader and the height data we
    // uploaded as part of finish_update and paint_atlas_index.
    //
    // The terrain may already have been updated by other passes. The implementor should be
    // sure to coordinate to make sure that all passes sum together nicely.
    fn displace_height(
        &self,
        vertex_count: u32,
        mesh_bind_group: &wgpu::BindGroup,
        encoder: &mut wgpu::CommandEncoder,
    );
}

pub trait NormalsTileSet: TileSet {
    // Implementors should read from the provided screen space world position coordinates and
    // accumulate into the provided normal accumulation buffer. Accumulation buffers will be
    // automatically cleared at the start of each frame. TileSets should pre-arrange with each
    // other the relative weight of their contributions.
    fn accumulate_normals(
        &self,
        extent: &wgpu::Extent3d,
        globals: &GlobalParametersBuffer,
        accumulate_common_bind_group: &wgpu::BindGroup,
        encoder: &mut wgpu::CommandEncoder,
    );
}

pub trait ColorsTileSet: TileSet {
    // Implementors should read from the provided screen space world position coordinates and
    // accumulate into the provided color accumulation buffer. Accumulation buffers will be
    // automatically cleared at the start of each frame. TileSets should pre-arrange with each
    // other the relative weight of their contributions.
    fn accumulate_colors(
        &self,
        extent: &wgpu::Extent3d,
        globals: &GlobalParametersBuffer,
        accumulate_common_bind_group: &wgpu::BindGroup,
        encoder: &mut wgpu::CommandEncoder,
    );
}

#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
enum GenericTileSet {
    SphericalHeights(SphericalHeightTileSet),
    SphericalNormals(SphericalNormalsTileSet),
    SphericalColors(SphericalColorTileSet),
}

#[derive(Debug)]
pub(crate) struct TileSetDescriptor {
    prefix: String,
    kind: DataSetDataKind,
    coordinates: DataSetCoordinates,
    tile_set: Option<GenericTileSet>,
}

impl TileSetDescriptor {
    fn new(prefix: &str, kind: DataSetDataKind, coordinates: DataSetCoordinates) -> Self {
        Self {
            prefix: prefix.to_owned(),
            kind,
            coordinates,
            tile_set: None,
        }
    }
}

// A collection of TileSet, potentially more than one per kind.
#[derive(Debug)]
pub(crate) struct TileSetBuilder {
    descriptors: Vec<TileSetDescriptor>,
}

impl TileSetBuilder {
    pub(crate) fn discover_tiles(catalog: &Catalog) -> Result<Self> {
        let mut descriptors = Vec::new();
        for index_fid in catalog.find_glob_with_extension("*-index.json", Some("json"))? {
            // Parse the index to figure out what sort of TileSet to create.
            let index_data = from_utf8_string(catalog.read(index_fid)?)?;
            let index_json = json::parse(index_data.as_ref())?;
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
            descriptors.push(TileSetDescriptor::new(prefix, kind, coordinates));
        }
        Ok(Self { descriptors })
    }

    pub(crate) fn build_parallel(
        mut self,
        displace_height_bind_group_layout: &wgpu::BindGroupLayout,
        accumulate_common_bind_group_layout: &wgpu::BindGroupLayout,
        tile_cache_size: u32,
        catalog: &Catalog,
        globals_buffer: &GlobalParametersBuffer,
        gpu: &Gpu,
    ) -> Result<Self> {
        self.descriptors
            .par_iter_mut()
            .for_each(|mut desc| match (desc.coordinates, desc.kind) {
                (DataSetCoordinates::Spherical, DataSetDataKind::Height) => {
                    let tile_set = GenericTileSet::SphericalHeights(
                        SphericalHeightTileSet::new(
                            displace_height_bind_group_layout,
                            catalog,
                            &desc.prefix,
                            tile_cache_size,
                            gpu,
                        )
                        .unwrap(),
                    );
                    desc.tile_set = Some(tile_set);
                }
                (DataSetCoordinates::Spherical, DataSetDataKind::Normal) => {
                    let tile_set = GenericTileSet::SphericalNormals(
                        SphericalNormalsTileSet::new(
                            accumulate_common_bind_group_layout,
                            catalog,
                            &desc.prefix,
                            globals_buffer,
                            tile_cache_size,
                            gpu,
                        )
                        .unwrap(),
                    );
                    desc.tile_set = Some(tile_set);
                }
                (DataSetCoordinates::Spherical, DataSetDataKind::Color) => {
                    let tile_set = GenericTileSet::SphericalColors(
                        SphericalColorTileSet::new(
                            accumulate_common_bind_group_layout,
                            catalog,
                            &desc.prefix,
                            globals_buffer,
                            tile_cache_size,
                            gpu,
                        )
                        .unwrap(),
                    );
                    desc.tile_set = Some(tile_set);
                }
                (DataSetCoordinates::CartesianPolar, _) => {
                    panic!("unimplemented polar tiles")
                }
            });
        Ok(self)
    }

    pub(crate) fn inject_into_runtime(mut self, runtime: &mut Runtime) -> Result<()> {
        for desc in self.descriptors.drain(..) {
            match desc.tile_set.unwrap() {
                GenericTileSet::SphericalHeights(tile_set) => runtime
                    .spawn_named(&make_symbol(desc.prefix))?
                    .insert_named(tile_set)?,
                GenericTileSet::SphericalNormals(tile_set) => runtime
                    .spawn_named(&make_symbol(desc.prefix))?
                    .insert_named(tile_set)?,
                GenericTileSet::SphericalColors(tile_set) => runtime
                    .spawn_named(&make_symbol(desc.prefix))?
                    .insert_named(tile_set)?,
            };
        }
        Ok(())
    }
}
