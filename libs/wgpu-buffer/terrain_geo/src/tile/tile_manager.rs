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
    tile::{tile_info::TileInfo, tile_set::TileSet, DataSetCoordinates, DataSetDataKind},
    GpuDetail, VisiblePatch,
};
use catalog::{from_utf8_string, Catalog};
use failure::Fallible;
use gpu::{UploadTracker, GPU};
use smallvec::{smallvec, SmallVec};
use std::{mem, num::NonZeroU64, sync::Arc};
use tokio::{runtime::Runtime, sync::RwLock};

// A collection of TileSet, potentially more than one per kind.
#[derive(Debug)]
pub(crate) struct TileManager {
    // TODO: we will probably need some way of finding the right ones efficiently.
    tile_sets: Vec<TileSet>,

    tile_set_bind_group_layout_sint: wgpu::BindGroupLayout,
    tile_set_bind_group_layout_float: wgpu::BindGroupLayout,
}

impl TileManager {
    pub(crate) fn new(
        displace_height_bind_group_layout: &wgpu::BindGroupLayout,
        catalog: &Catalog,
        gpu_detail: &GpuDetail,
        gpu: &mut GPU,
    ) -> Fallible<Self> {
        let mut tile_sets = Vec::new();

        // This layout is common for all indexed data sets.
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
                            ty: wgpu::BindingType::SampledTexture {
                                dimension: wgpu::TextureViewDimension::D2,
                                component_type: wgpu::TextureComponentType::Uint,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Sampler { comparison: false },
                            count: None,
                        },
                        // Atlas Textures, as referenced by the above index
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::SampledTexture {
                                dimension: wgpu::TextureViewDimension::D2Array,
                                component_type: wgpu::TextureComponentType::Sint,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Sampler { comparison: false },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 4,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::StorageBuffer {
                                dynamic: false,
                                readonly: true,
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
                            ty: wgpu::BindingType::SampledTexture {
                                dimension: wgpu::TextureViewDimension::D2,
                                component_type: wgpu::TextureComponentType::Uint,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Sampler { comparison: false },
                            count: None,
                        },
                        // Atlas Textures, as referenced by the above index
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::SampledTexture {
                                dimension: wgpu::TextureViewDimension::D2Array,
                                component_type: wgpu::TextureComponentType::Float,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Sampler { comparison: false },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 4,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::StorageBuffer {
                                dynamic: false,
                                readonly: true,
                                min_binding_size: NonZeroU64::new(atlas_tile_info_buffer_size),
                            },
                            count: None,
                        },
                    ],
                });

        // Scan catalog for all tile sets.
        for index_fid in catalog.find_labeled_matching("default", "*-index.json", Some("json"))? {
            let index_data = from_utf8_string(catalog.read_sync(index_fid)?)?;
            let index_json = json::parse(&index_data)?;
            tile_sets.push(TileSet::new(
                atlas_tile_info_buffer_size,
                [
                    &tile_set_bind_group_layout_sint,
                    &tile_set_bind_group_layout_float,
                ],
                displace_height_bind_group_layout,
                catalog,
                index_json,
                gpu_detail,
                gpu,
            )?);
        }

        Ok(Self {
            tile_set_bind_group_layout_sint,
            tile_set_bind_group_layout_float,
            tile_sets,
        })
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
    ) -> Fallible<wgpu::CommandEncoder> {
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
    ) -> Fallible<wgpu::ComputePass<'a>> {
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
