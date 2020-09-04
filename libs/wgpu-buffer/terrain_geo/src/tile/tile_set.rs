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
use crate::{
    tile::{
        index_paint_vertex::IndexPaintVertex,
        quad_tree::{QuadTree, QuadTreeId},
        DataSetCoordinates, DataSetDataKind, TerrainLevel, TILE_EXTENT,
    },
    GpuDetail,
};
use absolute_unit::arcseconds;
use catalog::Catalog;
use failure::{err_msg, Fallible};
use geodesy::{GeoCenter, Graticule};
use gpu::{texture_format_size, UploadTracker, GPU};
use image::{ImageBuffer, Rgb};
use log::trace;
use std::{
    collections::{BTreeMap, BinaryHeap},
    fs,
    num::NonZeroU32,
    ops::Range,
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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum TileState {
    NoSpace,
    Pending(usize),
    Reading(usize),
    Active(usize),
}

impl TileState {
    fn is_pending(&self) -> bool {
        match self {
            Self::NoSpace => false,
            Self::Pending(_) => true,
            Self::Reading(_) => false,
            Self::Active(_) => false,
        }
    }

    fn is_reading(&self) -> bool {
        match self {
            Self::NoSpace => false,
            Self::Pending(_) => false,
            Self::Reading(_) => true,
            Self::Active(_) => false,
        }
    }

    fn atlas_slot(&self) -> usize {
        match *self {
            Self::NoSpace => panic!("called atlas slot on no-space state"),
            Self::Pending(slot) => slot,
            Self::Reading(slot) => slot,
            Self::Active(slot) => slot,
        }
    }
}

pub(crate) struct TileSet {
    index_texture_format: wgpu::TextureFormat,
    index_texture_extent: wgpu::Extent3d,
    index_texture: wgpu::Texture,
    #[allow(unused)]
    index_texture_view: wgpu::TextureView,
    #[allow(unused)]
    index_texture_sampler: wgpu::Sampler,

    index_paint_pipeline: wgpu::RenderPipeline,
    index_paint_range: Range<u32>,
    index_paint_vert_buffer: Arc<Box<wgpu::Buffer>>,

    atlas_texture_format: wgpu::TextureFormat,
    atlas_texture_extent: wgpu::Extent3d,
    atlas_texture: Arc<Box<wgpu::Texture>>,
    #[allow(unused)]
    atlas_texture_view: wgpu::TextureView,
    #[allow(unused)]
    atlas_texture_sampler: wgpu::Sampler,

    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,

    #[allow(unused)]
    kind: DataSetDataKind,
    #[allow(unused)]
    coordinates: DataSetCoordinates,

    // For each offset in the atlas, records the allocation state and the target tile, if allocated.
    atlas_tile_map: Vec<Option<QuadTreeId>>,

    // A list of all free offsets in the atlas.
    atlas_free_list: Vec<usize>,

    // The full tree of possible tiles.
    tile_tree: QuadTree,

    // Map of the tile states, given the list of adds and removals from the tile_tree as the view
    // moves about. This can be empty, NoSpace if there is not a slot free in the atlas, Pending
    // while waiting to read the tile, Reading when the background thread is outstanding,
    // then Active once the tile is uploaded. Since this is a BTreeMap, the keys are sorted. Since
    // we allocate QuadTreeId breadth first, we can use the ordering as a paint list.
    tile_state: BTreeMap<QuadTreeId, TileState>,

    // A list of requested loads, sorted by vote count. If we have empty read slots, per the tile
    // tile_read_count, we'll pull from this. Given the async nature of tile loads, this will
    // frequently contain repeats and dead tiles that have since moved out of view.
    tile_load_queue: BinaryHeap<(u32, QuadTreeId)>,

    // Number of async read slots currently being utilized. We will ideally set this higher
    // on machines with more disk parallelism.
    // TODO: figure out what disk the catalog is coming from and use some heuristics
    tile_read_count: usize,

    // Tile transfer from the background read thread to the main thread.
    tile_sender: UnboundedSender<(QuadTreeId, Vec<u8>)>,
    tile_receiver: UnboundedReceiver<(QuadTreeId, Vec<u8>)>,

    // Set to true to take a snapshot at the start of the next frame.
    take_index_snapshot: bool,
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

        // The index texture is just a more or less normal texture. The longitude in spherical
        // coordinates maps to `s` and the latitude maps to `t` (with some important finagling).
        // Each pixel of the index is arranged such that it maps to a single tile at highest
        // resolution: 30 arcseconds per sample at 510 samples. Lower resolution tiles, naturally
        // fill more than a single pixel of the index. We sample the index texture with "nearest"
        // filtering such that any sample taken in the tile area will map exactly to the right
        // tile. Tiles are additionally fringed with a border such that linear filtering can be
        // used in the tile lookup without further effort. In combination, this lets us point the
        // full power of the texturing hardware at the problem, with very little extra overhead.
        let index_texture_format = wgpu::TextureFormat::R16Uint; // offset into atlas stack
        println!(
            "base: {} {}",
            arcseconds!(TerrainLevel::base().latitude),
            arcseconds!(TerrainLevel::base().longitude)
        );
        println!(
            "index_base: {} {}",
            arcseconds!(TerrainLevel::index_base().latitude),
            arcseconds!(TerrainLevel::index_base().longitude)
        );
        println!("ang_ext: {}", TerrainLevel::base_angular_extent());

        // FIXME: find a way to use a smaller texture.
        // let index_texture_extent = wgpu::Extent3d {
        //     width: TerrainLevel::base_scale().f64() as u32,
        //     height: TerrainLevel::base_scale().f64() as u32,
        //     depth: 1,
        // };
        let index_texture_extent = wgpu::Extent3d {
            width: TerrainLevel::index_resolution().1,
            height: TerrainLevel::index_resolution().0,
            depth: 1,
        };
        let index_texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("terrain-geo-tile-index-texture"),
            size: index_texture_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: index_texture_format,
            usage: wgpu::TextureUsage::all(),
        });
        let index_texture_view = index_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("terrain-index-texture-view"),
            format: None,
            dimension: None,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        });
        let index_texture_sampler = gpu.device().create_sampler(&wgpu::SamplerDescriptor {
            label: Some("terrain-index-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_min_clamp: 0f32,
            lod_max_clamp: 9_999_999f32,
            compare: None,
            anisotropy_clamp: None,
        });
        let index_paint_range = 0u32..(6 * gpu_detail.tile_cache_size);
        let index_paint_vert_buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("index-paint-vert-buffer"),
            size: (IndexPaintVertex::mem_size() * 6 * gpu_detail.tile_cache_size as usize)
                as wgpu::BufferAddress,
            usage: wgpu::BufferUsage::COPY_DST | wgpu::BufferUsage::VERTEX,
            mapped_at_creation: false,
        });
        let index_paint_vert_shader =
            gpu.create_shader_module(include_bytes!("../../target/index_paint.vert.spirv"))?;
        let index_paint_frag_shader =
            gpu.create_shader_module(include_bytes!("../../target/index_paint.frag.spirv"))?;
        let index_paint_pipeline =
            gpu.device()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("terrain-index-paint-pipeline"),
                    layout: Some(&gpu.device().create_pipeline_layout(
                        &wgpu::PipelineLayoutDescriptor {
                            label: Some("terrain-index-paint-pipeline-layout"),
                            push_constant_ranges: &[],
                            bind_group_layouts: &[],
                        },
                    )),
                    vertex_stage: wgpu::ProgrammableStageDescriptor {
                        module: &index_paint_vert_shader,
                        entry_point: "main",
                    },
                    fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
                        module: &index_paint_frag_shader,
                        entry_point: "main",
                    }),
                    rasterization_state: Some(wgpu::RasterizationStateDescriptor {
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: wgpu::CullMode::Back,
                        depth_bias: 0,
                        depth_bias_slope_scale: 0.0,
                        depth_bias_clamp: 0.0,
                        clamp_depth: false,
                    }),
                    primitive_topology: wgpu::PrimitiveTopology::TriangleList,
                    color_states: &[wgpu::ColorStateDescriptor {
                        format: index_texture_format,
                        alpha_blend: wgpu::BlendDescriptor::REPLACE,
                        color_blend: wgpu::BlendDescriptor::REPLACE,
                        write_mask: wgpu::ColorWrite::ALL,
                    }],
                    depth_stencil_state: None,
                    vertex_state: wgpu::VertexStateDescriptor {
                        index_format: wgpu::IndexFormat::Uint16,
                        vertex_buffers: &[IndexPaintVertex::descriptor()],
                    },
                    sample_count: 1,
                    sample_mask: !0,
                    alpha_to_coverage_enabled: false,
                });

        // The atlas texture is a 2d array of tiles. All tiles have the same size, but may be
        // pre-sampled at various scaling factors, allowing us to use a single atlas for all
        // resolutions. Management of tile layers is done on the CPU between frames, using the
        // patch tree to figure out what is going to be most useful to have in the cache.
        let atlas_texture_format = wgpu::TextureFormat::R16Sint;
        let atlas_texture_extent = wgpu::Extent3d {
            width: TILE_SIZE,
            height: TILE_SIZE,
            depth: gpu_detail.tile_cache_size, // TODO: is texture array size specified here now?
        };
        let atlas_texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("terrain-geo-tile-atlas-texture"),
            size: atlas_texture_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: atlas_texture_format,
            usage: wgpu::TextureUsage::all(),
        });
        let atlas_texture_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("terrain-atlas-texture-view"),
            format: None,
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            level_count: None,
            base_array_layer: 0,
            array_layer_count: NonZeroU32::new(gpu_detail.tile_cache_size),
        });
        let atlas_texture_sampler = gpu.device().create_sampler(&wgpu::SamplerDescriptor {
            label: Some("terrain-atlas-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest, // We should be able to mip between levels...
            lod_min_clamp: 0f32,
            lod_max_clamp: 9_999_999f32,
            compare: None,
            anisotropy_clamp: None,
        });

        let bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("terrain-geo-tile-bind-group-layout"),
                    entries: &[
                        // Index Texture
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStage::VERTEX | wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::SampledTexture {
                                dimension: wgpu::TextureViewDimension::D2,
                                component_type: wgpu::TextureComponentType::Uint,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStage::VERTEX | wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::Sampler { comparison: false },
                            count: None,
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
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStage::VERTEX | wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::Sampler { comparison: false },
                            count: None,
                        },
                    ],
                });

        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain-geo-tile-bind-group"),
            layout: &bind_group_layout,
            entries: &[
                // Index
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&index_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&index_texture_sampler),
                },
                // Atlas
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&atlas_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&atlas_texture_sampler),
                },
            ],
        });

        let (tile_sender, tile_receiver) = unbounded_channel();

        Ok(Self {
            index_texture_format,
            index_texture_extent,
            index_texture,
            index_texture_view,
            index_texture_sampler,

            index_paint_pipeline,
            index_paint_range,
            index_paint_vert_buffer: Arc::new(Box::new(index_paint_vert_buffer)),

            atlas_texture_format,
            atlas_texture_extent,
            atlas_texture: Arc::new(Box::new(atlas_texture)),
            atlas_texture_view,
            atlas_texture_sampler,

            bind_group_layout,
            bind_group,

            kind,
            coordinates,

            atlas_tile_map: vec![None; gpu_detail.tile_cache_size as usize],
            atlas_free_list: (0..gpu_detail.tile_cache_size as usize).collect(),

            tile_tree,
            tile_state: BTreeMap::new(),
            tile_load_queue: BinaryHeap::new(),
            tile_read_count: 0,
            tile_sender,
            tile_receiver,

            take_index_snapshot: false,
        })
    }

    pub fn begin_update(&mut self) {
        self.tile_tree.begin_update();
    }

    pub fn note_required(&mut self, grat: &Graticule<GeoCenter>) {
        self.tile_tree.note_required(grat);
    }

    pub fn finish_update(
        &mut self,
        catalog: Arc<RwLock<Catalog>>,
        async_rt: &mut Runtime,
        gpu: &mut GPU,
        tracker: &mut UploadTracker,
    ) {
        if self.take_index_snapshot {
            self.capture_and_save_index_snapshot(async_rt, gpu).unwrap();
            self.take_index_snapshot = false;
        }

        let mut additions = Vec::new();
        let mut removals = Vec::new();
        self.tile_tree.finish_update(&mut additions, &mut removals);

        // Apply removals and additions.
        for &qtid in &removals {
            self.deallocate_atlas_slot(qtid);
        }
        for &(votes, qtid) in &additions {
            self.allocate_atlas_slot(votes, qtid);
        }

        // Kick off any loads, if there is space remaining.
        while !self.tile_load_queue.is_empty() && self.tile_read_count < MAX_CONCURRENT_READS {
            let (_, qtid) = self.tile_load_queue.pop().expect("checked is_empty");

            // There may be many frames between when a thing is inserted in the load queue and when
            // we have disk bandwidth available to read it. Thus we need to double-check that we
            // even still have it as an active tile and it's current state.
            let maybe_state = self.tile_state.get(&qtid);
            if maybe_state.is_none() || !maybe_state.unwrap().is_pending() {
                continue;
            }

            // If the state was pending, move us to the reading state and consume a read slot.
            let atlas_slot = maybe_state.unwrap().atlas_slot();
            self.tile_state.insert(qtid, TileState::Reading(atlas_slot));
            self.tile_read_count += 1;

            // Do the read in a disconnected greenthread and send it back on an mpsc queue.
            let fid = self.tile_tree.file_id(&qtid);
            let closure_catalog = catalog.clone();
            let closer_sender = self.tile_sender.clone();
            async_rt.spawn(async move {
                if let Ok(data) = closure_catalog.read().await.read(fid).await {
                    closer_sender.send((qtid, data)).ok();
                }
            });
        }

        // Check for any completed reads.
        while let Ok((qtid, data)) = self.tile_receiver.try_recv() {
            // If the reading tile has gone out of view in the time since it was enqueued, we
            // may have lost our atlas slot. That's fine, just dump the bytes on the floor.
            let maybe_state = self.tile_state.get(&qtid);
            if maybe_state.is_none() || !maybe_state.unwrap().is_reading() {
                continue;
            }

            let atlas_slot = maybe_state.unwrap().atlas_slot();
            self.tile_read_count -= 1;
            self.tile_state.insert(qtid, TileState::Active(atlas_slot));

            // TODO: push this into the atlas and update the index
            trace!("uploading {:?} -> {}", qtid, atlas_slot);

            let buffer = gpu.push_slice(
                "terrain-geo-atlas-tile-upload-buffer",
                &data,
                wgpu::BufferUsage::COPY_SRC,
            );
            tracker.upload_to_texture(
                buffer,
                self.atlas_texture.clone(),
                self.atlas_texture_extent,
                self.atlas_texture_format,
                atlas_slot as u32,
                1,
            );
        }

        // Use the list of allocated tiles to generate a vertex buffer to upload.
        // FIXME: don't re-allocate every frame
        let index_ang_extent = TerrainLevel::index_extent();
        let iextent_lat = index_ang_extent.0.f64() / 2.; // range from [-1,1]
        let iextent_lon = index_ang_extent.1.f64() / 2.;
        let mut tris = Vec::new();
        // println!("START");
        for (qtid, tile_state) in self.tile_state.iter() {
            // FIXME: where do we actually want these?
            if let TileState::Active(slot) = tile_state {
                // Project the tile base and angular extent into the index.
                // Note that the base may be outside the index extents.
                let tile_base = self.tile_tree.base(qtid);
                let ang_extent = self.tile_tree.angular_extent(qtid);

                let lat0 = arcseconds!(tile_base.latitude);
                let lon0 = arcseconds!(tile_base.longitude);
                let lat1 = arcseconds!(tile_base.latitude + ang_extent);
                let lon1 = arcseconds!(tile_base.longitude + ang_extent);
                let t0 = (lat0 / iextent_lat).f32();
                let s0 = (lon0 / iextent_lon).f32();
                let t1 = (lat1 / iextent_lat).f32();
                let s1 = (lon1 / iextent_lon).f32();
                let c = [*slot as u16, 0];
                tris.push(IndexPaintVertex::new([s0, t0], c));
                tris.push(IndexPaintVertex::new([s1, t0], c));
                tris.push(IndexPaintVertex::new([s0, t1], c));
                tris.push(IndexPaintVertex::new([s1, t0], c));
                tris.push(IndexPaintVertex::new([s1, t1], c));
                tris.push(IndexPaintVertex::new([s0, t1], c));

                println!("BASE: {:?}: {} {} -> {} {}", qtid, s0, t0, s1, t1);

                /*
                let base = self.tile_tree.base(qtid);
                let pix_x = arcseconds!(base.longitude).f64() as i64 / TILE_EXTENT;
                let pix_y = arcseconds!(base.latitude).f64() as i64 / TILE_EXTENT;
                let pix_s = self.tile_tree.angular_extent(qtid).f64() as i64 / TILE_EXTENT;

                let tex_x = pix_x as f32 / 2048.;
                let tex_y = pix_y as f32 / 2048.;
                let tex_s = pix_s as f32 / 2048.;

                println!(
                    "Would paint at {}x{} ({}x{}) -> {} ({})",
                    pix_x, pix_y, tex_x, tex_y, pix_s, tex_s
                );
                let c = [*slot as u16, 0];
                tris.push(IndexPaintVertex::new([tex_x, tex_y], c));
                tris.push(IndexPaintVertex::new([tex_x + tex_s, tex_y], c));
                tris.push(IndexPaintVertex::new([tex_x, tex_y + tex_s], c));
                tris.push(IndexPaintVertex::new([tex_x + tex_s, tex_y], c));
                tris.push(IndexPaintVertex::new([tex_x + tex_s, tex_y + tex_s], c));
                tris.push(IndexPaintVertex::new([tex_x, tex_y + tex_s], c));
                 */
            }
        }
        while tris.len() < self.index_paint_range.end as usize {
            tris.push(IndexPaintVertex::new([0f32, 0f32], [0, 0]));
        }
        let upload_buffer = gpu.push_slice(
            "index-paint-tris-upload",
            &tris,
            wgpu::BufferUsage::COPY_SRC,
        );
        tracker.upload(
            upload_buffer,
            self.index_paint_vert_buffer.clone(),
            IndexPaintVertex::mem_size() * tris.len(),
        );
    }

    pub fn snapshot_index(&mut self) {
        self.take_index_snapshot = true;
    }

    #[allow(clippy::transmute_ptr_to_ptr)]
    fn capture_and_save_index_snapshot(
        &mut self,
        async_rt: &mut Runtime,
        gpu: &mut GPU,
    ) -> Fallible<()> {
        let _ = fs::create_dir("__dump__");
        let buf_size = u64::from(
            self.index_texture_extent.width
                * self.index_texture_extent.height
                * texture_format_size(self.index_texture_format),
        );
        let index_snapshot_download_buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("index-snapshot-download-buffer"),
            size: buf_size,
            usage: wgpu::BufferUsage::all(),
            mapped_at_creation: false,
        });
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("index-snapshot-download-command-encoder"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TextureCopyView {
                texture: &self.index_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            wgpu::BufferCopyView {
                buffer: &index_snapshot_download_buffer,
                layout: wgpu::TextureDataLayout {
                    offset: 0,
                    bytes_per_row: self.index_texture_extent.width
                        * texture_format_size(self.index_texture_format),
                    rows_per_image: self.index_texture_extent.height,
                },
            },
            self.index_texture_extent,
        );
        gpu.queue_mut().submit(vec![encoder.finish()]);
        gpu.device().poll(wgpu::Maintain::Wait);
        let reader = index_snapshot_download_buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read);
        gpu.device().poll(wgpu::Maintain::Wait);
        let extent = self.index_texture_extent;
        async_rt.spawn(async move {
            reader.await.unwrap();
            let raw = index_snapshot_download_buffer
                .slice(..)
                .get_mapped_range()
                .to_owned();
            println!("writing to __dump__/terrain_geo_index_texture_raw.bin");
            fs::write("__dump__/terrain_geo_index_texture_raw.bin", &raw).unwrap();

            let pix_cnt = extent.width as usize * extent.height as usize;
            let img_len = pix_cnt * 3;
            let shorts: &[u16] = unsafe { std::mem::transmute(&raw as &[u8]) };
            let mut data = vec![0u8; img_len];
            for x in 0..extent.width as usize {
                for y in 0..extent.height as usize {
                    let src_offset = x + (y * extent.width as usize);
                    let dst_offset = 3 * (x + (y * extent.width as usize));
                    let a = (shorts[src_offset] & 0x00FF) as u8;
                    data[dst_offset] = a;
                    data[dst_offset + 1] = a;
                    data[dst_offset + 2] = a;
                }
            }
            let img =
                ImageBuffer::<Rgb<u8>, _>::from_raw(extent.width, extent.height, data).unwrap();
            println!("writing to __dump__/terrain_geo_index_texture.png");
            img.save("__dump__/terrain_geo_index_texture.png")
        });
        Ok(())
    }

    fn allocate_atlas_slot(&mut self, votes: u32, qtid: QuadTreeId) {
        // If we got an addition, the tile should have been removed by the tree.
        assert!(!self.tile_state.contains_key(&qtid));

        let state = if let Some(atlas_slot) = self.atlas_free_list.pop() {
            assert!(self.atlas_tile_map[atlas_slot].is_none());
            self.atlas_tile_map[atlas_slot] = Some(qtid);
            self.tile_load_queue.push((votes, qtid));
            TileState::Pending(atlas_slot)
        } else {
            TileState::NoSpace
        };
        self.tile_state.insert(qtid, state);
    }

    fn deallocate_atlas_slot(&mut self, qtid: QuadTreeId) {
        // If the tile went out of scope, it must have been in scope before.
        assert!(self.tile_state.contains_key(&qtid));

        // Note that this orphans any instances of qtid in the load queue or in the background
        // read thread. We need to re-check the state any time we would look at it from one of
        // those sources.
        let state = self.tile_state.remove(&qtid).unwrap();
        let atlas_slot = match state {
            TileState::NoSpace => return,
            TileState::Pending(slot) => slot,
            TileState::Reading(slot) => slot,
            TileState::Active(slot) => slot,
        };
        self.atlas_tile_map[atlas_slot] = None;
        self.atlas_free_list.push(atlas_slot);
    }

    pub fn paint_atlas_index(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                attachment: &self.index_texture_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: true,
                },
            }],
            depth_stencil_attachment: None,
        });
        rpass.set_pipeline(&self.index_paint_pipeline);
        rpass.set_vertex_buffer(0, self.index_paint_vert_buffer.slice(..));
        rpass.draw(self.index_paint_range.clone(), 0..1);
        // rpass.draw(6..18, 0..1);
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }
}
