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
};
use failure::{bail, Fallible};
use futures::executor::block_on;
use gpu::GPU;
use image::{ImageBuffer, Luma, Rgb};
use log::trace;
use std::{env, fs, mem, num::NonZeroU64, slice, time::Instant};

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

const TRANSMITTANCE_TEXTURE_WIDTH: u32 = 256;
const TRANSMITTANCE_TEXTURE_HEIGHT: u32 = 64;

const SCATTERING_TEXTURE_R_SIZE: u32 = 32;
const SCATTERING_TEXTURE_MU_SIZE: u32 = 128;
const SCATTERING_TEXTURE_MU_S_SIZE: u32 = 32;
const SCATTERING_TEXTURE_NU_SIZE: u32 = 8;

const SCATTERING_TEXTURE_WIDTH: u32 = SCATTERING_TEXTURE_NU_SIZE * SCATTERING_TEXTURE_MU_S_SIZE;
const SCATTERING_TEXTURE_HEIGHT: u32 = SCATTERING_TEXTURE_MU_SIZE;
const SCATTERING_TEXTURE_DEPTH: u32 = SCATTERING_TEXTURE_R_SIZE;

const IRRADIANCE_TEXTURE_WIDTH: u32 = 64;
const IRRADIANCE_TEXTURE_HEIGHT: u32 = 16;

const CACHE_DIR: &str = ".__nitrogen_cache__";

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

    // Extents
    transmittance_extent: wgpu::Extent3d,
    irradiance_extent: wgpu::Extent3d,
    scattering_extent: wgpu::Extent3d,

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
    pub fn precompute(
        num_precomputed_wavelengths: usize,
        num_scattering_passes: usize,
        gpu: &mut GPU,
    ) -> Fallible<(
        wgpu::Buffer,
        wgpu::Texture,
        wgpu::Texture,
        wgpu::Texture,
        wgpu::Texture,
    )> {
        let pc = Self::new(gpu)?;

        let srgb_atmosphere_buffer =
            pc.build_textures(num_precomputed_wavelengths, num_scattering_passes, gpu)?;

        Ok((
            srgb_atmosphere_buffer,
            pc.transmittance_texture,
            pc.irradiance_texture,
            pc.scattering_texture,
            pc.single_mie_scattering_texture,
        ))
    }

    pub fn new(gpu: &GPU) -> Fallible<Self> {
        let device = gpu.device();
        let params = EarthParameters::new();

        fn uniform(binding: u32, min_binding_size: usize) -> wgpu::BindGroupLayoutEntry {
            wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStage::COMPUTE,
                ty: wgpu::BindingType::UniformBuffer {
                    dynamic: false,
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
                    dimension: wgpu::TextureViewDimension::D2,
                    format: wgpu::TextureFormat::R32Float,
                    readonly: false,
                },
                count: None,
            }
        }
        fn storage_texture3d(binding: u32) -> wgpu::BindGroupLayoutEntry {
            wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStage::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    dimension: wgpu::TextureViewDimension::D3,
                    format: wgpu::TextureFormat::R32Float,
                    readonly: false,
                },
                count: None,
            }
        }
        fn texture2d(binding: u32) -> wgpu::BindGroupLayoutEntry {
            wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStage::COMPUTE,
                ty: wgpu::BindingType::SampledTexture {
                    multisampled: true,
                    component_type: wgpu::TextureComponentType::Float,
                    dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            }
        }
        fn texture3d(binding: u32) -> wgpu::BindGroupLayoutEntry {
            wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStage::COMPUTE,
                ty: wgpu::BindingType::SampledTexture {
                    multisampled: true,
                    component_type: wgpu::TextureComponentType::Float,
                    dimension: wgpu::TextureViewDimension::D3,
                },
                count: None,
            }
        }
        fn sampler(binding: u32) -> wgpu::BindGroupLayoutEntry {
            wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStage::COMPUTE,
                ty: wgpu::BindingType::Sampler { comparison: false },
                count: None,
            }
        }

        // Transmittance
        let build_transmittance_lut_shader = gpu.create_shader_module(include_bytes!(
            "../target/build_transmittance_lut.comp.spirv"
        ))?;
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
                compute_stage: wgpu::ProgrammableStageDescriptor {
                    module: &build_transmittance_lut_shader,
                    entry_point: "main",
                },
            });

        // Direct Irradiance
        let build_direct_irradiance_lut_shader = gpu.create_shader_module(include_bytes!(
            "../target/build_direct_irradiance_lut.comp.spirv"
        ))?;
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
                compute_stage: wgpu::ProgrammableStageDescriptor {
                    module: &build_direct_irradiance_lut_shader,
                    entry_point: "main",
                },
            });

        // Single Scattering
        let build_single_scattering_lut_shader = gpu.create_shader_module(include_bytes!(
            "../target/build_single_scattering_lut.comp.spirv"
        ))?;
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
                compute_stage: wgpu::ProgrammableStageDescriptor {
                    module: &build_single_scattering_lut_shader,
                    entry_point: "main",
                },
            });

        // Scattering Density
        let build_scattering_density_lut_shader = gpu.create_shader_module(include_bytes!(
            "../target/build_scattering_density_lut.comp.spirv"
        ))?;
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
                compute_stage: wgpu::ProgrammableStageDescriptor {
                    module: &build_scattering_density_lut_shader,
                    entry_point: "main",
                },
            });

        // Indirect Irradiance
        let build_indirect_irradiance_lut_shader = gpu.create_shader_module(include_bytes!(
            "../target/build_indirect_irradiance_lut.comp.spirv"
        ))?;
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
                compute_stage: wgpu::ProgrammableStageDescriptor {
                    module: &build_indirect_irradiance_lut_shader,
                    entry_point: "main",
                },
            });

        // Multiple Scattering
        let build_multiple_scattering_lut_shader = gpu.create_shader_module(include_bytes!(
            "../target/build_multiple_scattering_lut.comp.spirv"
        ))?;
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
                compute_stage: wgpu::ProgrammableStageDescriptor {
                    module: &build_multiple_scattering_lut_shader,
                    entry_point: "main",
                },
            });

        let transmittance_extent = wgpu::Extent3d {
            width: TRANSMITTANCE_TEXTURE_WIDTH,
            height: TRANSMITTANCE_TEXTURE_HEIGHT,
            depth: 1,
        };
        let irradiance_extent = wgpu::Extent3d {
            width: IRRADIANCE_TEXTURE_WIDTH,
            height: IRRADIANCE_TEXTURE_HEIGHT,
            depth: 1,
        };
        let scattering_extent = wgpu::Extent3d {
            width: SCATTERING_TEXTURE_WIDTH,
            height: SCATTERING_TEXTURE_HEIGHT,
            depth: SCATTERING_TEXTURE_DEPTH,
        };

        // Allocate all of our memory up front.
        let delta_irradiance_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atmosphere-delta-irradiance-texture"),
            size: irradiance_extent,
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
            size: scattering_extent,
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
            size: scattering_extent,
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
            size: scattering_extent,
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
            size: scattering_extent,
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
            size: transmittance_extent,
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
            size: scattering_extent,
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
            size: scattering_extent,
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
            size: irradiance_extent,
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

            transmittance_extent,
            irradiance_extent,
            scattering_extent,

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

    pub fn build_textures(
        &self,
        num_precomputed_wavelengths: usize,
        num_scattering_passes: usize,
        gpu: &mut GPU,
    ) -> Fallible<wgpu::Buffer> /* AtmosphereParameters */ {
        let mut srgb_atmosphere = self.params.sample(RGB_LAMBDAS);
        srgb_atmosphere.ground_albedo = [0f32, 0f32, 0.04f32, 0f32];
        let srgb_atmosphere_buffer = gpu.push_data(
            "atmosphere-srgb-params-buffer",
            &srgb_atmosphere,
            wgpu::BufferUsage::UNIFORM,
        );

        if self.load_cache(gpu).is_ok() {
            trace!("Using from cached atmosphere parameters");
            return Ok(srgb_atmosphere_buffer);
        }
        trace!("Building atmosphere parameters");

        let num_iterations = (num_precomputed_wavelengths + 3) / 4;
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
            self.precompute_one_step(lambdas, num_scattering_passes, rad_to_lum, gpu)?;

            gpu.device().poll(wgpu::Maintain::Poll);
        }

        // Rebuild transmittance at RGB instead of high UV.
        // Upload atmosphere parameters for this set of wavelengths.
        self.compute_transmittance_at(RGB_LAMBDAS, gpu, &srgb_atmosphere_buffer)?;

        if DUMP_FINAL {
            block_on(Self::dump_texture(
                "final-transmittance".to_owned(),
                RGB_LAMBDAS,
                gpu,
                self.transmittance_extent,
                &self.transmittance_texture,
            ));
            block_on(Self::dump_texture(
                "final-irradiance".to_owned(),
                RGB_LAMBDAS,
                gpu,
                self.irradiance_extent,
                &self.irradiance_texture,
            ));
            block_on(Self::dump_texture(
                "final-scattering".to_owned(),
                RGB_LAMBDAS,
                gpu,
                self.scattering_extent,
                &self.scattering_texture,
            ));
            block_on(Self::dump_texture(
                "final-single-mie-scattering".to_owned(),
                RGB_LAMBDAS,
                gpu,
                self.scattering_extent,
                &self.single_mie_scattering_texture,
            ));
        }

        block_on(self.update_cache(gpu))?;
        Ok(srgb_atmosphere_buffer)
    }

    fn precompute_one_step(
        &self,
        lambdas: [f64; 4],
        num_scattering_passes: usize,
        rad_to_lum: [f64; 16],
        gpu: &mut GPU,
    ) -> Fallible<()> {
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
        self.compute_transmittance_at(lambdas, gpu, &atmosphere_params_buffer)?;
        let transmittance_time = transmittance_start.elapsed();
        println!(
            "transmittance      {:?}: {}.{}ms",
            lambdas,
            transmittance_time.as_secs() * 1000 + u64::from(transmittance_time.subsec_millis()),
            transmittance_time.subsec_micros()
        );

        let direct_irradiance_start = Instant::now();
        self.compute_direct_irradiance_at(lambdas, gpu, &atmosphere_params_buffer)?;
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
        )?;
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
            )?;
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
            )?;
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
            )?;
            let multiple_scattering_time = multiple_scattering_start.elapsed();
            println!(
                "multiple-scattering{:?}: {}.{}ms",
                lambdas,
                multiple_scattering_time.as_secs() * 1000
                    + u64::from(multiple_scattering_time.subsec_millis()),
                multiple_scattering_time.subsec_micros()
            );
        }

        Ok(())
    }

    fn compute_transmittance_at(
        &self,
        lambdas: [f64; 4],
        gpu: &mut GPU,
        atmosphere_params_buffer: &wgpu::Buffer, // AtmosphereParameters
    ) -> Fallible<()> {
        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atmosphere-compute-transmittance-bind-group"),
            layout: &self.build_transmittance_lut_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(atmosphere_params_buffer.slice(..)),
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
            let mut cpass = encoder.begin_compute_pass();
            cpass.set_pipeline(&self.build_transmittance_lut_pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.dispatch(
                TRANSMITTANCE_TEXTURE_WIDTH / 8,
                TRANSMITTANCE_TEXTURE_HEIGHT / 8,
                1,
            );
        }
        gpu.queue_mut().submit(vec![encoder.finish()]);

        if DUMP_TRANSMITTANCE {
            block_on(Self::dump_texture(
                "transmittance".to_owned(),
                lambdas,
                gpu,
                self.transmittance_extent,
                &self.transmittance_texture,
            ));
        }

        Ok(())
    }

    fn compute_direct_irradiance_at(
        &self,
        lambdas: [f64; 4],
        gpu: &mut GPU,
        atmosphere_params_buffer: &wgpu::Buffer, // AtmosphereParameters
    ) -> Fallible<()> {
        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atmosphere-compute-direct-irradiance-bind-group"),
            layout: &self.build_direct_irradiance_lut_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(atmosphere_params_buffer.slice(..)),
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
            let mut cpass = encoder.begin_compute_pass();
            cpass.set_pipeline(&self.build_direct_irradiance_lut_pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.dispatch(
                IRRADIANCE_TEXTURE_WIDTH / 8,
                IRRADIANCE_TEXTURE_HEIGHT / 8,
                1,
            );
        }
        gpu.queue_mut().submit(vec![encoder.finish()]);

        if DUMP_DIRECT_IRRADIANCE {
            block_on(Self::dump_texture(
                "direct-irradiance".to_owned(),
                lambdas,
                gpu,
                self.irradiance_extent,
                &self.delta_irradiance_texture,
            ));
        }

        Ok(())
    }

    fn compute_single_scattering_at(
        &self,
        lambdas: [f64; 4],
        gpu: &mut GPU,
        atmosphere_params_buffer: &wgpu::Buffer,
        rad_to_lum_buffer: &wgpu::Buffer,
    ) -> Fallible<()> {
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
                    resource: wgpu::BindingResource::Buffer(atmosphere_params_buffer.slice(..)),
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
                    resource: wgpu::BindingResource::Buffer(rad_to_lum_buffer.slice(..)),
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
            let mut cpass = encoder.begin_compute_pass();
            cpass.set_pipeline(&self.build_single_scattering_lut_pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.dispatch(
                SCATTERING_TEXTURE_WIDTH / 8,
                SCATTERING_TEXTURE_HEIGHT / 8,
                SCATTERING_TEXTURE_DEPTH / 8,
            );
        }
        gpu.queue_mut().submit(vec![encoder.finish()]);

        if DUMP_SINGLE_RAYLEIGH {
            block_on(Self::dump_texture(
                "single-scattering-delta-rayleigh".to_owned(),
                lambdas,
                gpu,
                self.scattering_extent,
                &self.delta_rayleigh_scattering_texture,
            ));
        }
        if DUMP_SINGLE_ACC {
            block_on(Self::dump_texture(
                "single-scattering-acc".to_owned(),
                lambdas,
                gpu,
                self.scattering_extent,
                &self.scattering_texture,
            ));
        }
        if DUMP_SINGLE_MIE {
            block_on(Self::dump_texture(
                "single-scattering-delta-mie".to_owned(),
                lambdas,
                gpu,
                self.scattering_extent,
                &self.delta_mie_scattering_texture,
            ));
        }
        if DUMP_SINGLE_MIE_ACC {
            block_on(Self::dump_texture(
                "single-scattering-mie-acc".to_owned(),
                lambdas,
                gpu,
                self.scattering_extent,
                &self.single_mie_scattering_texture,
            ));
        }

        Ok(())
    }

    fn compute_scattering_density_at(
        &self,
        lambdas: [f64; 4],
        scattering_order: usize,
        gpu: &mut GPU,
        atmosphere_params_buffer: &wgpu::Buffer,
        scattering_order_buffer: &wgpu::Buffer,
    ) -> Fallible<()> {
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
                    resource: wgpu::BindingResource::Buffer(atmosphere_params_buffer.slice(..)),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(scattering_order_buffer.slice(..)),
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
            let mut cpass = encoder.begin_compute_pass();
            cpass.set_pipeline(&self.build_scattering_density_lut_pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.dispatch(
                SCATTERING_TEXTURE_WIDTH / 8,
                SCATTERING_TEXTURE_HEIGHT / 8,
                SCATTERING_TEXTURE_DEPTH / 8,
            );
        }
        gpu.queue_mut().submit(vec![encoder.finish()]);

        if DUMP_SCATTERING_DENSITY {
            block_on(Self::dump_texture(
                format!("delta-scattering-density-{}", scattering_order),
                lambdas,
                gpu,
                self.scattering_extent,
                &self.delta_scattering_density_texture,
            ));
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn compute_indirect_irradiance_at(
        &self,
        lambdas: [f64; 4],
        scattering_order: usize,
        gpu: &mut GPU,
        atmosphere_params_buffer: &wgpu::Buffer,
        rad_to_lum_buffer: &wgpu::Buffer,
        scattering_order_buffer: &wgpu::Buffer,
    ) -> Fallible<()> {
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
                    resource: wgpu::BindingResource::Buffer(atmosphere_params_buffer.slice(..)),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(rad_to_lum_buffer.slice(..)),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(scattering_order_buffer.slice(..)),
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
            let mut cpass = encoder.begin_compute_pass();
            cpass.set_pipeline(&self.build_indirect_irradiance_lut_pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.dispatch(
                IRRADIANCE_TEXTURE_WIDTH / 8,
                IRRADIANCE_TEXTURE_HEIGHT / 8,
                1,
            );
        }
        gpu.queue_mut().submit(vec![encoder.finish()]);

        if DUMP_INDIRECT_IRRADIANCE_DELTA {
            block_on(Self::dump_texture(
                format!("indirect-delta-irradiance-{}", scattering_order),
                lambdas,
                gpu,
                self.irradiance_extent,
                &self.delta_irradiance_texture,
            ));
        }
        if DUMP_INDIRECT_IRRADIANCE_ACC {
            block_on(Self::dump_texture(
                format!("indirect-irradiance-acc-{}", scattering_order),
                lambdas,
                gpu,
                self.irradiance_extent,
                &self.irradiance_texture,
            ));
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn compute_multiple_scattering_at(
        &self,
        lambdas: [f64; 4],
        scattering_order: usize,
        gpu: &mut GPU,
        atmosphere_params_buffer: &wgpu::Buffer,
        rad_to_lum_buffer: &wgpu::Buffer,
        scattering_order_buffer: &wgpu::Buffer,
    ) -> Fallible<()> {
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
                    resource: wgpu::BindingResource::Buffer(atmosphere_params_buffer.slice(..)),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(rad_to_lum_buffer.slice(..)),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(scattering_order_buffer.slice(..)),
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
            let mut cpass = encoder.begin_compute_pass();
            cpass.set_pipeline(&self.build_multiple_scattering_lut_pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.dispatch(
                SCATTERING_TEXTURE_WIDTH / 8,
                SCATTERING_TEXTURE_HEIGHT / 8,
                SCATTERING_TEXTURE_DEPTH / 8,
            );
        }
        gpu.queue_mut().submit(vec![encoder.finish()]);

        if DUMP_MULTIPLE_SCATTERING {
            block_on(Self::dump_texture(
                format!("delta-multiple-scattering-{}", scattering_order),
                lambdas,
                gpu,
                self.scattering_extent,
                &self.delta_multiple_scattering_texture,
            ));
            block_on(Self::dump_texture(
                format!("multiple-scattering-{}", scattering_order),
                lambdas,
                gpu,
                self.scattering_extent,
                &self.scattering_texture,
            ));
        }

        Ok(())
    }

    async fn dump_texture(
        prefix: String,
        lambdas: [f64; 4],
        gpu: &mut GPU,
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
        Self::show_range(&floats, &prefix);

        let (p0, p1) = Self::split_pixels(&floats, extent);
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

            assert!(r1 >= 0.0 && r1 <= 1.0);
            assert!(g1 >= 0.0 && g1 <= 1.0);
            assert!(b1 >= 0.0 && b1 <= 1.0);
            assert!(a1 >= 0.0 && a1 <= 1.0);

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

    async fn update_cache(&self, gpu: &mut GPU) -> Fallible<()> {
        let _ = fs::create_dir(CACHE_DIR);

        let transmittance_buf_size =
            u64::from(self.transmittance_extent.width * self.transmittance_extent.height * 16);
        let irradiance_buf_size =
            u64::from(self.irradiance_extent.width * self.irradiance_extent.height * 16);
        let scattering_buf_size = u64::from(
            self.scattering_extent.width
                * self.scattering_extent.height
                * self.scattering_extent.depth
                * 16,
        );

        let transmittance_buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("atmosphere-cache-download-transmittance-buffer"),
            size: transmittance_buf_size,
            usage: wgpu::BufferUsage::all(),
            mapped_at_creation: false,
        });
        let irradiance_buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("atmosphere-cache-download-irradiance-buffer"),
            size: irradiance_buf_size,
            usage: wgpu::BufferUsage::all(),
            mapped_at_creation: false,
        });
        let scattering_buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("atmosphere-cache-download-scatter-buffer"),
            size: scattering_buf_size,
            usage: wgpu::BufferUsage::all(),
            mapped_at_creation: false,
        });
        let single_mie_scattering_buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("atmosphere-cache-download-single-mie-scatter-buffer"),
            size: scattering_buf_size,
            usage: wgpu::BufferUsage::all(),
            mapped_at_creation: false,
        });

        fn mk_copy(
            encoder: &mut wgpu::CommandEncoder,
            texture: &wgpu::Texture,
            buffer: &wgpu::Buffer,
            extent: wgpu::Extent3d,
        ) {
            encoder.copy_texture_to_buffer(
                wgpu::TextureCopyView {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                },
                wgpu::BufferCopyView {
                    buffer,
                    layout: wgpu::TextureDataLayout {
                        offset: 0,
                        bytes_per_row: extent.width * 16,
                        rows_per_image: extent.height,
                    },
                },
                extent,
            );
        }
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("atmosphere-cache-download-command-encoder"),
            });
        mk_copy(
            &mut encoder,
            &self.transmittance_texture,
            &transmittance_buffer,
            self.transmittance_extent,
        );
        mk_copy(
            &mut encoder,
            &self.irradiance_texture,
            &irradiance_buffer,
            self.irradiance_extent,
        );
        mk_copy(
            &mut encoder,
            &self.scattering_texture,
            &scattering_buffer,
            self.scattering_extent,
        );
        mk_copy(
            &mut encoder,
            &self.single_mie_scattering_texture,
            &single_mie_scattering_buffer,
            self.scattering_extent,
        );
        gpu.queue_mut().submit(vec![encoder.finish()]);
        gpu.device().poll(wgpu::Maintain::Wait);

        // let reader = staging_buffer.slice(..).map_async(wgpu::MapMode::Read);
        // gpu.device().poll(wgpu::Maintain::Wait);
        // reader.await.unwrap();

        let transmittance_reader = transmittance_buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read);
        let irradiance_reader = irradiance_buffer.slice(..).map_async(wgpu::MapMode::Read);
        let scatter_reader = scattering_buffer.slice(..).map_async(wgpu::MapMode::Read);
        let single_mie_scatter_reader = single_mie_scattering_buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read);
        gpu.device().poll(wgpu::Maintain::Wait);
        transmittance_reader.await?;
        irradiance_reader.await?;
        scatter_reader.await?;
        single_mie_scatter_reader.await?;
        fs::write(
            &format!("{}/solar_transmittance.wgpu.bin", CACHE_DIR),
            &transmittance_buffer.slice(..).get_mapped_range().to_owned(),
        )?;
        fs::write(
            &format!("{}/solar_irradiance.wgpu.bin", CACHE_DIR),
            &irradiance_buffer.slice(..).get_mapped_range().to_owned(),
        )?;
        fs::write(
            &format!("{}/solar_scattering.wgpu.bin", CACHE_DIR),
            &scattering_buffer.slice(..).get_mapped_range().to_owned(),
        )?;
        fs::write(
            &format!("{}/solar_single_mie_scattering.wgpu.bin", CACHE_DIR),
            &single_mie_scattering_buffer
                .slice(..)
                .get_mapped_range()
                .to_owned(),
        )?;

        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    fn load_cache(&self, gpu: &mut GPU) -> Fallible<()> {
        bail!("TODO: no atmosphere cache on wasm32");
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn load_cache(&self, gpu: &mut GPU) -> Fallible<()> {
        use memmap::MmapOptions;

        let transmittance_path = format!("{}/solar_transmittance.wgpu.bin", CACHE_DIR);
        let irradiance_path = format!("{}/solar_irradiance.wgpu.bin", CACHE_DIR);
        let scattering_path = format!("{}/solar_scattering.wgpu.bin", CACHE_DIR);
        let mie_scattering_path = format!("{}/solar_single_mie_scattering.wgpu.bin", CACHE_DIR);

        if let Ok(var) = env::var("NITROGEN_DROP_CACHE") {
            if var == "1" {
                println!("Dropping atmosphere cache...");
                fs::remove_file(transmittance_path).ok();
                fs::remove_file(irradiance_path).ok();
                fs::remove_file(scattering_path).ok();
                fs::remove_file(mie_scattering_path).ok();
                bail!("dropped cache");
            }
        }

        let transmittance_fp = fs::File::open(&transmittance_path)?;
        let irradiance_fp = fs::File::open(&irradiance_path)?;
        let scattering_fp = fs::File::open(&scattering_path)?;
        let single_mie_scattering_fp = fs::File::open(&mie_scattering_path)?;

        let transmittance_map = unsafe { MmapOptions::new().map(&transmittance_fp) }?;
        let transmittance_buffer = gpu.push_buffer(
            "atmosphere-transmittance-file-upload-buffer",
            &transmittance_map,
            wgpu::BufferUsage::all(),
        );

        let irradiance_map = unsafe { MmapOptions::new().map(&irradiance_fp) }?;
        let irradiance_buffer = gpu.push_buffer(
            "atmosphere-irradiance-file-upload-buffer",
            &irradiance_map,
            wgpu::BufferUsage::all(),
        );

        let scattering_map = unsafe { MmapOptions::new().map(&scattering_fp) }?;
        let scattering_buffer = gpu.push_buffer(
            "atmosphere-scattering-file-upload-buffer",
            &scattering_map,
            wgpu::BufferUsage::all(),
        );

        let single_mie_scattering_map =
            unsafe { MmapOptions::new().map(&single_mie_scattering_fp) }?;
        let single_mie_scattering_buffer = gpu.push_buffer(
            "atmosphere-single-mie-scattering-file-upload-buffer",
            &single_mie_scattering_map,
            wgpu::BufferUsage::all(),
        );

        fn mk_copy(
            encoder: &mut wgpu::CommandEncoder,
            buffer: &wgpu::Buffer,
            texture: &wgpu::Texture,
            extent: wgpu::Extent3d,
        ) {
            encoder.copy_buffer_to_texture(
                wgpu::BufferCopyView {
                    buffer,
                    layout: wgpu::TextureDataLayout {
                        offset: 0,
                        bytes_per_row: extent.width * 16,
                        rows_per_image: extent.height,
                    },
                },
                wgpu::TextureCopyView {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                },
                extent,
            );
        }
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("atmosphere-texturize-command-encoder"),
            });
        mk_copy(
            &mut encoder,
            &transmittance_buffer,
            &self.transmittance_texture,
            self.transmittance_extent,
        );
        mk_copy(
            &mut encoder,
            &irradiance_buffer,
            &self.irradiance_texture,
            self.irradiance_extent,
        );
        mk_copy(
            &mut encoder,
            &scattering_buffer,
            &self.scattering_texture,
            self.scattering_extent,
        );
        mk_copy(
            &mut encoder,
            &single_mie_scattering_buffer,
            &self.single_mie_scattering_texture,
            self.scattering_extent,
        );
        gpu.queue_mut().submit(vec![encoder.finish()]);
        gpu.device().poll(wgpu::Maintain::Wait);

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_create() -> Fallible<()> {
        let input = input::InputSystem::new(vec![])?;
        let mut gpu = gpu::GPU::new(&input, Default::default())?;
        let precompute_start = Instant::now();
        let (
            _atmosphere_params_buffer,
            _transmittance_texture,
            _irradiance_texture,
            _scattering_texture,
            _single_mie_scattering_texture,
        ) = Precompute::precompute(40, 4, &mut gpu)?;
        let precompute_time = precompute_start.elapsed();
        println!(
            "AtmosphereBuffers::precompute timing: {}.{}ms",
            precompute_time.as_secs() * 1000 + u64::from(precompute_time.subsec_millis()),
            precompute_time.subsec_micros()
        );
        Ok(())
    }
}
