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
        spherical_tile_set::SphericalTileSet, tile_info::TileInfo, DataSetCoordinates,
        DataSetDataKind,
    },
    GpuDetail, VisiblePatch,
};
use anyhow::{anyhow, bail, Result};
use catalog::{from_utf8_string, Catalog};
use gpu::{UploadTracker, GPU};
use smallvec::{smallvec, SmallVec};
use std::{fmt::Debug, mem, num::NonZeroU64, sync::Arc};
use tokio::{runtime::Runtime, sync::RwLock};

// A collection of TileSet, potentially more than one per kind.
#[derive(Debug)]
pub(crate) struct TileManager {
    // TODO: we will probably need some way of finding the right ones efficiently.
    tile_sets: Vec<Box<dyn TileSet>>,

    tile_set_bind_group_layout_sint: wgpu::BindGroupLayout,
    tile_set_bind_group_layout_float: wgpu::BindGroupLayout,
}

/// Layouts that are shared by all shaders implementing TileSetInterface.
#[derive(Debug)]
pub(crate) struct BindGroupLayouts<'a> {
    pub displace_height: &'a wgpu::BindGroupLayout,
    pub accumulate_tiled_sint: &'a wgpu::BindGroupLayout,
    pub accumulate_tiled_float: &'a wgpu::BindGroupLayout,
}

pub trait TileSet: Debug + Send + Sync + 'static {
    // Indicates what passes this tile set will be used for.
    fn kind(&self) -> DataSetDataKind;
    fn coordinates(&self) -> DataSetCoordinates;

    // Maintain runtime visibility tracking based on VisiblePatch notifications derived
    // from the global geometry calculations.
    fn begin_update(&mut self);
    fn note_required(&mut self, visible_patch: &VisiblePatch);
    fn finish_update(
        &mut self,
        catalog: Arc<RwLock<Catalog>>,
        async_rt: &mut Runtime,
        gpu: &mut GPU,
        tracker: &mut UploadTracker,
    );

    // Indicate that the current index should be written to the debug file.
    fn snapshot_index(&mut self);

    // Per-frame opportunity to update the index based on any visibility updates pushed above.
    fn paint_atlas_index(&self, encoder: &mut wgpu::CommandEncoder) -> Result<()>;

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

    // The bind group representing the GPU resources that will be bound before....?
    // combining the index, atlas, and TileInfo buffer that describes
    // the atlas content.
    fn bind_group(&self) -> &wgpu::BindGroup;
}

impl TileManager {
    pub(crate) fn new(
        displace_height_bind_group_layout: &wgpu::BindGroupLayout,
        catalog: &Catalog,
        gpu_detail: &GpuDetail,
        gpu: &mut GPU,
    ) -> Result<Self> {
        let mut tile_sets = Vec::new();

        // This layout is common for all indexed data sets.
        // Note: this size must match the buffer size we allocate in all tile sets.
        let atlas_tile_info_buffer_size =
            (mem::size_of::<TileInfo>() as u32 * gpu_detail.tile_cache_size) as wgpu::BufferAddress;
        let tile_set_bind_group_layout_sint =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("terrain-geo-tile-bind-group-layout"),
                    entries: &[
                        // Index Texture
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Uint,
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Sampler {
                                filtering: true,
                                comparison: false,
                            },
                            count: None,
                        },
                        // Atlas Textures, as referenced by the above index
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                view_dimension: wgpu::TextureViewDimension::D2Array,
                                sample_type: wgpu::TextureSampleType::Sint,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Sampler {
                                filtering: true,
                                comparison: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 4,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: NonZeroU64::new(atlas_tile_info_buffer_size),
                            },
                            count: None,
                        },
                    ],
                });
        let tile_set_bind_group_layout_float =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("terrain-geo-tile-bind-group-layout"),
                    entries: &[
                        // Index Texture
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                view_dimension: wgpu::TextureViewDimension::D2,
                                sample_type: wgpu::TextureSampleType::Uint,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Sampler {
                                filtering: true,
                                comparison: false,
                            },
                            count: None,
                        },
                        // Atlas Textures, as referenced by the above index
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                view_dimension: wgpu::TextureViewDimension::D2Array,
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Sampler {
                                filtering: true,
                                comparison: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 4,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: NonZeroU64::new(atlas_tile_info_buffer_size),
                            },
                            count: None,
                        },
                    ],
                });

        let bind_group_layouts = BindGroupLayouts {
            displace_height: displace_height_bind_group_layout,
            accumulate_tiled_sint: &tile_set_bind_group_layout_sint,
            accumulate_tiled_float: &tile_set_bind_group_layout_float,
        };

        // Scan catalog for all tile sets.
        for index_fid in catalog.find_labeled_matching("default", "*-index.json", Some("json"))? {
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

            let tile_set = match coordinates {
                DataSetCoordinates::Spherical => SphericalTileSet::new(
                    &bind_group_layouts,
                    catalog,
                    prefix,
                    kind,
                    coordinates,
                    gpu_detail,
                    gpu,
                )?,
                DataSetCoordinates::CartesianPolar => {
                    bail!("unimplemented polar tiles")
                }
            };
            tile_sets.push(Box::new(tile_set) as Box<dyn TileSet>);
        }

        Ok(Self {
            tile_set_bind_group_layout_sint,
            tile_set_bind_group_layout_float,
            tile_sets,
        })
    }

    pub fn add_tile_set(&mut self, tile_set: Box<dyn TileSet>) {
        self.tile_sets.push(tile_set);
    }

    pub fn begin_update(&mut self) {
        for ts in self.tile_sets.iter_mut() {
            ts.begin_update();
        }
    }

    pub fn note_required(&mut self, visible_patch: &VisiblePatch) {
        for ts in self.tile_sets.iter_mut() {
            ts.note_required(visible_patch);
        }
    }

    pub fn finish_update(
        &mut self,
        catalog: Arc<RwLock<Catalog>>,
        async_rt: &mut Runtime,
        gpu: &mut GPU,
        tracker: &mut UploadTracker,
    ) {
        for ts in self.tile_sets.iter_mut() {
            ts.finish_update(catalog.clone(), async_rt, gpu, tracker);
        }
    }

    pub fn snapshot_index(&mut self) {
        for ts in self.tile_sets.iter_mut() {
            ts.snapshot_index();
        }
    }

    pub fn paint_atlas_indices(
        &self,
        mut encoder: wgpu::CommandEncoder,
    ) -> Result<wgpu::CommandEncoder> {
        for ts in self.tile_sets.iter() {
            ts.paint_atlas_index(&mut encoder)?
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

    pub fn tile_set_bind_group_layout_sint(&self) -> &wgpu::BindGroupLayout {
        &self.tile_set_bind_group_layout_sint
    }

    pub fn tile_set_bind_group_layout_float(&self) -> &wgpu::BindGroupLayout {
        &self.tile_set_bind_group_layout_float
    }

    pub fn spherical_normal_bind_groups(&self) -> SmallVec<[&wgpu::BindGroup; 4]> {
        let mut out = smallvec![];
        for ts in &self.tile_sets {
            if ts.kind() == DataSetDataKind::Normal
                && ts.coordinates() == DataSetCoordinates::Spherical
            {
                out.push(ts.bind_group())
            }
        }
        out
    }

    pub fn spherical_color_bind_groups(&self) -> SmallVec<[&wgpu::BindGroup; 4]> {
        let mut out = smallvec![];
        for ts in &self.tile_sets {
            if ts.kind() == DataSetDataKind::Color
                && ts.coordinates() == DataSetCoordinates::Spherical
            {
                out.push(ts.bind_group())
            }
        }
        out
    }
}
