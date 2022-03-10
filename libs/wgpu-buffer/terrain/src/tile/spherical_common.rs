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

// Common functionality shared by spherical tile sets.
// This includes:
//   * Background tile loading
//   * Tile use discovery
//   * Index update and management
//   * Atlas upload and management
use crate::{
    tile::{
        index_paint_vertex::IndexPaintVertex,
        quad_tree::{QuadTree, QuadTreeId},
        tile_info::TileInfo,
        DataSetDataKind, TerrainLevel, TileCompression, TILE_PHYSICAL_SIZE,
    },
    VisiblePatch,
};
use absolute_unit::arcseconds;
use anyhow::Result;
use bzip2::read::BzDecoder;
use catalog::Catalog;
use crossbeam::channel::{self, Receiver, Sender};
use geometry::Aabb;
use gpu::{texture_format_size, Gpu};
use image::{ImageBuffer, Rgb};
use std::{
    collections::{BTreeMap, BinaryHeap},
    env,
    io::Read,
    mem,
    num::{NonZeroU32, NonZeroU64},
    ops::Range,
    path::PathBuf,
    sync::Arc,
    time::Instant,
};
use zerocopy::LayoutVerified;

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

#[derive(Debug)]
pub(crate) struct SphericalTileSetCommon {
    kind: DataSetDataKind,

    index_texture_format: wgpu::TextureFormat,
    index_texture_extent: wgpu::Extent3d,
    index_texture: wgpu::Texture,
    index_texture_view: wgpu::TextureView,
    #[allow(unused)]
    index_texture_sampler: wgpu::Sampler,

    index_paint_pipeline: wgpu::RenderPipeline,
    index_paint_range: Range<u32>,
    index_paint_vert_buffer: Arc<wgpu::Buffer>,

    atlas_texture_format: wgpu::TextureFormat,
    atlas_texture_extent: wgpu::Extent3d,
    atlas_texture: Arc<wgpu::Texture>,
    #[allow(unused)]
    atlas_texture_view: wgpu::TextureView,
    #[allow(unused)]
    atlas_texture_sampler: wgpu::Sampler,
    atlas_tile_info: Arc<wgpu::Buffer>,

    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,

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
    tile_sender: Sender<(QuadTreeId, Vec<u8>)>,
    tile_receiver: Receiver<(QuadTreeId, Vec<u8>)>,

    // Set to some to capture the index as a png
    maybe_snapshot_index: Option<PathBuf>,
}

impl SphericalTileSetCommon {
    pub(crate) fn new(
        catalog: &Catalog,
        prefix: &str,
        kind: DataSetDataKind,
        tile_cache_size: u32,
        gpu: &Gpu,
    ) -> Result<Self> {
        let qt_start = Instant::now();
        let tile_tree = QuadTree::from_layers(prefix, catalog)?;
        let qt_time = qt_start.elapsed();
        println!(
            "QuadTree::from_catalog timing: {}.{}ms",
            qt_time.as_secs() * 1000 + u64::from(qt_time.subsec_millis()),
            qt_time.subsec_micros()
        );

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
        let index_texture_extent = wgpu::Extent3d {
            width: TerrainLevel::index_resolution().1,
            height: TerrainLevel::index_resolution().0,
            depth_or_array_layers: 1,
        };
        let index_texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("terrain-geo-tile-index-texture"),
            size: index_texture_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: index_texture_format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC,
        });
        let index_texture_view = index_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("terrain-index-texture-view"),
            format: None,
            dimension: None,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None,
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
            border_color: None,
        });
        let index_paint_range = 0u32..(6 * tile_cache_size);
        let index_paint_vert_buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("index-paint-vert-buffer"),
            size: (IndexPaintVertex::mem_size() * 6 * tile_cache_size as usize)
                as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::VERTEX,
            mapped_at_creation: false,
        });
        let index_paint_vert_shader = gpu.create_shader_module(
            "index_paint.vert",
            include_bytes!("../../target/index_paint.vert.spirv"),
        );
        let index_paint_frag_shader = gpu.create_shader_module(
            "index_paint.frag",
            include_bytes!("../../target/index_paint.frag.spirv"),
        );
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
                    vertex: wgpu::VertexState {
                        module: &index_paint_vert_shader,
                        entry_point: "main",
                        buffers: &[IndexPaintVertex::descriptor()],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &index_paint_frag_shader,
                        entry_point: "main",
                        targets: &[wgpu::ColorTargetState {
                            format: index_texture_format,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        }],
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: Some(wgpu::Face::Back),
                        unclipped_depth: false,
                        polygon_mode: wgpu::PolygonMode::Fill,
                        conservative: false,
                    },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState {
                        count: 1,
                        mask: !0,
                        alpha_to_coverage_enabled: false,
                    },
                    multiview: None,
                });

        // The atlas texture is a 2d array of tiles. All tiles have the same size, but may be
        // pre-sampled at various scaling factors, allowing us to use a single atlas for all
        // resolutions. Management of tile layers is done on the CPU between frames, using the
        // patch tree to figure out what is going to be most useful to have in the cache.
        let atlas_texture_format = kind.texture_format();
        let atlas_texture_extent = wgpu::Extent3d {
            width: TILE_SIZE,
            height: TILE_SIZE,
            depth_or_array_layers: tile_cache_size,
        };
        let atlas_texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("terrain-geo-tile-atlas-texture"),
            size: atlas_texture_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: atlas_texture_format,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        });
        let atlas_texture_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("terrain-atlas-texture-view"),
            format: None,
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: NonZeroU32::new(tile_cache_size),
        });
        let atlas_texture_filter_mode = kind.filter_mode();
        let atlas_texture_sampler = gpu.device().create_sampler(&wgpu::SamplerDescriptor {
            label: Some("terrain-atlas-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: atlas_texture_filter_mode,
            min_filter: atlas_texture_filter_mode,
            mipmap_filter: wgpu::FilterMode::Nearest, // We should be able to mip between levels...
            lod_min_clamp: 0f32,
            lod_max_clamp: 9_999_999f32,
            compare: None,
            anisotropy_clamp: None,
            border_color: None,
        });
        let atlas_tile_info_buffer_size =
            (mem::size_of::<TileInfo>() as u32 * tile_cache_size) as wgpu::BufferAddress;
        let atlas_tile_info = Arc::new(gpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("terrain-geo-tile-info-buffer"),
            size: atlas_tile_info_buffer_size,
            mapped_at_creation: false,
            usage: wgpu::BufferUsages::all(),
        }));

        // Note: layout has to correspond to kind.texture_format()
        let bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("terrain-tile-bind-group-layout"),
                    entries: &[
                        // Index Texture
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Uint,
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                            count: None,
                        },
                        // Atlas Textures, as referenced by the above index
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                view_dimension: wgpu::TextureViewDimension::D2Array,
                                sample_type: kind.texture_sample_type(),
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                        // Tile metadata
                        wgpu::BindGroupLayoutEntry {
                            binding: 4,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: NonZeroU64::new(atlas_tile_info_buffer_size),
                            },
                            count: None,
                        },
                    ],
                });

        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain-tile-bind-group"),
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
                // Tile Atlas
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&atlas_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&atlas_texture_sampler),
                },
                // Tile Metadata
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &atlas_tile_info,
                        offset: 0,
                        size: None,
                    }),
                },
            ],
        });

        let (tile_sender, tile_receiver) = channel::unbounded();

        Ok(Self {
            index_texture_format,
            index_texture_extent,
            index_texture,
            index_texture_view,
            index_texture_sampler,

            index_paint_pipeline,
            index_paint_range,
            index_paint_vert_buffer: Arc::new(index_paint_vert_buffer),

            atlas_texture_format,
            atlas_texture_extent,
            atlas_texture: Arc::new(atlas_texture),
            atlas_texture_view,
            atlas_texture_sampler,
            atlas_tile_info,

            bind_group_layout,
            bind_group,

            kind,

            atlas_tile_map: vec![None; tile_cache_size as usize],
            atlas_free_list: (0..tile_cache_size as usize).collect(),

            tile_tree,
            tile_state: BTreeMap::new(),
            tile_load_queue: BinaryHeap::new(),
            tile_read_count: 0,
            tile_sender,
            tile_receiver,

            maybe_snapshot_index: None,
        })
    }

    pub(crate) fn capture_and_save_index_snapshot(
        &mut self,
        path: PathBuf,
        gpu: &mut Gpu,
    ) -> Result<()> {
        Gpu::dump_texture(
            &self.index_texture,
            self.index_texture_extent,
            self.index_texture_format,
            gpu,
            Box::new(
                move |extent: wgpu::Extent3d, _: wgpu::TextureFormat, data: Vec<u8>| {
                    let pix_cnt = extent.width as usize * extent.height as usize;
                    let img_len = pix_cnt * 3;
                    let shorts =
                        LayoutVerified::<&[u8], [u16]>::new_slice(&data).expect("as [u16]");
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
                        ImageBuffer::<Rgb<u8>, _>::from_raw(extent.width, extent.height, data)
                            .expect("built image");
                    println!("writing to {:?}", path);
                    img.save(path).expect("wrote file");
                },
            ),
        )
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

    pub(crate) fn begin_visibility_update(&mut self) {
        self.tile_tree.begin_update();
    }

    pub(crate) fn note_required(&mut self, visible_patch: &VisiblePatch) {
        // Assuming 30m is 1"
        let angular_resolution = arcseconds!(visible_patch.edge_length.f64() / 30.0);

        // Find an aabb for the given triangle.
        let g0 = &visible_patch.g0;
        let g1 = &visible_patch.g1;
        let g2 = &visible_patch.g2;
        let min_lat = g0.latitude.min(g1.latitude).min(g2.latitude);
        let max_lat = g0.latitude.max(g1.latitude).max(g2.latitude);
        let min_lon = g0.longitude.min(g1.longitude).min(g2.longitude);
        let max_lon = g0.longitude.max(g1.longitude).max(g2.longitude);
        let aabb = Aabb::new(
            [
                arcseconds!(min_lat).round() as i32,
                arcseconds!(min_lon).round() as i32,
            ],
            [
                arcseconds!(max_lat).round() as i32,
                arcseconds!(max_lon).round() as i32,
            ],
        );
        self.tile_tree.note_required(&aabb, angular_resolution);
    }

    pub(crate) fn finish_visibility_update(&mut self, catalog: &mut Catalog) {
        let mut additions = Vec::new();
        let mut removals = Vec::new();
        self.tile_tree
            .finish_visibility_updates(&mut additions, &mut removals);

        // Apply removals and additions to CPU tracking.
        for &qtid in &removals {
            self.deallocate_atlas_slot(qtid);
        }
        for &(votes, qtid) in &additions {
            self.allocate_atlas_slot(votes, qtid);
        }

        // FIXME: precompute this
        let raw_tile_size = self.atlas_texture_extent.width as usize
            * self.atlas_texture_extent.height as usize
            * texture_format_size(self.atlas_texture_format) as usize;

        // Kick off any loads, if there is space remaining.
        let mut reads_started_count = 0;
        while !self.tile_load_queue.is_empty() && self.tile_read_count < MAX_CONCURRENT_READS {
            let (_, qtid) = self.tile_load_queue.pop().expect("checked is_empty");

            // There may be many frames between when a thing is inserted in the load queue and when
            // we have disk bandwidth available to read it. Thus we need to double-check that we
            // even still have it as an active tile and it's current state.
            let maybe_state = self.tile_state.get(&qtid);
            if maybe_state.is_none() || !maybe_state.unwrap().is_pending() {
                continue;
            }
            reads_started_count += 1;

            // If the state was pending, move us to the reading state and consume a read slot.
            let atlas_slot = maybe_state.unwrap().atlas_slot();
            self.tile_state.insert(qtid, TileState::Reading(atlas_slot));
            self.tile_read_count += 1;

            // Do the read in a disconnected thread and send it back on an mpsc queue.
            let fid = self.tile_tree.file_id(&qtid);
            let compression = self.tile_tree.tile_compression(&qtid);
            let extent = self.tile_tree.file_extent(&qtid);
            let closure_kind = self.kind;
            let closer_sender = self.tile_sender.clone();
            if let Ok(packed_data) = catalog.read_mapped_slice(fid, extent) {
                // SAFETY: The Catalog must not get dropped before the following thread is done.
                // This is ensured by finish_safely below, run by the shutdown scheduler,
                // after the main loop can call this method, but before resources are dropped.
                let packed_data: &'static [u8] = unsafe { mem::transmute(packed_data) };
                rayon::spawn(move || {
                    let data = match compression {
                        TileCompression::None => packed_data.to_vec(),
                        TileCompression::Bz2 => {
                            let mut decompressed = Vec::with_capacity(raw_tile_size);
                            BzDecoder::new(packed_data)
                                .read_to_end(&mut decompressed)
                                .expect("compress buffer");
                            assert_eq!(decompressed.len(), raw_tile_size);
                            decompressed
                        }
                    };
                    // FIXME: encode this in the header
                    // FIXME: do upscale on the GPU somehow?
                    let data = if closure_kind == DataSetDataKind::Color {
                        // Re-encode from rgb to rgba
                        let mut data2 = vec![255u8; TILE_PHYSICAL_SIZE * TILE_PHYSICAL_SIZE * 4];
                        for (i, c) in data.chunks(3).enumerate() {
                            for j in 0..3 {
                                data2[i * 4 + j] = c[j];
                            }
                        }
                        data2
                    } else {
                        data
                    };
                    closer_sender.send((qtid, data)).ok();
                });
            } else {
                panic!("Read failed in {:?}", fid);
            }
        }

        log::trace!(
            "{:?} +:{} -:{} st:{} out:{}",
            self.kind,
            additions.len(),
            removals.len(),
            reads_started_count,
            self.tile_read_count,
        );
    }

    pub(crate) fn encode_uploads(&mut self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder) {
        // FIXME: precompute this
        let raw_tile_size = self.atlas_texture_extent.width as usize
            * self.atlas_texture_extent.height as usize
            * texture_format_size(self.atlas_texture_format) as usize;

        // Check for any completed reads.
        let mut reads_ended_count = 0;
        while let Ok((qtid, data)) = self.tile_receiver.try_recv() {
            self.tile_read_count -= 1;
            reads_ended_count += 1;

            // If the reading tile has gone out of view in the time since it was enqueued, we
            // may have lost our atlas slot. That's fine, just dump the bytes on the floor.
            let maybe_state = self.tile_state.get(&qtid);
            if maybe_state.is_none() || !maybe_state.unwrap().is_reading() {
                continue;
            }

            let atlas_slot = maybe_state.unwrap().atlas_slot();
            self.tile_state.insert(qtid, TileState::Active(atlas_slot));

            assert_eq!(data.len(), raw_tile_size);
            let texture_buffer = gpu.push_slice(
                "terrain-geo-atlas-tile-texture-upload-buffer",
                &data,
                wgpu::BufferUsages::COPY_SRC,
            );
            encoder.copy_buffer_to_texture(
                wgpu::ImageCopyBuffer {
                    buffer: &texture_buffer,
                    layout: wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: NonZeroU32::new(
                            self.atlas_texture_extent.width
                                * texture_format_size(self.atlas_texture_format),
                        ),
                        rows_per_image: NonZeroU32::new(self.atlas_texture_extent.height),
                    },
                },
                wgpu::ImageCopyTexture {
                    texture: &self.atlas_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: atlas_slot as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: self.atlas_texture_extent.width,
                    height: self.atlas_texture_extent.height,
                    depth_or_array_layers: 1,
                },
            );

            let (tile_base_lat_as, tile_base_lon_as) = self.tile_tree.base(&qtid);
            let tile_base = [tile_base_lat_as as f32, tile_base_lon_as as f32];
            let angular_extent = arcseconds!(self.tile_tree.angular_extent_as(&qtid)).f32();
            let tile_info = TileInfo::new(tile_base, angular_extent, atlas_slot);
            let info_buffer = gpu.push_data(
                "terrain-geo-atlas-tile-info-upload-buffer",
                &tile_info,
                wgpu::BufferUsages::COPY_SRC,
            );
            encoder.copy_buffer_to_buffer(
                &info_buffer,
                0,
                &self.atlas_tile_info,
                (mem::size_of::<TileInfo>() * atlas_slot) as wgpu::BufferAddress,
                mem::size_of::<TileInfo>() as wgpu::BufferAddress,
            );
        }

        // Use the list of allocated tiles to generate a vertex buffer to upload for
        // painting the new data into the index.
        // FIXME: don't re-allocate every frame
        let index_ang_extent = TerrainLevel::index_extent();
        let iextent_lat = index_ang_extent.0.f32() / 2.; // range from [-1,1]
        let iextent_lon = index_ang_extent.1.f32() / 2.;
        let mut active_atlas_slots = 0;
        let mut max_active_level = 0;
        let mut tris = Vec::new();
        for (qtid, tile_state) in self.tile_state.iter() {
            if let TileState::Active(slot) = tile_state {
                active_atlas_slots += 1;
                let level = self.tile_tree.level(qtid);
                if level > max_active_level {
                    max_active_level = level;
                }

                // Project the tile base and angular extent into the index.
                // Note that the base may be outside the index extents.
                let (tile_base_lat_as, tile_base_lon_as) = self.tile_tree.base(qtid);
                let ang_extent_as = self.tile_tree.angular_extent_as(qtid);

                let lat0 = -tile_base_lat_as as f32;
                let lon0 = tile_base_lon_as as f32;
                let lat1 = -(tile_base_lat_as + ang_extent_as) as f32;
                let lon1 = (tile_base_lon_as + ang_extent_as) as f32;
                let t0 = lat0 / iextent_lat;
                let s0 = lon0 / iextent_lon;
                let t1 = lat1 / iextent_lat;
                let s1 = lon1 / iextent_lon;
                let c = *slot as u16;

                // FIXME 1: this could easily be indexed, saving us a bunch of bandwidth.
                // FIXME 2: we could upload these vertices as shorts, saving some more bandwidth.
                tris.push(IndexPaintVertex::new([s0, t0], c));
                tris.push(IndexPaintVertex::new([s0, t1], c));
                tris.push(IndexPaintVertex::new([s1, t0], c));
                tris.push(IndexPaintVertex::new([s1, t0], c));
                tris.push(IndexPaintVertex::new([s0, t1], c));
                tris.push(IndexPaintVertex::new([s1, t1], c));
            }
        }
        while tris.len() < self.index_paint_range.end as usize {
            tris.push(IndexPaintVertex::new([0f32, 0f32], 0));
        }
        gpu.upload_slice_to(
            "index-paint-tris-upload",
            &tris,
            self.index_paint_vert_buffer.clone(),
            encoder,
        );

        log::trace!(
            "{:?} ed:{} act:{}, d:{}",
            self.kind,
            reads_ended_count,
            active_atlas_slots,
            max_active_level
        );
    }

    pub(crate) fn shutdown_safely(&mut self) {
        // We have entered shutdown, but the system is still running, so nothing reachable has yet
        // been dropped. We need to pump our background jobs clean (including their unsafe mapped
        // pointers) before the system starts dropping things (like the owning Mmaps) on the main
        // thread.
        while self.tile_read_count > 0 {
            let _rv = self.tile_receiver.recv();
            self.tile_read_count -= 1;
        }
    }

    pub(crate) fn dump_index(&mut self, path: &str) -> Result<()> {
        let mut buf = env::current_dir()?;
        buf.push("__dump__");
        buf.push(path);
        buf.set_extension("png");
        self.maybe_snapshot_index = Some(buf);
        Ok(())
    }

    pub(crate) fn snapshot_index(&mut self, gpu: &mut Gpu) {
        if self.maybe_snapshot_index.is_some() {
            let mut path = None;
            mem::swap(&mut path, &mut self.maybe_snapshot_index);
            self.capture_and_save_index_snapshot(path.unwrap(), gpu)
                .ok();
        }
    }

    pub(crate) fn paint_atlas_index(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("paint-atlas-index-render-pass"),
            color_attachments: &[wgpu::RenderPassColorAttachment {
                view: &self.index_texture_view,
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
    }

    pub(crate) fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    pub(crate) fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }
}
