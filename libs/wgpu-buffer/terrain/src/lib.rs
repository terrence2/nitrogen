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
mod patch;
mod tables;

pub mod tile;

use crate::{patch::PatchManager, tile::TileManager};
pub use crate::{
    patch::{PatchWinding, TerrainVertex},
    tile::TileSet,
};

use absolute_unit::{Length, Meters};
use anyhow::Result;
use camera::Camera;
use catalog::Catalog;
use geodesy::{GeoCenter, Graticule};
use global_data::GlobalParametersBuffer;
use gpu::{CpuDetailLevel, Gpu, GpuDetailLevel, RenderExtentChangeReceiver, UploadTracker};
use nitrous::{Interpreter, Value};
use nitrous_injector::{inject_nitrous_module, method, NitrousModule};
use parking_lot::RwLock;
use shader_shared::Group;
use std::{ops::Range, sync::Arc};
use tokio::{runtime::Runtime, sync::RwLock as AsyncRwLock};

#[allow(unused)]
const DBG_COLORS_BY_LEVEL: [[f32; 3]; 19] = [
    [0.75, 0.25, 0.25],
    [0.25, 0.75, 0.75],
    [0.75, 0.42, 0.25],
    [0.25, 0.58, 0.75],
    [0.75, 0.58, 0.25],
    [0.25, 0.42, 0.75],
    [0.75, 0.75, 0.25],
    [0.25, 0.25, 0.75],
    [0.58, 0.75, 0.25],
    [0.42, 0.25, 0.75],
    [0.58, 0.25, 0.75],
    [0.42, 0.75, 0.25],
    [0.25, 0.75, 0.25],
    [0.75, 0.25, 0.75],
    [0.25, 0.75, 0.42],
    [0.75, 0.25, 0.58],
    [0.25, 0.75, 0.58],
    [0.75, 0.25, 0.42],
    [0.10, 0.75, 0.72],
];

pub(crate) struct CpuDetail {
    max_level: usize,
    target_refinement: f64,
    desired_patch_count: usize,
}

impl CpuDetail {
    fn new(max_level: usize, target_refinement: f64, desired_patch_count: usize) -> Self {
        Self {
            max_level,
            target_refinement,
            desired_patch_count,
        }
    }

    fn for_level(level: CpuDetailLevel) -> Self {
        match level {
            CpuDetailLevel::Low => Self::new(8, 150.0, 200),
            CpuDetailLevel::Medium => Self::new(15, 150.0, 300),
            CpuDetailLevel::High => Self::new(16, 150.0, 400),
            CpuDetailLevel::Ultra => Self::new(17, 150.0, 500),
        }
    }
}

pub(crate) struct GpuDetail {
    // Number of tesselation subdivision steps to compute on the GPU each frame.
    subdivisions: usize,

    // The number of tiles to store on the GPU.
    tile_cache_size: u32,
}

impl GpuDetail {
    fn new(subdivisions: usize, tile_cache_size: u32) -> Self {
        Self {
            subdivisions,
            tile_cache_size,
        }
    }

    fn for_level(level: GpuDetailLevel) -> Self {
        match level {
            GpuDetailLevel::Low => Self::new(3, 32), // 64MiB
            GpuDetailLevel::Medium => Self::new(4, 64),
            GpuDetailLevel::High => Self::new(6, 128),
            GpuDetailLevel::Ultra => Self::new(7, 256),
        }
    }

    fn vertices_per_subdivision(d: usize) -> usize {
        (((2f64.powf(d as f64) + 1.0) * (2f64.powf(d as f64) + 2.0)) / 2.0).floor() as usize
    }
}

#[derive(Debug)]
pub struct VisiblePatch {
    g0: Graticule<GeoCenter>,
    g1: Graticule<GeoCenter>,
    g2: Graticule<GeoCenter>,
    edge_length: Length<Meters>,
}

#[derive(Debug, NitrousModule)]
pub struct TerrainBuffer {
    patch_manager: PatchManager,
    tile_manager: TileManager,

    // Cache allocation for transferring visible allocations from patches to tiles.
    visible_regions: Vec<VisiblePatch>,

    acc_extent: wgpu::Extent3d,
    deferred_texture_pipeline: wgpu::RenderPipeline,
    deferred_texture: (wgpu::Texture, wgpu::TextureView),
    deferred_depth: (wgpu::Texture, wgpu::TextureView),
    color_acc: (wgpu::Texture, wgpu::TextureView),
    normal_acc: (wgpu::Texture, wgpu::TextureView),
    sampler_linear: wgpu::Sampler,
    sampler_nearest: wgpu::Sampler,

    composite_bind_group_layout: wgpu::BindGroupLayout,
    composite_bind_group: wgpu::BindGroup,
    accumulate_common_bind_group_layout: wgpu::BindGroupLayout,
    accumulate_common_bind_group: wgpu::BindGroup,

    accumulate_clear_pipeline: wgpu::ComputePipeline,
}

#[inject_nitrous_module]
impl TerrainBuffer {
    const DEFERRED_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;
    const DEFERRED_TEXTURE_DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
    const NORMAL_ACCUMULATION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Sint;
    const COLOR_ACCUMULATION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

    pub fn new(
        catalog: &Catalog,
        cpu_detail_level: CpuDetailLevel,
        gpu_detail_level: GpuDetailLevel,
        globals_buffer: &GlobalParametersBuffer,
        gpu: &mut Gpu,
        interpreter: &mut Interpreter,
    ) -> Result<Arc<RwLock<Self>>> {
        let cpu_detail = CpuDetail::for_level(cpu_detail_level);
        let gpu_detail = GpuDetail::for_level(gpu_detail_level);

        let patch_manager = PatchManager::new(
            cpu_detail.max_level,
            cpu_detail.target_refinement,
            cpu_detail.desired_patch_count,
            gpu_detail.subdivisions,
            gpu,
        )?;

        let deferred_texture_pipeline =
            gpu.device()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("deferred-texture-pipeline"),
                    layout: Some(&gpu.device().create_pipeline_layout(
                        &wgpu::PipelineLayoutDescriptor {
                            label: Some("deferred-texture-pipeline-layout"),
                            push_constant_ranges: &[],
                            bind_group_layouts: &[globals_buffer.bind_group_layout()],
                        },
                    )),
                    vertex: wgpu::VertexState {
                        module: &gpu.create_shader_module(
                            "draw_deferred_texture.vert",
                            include_bytes!("../target/draw_deferred_texture.vert.spirv"),
                        )?,
                        entry_point: "main",
                        buffers: &[TerrainVertex::descriptor()],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &gpu.create_shader_module(
                            "draw_deferred_texture.frag",
                            include_bytes!("../target/draw_deferred_texture.frag.spirv"),
                        )?,
                        entry_point: "main",
                        targets: &[wgpu::ColorTargetState {
                            format: Self::DEFERRED_TEXTURE_FORMAT,
                            color_blend: wgpu::BlendState::REPLACE,
                            alpha_blend: wgpu::BlendState::REPLACE,
                            write_mask: wgpu::ColorWrite::ALL,
                        }],
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleStrip,
                        strip_index_format: Some(wgpu::IndexFormat::Uint32),
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: wgpu::CullMode::Back,
                        polygon_mode: wgpu::PolygonMode::Fill,
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: Self::DEFERRED_TEXTURE_DEPTH,
                        depth_write_enabled: true,
                        depth_compare: wgpu::CompareFunction::Greater,
                        stencil: wgpu::StencilState {
                            front: wgpu::StencilFaceState::IGNORE,
                            back: wgpu::StencilFaceState::IGNORE,
                            read_mask: 0,
                            write_mask: 0,
                        },
                        bias: wgpu::DepthBiasState {
                            constant: 0,
                            slope_scale: 0.0,
                            clamp: 0.0,
                        },
                        clamp_depth: false,
                    }),
                    multisample: wgpu::MultisampleState {
                        count: 1,
                        mask: !0,
                        alpha_to_coverage_enabled: false,
                    },
                });

        let sampler_linear = gpu.device().create_sampler(&wgpu::SamplerDescriptor {
            label: Some("terrain-sampler-linear"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_min_clamp: 1f32,
            lod_max_clamp: 1f32,
            compare: None,
            anisotropy_clamp: None,
            border_color: None,
        });

        let sampler_nearest = gpu.device().create_sampler(&wgpu::SamplerDescriptor {
            label: Some("terrain-sampler-nearest"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_min_clamp: 1f32,
            lod_max_clamp: 1f32,
            compare: None,
            anisotropy_clamp: None,
            border_color: None,
        });

        // A bind group layout with readonly, filtered access to accumulator buffers for
        // compositing all of it together to the screen.
        let composite_bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("terrain-composite-bind-group-layout"),
                    entries: &[
                        // deferred texture
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                view_dimension: wgpu::TextureViewDimension::D2,
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                multisampled: false,
                            },
                            count: None,
                        },
                        // deferred depth
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                view_dimension: wgpu::TextureViewDimension::D2,
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                multisampled: false,
                            },
                            count: None,
                        },
                        // color accumulator
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                view_dimension: wgpu::TextureViewDimension::D2,
                                sample_type: wgpu::TextureSampleType::Uint,
                                multisampled: false,
                            },
                            count: None,
                        },
                        // normal accumulator
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                view_dimension: wgpu::TextureViewDimension::D2,
                                sample_type: wgpu::TextureSampleType::Sint,
                                multisampled: false,
                            },
                            count: None,
                        },
                        // linear sampler
                        wgpu::BindGroupLayoutEntry {
                            binding: 4,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::Sampler {
                                filtering: true,
                                comparison: false,
                            },
                            count: None,
                        },
                        // nearest sampler
                        wgpu::BindGroupLayoutEntry {
                            binding: 5,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::Sampler {
                                filtering: false,
                                comparison: false,
                            },
                            count: None,
                        },
                    ],
                });

        // A bind group layout with read/write access to the accumulators for accumulating.
        let accumulate_common_bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("terrain-accumulate-bind-group-layout"),
                    entries: &[
                        // deferred texture
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                view_dimension: wgpu::TextureViewDimension::D2,
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                multisampled: false,
                            },
                            count: None,
                        },
                        // deferred depth
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                view_dimension: wgpu::TextureViewDimension::D2,
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                multisampled: false,
                            },
                            count: None,
                        },
                        // Color acc as storage
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::StorageTexture {
                                format: Self::COLOR_ACCUMULATION_FORMAT,
                                access: wgpu::StorageTextureAccess::ReadWrite,
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                        // Normal acc as storage
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::StorageTexture {
                                format: Self::NORMAL_ACCUMULATION_FORMAT,
                                access: wgpu::StorageTextureAccess::ReadWrite,
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                        // linear sampler
                        wgpu::BindGroupLayoutEntry {
                            binding: 4,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Sampler {
                                filtering: true,
                                comparison: false,
                            },
                            count: None,
                        },
                    ],
                });

        let tile_manager = TileManager::new(
            patch_manager.displace_height_bind_group_layout(),
            &accumulate_common_bind_group_layout,
            catalog,
            globals_buffer,
            &gpu_detail,
            gpu,
        )?;

        let accumulate_clear_pipeline =
            gpu.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("terrain-accumulate-pipeline"),
                    layout: Some(&gpu.device().create_pipeline_layout(
                        &wgpu::PipelineLayoutDescriptor {
                            label: Some("terrain-accumulate-pipeline-layout"),
                            push_constant_ranges: &[],
                            bind_group_layouts: &[
                                globals_buffer.bind_group_layout(),
                                &accumulate_common_bind_group_layout,
                            ],
                        },
                    )),
                    module: &gpu.create_shader_module(
                        "accumulate_clear.comp",
                        include_bytes!("../target/accumulate_clear.comp.spirv"),
                    )?,
                    entry_point: "main",
                });

        let deferred_texture = Self::_make_deferred_texture_targets(gpu);
        let deferred_depth = Self::_make_deferred_depth_targets(gpu);
        let color_acc = Self::_make_color_accumulator_targets(gpu);
        let normal_acc = Self::_make_normal_accumulator_targets(gpu);
        let composite_bind_group = Self::_make_composite_bind_group(
            gpu.device(),
            &composite_bind_group_layout,
            &deferred_texture.1,
            &deferred_depth.1,
            &color_acc.1,
            &normal_acc.1,
            &sampler_linear,
            &sampler_nearest,
        );
        let accumulate_common_bind_group = Self::_make_accumulate_common_bind_group(
            gpu.device(),
            &accumulate_common_bind_group_layout,
            &deferred_texture.1,
            &deferred_depth.1,
            &color_acc.1,
            &normal_acc.1,
            &sampler_linear,
        );

        let terrain = Arc::new(RwLock::new(Self {
            patch_manager,
            tile_manager,
            visible_regions: Vec::new(),
            acc_extent: gpu.attachment_extent(),
            deferred_texture_pipeline,
            deferred_texture,
            deferred_depth,
            color_acc,
            normal_acc,
            composite_bind_group_layout,
            composite_bind_group,
            accumulate_common_bind_group_layout,
            accumulate_common_bind_group,
            sampler_linear,
            sampler_nearest,
            accumulate_clear_pipeline,
        }));

        gpu.register_render_extent_change_receiver(terrain.clone());

        interpreter.put_global("terrain", Value::Module(terrain.clone()));

        Ok(terrain)
    }

    pub fn init(self) -> Result<Arc<RwLock<Self>>> {
        let terrain = Arc::new(RwLock::new(self));
        Ok(terrain)
    }

    fn _make_deferred_texture_targets(gpu: &Gpu) -> (wgpu::Texture, wgpu::TextureView) {
        let size = gpu.render_extent();
        let target = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("deferred-texture-target"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEFERRED_TEXTURE_FORMAT,
            usage: wgpu::TextureUsage::RENDER_ATTACHMENT
                | wgpu::TextureUsage::COPY_SRC
                | wgpu::TextureUsage::SAMPLED,
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor {
            label: Some("deferred-texture-target-view"),
            format: None,
            dimension: None,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        });
        (target, view)
    }

    fn _make_deferred_depth_targets(gpu: &Gpu) -> (wgpu::Texture, wgpu::TextureView) {
        let size = gpu.render_extent();
        let depth_texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("deferred-depth-texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEFERRED_TEXTURE_DEPTH,
            usage: wgpu::TextureUsage::RENDER_ATTACHMENT
                | wgpu::TextureUsage::COPY_SRC
                | wgpu::TextureUsage::SAMPLED,
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("deferred-depth-texture-view"),
            format: None,
            dimension: None,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        });
        (depth_texture, depth_view)
    }

    fn _make_color_accumulator_targets(gpu: &Gpu) -> (wgpu::Texture, wgpu::TextureView) {
        let size = gpu.render_extent();
        let color_acc = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("terrain-color-acc-texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::COLOR_ACCUMULATION_FORMAT,
            usage: wgpu::TextureUsage::COPY_SRC
                | wgpu::TextureUsage::SAMPLED
                | wgpu::TextureUsage::STORAGE,
        });
        let color_view = color_acc.create_view(&wgpu::TextureViewDescriptor {
            label: Some("terrain-color-acc-texture-view"),
            format: None,
            dimension: None,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        });
        (color_acc, color_view)
    }

    fn _make_normal_accumulator_targets(gpu: &Gpu) -> (wgpu::Texture, wgpu::TextureView) {
        let size = gpu.render_extent();
        let normal_acc = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("terrain-normal-acc-texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::NORMAL_ACCUMULATION_FORMAT,
            usage: wgpu::TextureUsage::COPY_SRC
                | wgpu::TextureUsage::SAMPLED
                | wgpu::TextureUsage::STORAGE,
        });
        let normal_view = normal_acc.create_view(&wgpu::TextureViewDescriptor {
            label: Some("terrain-normal-acc-texture-view"),
            format: None,
            dimension: None,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        });
        (normal_acc, normal_view)
    }

    #[allow(clippy::too_many_arguments)]
    fn _make_composite_bind_group(
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
        deferred_texture: &wgpu::TextureView,
        deferred_depth: &wgpu::TextureView,
        color_acc: &wgpu::TextureView,
        normal_acc: &wgpu::TextureView,
        sampler_linear: &wgpu::Sampler,
        sampler_nearest: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain-composite-bind-group"),
            layout: bind_group_layout,
            entries: &[
                // deferred texture
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(deferred_texture),
                },
                // deferred depth
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(deferred_depth),
                },
                // color accumulator
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(color_acc),
                },
                // normal accumulator
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(normal_acc),
                },
                // Linear sampler
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(sampler_linear),
                },
                // Nearest sampler
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(sampler_nearest),
                },
            ],
        })
    }

    fn _make_accumulate_common_bind_group(
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
        deferred_texture: &wgpu::TextureView,
        deferred_depth: &wgpu::TextureView,
        color_acc: &wgpu::TextureView,
        normal_acc: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain-accumulate-bind-group"),
            layout: bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(deferred_texture),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(deferred_depth),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(color_acc),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(normal_acc),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    // Given the new camera position, update our internal CPU tracking.
    pub fn track_state_changes(
        &mut self,
        camera: &Camera,
        optimize_camera: &Camera,
        catalog: Arc<AsyncRwLock<Catalog>>,
        async_rt: &Runtime,
    ) -> Result<()> {
        // Upload patches and capture visibility regions.
        self.visible_regions.clear();
        self.patch_manager.track_state_changes(
            camera,
            optimize_camera,
            &mut self.visible_regions,
        )?;

        // Dispatch visibility to tiles so that they can manage the actively loaded set.
        self.tile_manager.begin_visibility_update();
        for visible_patch in &self.visible_regions {
            self.tile_manager.note_required(visible_patch);
        }
        self.tile_manager
            .finish_visibility_update(camera, catalog, async_rt);

        Ok(())
    }

    // Push CPU state to GPU
    pub fn ensure_uploaded(
        &mut self,
        async_rt: &Runtime,
        gpu: &mut Gpu,
        tracker: &mut UploadTracker,
    ) -> Result<()> {
        self.patch_manager.ensure_uploaded(gpu, tracker);
        self.tile_manager.ensure_uploaded(async_rt, gpu, tracker);
        Ok(())
    }

    pub fn paint_atlas_indices(
        &self,
        encoder: wgpu::CommandEncoder,
    ) -> Result<wgpu::CommandEncoder> {
        self.tile_manager.paint_atlas_indices(encoder)
    }

    pub fn tessellate<'a>(
        &'a self,
        mut cpass: wgpu::ComputePass<'a>,
    ) -> Result<wgpu::ComputePass<'a>> {
        // Use the CPU input mesh to tessellate on the GPU.
        cpass = self.patch_manager.tessellate(cpass)?;

        // Use our height tiles to displace mesh.
        cpass = self.tile_manager.displace_height(
            self.patch_manager.target_vertex_count(),
            self.patch_manager.displace_height_bind_group(),
            cpass,
        )?;

        Ok(cpass)
    }

    pub fn deferred_texture_target(
        &self,
    ) -> (
        [wgpu::RenderPassColorAttachmentDescriptor; 1],
        Option<wgpu::RenderPassDepthStencilAttachmentDescriptor>,
    ) {
        (
            [wgpu::RenderPassColorAttachmentDescriptor {
                attachment: &self.deferred_texture.1,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::RED),
                    store: true,
                },
            }],
            Some(wgpu::RenderPassDepthStencilAttachmentDescriptor {
                attachment: &self.deferred_depth.1,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(-1f32),
                    store: true,
                }),
                stencil_ops: None,
            }),
        )
    }

    /// Draw the tessellated and height-displaced patch geometry to an offscreen buffer colored
    /// with the texture coordinates. This is the only geometry pass. All other terrain passes
    /// work in the screen space that we create here.
    pub fn deferred_texture<'a>(
        &'a self,
        mut rpass: wgpu::RenderPass<'a>,
        globals_buffer: &'a GlobalParametersBuffer,
    ) -> Result<wgpu::RenderPass<'a>> {
        rpass.set_pipeline(&self.deferred_texture_pipeline);
        rpass.set_bind_group(Group::Globals.index(), globals_buffer.bind_group(), &[]);
        rpass.set_vertex_buffer(0, self.patch_manager.vertex_buffer());
        for i in 0..self.patch_manager.num_patches() {
            let winding = self.patch_manager.patch_winding(i);
            let base_vertex = self.patch_manager.patch_vertex_buffer_offset(i);
            rpass.set_index_buffer(
                self.patch_manager.tristrip_index_buffer(winding),
                wgpu::IndexFormat::Uint32,
            );
            rpass.draw_indexed(
                self.patch_manager.tristrip_index_range(winding),
                base_vertex,
                0..1,
            );
        }
        Ok(rpass)
    }

    /// Use the offscreen texcoord buffer to build offscreen color and normals buffers.
    /// These offscreen buffers will get fed into the `world` renderer with the atmosphere,
    /// clouds, shadowmap, etc, to composite a final "world" image.
    pub fn accumulate_normal_and_color<'a>(
        &'a self,
        mut cpass: wgpu::ComputePass<'a>,
        globals_buffer: &'a GlobalParametersBuffer,
    ) -> Result<wgpu::ComputePass<'a>> {
        cpass.set_pipeline(&self.accumulate_clear_pipeline);
        cpass.set_bind_group(Group::Globals.index(), globals_buffer.bind_group(), &[]);
        cpass.set_bind_group(
            Group::TerrainAccumulateCommon.index(),
            &self.accumulate_common_bind_group,
            &[],
        );
        cpass.dispatch(self.acc_extent.width / 8, self.acc_extent.height / 8, 1);

        cpass = self.tile_manager.accumulate_normals(
            cpass,
            &self.acc_extent,
            globals_buffer,
            &self.accumulate_common_bind_group,
        )?;

        cpass = self.tile_manager.accumulate_colors(
            cpass,
            &self.acc_extent,
            globals_buffer,
            &self.accumulate_common_bind_group,
        )?;

        Ok(cpass)
    }

    pub fn add_tile_set(&mut self, tile_set: Box<dyn TileSet>) {
        self.tile_manager.add_tile_set(tile_set);
    }

    pub fn accumulate_common_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.accumulate_common_bind_group_layout
    }

    #[method]
    pub fn capture_index_snapshot(&mut self) {
        self.tile_manager.snapshot_index();
    }

    pub fn composite_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.composite_bind_group_layout
    }

    pub fn composite_bind_group(&self) -> &wgpu::BindGroup {
        &self.composite_bind_group
    }

    pub fn mesh_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        self.patch_manager.displace_height_bind_group_layout()
    }

    pub fn num_patches(&self) -> i32 {
        self.patch_manager.num_patches()
    }

    pub fn vertex_buffer(&self) -> wgpu::BufferSlice {
        self.patch_manager.vertex_buffer()
    }

    pub fn patch_vertex_buffer_offset(&self, patch_number: i32) -> i32 {
        self.patch_manager.patch_vertex_buffer_offset(patch_number)
    }

    pub fn patch_winding(&self, patch_number: i32) -> PatchWinding {
        self.patch_manager.patch_winding(patch_number)
    }

    pub fn wireframe_index_buffer(&self, winding: PatchWinding) -> wgpu::BufferSlice {
        self.patch_manager.wireframe_index_buffer(winding)
    }

    pub fn wireframe_index_range(&self, winding: PatchWinding) -> Range<u32> {
        self.patch_manager.wireframe_index_range(winding)
    }
}

impl RenderExtentChangeReceiver for TerrainBuffer {
    fn on_render_extent_changed(&mut self, gpu: &Gpu) -> Result<()> {
        self.acc_extent = gpu.attachment_extent();
        self.deferred_texture = Self::_make_deferred_texture_targets(gpu);
        self.deferred_depth = Self::_make_deferred_depth_targets(gpu);
        self.color_acc = Self::_make_color_accumulator_targets(gpu);
        self.normal_acc = Self::_make_normal_accumulator_targets(gpu);
        self.composite_bind_group = Self::_make_composite_bind_group(
            gpu.device(),
            &self.composite_bind_group_layout,
            &self.deferred_texture.1,
            &self.deferred_depth.1,
            &self.color_acc.1,
            &self.normal_acc.1,
            &self.sampler_linear,
            &self.sampler_nearest,
        );
        self.accumulate_common_bind_group = Self::_make_accumulate_common_bind_group(
            gpu.device(),
            &self.accumulate_common_bind_group_layout,
            &self.deferred_texture.1,
            &self.deferred_depth.1,
            &self.color_acc.1,
            &self.normal_acc.1,
            &self.sampler_linear,
        );
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_subdivision_vertex_counts() {
        let expect = vec![3, 6, 15, 45, 153, 561, 2145, 8385];
        for (i, &value) in expect.iter().enumerate() {
            assert_eq!(value, GpuDetail::vertices_per_subdivision(i));
        }
    }

    #[test]
    fn test_built_index_lut() {
        // let lut = TerrainGeoBuffer::build_index_dependence_lut();
        // for (i, (j, k)) in lut.iter().skip(3).enumerate() {
        //     println!("at offset: {}: {}, {}", i + 3, j, k);
        //     assert!((i as u32) + 3 > *j);
        //     assert!((i as u32) + 3 > *k);
        // }
        // assert_eq!(lut[0], (0, 0));
        // assert_eq!(lut[1], (0, 0));
        // assert_eq!(lut[2], (0, 0));
        // assert_eq!(lut[3], (0, 1));
        // assert_eq!(lut[4], (1, 2));
        // assert_eq!(lut[5], (2, 0));
        for i in 0..300 {
            let patch_id = i / 3;
            let offset = i % 3;
            assert_eq!(i, patch_id * 3 + offset);
        }
    }
}
