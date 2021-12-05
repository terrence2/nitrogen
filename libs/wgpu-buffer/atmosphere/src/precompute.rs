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
use crate::earth_consts::AtmosphereParameters;
use crate::{
    colorspace::{wavelength_to_srgb, MAX_LAMBDA, MIN_LAMBDA},
    earth_consts::{EarthParameters, RGB_LAMBDAS},
    table_helpers::{IRRADIANCE_EXTENT, SCATTERING_EXTENT, TRANSMITTANCE_EXTENT},
};
use anyhow::Result;
use futures::executor::block_on;
use gpu::Gpu;
use image::{ImageBuffer, Luma, Rgb};
use log::trace;
use std::{mem, num::NonZeroU64, slice, time::Instant};

const NUM_PRECOMPUTED_WAVELENGTHS: usize = 40;
const NUM_SCATTERING_PASSES: usize = 4;

const DUMP_TRANSMITTANCE: bool = false;
const DUMP_DIRECT_IRRADIANCE: bool = false;
const DUMP_SINGLE_RAYLEIGH: bool = false;
const DUMP_SINGLE_MIE: bool = false;
const DUMP_SINGLE_ACC: bool = false;
const DUMP_SINGLE_MIE_ACC: bool = false;
const DUMP_SCATTERING_DENSITY: bool = false;
const DUMP_INDIRECT_IRRADIANCE_DELTA: bool = false;
const DUMP_INDIRECT_IRRADIANCE_ACC: bool = false;
const DUMP_MULTIPLE_SCATTERING: bool = false;
const DUMP_FINAL: bool = false;

// Note: must match the block size defined in compute shader source
pub const BLOCK_SIZE: u32 = 8;

pub struct Precompute {
    build_transmittance_lut_bind_group_layout: wgpu::BindGroupLayout,
    build_transmittance_lut_pipeline: wgpu::ComputePipeline,
    build_direct_irradiance_lut_bind_group_layout: wgpu::BindGroupLayout,
    build_direct_irradiance_lut_pipeline: wgpu::ComputePipeline,
    build_single_scattering_lut_bind_group_layout: wgpu::BindGroupLayout,
    build_single_scattering_lut_pipeline: wgpu::ComputePipeline,
    build_scattering_density_lut_bind_group_layout: wgpu::BindGroupLayout,
    build_scattering_density_lut_pipeline: wgpu::ComputePipeline,
    build_indirect_irradiance_lut_bind_group_layout: wgpu::BindGroupLayout,
    build_indirect_irradiance_lut_pipeline: wgpu::ComputePipeline,
    build_multiple_scattering_lut_bind_group_layout: wgpu::BindGroupLayout,
    build_multiple_scattering_lut_pipeline: wgpu::ComputePipeline,

    // Temporary textures.
    delta_irradiance_texture: wgpu::Texture,
    delta_irradiance_texture_view: wgpu::TextureView,
    delta_rayleigh_scattering_texture: wgpu::Texture,
    delta_rayleigh_scattering_texture_view: wgpu::TextureView,
    delta_mie_scattering_texture: wgpu::Texture,
    delta_mie_scattering_texture_view: wgpu::TextureView,
    delta_multiple_scattering_texture: wgpu::Texture,
    delta_multiple_scattering_texture_view: wgpu::TextureView,
    delta_scattering_density_texture: wgpu::Texture,
    delta_scattering_density_texture_view: wgpu::TextureView,

    // Permanent/accumulator textures.
    transmittance_texture: wgpu::Texture,
    transmittance_texture_view: wgpu::TextureView,
    scattering_texture: wgpu::Texture,
    scattering_texture_view: wgpu::TextureView,
    single_mie_scattering_texture: wgpu::Texture,
    single_mie_scattering_texture_view: wgpu::TextureView,
    irradiance_texture: wgpu::Texture,
    irradiance_texture_view: wgpu::TextureView,

    sampler_resource: wgpu::Sampler,

    params: EarthParameters,
}

impl Precompute {
    pub fn new(gpu: &Gpu) -> Result<Self> {
        let device = gpu.device();
        let params = EarthParameters::new();

        fn uniform(binding: u32, min_binding_size: usize) -> wgpu::BindGroupLayoutEntry {
            wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStage::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(min_binding_size as u64),
                },
                count: None,
            }
        }
        fn storage_texture2d(binding: u32) -> wgpu::BindGroupLayoutEntry {
            wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStage::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    view_dimension: wgpu::TextureViewDimension::D2,
                    format: wgpu::TextureFormat::R32Float,
                    access: wgpu::StorageTextureAccess::ReadWrite,
                },
                count: None,
            }
        }
        fn storage_texture3d(binding: u32) -> wgpu::BindGroupLayoutEntry {
            wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStage::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    view_dimension: wgpu::TextureViewDimension::D3,
                    format: wgpu::TextureFormat::R32Float,
                    access: wgpu::StorageTextureAccess::ReadWrite,
                },
                count: None,
            }
        }
        fn texture2d(binding: u32) -> wgpu::BindGroupLayoutEntry {
            wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStage::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            }
        }
        fn texture3d(binding: u32) -> wgpu::BindGroupLayoutEntry {
            wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStage::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D3,
                },
                count: None,
            }
        }
        fn sampler(binding: u32) -> wgpu::BindGroupLayoutEntry {
            wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStage::COMPUTE,
                ty: wgpu::BindingType::Sampler {
                    filtering: true,
                    comparison: false,
                },
                count: None,
            }
        }

        // Transmittance
        let build_transmittance_lut_shader = gpu.create_shader_module(
            "build_transmittance_lut.comp",
            include_bytes!("../target/build_transmittance_lut.comp.spirv"),
        )?;
        let build_transmittance_lut_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("atmosphere-build-transmittance-lut-bind-group"),
                entries: &[
                    uniform(0, mem::size_of::<AtmosphereParameters>()), // atmosphere
                    storage_texture2d(1),                               // out transmittance
                ],
            });
        let build_transmittance_lut_pipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("atmosphere-build-transmittance-lut-pipeline"),
                layout: Some(
                    &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("atmosphere-build-transmittance-lut-pipeline-layout"),
                        push_constant_ranges: &[],
                        bind_group_layouts: &[&build_transmittance_lut_bind_group_layout],
                    }),
                ),
                module: &build_transmittance_lut_shader,
                entry_point: "main",
            });

        // Direct Irradiance
        let build_direct_irradiance_lut_shader = gpu.create_shader_module(
            "build_direct_irradiance_lut.comp",
            include_bytes!("../target/build_direct_irradiance_lut.comp.spirv"),
        )?;
        let build_direct_irradiance_lut_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("atmosphere-build-direct-irradiance-lut-bind-group"),
                entries: &[
                    uniform(0, mem::size_of::<AtmosphereParameters>()), // atmosphere
                    texture2d(1),                                       // transmittance_texture
                    sampler(2),                                         // transmittance_sampler
                    storage_texture2d(3),                               // delta_irradiance_texture
                ],
            });
        let build_direct_irradiance_lut_pipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("atmosphere-build-direct-irradiance-lut-pipeline"),
                layout: Some(
                    &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("atmosphere-build-direct-irradiance-lut-pipeline-layout"),
                        push_constant_ranges: &[],
                        bind_group_layouts: &[&build_direct_irradiance_lut_bind_group_layout],
                    }),
                ),
                module: &build_direct_irradiance_lut_shader,
                entry_point: "main",
            });

        // Single Scattering
        let build_single_scattering_lut_shader = gpu.create_shader_module(
            "build_single_scattering_lut.comp",
            include_bytes!("../target/build_single_scattering_lut.comp.spirv"),
        )?;
        let build_single_scattering_lut_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("atmosphere-build-single-scattering-lut-bind-group"),
                entries: &[
                    uniform(0, mem::size_of::<AtmosphereParameters>()), // atmosphere
                    texture2d(1),                                       // transmittance_texture
                    sampler(2),                                         // transmittance_sampler
                    uniform(3, mem::size_of::<[[f32; 4]; 4]>()),        // rad_to_lum
                    storage_texture3d(4), // delta_rayleigh_scattering_texture
                    storage_texture3d(5), // delta_mie_scattering_texture
                    storage_texture3d(6), // scattering_texture
                    storage_texture3d(7), // single_mie_scattering_texture
                ],
            });
        let build_single_scattering_lut_pipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("atmosphere-build-single-scattering-lut-pipeline"),
                layout: Some(
                    &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("atmosphere-build-single-scattering-lut-pipeline-layout"),
                        push_constant_ranges: &[],
                        bind_group_layouts: &[&build_single_scattering_lut_bind_group_layout],
                    }),
                ),
                module: &build_single_scattering_lut_shader,
                entry_point: "main",
            });

        // Scattering Density
        let build_scattering_density_lut_shader = gpu.create_shader_module(
            "build_scattering_density_lut.comp",
            include_bytes!("../target/build_scattering_density_lut.comp.spirv"),
        )?;
        let build_scattering_density_lut_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("atmosphere-build-scattering-density-lut-bind-group"),
                entries: &[
                    uniform(0, mem::size_of::<AtmosphereParameters>()), // atmosphere
                    uniform(1, mem::size_of::<u32>()),                  // scattering_order
                    texture2d(2),                                       // transmittance_texture
                    sampler(3),                                         // transmittance_sampler
                    texture3d(4),          // delta_rayleigh_scattering_texture
                    sampler(5),            // delta_rayleigh_scattering_sampler
                    texture3d(6),          // delta_mie_scattering_texture
                    sampler(7),            // delta_mie_scattering_sampler
                    texture3d(8),          // delta_multiple_scattering_texture
                    sampler(9),            // delta_multiple_scattering_sampler
                    texture2d(10),         // delta_irradiance_texture
                    sampler(11),           // delta_irradiance_sampler
                    storage_texture3d(12), // delta_scattering_density_texture
                ],
            });
        let build_scattering_density_lut_pipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("atmosphere-build-scattering-density-lut-pipeline"),
                layout: Some(
                    &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("atmosphere-build-scattering-density-lut-pipeline-layout"),
                        push_constant_ranges: &[],
                        bind_group_layouts: &[&build_scattering_density_lut_bind_group_layout],
                    }),
                ),
                module: &build_scattering_density_lut_shader,
                entry_point: "main",
            });

        // Indirect Irradiance
        let build_indirect_irradiance_lut_shader = gpu.create_shader_module(
            "build_indirect_irradiance_lut.comp",
            include_bytes!("../target/build_indirect_irradiance_lut.comp.spirv"),
        )?;
        let build_indirect_irradiance_lut_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("atmosphere-build-indirect-irradiance-lut-bind-group"),
                entries: &[
                    uniform(0, mem::size_of::<AtmosphereParameters>()), // atmosphere
                    uniform(1, mem::size_of::<[[f32; 4]; 4]>()),        // rad_to_lum
                    uniform(2, mem::size_of::<u32>()),                  // scattering_order
                    texture3d(3),          // delta_rayleigh_scattering_texture
                    sampler(4),            // delta_rayleigh_scattering_sampler
                    texture3d(5),          // delta_mie_scattering_texture
                    sampler(6),            // delta_mie_scattering_sampler
                    texture3d(7),          // delta_multiple_scattering_texture
                    sampler(8),            // delta_multiple_scattering_sampler
                    storage_texture2d(9),  // delta_irradiance_texture
                    storage_texture2d(10), // irradiance_texture
                ],
            });
        let build_indirect_irradiance_lut_pipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("atmosphere-build-indirect-irradiance-lut-pipeline"),
                layout: Some(
                    &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("atmosphere-build-indirect-irradiance-lut-pipeline-layout"),
                        push_constant_ranges: &[],
                        bind_group_layouts: &[&build_indirect_irradiance_lut_bind_group_layout],
                    }),
                ),
                module: &build_indirect_irradiance_lut_shader,
                entry_point: "main",
            });

        // Multiple Scattering
        let build_multiple_scattering_lut_shader = gpu.create_shader_module(
            "build_multiple_scattering_lut.comp",
            include_bytes!("../target/build_multiple_scattering_lut.comp.spirv"),
        )?;
        let build_multiple_scattering_lut_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("atmosphere-build-multiple-scattering-lut-bind-group"),
                entries: &[
                    uniform(0, mem::size_of::<AtmosphereParameters>()), // atmosphere; };
                    uniform(1, mem::size_of::<[[f32; 4]; 4]>()),        // rad_to_lum; };
                    uniform(2, mem::size_of::<u32>()),                  // scattering_order; };
                    texture2d(3),                                       // transmittance_texture;
                    sampler(4),                                         // transmittance_sampler;
                    texture3d(5),         // delta_scattering_density_texture;
                    sampler(6),           // delta_scattering_density_sampler;
                    storage_texture3d(7), // delta_multiple_scattering_texture;
                    storage_texture3d(8), // scattering_texture;
                ],
            });
        let build_multiple_scattering_lut_pipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("atmosphere-build-multiple-scattering-lut-pipeline"),
                layout: Some(
                    &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("atmosphere-build-multiple-scattering-lut-pipeline-layout"),
                        push_constant_ranges: &[],
                        bind_group_layouts: &[&build_multiple_scattering_lut_bind_group_layout],
                    }),
                ),
                module: &build_multiple_scattering_lut_shader,
                entry_point: "main",
            });

        // Allocate all of our memory up front.
        let delta_irradiance_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atmosphere-delta-irradiance-texture"),
            size: IRRADIANCE_EXTENT,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsage::all(),
        });
        let delta_irradiance_texture_view =
            delta_irradiance_texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("atmosphere-delta-irradiance-texture-view"),
                format: None,
                dimension: None,
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                level_count: None,
                base_array_layer: 0,
                array_layer_count: None,
            });
        let delta_rayleigh_scattering_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atmosphere-delta-rayleigh-scattering-texture"),
            size: SCATTERING_EXTENT,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsage::all(),
        });
        let delta_rayleigh_scattering_texture_view =
            delta_rayleigh_scattering_texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("atmosphere-delta-rayleigh-scattering-texture-view"),
                format: None,
                dimension: None,
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                level_count: None,
                base_array_layer: 0,
                array_layer_count: None,
            });
        let delta_mie_scattering_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atmosphere-delta-mie-scattering-texture"),
            size: SCATTERING_EXTENT,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsage::all(),
        });
        let delta_mie_scattering_texture_view =
            delta_mie_scattering_texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("atmosphere-delta-mie-scattering-texture-view"),
                format: None,
                dimension: None,
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                level_count: None,
                base_array_layer: 0,
                array_layer_count: None,
            });
        let delta_multiple_scattering_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atmosphere-delta-multiple-scattering-texture"),
            size: SCATTERING_EXTENT,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsage::all(),
        });
        let delta_multiple_scattering_texture_view =
            delta_multiple_scattering_texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("atmosphere-delta-multiple-scattering-texture-view"),
                format: None,
                dimension: None,
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                level_count: None,
                base_array_layer: 0,
                array_layer_count: None,
            });
        let delta_scattering_density_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atmosphere-delta-scattering-density-texture"),
            size: SCATTERING_EXTENT,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsage::all(),
        });
        let delta_scattering_density_texture_view =
            delta_scattering_density_texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("atmosphere-delta-scattering-density-texture-view"),
                format: None,
                dimension: None,
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                level_count: None,
                base_array_layer: 0,
                array_layer_count: None,
            });

        let transmittance_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atmosphere-transmittance-texture"),
            size: TRANSMITTANCE_EXTENT,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsage::all(),
        });
        let transmittance_texture_view =
            transmittance_texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("atmosphere-transmittance-texture-view"),
                format: None,
                dimension: None,
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                level_count: None,
                base_array_layer: 0,
                array_layer_count: None,
            });
        let scattering_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atmosphere-scattering-texture"),
            size: SCATTERING_EXTENT,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsage::all(),
        });
        let scattering_texture_view =
            scattering_texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("atmosphere-scattering-texture-view"),
                format: None,
                dimension: None,
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                level_count: None,
                base_array_layer: 0,
                array_layer_count: None,
            });
        let single_mie_scattering_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atmosphere-single-mie-scattering-texture"),
            size: SCATTERING_EXTENT,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsage::all(),
        });
        let single_mie_scattering_texture_view =
            single_mie_scattering_texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("atmosphere-single-mie-scattering-texture-view"),
                format: None,
                dimension: None,
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                level_count: None,
                base_array_layer: 0,
                array_layer_count: None,
            });
        let irradiance_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atmosphere-irradiance-texture"),
            size: IRRADIANCE_EXTENT,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsage::all(),
        });
        let irradiance_texture_view =
            irradiance_texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("atmosphere-irradiance-texture-view"),
                format: None,
                dimension: None,
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                level_count: None,
                base_array_layer: 0,
                array_layer_count: None,
            });

        let sampler_resource = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atmosphere-sampler-resource"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            lod_min_clamp: 0f32,
            lod_max_clamp: 9_999_999f32,
            compare: None,
            anisotropy_clamp: None,
            border_color: None,
        });

        Ok(Self {
            build_transmittance_lut_bind_group_layout,
            build_transmittance_lut_pipeline,
            build_direct_irradiance_lut_bind_group_layout,
            build_direct_irradiance_lut_pipeline,
            build_single_scattering_lut_bind_group_layout,
            build_single_scattering_lut_pipeline,
            build_scattering_density_lut_bind_group_layout,
            build_scattering_density_lut_pipeline,
            build_indirect_irradiance_lut_bind_group_layout,
            build_indirect_irradiance_lut_pipeline,
            build_multiple_scattering_lut_bind_group_layout,
            build_multiple_scattering_lut_pipeline,

            delta_irradiance_texture,
            delta_irradiance_texture_view,
            delta_rayleigh_scattering_texture,
            delta_rayleigh_scattering_texture_view,
            delta_mie_scattering_texture,
            delta_mie_scattering_texture_view,
            delta_multiple_scattering_texture,
            delta_multiple_scattering_texture_view,
            delta_scattering_density_texture,
            delta_scattering_density_texture_view,

            transmittance_texture,
            transmittance_texture_view,
            scattering_texture,
            scattering_texture_view,
            single_mie_scattering_texture,
            single_mie_scattering_texture_view,
            irradiance_texture,
            irradiance_texture_view,

            sampler_resource,
            params,
        })
    }

    pub fn build_textures(&self, gpu: &mut Gpu) -> Result<wgpu::Buffer> /* AtmosphereParameters */ {
        let mut srgb_atmosphere = self.params.sample(RGB_LAMBDAS);
        srgb_atmosphere.ground_albedo = [0f32, 0f32, 0.04f32, 0f32];
        let srgb_atmosphere_buffer = gpu.push_data(
            "atmosphere-srgb-params-buffer",
            &srgb_atmosphere,
            wgpu::BufferUsage::UNIFORM,
        );

        trace!("Building atmosphere parameters");
        let num_iterations = (NUM_PRECOMPUTED_WAVELENGTHS + 3) / 4;
        let delta_lambda = (MAX_LAMBDA - MIN_LAMBDA) / (4.0 * num_iterations as f64);
        for i in 0..num_iterations {
            let lambdas = [
                MIN_LAMBDA + (4.0 * i as f64 + 0.5) * delta_lambda,
                MIN_LAMBDA + (4.0 * i as f64 + 1.5) * delta_lambda,
                MIN_LAMBDA + (4.0 * i as f64 + 2.5) * delta_lambda,
                MIN_LAMBDA + (4.0 * i as f64 + 3.5) * delta_lambda,
            ];
            // Do not include MAX_LUMINOUS_EFFICACY here to keep values
            // as close to 0 as possible to preserve maximal precision.
            // It is included in SKY_SPECTRA_RADIANCE_TO_LUMINANCE.
            // Note: Why do we scale by delta_lambda here?
            let l0 = wavelength_to_srgb(lambdas[0], delta_lambda);
            let l1 = wavelength_to_srgb(lambdas[1], delta_lambda);
            let l2 = wavelength_to_srgb(lambdas[2], delta_lambda);
            let l3 = wavelength_to_srgb(lambdas[3], delta_lambda);
            // Stuff these factors into a matrix by columns so that our GPU can do the
            // conversion for us quickly; Note that glsl is in column-major order, so this
            // is just the concatenation of our 4 arrays with 0s interspersed.
            let rad_to_lum = [
                l0[0], l0[1], l0[2], 0f64, l1[0], l1[1], l1[2], 0f64, l2[0], l2[1], l2[2], 0f64,
                l3[0], l3[1], l3[2], 0f64,
            ];
            self.precompute_one_step(lambdas, NUM_SCATTERING_PASSES, rad_to_lum, gpu);

            gpu.device().poll(wgpu::Maintain::Poll);
        }

        // Rebuild transmittance at RGB instead of high UV.
        // Upload atmosphere parameters for this set of wavelengths.
        self.compute_transmittance_at(RGB_LAMBDAS, gpu, &srgb_atmosphere_buffer);

        if DUMP_FINAL {
            block_on(Self::dump_texture(
                "final-transmittance".to_owned(),
                RGB_LAMBDAS,
                gpu,
                TRANSMITTANCE_EXTENT,
                &self.transmittance_texture,
            ));
            block_on(Self::dump_texture(
                "final-irradiance".to_owned(),
                RGB_LAMBDAS,
                gpu,
                IRRADIANCE_EXTENT,
                &self.irradiance_texture,
            ));
            block_on(Self::dump_texture(
                "final-scattering".to_owned(),
                RGB_LAMBDAS,
                gpu,
                SCATTERING_EXTENT,
                &self.scattering_texture,
            ));
            block_on(Self::dump_texture(
                "final-single-mie-scattering".to_owned(),
                RGB_LAMBDAS,
                gpu,
                SCATTERING_EXTENT,
                &self.single_mie_scattering_texture,
            ));
        }

        Ok(srgb_atmosphere_buffer)
    }

    fn precompute_one_step(
        &self,
        lambdas: [f64; 4],
        num_scattering_passes: usize,
        rad_to_lum: [f64; 16],
        gpu: &mut Gpu,
    ) {
        // Upload atmosphere parameters for this set of wavelengths.
        let atmosphere_params_buffer = gpu.push_data(
            "atmosphere-params-buffer",
            &self.params.sample(lambdas),
            wgpu::BufferUsage::UNIFORM,
        );

        let rad_to_lum32: [[f32; 4]; 4] = [
            [
                rad_to_lum[0] as f32,
                rad_to_lum[1] as f32,
                rad_to_lum[2] as f32,
                rad_to_lum[3] as f32,
            ],
            [
                rad_to_lum[4] as f32,
                rad_to_lum[5] as f32,
                rad_to_lum[6] as f32,
                rad_to_lum[7] as f32,
            ],
            [
                rad_to_lum[8] as f32,
                rad_to_lum[9] as f32,
                rad_to_lum[10] as f32,
                rad_to_lum[11] as f32,
            ],
            [
                rad_to_lum[12] as f32,
                rad_to_lum[13] as f32,
                rad_to_lum[14] as f32,
                rad_to_lum[15] as f32,
            ],
        ];
        let rad_to_lum_buffer = gpu.push_slice(
            "atmosphere-rad-to-lum-buffer",
            &rad_to_lum32,
            wgpu::BufferUsage::UNIFORM,
        );

        let transmittance_start = Instant::now();
        self.compute_transmittance_at(lambdas, gpu, &atmosphere_params_buffer);
        let transmittance_time = transmittance_start.elapsed();
        println!(
            "transmittance      {:?}: {}.{}ms",
            lambdas,
            transmittance_time.as_secs() * 1000 + u64::from(transmittance_time.subsec_millis()),
            transmittance_time.subsec_micros()
        );

        let direct_irradiance_start = Instant::now();
        self.compute_direct_irradiance_at(lambdas, gpu, &atmosphere_params_buffer);
        let direct_irradiance_time = direct_irradiance_start.elapsed();
        println!(
            "direct-irradiance  {:?}: {}.{}ms",
            lambdas,
            direct_irradiance_time.as_secs() * 1000
                + u64::from(direct_irradiance_time.subsec_millis()),
            direct_irradiance_time.subsec_micros()
        );

        let single_scattering_start = Instant::now();
        self.compute_single_scattering_at(
            lambdas,
            gpu,
            &atmosphere_params_buffer,
            &rad_to_lum_buffer,
        );
        let single_scattering_time = single_scattering_start.elapsed();
        println!(
            "single-scattering  {:?}: {}.{}ms",
            lambdas,
            single_scattering_time.as_secs() * 1000
                + u64::from(single_scattering_time.subsec_millis()),
            single_scattering_time.subsec_micros()
        );

        for scattering_order in 2..=num_scattering_passes {
            let scattering_order_buffer = gpu.push_slice(
                "atmosphere-scattering-order-buffer",
                &[scattering_order as u32],
                wgpu::BufferUsage::UNIFORM,
            );

            let scattering_density_start = Instant::now();
            self.compute_scattering_density_at(
                lambdas,
                scattering_order,
                gpu,
                &atmosphere_params_buffer,
                &scattering_order_buffer,
            );
            let scattering_density_time = scattering_density_start.elapsed();
            println!(
                "scattering-density {:?}: {}.{}ms",
                lambdas,
                scattering_density_time.as_secs() * 1000
                    + u64::from(scattering_density_time.subsec_millis()),
                scattering_density_time.subsec_micros()
            );

            let indirect_irradiance_start = Instant::now();
            self.compute_indirect_irradiance_at(
                lambdas,
                scattering_order,
                gpu,
                &atmosphere_params_buffer,
                &rad_to_lum_buffer,
                &scattering_order_buffer,
            );
            let indirect_irradiance_time = indirect_irradiance_start.elapsed();
            println!(
                "indirect-irradiance{:?}: {}.{}ms",
                lambdas,
                indirect_irradiance_time.as_secs() * 1000
                    + u64::from(indirect_irradiance_time.subsec_millis()),
                indirect_irradiance_time.subsec_micros()
            );

            let multiple_scattering_start = Instant::now();
            self.compute_multiple_scattering_at(
                lambdas,
                scattering_order,
                gpu,
                &atmosphere_params_buffer,
                &rad_to_lum_buffer,
                &scattering_order_buffer,
            );
            let multiple_scattering_time = multiple_scattering_start.elapsed();
            println!(
                "multiple-scattering{:?}: {}.{}ms",
                lambdas,
                multiple_scattering_time.as_secs() * 1000
                    + u64::from(multiple_scattering_time.subsec_millis()),
                multiple_scattering_time.subsec_micros()
            );
        }
    }

    fn compute_transmittance_at(
        &self,
        lambdas: [f64; 4],
        gpu: &mut Gpu,
        atmosphere_params_buffer: &wgpu::Buffer, // AtmosphereParameters
    ) {
        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atmosphere-compute-transmittance-bind-group"),
            layout: &self.build_transmittance_lut_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: atmosphere_params_buffer,
                        offset: 0,
                        size: None,
                    },
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.transmittance_texture_view),
                },
            ],
        });

        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("atmosphere-compute-transmittance-command-encoder"),
            });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("compute_transmittance_at"),
            });
            cpass.set_pipeline(&self.build_transmittance_lut_pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.dispatch(
                TRANSMITTANCE_EXTENT.width / BLOCK_SIZE,
                TRANSMITTANCE_EXTENT.height / BLOCK_SIZE,
                1,
            );
        }
        gpu.queue_mut().submit(vec![encoder.finish()]);

        if DUMP_TRANSMITTANCE {
            block_on(Self::dump_texture(
                "transmittance".to_owned(),
                lambdas,
                gpu,
                TRANSMITTANCE_EXTENT,
                &self.transmittance_texture,
            ));
        }
    }

    fn compute_direct_irradiance_at(
        &self,
        lambdas: [f64; 4],
        gpu: &mut Gpu,
        atmosphere_params_buffer: &wgpu::Buffer, // AtmosphereParameters
    ) {
        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atmosphere-compute-direct-irradiance-bind-group"),
            layout: &self.build_direct_irradiance_lut_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: atmosphere_params_buffer,
                        offset: 0,
                        size: None,
                    },
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.transmittance_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_resource),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(
                        &self.delta_irradiance_texture_view,
                    ),
                },
            ],
        });

        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("atmosphere-compute-direct-irradiance-command-encoder"),
            });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("compute_direct_irradiance_at"),
            });
            cpass.set_pipeline(&self.build_direct_irradiance_lut_pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.dispatch(
                IRRADIANCE_EXTENT.width / BLOCK_SIZE,
                IRRADIANCE_EXTENT.height / BLOCK_SIZE,
                1,
            );
        }
        gpu.queue_mut().submit(vec![encoder.finish()]);

        if DUMP_DIRECT_IRRADIANCE {
            block_on(Self::dump_texture(
                "direct-irradiance".to_owned(),
                lambdas,
                gpu,
                IRRADIANCE_EXTENT,
                &self.delta_irradiance_texture,
            ));
        }
    }

    fn compute_single_scattering_at(
        &self,
        lambdas: [f64; 4],
        gpu: &mut Gpu,
        atmosphere_params_buffer: &wgpu::Buffer,
        rad_to_lum_buffer: &wgpu::Buffer,
    ) {
        /*
        uniform(0),           // atmosphere
        texture2d(1),         // transmittance_texture
        sampler(2),           // transmittance_sampler
        uniform(3),           // rad_to_lum
        storage_texture3d(4), // delta_rayleigh_scattering_texture
        storage_texture3d(5), // delta_mie_scattering_texture
        storage_texture3d(6), // scattering_texture
        storage_texture3d(7), // single_mie_scattering_texture
        */
        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atmosphere-compute-single-scattering-bind-group"),
            layout: &self.build_single_scattering_lut_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: atmosphere_params_buffer,
                        offset: 0,
                        size: None,
                    },
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.transmittance_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_resource),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: rad_to_lum_buffer,
                        offset: 0,
                        size: None,
                    },
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(
                        &self.delta_rayleigh_scattering_texture_view,
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(
                        &self.delta_mie_scattering_texture_view,
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&self.scattering_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(
                        &self.single_mie_scattering_texture_view,
                    ),
                },
            ],
        });

        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("atmosphere-compute-single-scattering-command-encoder"),
            });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("compute_single_scattering_at"),
            });
            cpass.set_pipeline(&self.build_single_scattering_lut_pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.dispatch(
                SCATTERING_EXTENT.width / BLOCK_SIZE,
                SCATTERING_EXTENT.height / BLOCK_SIZE,
                SCATTERING_EXTENT.depth / BLOCK_SIZE,
            );
        }
        gpu.queue_mut().submit(vec![encoder.finish()]);

        if DUMP_SINGLE_RAYLEIGH {
            block_on(Self::dump_texture(
                "single-scattering-delta-rayleigh".to_owned(),
                lambdas,
                gpu,
                SCATTERING_EXTENT,
                &self.delta_rayleigh_scattering_texture,
            ));
        }
        if DUMP_SINGLE_ACC {
            block_on(Self::dump_texture(
                "single-scattering-acc".to_owned(),
                lambdas,
                gpu,
                SCATTERING_EXTENT,
                &self.scattering_texture,
            ));
        }
        if DUMP_SINGLE_MIE {
            block_on(Self::dump_texture(
                "single-scattering-delta-mie".to_owned(),
                lambdas,
                gpu,
                SCATTERING_EXTENT,
                &self.delta_mie_scattering_texture,
            ));
        }
        if DUMP_SINGLE_MIE_ACC {
            block_on(Self::dump_texture(
                "single-scattering-mie-acc".to_owned(),
                lambdas,
                gpu,
                SCATTERING_EXTENT,
                &self.single_mie_scattering_texture,
            ));
        }
    }

    fn compute_scattering_density_at(
        &self,
        lambdas: [f64; 4],
        scattering_order: usize,
        gpu: &mut Gpu,
        atmosphere_params_buffer: &wgpu::Buffer,
        scattering_order_buffer: &wgpu::Buffer,
    ) {
        /*
        uniform(0),            // atmosphere
        uniform(1),            // scattering_order
        texture2d(2),          // transmittance_texture
        sampler(3),            // transmittance_sampler
        texture3d(4),          // delta_rayleigh_scattering_texture
        sampler(5),            // delta_rayleigh_scattering_sampler
        texture3d(6),          // delta_mie_scattering_texture
        sampler(7),            // delta_mie_scattering_sampler
        texture3d(8),          // delta_multiple_scattering_texture
        sampler(9),            // delta_multiple_scattering_sampler
        texture2d(10),         // delta_irradiance_texture
        sampler(11),           // delta_irradiance_sampler
        storage_texture3d(12), // delta_scattering_density_texture
        */
        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atmosphere-compute-scattering-density-bind-group"),
            layout: &self.build_scattering_density_lut_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: atmosphere_params_buffer,
                        offset: 0,
                        size: None,
                    },
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: scattering_order_buffer,
                        offset: 0,
                        size: None,
                    },
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.transmittance_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_resource),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(
                        &self.delta_rayleigh_scattering_texture_view,
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_resource),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(
                        &self.delta_mie_scattering_texture_view,
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_resource),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::TextureView(
                        &self.delta_multiple_scattering_texture_view,
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_resource),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: wgpu::BindingResource::TextureView(
                        &self.delta_irradiance_texture_view,
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 11,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_resource),
                },
                wgpu::BindGroupEntry {
                    binding: 12,
                    resource: wgpu::BindingResource::TextureView(
                        &self.delta_scattering_density_texture_view,
                    ),
                },
            ],
        });

        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("atmosphere-compute-scattering-density-command-encoder"),
            });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("compute_scattering_density"),
            });
            cpass.set_pipeline(&self.build_scattering_density_lut_pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.dispatch(
                SCATTERING_EXTENT.width / BLOCK_SIZE,
                SCATTERING_EXTENT.height / BLOCK_SIZE,
                SCATTERING_EXTENT.depth / BLOCK_SIZE,
            );
        }
        gpu.queue_mut().submit(vec![encoder.finish()]);

        if DUMP_SCATTERING_DENSITY {
            block_on(Self::dump_texture(
                format!("delta-scattering-density-{}", scattering_order),
                lambdas,
                gpu,
                SCATTERING_EXTENT,
                &self.delta_scattering_density_texture,
            ));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn compute_indirect_irradiance_at(
        &self,
        lambdas: [f64; 4],
        scattering_order: usize,
        gpu: &mut Gpu,
        atmosphere_params_buffer: &wgpu::Buffer,
        rad_to_lum_buffer: &wgpu::Buffer,
        scattering_order_buffer: &wgpu::Buffer,
    ) {
        /*
        uniform(0),            // atmosphere
        uniform(1),            // rad_to_lum
        uniform(2),            // scattering_order
        texture3d(3),          // delta_rayleigh_scattering_texture
        sampler(4),            // delta_rayleigh_scattering_sampler
        texture3d(5),          // delta_mie_scattering_texture
        sampler(6),            // delta_mie_scattering_sampler
        texture3d(7),          // delta_multiple_scattering_texture
        sampler(8),            // delta_multiple_scattering_sampler
        storage_texture2d(9),  // delta_irradiance_texture
        storage_texture2d(10), // irradiance_texture
        */
        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atmosphere-compute-indirect-irradiance-bind-group"),
            layout: &self.build_indirect_irradiance_lut_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: atmosphere_params_buffer,
                        offset: 0,
                        size: None,
                    },
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: rad_to_lum_buffer,
                        offset: 0,
                        size: None,
                    },
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: scattering_order_buffer,
                        offset: 0,
                        size: None,
                    },
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(
                        &self.delta_rayleigh_scattering_texture_view,
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_resource),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(
                        &self.delta_mie_scattering_texture_view,
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_resource),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(
                        &self.delta_multiple_scattering_texture_view,
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_resource),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::TextureView(
                        &self.delta_irradiance_texture_view,
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: wgpu::BindingResource::TextureView(&self.irradiance_texture_view),
                },
            ],
        });

        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("atmosphere-compute-indirect-irradiance-command-encoder"),
            });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("compute_indirect_irradiance_at"),
            });
            cpass.set_pipeline(&self.build_indirect_irradiance_lut_pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.dispatch(
                IRRADIANCE_EXTENT.width / BLOCK_SIZE,
                IRRADIANCE_EXTENT.height / BLOCK_SIZE,
                1,
            );
        }
        gpu.queue_mut().submit(vec![encoder.finish()]);

        if DUMP_INDIRECT_IRRADIANCE_DELTA {
            block_on(Self::dump_texture(
                format!("indirect-delta-irradiance-{}", scattering_order),
                lambdas,
                gpu,
                IRRADIANCE_EXTENT,
                &self.delta_irradiance_texture,
            ));
        }
        if DUMP_INDIRECT_IRRADIANCE_ACC {
            block_on(Self::dump_texture(
                format!("indirect-irradiance-acc-{}", scattering_order),
                lambdas,
                gpu,
                IRRADIANCE_EXTENT,
                &self.irradiance_texture,
            ));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn compute_multiple_scattering_at(
        &self,
        lambdas: [f64; 4],
        scattering_order: usize,
        gpu: &mut Gpu,
        atmosphere_params_buffer: &wgpu::Buffer,
        rad_to_lum_buffer: &wgpu::Buffer,
        scattering_order_buffer: &wgpu::Buffer,
    ) {
        /*
        uniform(0),           // atmosphere; };
        uniform(1),           // rad_to_lum; };
        uniform(2),           // scattering_order; };
        texture2d(3),         // transmittance_texture;
        sampler(4),           // transmittance_sampler;
        texture3d(5),         // delta_scattering_density_texture;
        sampler(6),           // delta_scattering_density_sampler;
        storage_texture3d(7), // delta_multiple_scattering_texture;
        storage_texture3d(8), // scattering_texture;
        */
        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atmosphere-compute-multiple-scattering-bind-group"),
            layout: &self.build_multiple_scattering_lut_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: atmosphere_params_buffer,
                        offset: 0,
                        size: None,
                    },
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: rad_to_lum_buffer,
                        offset: 0,
                        size: None,
                    },
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: scattering_order_buffer,
                        offset: 0,
                        size: None,
                    },
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.transmittance_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_resource),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(
                        &self.delta_scattering_density_texture_view,
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_resource),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(
                        &self.delta_multiple_scattering_texture_view,
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::TextureView(&self.scattering_texture_view),
                },
            ],
        });

        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("atmosphere-compute-multiple-scattering-command-encoder"),
            });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("compute_multiple_scattering_at"),
            });
            cpass.set_pipeline(&self.build_multiple_scattering_lut_pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.dispatch(
                SCATTERING_EXTENT.width / BLOCK_SIZE,
                SCATTERING_EXTENT.height / BLOCK_SIZE,
                SCATTERING_EXTENT.depth / BLOCK_SIZE,
            );
        }
        gpu.queue_mut().submit(vec![encoder.finish()]);

        if DUMP_MULTIPLE_SCATTERING {
            block_on(Self::dump_texture(
                format!("delta-multiple-scattering-{}", scattering_order),
                lambdas,
                gpu,
                SCATTERING_EXTENT,
                &self.delta_multiple_scattering_texture,
            ));
            block_on(Self::dump_texture(
                format!("multiple-scattering-{}", scattering_order),
                lambdas,
                gpu,
                SCATTERING_EXTENT,
                &self.scattering_texture,
            ));
        }
    }

    pub fn transmittance_texture(&self) -> &wgpu::Texture {
        &self.transmittance_texture
    }

    pub fn irradiance_texture(&self) -> &wgpu::Texture {
        &self.irradiance_texture
    }

    pub fn scattering_texture(&self) -> &wgpu::Texture {
        &self.scattering_texture
    }

    pub fn single_mie_scattering_texture(&self) -> &wgpu::Texture {
        &self.single_mie_scattering_texture
    }

    async fn dump_texture(
        prefix: String,
        lambdas: [f64; 4],
        gpu: &mut Gpu,
        extent: wgpu::Extent3d,
        texture: &wgpu::Texture,
    ) {
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("atmosphere-debug-dump-command-encoder"),
            });
        let staging_buffer_size = u64::from(extent.width * extent.height * extent.depth * 16);
        let staging_buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("atmosphere-debug-dump-texture-buffer"),
            size: staging_buffer_size,
            usage: wgpu::BufferUsage::all(),
            mapped_at_creation: true,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TextureCopyView {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            wgpu::BufferCopyView {
                buffer: &staging_buffer,
                layout: wgpu::TextureDataLayout {
                    offset: 0,
                    bytes_per_row: extent.width * 16,
                    rows_per_image: extent.height,
                },
            },
            extent,
        );
        gpu.queue_mut().submit(vec![encoder.finish()]);

        let reader = staging_buffer.slice(..).map_async(wgpu::MapMode::Read);
        gpu.device().poll(wgpu::Maintain::Wait);
        reader.await.unwrap();
        let mapping = staging_buffer.slice(..).get_mapped_range();

        let offset = mapping.as_ptr().align_offset(mem::align_of::<f32>());
        assert_eq!(offset, 0);
        #[allow(clippy::cast_ptr_alignment)]
        let fp = mapping.as_ptr() as *const f32;
        let floats = unsafe { slice::from_raw_parts(fp, mapping.len() / 4) };
        Self::show_range(floats, &prefix);

        let (p0, p1) = Self::split_pixels(floats, extent);
        Self::save_layered(
            p0,
            3,
            extent,
            &format!(
                "dump/atmosphere/{}-{}-{}-{}",
                prefix, lambdas[0] as usize, lambdas[1] as usize, lambdas[2] as usize
            ),
        );
        Self::save_layered(
            p1,
            1,
            extent,
            &format!("dump/{}-{}", prefix, lambdas[3] as usize),
        );
    }

    fn show_range(buf: &[f32], path: &str) {
        use num_traits::float::Float;
        let mut minf = f32::max_value();
        let mut maxf = f32::min_value();
        for v in buf {
            if *v > maxf {
                maxf = *v;
            }
            if *v < minf {
                minf = *v;
            }
        }
        println!("RANGE: {} -> {} in {}", minf, maxf, path);
    }

    fn split_pixels(src: &[f32], dim: wgpu::Extent3d) -> (Vec<u8>, Vec<u8>) {
        let mut p0 = Vec::with_capacity((dim.width * dim.height * dim.depth) as usize * 3);
        let mut p1 = Vec::with_capacity((dim.width * dim.height * dim.depth) as usize);
        const WHITE_POINT_R: f32 = 1.082_414f32;
        const WHITE_POINT_G: f32 = 0.967_556f32;
        const WHITE_POINT_B: f32 = 0.950_030f32;
        const WHITE_POINT_A: f32 = 1.0;
        const EXPOSURE: f32 = 683f32 * 0.0001f32;
        for i in 0usize..(dim.width * dim.height * dim.depth) as usize {
            let r0 = src[4 * i];
            let g0 = src[4 * i + 1];
            let b0 = src[4 * i + 2];
            let a0 = src[4 * i + 3];

            let mut r1 = (1.0 - (-r0 / WHITE_POINT_R * EXPOSURE).exp()).powf(1.0 / 2.2);
            let mut g1 = (1.0 - (-g0 / WHITE_POINT_G * EXPOSURE).exp()).powf(1.0 / 2.2);
            let mut b1 = (1.0 - (-b0 / WHITE_POINT_B * EXPOSURE).exp()).powf(1.0 / 2.2);
            let mut a1 = (1.0 - (-a0 / WHITE_POINT_A * EXPOSURE).exp()).powf(1.0 / 2.2);

            if r1.is_nan() {
                r1 = 0f32;
            }
            if g1.is_nan() {
                g1 = 0f32;
            }
            if b1.is_nan() {
                b1 = 0f32;
            }
            if a1.is_nan() {
                a1 = 0f32;
            }

            assert!((0.0..=1.0).contains(&r1));
            assert!((0.0..=1.0).contains(&g1));
            assert!((0.0..=1.0).contains(&b1));
            assert!((0.0..=1.0).contains(&a1));

            p0.push((r1 * 255f32) as u8);
            p0.push((g1 * 255f32) as u8);
            p0.push((b1 * 255f32) as u8);
            p1.push((a1 * 255f32) as u8);
        }
        (p0, p1)
    }

    fn save_layered(data: Vec<u8>, px_size: usize, extent: wgpu::Extent3d, prefix: &str) {
        let layer_size = (extent.width * extent.height) as usize * px_size;
        for layer_num in 0..extent.depth as usize {
            let data = &data[layer_num * layer_size..(layer_num + 1) * layer_size];
            let name = format!("{}-layer{:02}.png", prefix, layer_num);
            if px_size == 3 {
                let img =
                    ImageBuffer::<Rgb<u8>, _>::from_raw(extent.width, extent.height, data).unwrap();
                img.save(&name).unwrap();
            } else {
                assert_eq!(px_size, 1);
                let img = ImageBuffer::<Luma<u8>, _>::from_raw(extent.width, extent.height, data)
                    .unwrap();
                img.save(&name).unwrap();
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use gpu::TestResources;
    use std::time::Instant;

    #[cfg(unix)]
    #[test]
    fn test_create() -> Result<()> {
        let TestResources { gpu, .. } = Gpu::for_test_unix()?;
        let precompute_start = Instant::now();
        let pcp = Precompute::new(&gpu.read())?;
        let _atmosphere_params_buf = pcp.build_textures(&mut gpu.write());
        let precompute_time = precompute_start.elapsed();
        println!(
            "AtmosphereBuffers::precompute timing: {}.{}ms",
            precompute_time.as_secs() * 1000 + u64::from(precompute_time.subsec_millis()),
            precompute_time.subsec_micros()
        );
        Ok(())
    }
}
