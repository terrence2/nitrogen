// This file is part of OpenFA.
//
// OpenFA is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// OpenFA is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with OpenFA.  If not, see <http://www.gnu.org/licenses/>.

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

// First pass: hard code everything.
use crate::{
    tile::{
        quad_tree::{QuadTree, QuadTreeId},
        DataSetCoordinates, DataSetDataKind,
    },
    GpuDetail,
};
use catalog::{from_utf8_string, Catalog};
use failure::{err_msg, Fallible};
use futures::Future;
use geodesy::{GeoCenter, Graticule};
use gpu::GPU;
use std::{
    collections::{BinaryHeap, HashMap},
    sync::Arc,
};
use tokio::{
    runtime::Runtime,
    sync::{
        mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
        RwLock,
    },
};

// FIXME: this should be system dependent and configurable
const MAX_CONCURRENT_READS: usize = 5;

const TILE_SIZE: u32 = 512;

const INDEX_WIDTH: u32 = 2560;
const INDEX_HEIGHT: u32 = 1280;

pub(crate) struct TileManager {
    tile_sets: Vec<TileSet>,
}

impl TileManager {
    pub(crate) fn new(catalog: &Catalog, gpu_detail: &GpuDetail, gpu: &mut GPU) -> Fallible<Self> {
        let mut tile_sets = Vec::new();

        // Scan catalog for all tile sets.
        for index_fid in catalog.find_matching("*-index.json")? {
            let index_data = from_utf8_string(catalog.read_sync(index_fid)?)?;
            let index_json = json::parse(&index_data)?;
            tile_sets.push(TileSet::new(catalog, index_json, gpu_detail, gpu)?);
        }

        Ok(Self { tile_sets })
    }

    pub fn begin_update(&mut self) {
        for ts in self.tile_sets.iter_mut() {
            ts.begin_update();
        }
    }

    pub fn note_required(&mut self, grat: &Graticule<GeoCenter>) {
        for ts in self.tile_sets.iter_mut() {
            ts.note_required(grat);
        }
    }

    pub fn finish_update(&mut self, catalog: Arc<RwLock<Catalog>>, async_rt: &mut Runtime) {
        for ts in self.tile_sets.iter_mut() {
            ts.finish_update(catalog.clone(), async_rt);
        }
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        self.tile_sets[0].bind_group_layout()
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        self.tile_sets[0].bind_group()
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum TileState {
    Pending,
    Reading,
    Active,
}

pub(crate) struct TileSet {
    #[allow(unused)]
    index_texture_extent: wgpu::Extent3d,
    #[allow(unused)]
    index_texture: wgpu::Texture,
    #[allow(unused)]
    index_texture_view: wgpu::TextureView,
    #[allow(unused)]
    index_texture_sampler: wgpu::Sampler,

    #[allow(unused)]
    atlas_texture_extent: wgpu::Extent3d,
    #[allow(unused)]
    atlas_texture: wgpu::Texture,
    #[allow(unused)]
    atlas_texture_view: wgpu::TextureView,
    #[allow(unused)]
    atlas_texture_sampler: wgpu::Sampler,

    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,

    tile_tree: QuadTree,
    tile_state: HashMap<QuadTreeId, TileState>,
    tile_load_queue: BinaryHeap<(u32, QuadTreeId)>,
    tile_read_count: usize,
    tile_sender: UnboundedSender<(QuadTreeId, Vec<u8>)>,
    tile_receiver: UnboundedReceiver<(QuadTreeId, Vec<u8>)>,
}

impl TileSet {
    pub(crate) fn new(
        catalog: &Catalog,
        index_json: json::JsonValue,
        gpu_detail: &GpuDetail,
        gpu: &mut GPU,
    ) -> Fallible<Self> {
        let prefix = index_json["prefix"]
            .as_str()
            .ok_or_else(|| err_msg("no prefix listed in index"))?;
        let kind = DataSetDataKind::from_name(
            index_json["kind"]
                .as_str()
                .ok_or_else(|| err_msg("no kind listed in index"))?,
        )?;
        let coordinates = DataSetCoordinates::from_name(
            index_json["coordinates"]
                .as_str()
                .ok_or_else(|| err_msg("no coordinates listed in index"))?,
        )?;

        let tile_tree = QuadTree::from_catalog(&prefix, catalog)?;
        // let srtm_path = PathBuf::from("/home/terrence/storage/srtm/output/srtm/");

        // FIXME: abstract this out into a DataSet container of some sort so we can at least
        //        get rid of the extremely long names.

        // The index texture is just a more or less normal texture. The longitude in spherical
        // coordinates maps to `s` and the latitude maps to `t` (with some important finagling).
        // Each pixel of the index is arranged such that it maps to a single tile at highest
        // resolution: 30 arcseconds per sample at 510 samples. Lower resolution tiles, naturally
        // fill more than a single pixel of the index. We sample the index texture with "nearest"
        // filtering such that any sample taken in the tile area will map exactly to the right
        // tile. Tiles are additionally fringed with a border such that linear filtering can be
        // used in the tile lookup without further effort. In combination, this lets us point the
        // full power of the texturing hardware at the problem, with very little extra overhead.
        let index_texture_extent = wgpu::Extent3d {
            width: INDEX_WIDTH,
            height: INDEX_HEIGHT,
            depth: 1,
        };
        let index_texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("terrain-geo-tile-index-texture"),
            size: index_texture_extent,
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rg16Uint, // offset into atlas stack; also depth or scale?
            usage: wgpu::TextureUsage::all(),
        });
        let index_texture_view = index_texture.create_view(&wgpu::TextureViewDescriptor {
            format: wgpu::TextureFormat::Rg16Uint,
            dimension: wgpu::TextureViewDimension::D2,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            array_layer_count: 1,
        });
        let index_texture_sampler = gpu.device().create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_min_clamp: 0f32,
            lod_max_clamp: 9_999_999f32,
            compare: wgpu::CompareFunction::Never,
        });

        // The atlas texture is a 2d array of tiles. All tiles have the same size, but may be
        // pre-sampled at various scaling factors, allowing us to use a single atlas for all
        // resolutions. Management of tile layers is done on the CPU between frames, using the
        // patch tree to figure out what is going to be most useful to have in the cache.
        let atlas_texture_extent = wgpu::Extent3d {
            width: TILE_SIZE,
            height: TILE_SIZE,
            depth: 1, // Note: the texture array size is specified elsewhere.
        };
        let atlas_texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("terrain-geo-tile-atlas-texture"),
            size: atlas_texture_extent,
            array_layer_count: gpu_detail.tile_cache_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R16Sint,
            usage: wgpu::TextureUsage::all(),
        });
        let atlas_texture_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor {
            format: wgpu::TextureFormat::R16Sint, // heights
            dimension: wgpu::TextureViewDimension::D2Array,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            array_layer_count: gpu_detail.tile_cache_size,
        });
        let atlas_texture_sampler = gpu.device().create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest, // We should be able to mip between levels...
            lod_min_clamp: 0f32,
            lod_max_clamp: 9_999_999f32,
            compare: wgpu::CompareFunction::Never,
        });

        let bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("terrain-geo-tile-bind-group-layout"),
                    bindings: &[
                        // Index Texture
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStage::VERTEX | wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::SampledTexture {
                                dimension: wgpu::TextureViewDimension::D2,
                                component_type: wgpu::TextureComponentType::Uint,
                                multisampled: false,
                            },
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStage::VERTEX | wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::Sampler { comparison: false },
                        },
                        // Atlas Textures, as referenced by the above index
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStage::VERTEX | wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::SampledTexture {
                                dimension: wgpu::TextureViewDimension::D2Array,
                                component_type: wgpu::TextureComponentType::Sint,
                                multisampled: false,
                            },
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStage::VERTEX | wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::Sampler { comparison: false },
                        },
                    ],
                });

        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain-geo-tile-bind-group"),
            layout: &bind_group_layout,
            bindings: &[
                // Index
                wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&index_texture_view),
                },
                wgpu::Binding {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&index_texture_sampler),
                },
                // Atlas
                wgpu::Binding {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&atlas_texture_view),
                },
                wgpu::Binding {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&atlas_texture_sampler),
                },
            ],
        });

        // FIXME: test that our basic primitives work as expected.
        /*
        let root_data = {
            let mut path = srtm_path;
            path.push(srtm_index_json["path"].as_str().expect("string"));
            let mut fp = File::open(&path)?;
            let mut data = [0u8; 2 * 512 * 512];
            fp.read_exact(&mut data)?;
            //data
            let as2: &[u8] = &data;
            let result_data: LayoutVerified<&[u8], [u16]> = LayoutVerified::new_slice(as2).unwrap();
            result_data.into_slice().to_owned()
        };

        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("terrain-geo-initial-texture-uploader-command-encoder"),
            });
        let buffer = gpu.push_slice(
            "terrain-geo-root-atlas-upload-buffer",
            &root_data,
            wgpu::BufferUsage::COPY_SRC,
        );
        encoder.copy_buffer_to_texture(
            wgpu::BufferCopyView {
                buffer: &buffer,
                offset: 0,
                bytes_per_row: atlas_texture_extent.width * 2,
                rows_per_image: atlas_texture_extent.height,
            },
            wgpu::TextureCopyView {
                texture: &atlas_texture,
                mip_level: 0,
                array_layer: 0u32, // FIXME: hardcoded until we get the index working
                origin: wgpu::Origin3d::ZERO,
            },
            atlas_texture_extent,
        );
        gpu.queue_mut().submit(&[encoder.finish()]);
        gpu.device().poll(wgpu::Maintain::Wait);
         */

        let (tile_sender, tile_receiver) = unbounded_channel();

        Ok(Self {
            index_texture_extent,
            index_texture,
            index_texture_view,
            index_texture_sampler,

            atlas_texture_extent,
            atlas_texture,
            atlas_texture_view,
            atlas_texture_sampler,

            bind_group_layout,
            bind_group,

            tile_tree,
            tile_state: HashMap::new(),
            tile_load_queue: BinaryHeap::new(),
            tile_read_count: 0,
            tile_sender,
            tile_receiver,
        })
    }

    pub fn begin_update(&mut self) {
        self.tile_tree.begin_update();
    }

    pub fn note_required(&mut self, grat: &Graticule<GeoCenter>) {
        self.tile_tree.note_required(grat);
    }

    pub fn finish_update(&mut self, catalog: Arc<RwLock<Catalog>>, async_rt: &mut Runtime) {
        let mut additions = Vec::new();
        let mut removals = Vec::new();
        self.tile_tree.finish_update(&mut additions, &mut removals);

        // Apply removals and additions.
        for qtid in &removals {
            self.tile_state.remove(qtid);
        }
        for &(votes, qtid) in &additions {
            if !self.tile_state.contains_key(&qtid) {
                self.tile_state.insert(qtid, TileState::Pending);
                self.tile_load_queue.push((votes, qtid));
            }
        }

        // Kick off any loads, if there is space remaining.
        while !self.tile_load_queue.is_empty() && self.tile_read_count < MAX_CONCURRENT_READS {
            let (_, qtid) = self.tile_load_queue.pop().expect("checked is_empty");

            // There may be many frames between when a thing is inserted in the load queue and when
            // we have disk bandwidth available to read it. Thus we need to double-check that we
            // even still have it as an active tile and it's current state.
            let maybe_state = self.tile_state.get(&qtid);
            if maybe_state.is_none() {
                continue;
            }
            let state = *maybe_state.unwrap();
            if state != TileState::Pending {
                continue;
            }

            // If the state was pending, move us to the reading state and consume a read slot.
            self.tile_state.insert(qtid, TileState::Reading);
            self.tile_read_count += 1;

            // Do the read in a disconnected greenthread and send it back on an mpsc queue.
            let fid = self.tile_tree.file_id(&qtid);
            let closure_catalog = catalog.clone();
            let closer_sender = self.tile_sender.clone();
            async_rt.spawn(async move {
                let data = closure_catalog.read().await.read(fid).await.unwrap();
                let foo = closer_sender.send((qtid, data));
            });
        }

        // Check for any completed reads.
        while let Ok((qtid, data)) = self.tile_receiver.try_recv() {
            self.tile_read_count -= 1;
            self.tile_state.insert(qtid, TileState::Active);
            // TODO: push this into the atlas and update the index
            println!("DATA @ {:?} <- {}b", qtid, data.len());
        }

        println!("ADDITIONS: {:?}, REMOVALS: {:?}", additions, removals);
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::GpuDetailLevel;
    use input::InputSystem;

    #[test]
    fn test_tile_manager() -> Fallible<()> {
        let catalog = Catalog::empty();
        let input = InputSystem::new(vec![])?;
        let mut gpu = GPU::new(&input, Default::default())?;
        let _tm = TileManager::new(&catalog, &GpuDetailLevel::Low.parameters(), &mut gpu)?;
        gpu.device().poll(wgpu::Maintain::Wait);
        Ok(())
    }
}
