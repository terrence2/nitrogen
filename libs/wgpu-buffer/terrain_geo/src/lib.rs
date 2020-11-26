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

pub use crate::patch::{PatchWinding, TerrainVertex};
use crate::{patch::PatchManager, tile::TileManager};

use absolute_unit::{Length, Meters};
use camera::Camera;
use catalog::Catalog;
use command::Command;
use commandable::{command, commandable, Commandable};
use failure::Fallible;
use geodesy::{GeoCenter, Graticule};
use global_data::GlobalParametersBuffer;
use gpu::{UploadTracker, GPU};
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
}

pub enum CpuDetailLevel {
    Low,
    Medium,
    High,
    Ultra,
}

impl CpuDetailLevel {
    // max-level, target-refinement, buffer-size
    fn parameters(&self) -> CpuDetail {
        match self {
            Self::Low => CpuDetail::new(8, 150.0, 200),
            Self::Medium => CpuDetail::new(15, 150.0, 300),
            Self::High => CpuDetail::new(16, 150.0, 400),
            Self::Ultra => CpuDetail::new(17, 150.0, 500),
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
}

pub enum GpuDetailLevel {
    Low,
    Medium,
    High,
    Ultra,
}

impl GpuDetailLevel {
    // subdivisions
    fn parameters(&self) -> GpuDetail {
        match self {
            Self::Low => GpuDetail::new(3, 32), // 64MiB
            Self::Medium => GpuDetail::new(4, 64),
            Self::High => GpuDetail::new(6, 128),
            Self::Ultra => GpuDetail::new(7, 256),
        }
    }

    fn vertices_per_subdivision(d: usize) -> usize {
        (((2f64.powf(d as f64) + 1.0) * (2f64.powf(d as f64) + 2.0)) / 2.0).floor() as usize
    }
}

pub(crate) struct VisiblePatch {
    g0: Graticule<GeoCenter>,
    g1: Graticule<GeoCenter>,
    g2: Graticule<GeoCenter>,
    edge_length: Length<Meters>,
}

#[derive(Commandable)]
pub struct TerrainGeoBuffer {
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
    sampler: wgpu::Sampler,

    composite_bind_group_layout: wgpu::BindGroupLayout,
    composite_bind_group: wgpu::BindGroup,
    accumulate_common_bind_group_layout: wgpu::BindGroupLayout,
    accumulate_common_bind_group: wgpu::BindGroup,

    accumulate_clear_pipeline: wgpu::ComputePipeline,
    accumulate_spherical_normals_pipeline: wgpu::ComputePipeline,
    accumulate_spherical_colors_pipeline: wgpu::ComputePipeline,
}

#[commandable]
impl TerrainGeoBuffer {
    const DEFERRED_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;
    const DEFERRED_TEXTURE_DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
    const NORMAL_ACCUMULATION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Sint;
    const COLOR_ACCUMULATION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

    pub fn new(
        catalog: &Catalog,
        cpu_detail_level: CpuDetailLevel,
        gpu_detail_level: GpuDetailLevel,
        globals_buffer: &GlobalParametersBuffer,
        gpu: &mut GPU,
    ) -> Fallible<Self> {
        let cpu_detail = cpu_detail_level.parameters();
        let gpu_detail = gpu_detail_level.parameters();

        let patch_manager = PatchManager::new(
            cpu_detail.max_level,
            cpu_detail.target_refinement,
            cpu_detail.desired_patch_count,
            gpu_detail.subdivisions,
            gpu,
        )?;

        let tile_manager = TileManager::new(
            patch_manager.displace_height_bind_group_layout(),
            catalog,
            &gpu_detail,
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
                    vertex_stage: wgpu::ProgrammableStageDescriptor {
                        module: &gpu.create_shader_module(include_bytes!(
                            "../target/draw_deferred_texture.vert.spirv"
                        ))?,
                        entry_point: "main",
                    },
                    fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
                        module: &gpu.create_shader_module(include_bytes!(
                            "../target/draw_deferred_texture.frag.spirv"
                        ))?,
                        entry_point: "main",
                    }),
                    rasterization_state: Some(wgpu::RasterizationStateDescriptor {
                        front_face: wgpu::FrontFace::Cw,
                        cull_mode: wgpu::CullMode::Back,
                        depth_bias: 0,
                        depth_bias_slope_scale: 0.0,
                        depth_bias_clamp: 0.0,
                        clamp_depth: false,
                    }),
                    primitive_topology: wgpu::PrimitiveTopology::TriangleStrip,
                    color_states: &[wgpu::ColorStateDescriptor {
                        format: Self::DEFERRED_TEXTURE_FORMAT,
                        color_blend: wgpu::BlendDescriptor::REPLACE,
                        alpha_blend: wgpu::BlendDescriptor::REPLACE,
                        write_mask: wgpu::ColorWrite::ALL,
                    }],
                    depth_stencil_state: Some(wgpu::DepthStencilStateDescriptor {
                        format: Self::DEFERRED_TEXTURE_DEPTH,
                        depth_write_enabled: true,
                        depth_compare: wgpu::CompareFunction::Greater,
                        stencil: wgpu::StencilStateDescriptor {
                            front: wgpu::StencilStateFaceDescriptor::IGNORE,
                            back: wgpu::StencilStateFaceDescriptor::IGNORE,
                            read_mask: 0,
                            write_mask: 0,
                        },
                    }),
                    vertex_state: wgpu::VertexStateDescriptor {
                        index_format: wgpu::IndexFormat::Uint32,
                        vertex_buffers: &[TerrainVertex::descriptor()],
                    },
                    sample_count: 1,
                    sample_mask: !0,
                    alpha_to_coverage_enabled: false,
                });

        let sampler = gpu.device().create_sampler(&wgpu::SamplerDescriptor {
            label: Some("terrain_geo-dbg-sampler"),
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
        });

        // The bind group layout for compositing all of our buffers together (or debugging)
        let composite_bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("terrain_geo-composite-bind-group-layout"),
                    entries: &[
                        // deferred texture
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::SampledTexture {
                                dimension: wgpu::TextureViewDimension::D2,
                                component_type: wgpu::TextureComponentType::Float,
                                multisampled: false,
                            },
                            count: None,
                        },
                        // deferred depth
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::SampledTexture {
                                dimension: wgpu::TextureViewDimension::D2,
                                component_type: wgpu::TextureComponentType::Float,
                                multisampled: false,
                            },
                            count: None,
                        },
                        // color accumulator
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::SampledTexture {
                                dimension: wgpu::TextureViewDimension::D2,
                                component_type: wgpu::TextureComponentType::Uint,
                                multisampled: false,
                            },
                            count: None,
                        },
                        // normal accumulator
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::SampledTexture {
                                dimension: wgpu::TextureViewDimension::D2,
                                component_type: wgpu::TextureComponentType::Sint,
                                multisampled: false,
                            },
                            count: None,
                        },
                        // linear sampler
                        wgpu::BindGroupLayoutEntry {
                            binding: 4,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::Sampler { comparison: false },
                            count: None,
                        },
                    ],
                });

        let accumulate_common_bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("terrain_geo-accumulate-bind-group-layout"),
                    entries: &[
                        // deferred texture
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::SampledTexture {
                                dimension: wgpu::TextureViewDimension::D2,
                                component_type: wgpu::TextureComponentType::Float,
                                multisampled: false,
                            },
                            count: None,
                        },
                        // deferred depth
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::SampledTexture {
                                dimension: wgpu::TextureViewDimension::D2,
                                component_type: wgpu::TextureComponentType::Float,
                                multisampled: false,
                            },
                            count: None,
                        },
                        // Color acc as storage
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::StorageTexture {
                                dimension: wgpu::TextureViewDimension::D2,
                                format: Self::COLOR_ACCUMULATION_FORMAT,
                                readonly: false,
                            },
                            count: None,
                        },
                        // Normal acc as storage
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::StorageTexture {
                                dimension: wgpu::TextureViewDimension::D2,
                                format: Self::NORMAL_ACCUMULATION_FORMAT,
                                readonly: false,
                            },
                            count: None,
                        },
                        // linear sampler
                        wgpu::BindGroupLayoutEntry {
                            binding: 4,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Sampler { comparison: false },
                            count: None,
                        },
                    ],
                });

        let accumulate_clear_pipeline =
            gpu.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("terrain_geo-accumulate-pipeline"),
                    layout: Some(&gpu.device().create_pipeline_layout(
                        &wgpu::PipelineLayoutDescriptor {
                            label: Some("terrain_geo-accumulate-pipeline-layout"),
                            push_constant_ranges: &[],
                            bind_group_layouts: &[
                                globals_buffer.bind_group_layout(),
                                &accumulate_common_bind_group_layout,
                            ],
                        },
                    )),
                    compute_stage: wgpu::ProgrammableStageDescriptor {
                        module: &gpu.create_shader_module(include_bytes!(
                            "../target/accumulate_clear.comp.spirv"
                        ))?,
                        entry_point: "main",
                    },
                });

        let accumulate_spherical_normals_pipeline =
            gpu.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("terrain_geo-accumulate-spherical-normals-pipeline"),
                    layout: Some(&gpu.device().create_pipeline_layout(
                        &wgpu::PipelineLayoutDescriptor {
                            label: Some("terrain_geo-accumulate-pipeline-layout"),
                            push_constant_ranges: &[],
                            bind_group_layouts: &[
                                globals_buffer.bind_group_layout(),
                                &accumulate_common_bind_group_layout,
                                tile_manager.tile_set_bind_group_layout_sint(),
                            ],
                        },
                    )),
                    compute_stage: wgpu::ProgrammableStageDescriptor {
                        module: &gpu.create_shader_module(include_bytes!(
                            "../target/accumulate_spherical_normals.comp.spirv"
                        ))?,
                        entry_point: "main",
                    },
                });

        let accumulate_spherical_colors_pipeline =
            gpu.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("terrain_geo-accumulate-spherical-colors-pipeline"),
                    layout: Some(&gpu.device().create_pipeline_layout(
                        &wgpu::PipelineLayoutDescriptor {
                            label: Some("terrain_geo-accumulate-pipeline-layout"),
                            push_constant_ranges: &[],
                            bind_group_layouts: &[
                                globals_buffer.bind_group_layout(),
                                &accumulate_common_bind_group_layout,
                                tile_manager.tile_set_bind_group_layout_float(),
                            ],
                        },
                    )),
                    compute_stage: wgpu::ProgrammableStageDescriptor {
                        module: &gpu.create_shader_module(include_bytes!(
                            "../target/accumulate_spherical_colors.comp.spirv"
                        ))?,
                        entry_point: "main",
                    },
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
            &sampler,
        );
        let accumulate_common_bind_group = Self::_make_accumulate_common_bind_group(
            gpu.device(),
            &accumulate_common_bind_group_layout,
            &deferred_texture.1,
            &deferred_depth.1,
            &color_acc.1,
            &normal_acc.1,
            &sampler,
        );

        Ok(Self {
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
            sampler,
            accumulate_clear_pipeline,
            accumulate_spherical_normals_pipeline,
            accumulate_spherical_colors_pipeline,
        })
    }

    fn _make_deferred_texture_targets(gpu: &GPU) -> (wgpu::Texture, wgpu::TextureView) {
        let sz = gpu.physical_size();
        let target = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("deferred-texture-target"),
            size: wgpu::Extent3d {
                width: sz.width as u32,
                height: sz.height as u32,
                depth: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEFERRED_TEXTURE_FORMAT,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT
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

    fn _make_deferred_depth_targets(gpu: &GPU) -> (wgpu::Texture, wgpu::TextureView) {
        let sz = gpu.physical_size();
        let depth_texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("deferred-depth-texture"),
            size: wgpu::Extent3d {
                width: sz.width as u32,
                height: sz.height as u32,
                depth: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEFERRED_TEXTURE_DEPTH,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT
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

    fn _make_color_accumulator_targets(gpu: &GPU) -> (wgpu::Texture, wgpu::TextureView) {
        let sz = gpu.physical_size();
        let color_acc = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("terrain_geo-color-acc-texture"),
            size: wgpu::Extent3d {
                width: sz.width as u32,
                height: sz.height as u32,
                depth: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::COLOR_ACCUMULATION_FORMAT,
            usage: wgpu::TextureUsage::COPY_SRC
                | wgpu::TextureUsage::SAMPLED
                | wgpu::TextureUsage::STORAGE,
        });
        let color_view = color_acc.create_view(&wgpu::TextureViewDescriptor {
            label: Some("terrain_geo-color-acc-texture-view"),
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

    fn _make_normal_accumulator_targets(gpu: &GPU) -> (wgpu::Texture, wgpu::TextureView) {
        let sz = gpu.physical_size();
        let normal_acc = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("terrain_geo-normal-acc-texture"),
            size: wgpu::Extent3d {
                width: sz.width as u32,
                height: sz.height as u32,
                depth: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::NORMAL_ACCUMULATION_FORMAT,
            usage: wgpu::TextureUsage::COPY_SRC
                | wgpu::TextureUsage::SAMPLED
                | wgpu::TextureUsage::STORAGE,
        });
        let normal_view = normal_acc.create_view(&wgpu::TextureViewDescriptor {
            label: Some("terrain_geo-normal-acc-texture-view"),
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

    fn _make_composite_bind_group(
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
        deferred_texture: &wgpu::TextureView,
        deferred_depth: &wgpu::TextureView,
        color_acc: &wgpu::TextureView,
        normal_acc: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain_geo-composite-bind-group"),
            layout: &bind_group_layout,
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
                    resource: wgpu::BindingResource::Sampler(sampler),
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
            label: Some("terrain_geo-accumulate-bind-group"),
            layout: &bind_group_layout,
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

    pub fn note_resize(&mut self, gpu: &GPU) {
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
            &self.sampler,
        );
        self.accumulate_common_bind_group = Self::_make_accumulate_common_bind_group(
            gpu.device(),
            &self.accumulate_common_bind_group_layout,
            &self.deferred_texture.1,
            &self.deferred_depth.1,
            &self.color_acc.1,
            &self.normal_acc.1,
            &self.sampler,
        );
    }

    pub fn make_upload_buffer(
        &mut self,
        camera: &Camera,
        catalog: Arc<AsyncRwLock<Catalog>>,
        async_rt: &mut Runtime,
        gpu: &mut GPU,
        tracker: &mut UploadTracker,
    ) -> Fallible<()> {
        // Upload patches and capture visibility regions.
        self.visible_regions.clear();
        self.patch_manager
            .make_upload_buffer(camera, gpu, tracker, &mut self.visible_regions)?;

        // Dispatch visibility to tiles so that they can manage the actively loaded set.
        self.tile_manager.begin_update();
        for visible_patch in &self.visible_regions {
            self.tile_manager.note_required(visible_patch);
        }
        self.tile_manager
            .finish_update(catalog, async_rt, gpu, tracker);

        Ok(())
    }

    pub fn paint_atlas_indices(
        &self,
        encoder: wgpu::CommandEncoder,
    ) -> Fallible<wgpu::CommandEncoder> {
        self.tile_manager.paint_atlas_indices(encoder)
    }

    pub fn tessellate<'a>(
        &'a self,
        mut cpass: wgpu::ComputePass<'a>,
    ) -> Fallible<wgpu::ComputePass<'a>> {
        // Use the CPU input mesh to tessellate on the GPU.
        cpass = self.patch_manager.tessellate(cpass)?;

        // Use our height tiles to displace mesh.
        cpass = self.tile_manager.displace_height(
            self.patch_manager.target_vertex_count(),
            &self.patch_manager.displace_height_bind_group(),
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

    pub fn deferred_texture<'a>(
        &'a self,
        mut rpass: wgpu::RenderPass<'a>,
        globals_buffer: &'a GlobalParametersBuffer,
    ) -> Fallible<wgpu::RenderPass<'a>> {
        rpass.set_pipeline(&self.deferred_texture_pipeline);
        rpass.set_bind_group(Group::Globals.index(), &globals_buffer.bind_group(), &[]);
        rpass.set_vertex_buffer(0, self.patch_manager.vertex_buffer());
        for i in 0..self.patch_manager.num_patches() {
            let winding = self.patch_manager.patch_winding(i);
            let base_vertex = self.patch_manager.patch_vertex_buffer_offset(i);
            rpass.set_index_buffer(self.patch_manager.tristrip_index_buffer(winding));
            rpass.draw_indexed(
                self.patch_manager.tristrip_index_range(winding),
                base_vertex,
                0..1,
            );
        }
        Ok(rpass)
    }

    pub fn accumulate_normal_and_color<'a>(
        &'a self,
        mut cpass: wgpu::ComputePass<'a>,
        globals_buffer: &'a GlobalParametersBuffer,
    ) -> Fallible<wgpu::ComputePass<'a>> {
        cpass.set_pipeline(&self.accumulate_clear_pipeline);
        cpass.set_bind_group(Group::Globals.index(), globals_buffer.bind_group(), &[]);
        cpass.set_bind_group(
            Group::TerrainAcc.index(),
            &self.accumulate_common_bind_group,
            &[],
        );
        cpass.dispatch(self.acc_extent.width / 8, self.acc_extent.height / 8, 1);

        cpass.set_pipeline(&self.accumulate_spherical_normals_pipeline);
        cpass.set_bind_group(Group::Globals.index(), globals_buffer.bind_group(), &[]);
        for bind_group in self.tile_manager.spherical_normal_bind_groups() {
            cpass.set_bind_group(
                Group::TerrainAcc.index(),
                &self.accumulate_common_bind_group,
                &[],
            );
            cpass.set_bind_group(Group::TerrainTileSet.index(), bind_group, &[]);
            cpass.dispatch(self.acc_extent.width / 8, self.acc_extent.height / 8, 1);
        }

        cpass.set_pipeline(&self.accumulate_spherical_colors_pipeline);
        cpass.set_bind_group(Group::Globals.index(), globals_buffer.bind_group(), &[]);
        for bind_group in self.tile_manager.spherical_color_bind_groups() {
            cpass.set_bind_group(
                Group::TerrainAcc.index(),
                &self.accumulate_common_bind_group,
                &[],
            );
            cpass.set_bind_group(Group::TerrainTileSet.index(), bind_group, &[]);
            cpass.dispatch(self.acc_extent.width / 8, self.acc_extent.height / 8, 1);
        }

        Ok(cpass)
    }

    #[command]
    pub fn snapshot_index(&mut self, _command: &Command) {
        self.tile_manager.snapshot_index();
    }

    pub fn composite_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.composite_bind_group_layout
    }

    pub fn composite_bind_group(&self) -> &wgpu::BindGroup {
        &self.composite_bind_group
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

    // pub fn tristrip_index_buffer(&self, winding: PatchWinding) -> wgpu::BufferSlice {
    //     self.patch_manager.tristrip_index_buffer(winding)
    // }

    // pub fn tristrip_index_range(&self, winding: PatchWinding) -> Range<u32> {
    //     self.patch_manager.tristrip_index_range(winding)
    // }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_subdivision_vertex_counts() {
        let expect = vec![3, 6, 15, 45, 153, 561, 2145, 8385];
        for (i, &value) in expect.iter().enumerate() {
            assert_eq!(value, GpuDetailLevel::vertices_per_subdivision(i));
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
