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
use crate::earth_consts::{EarthParameters, RGB_LAMBDAS};
use anyhow::Result;
use gpu::{texture_format_size, Gpu};
use std::{fs, path::Path};

pub const TRANSMITTANCE_EXTENT: wgpu::Extent3d = wgpu::Extent3d {
    width: 256,
    height: 64,
    depth: 1,
};

const SCATTERING_TEXTURE_R_SIZE: u32 = 32;
const SCATTERING_TEXTURE_MU_SIZE: u32 = 128;
const SCATTERING_TEXTURE_MU_S_SIZE: u32 = 32;
const SCATTERING_TEXTURE_NU_SIZE: u32 = 8;
pub const SCATTERING_EXTENT: wgpu::Extent3d = wgpu::Extent3d {
    width: SCATTERING_TEXTURE_NU_SIZE * SCATTERING_TEXTURE_MU_S_SIZE,
    height: SCATTERING_TEXTURE_MU_SIZE,
    depth: SCATTERING_TEXTURE_R_SIZE,
};

pub const IRRADIANCE_EXTENT: wgpu::Extent3d = wgpu::Extent3d {
    width: 64,
    height: 16,
    depth: 1,
};

const TRANSMITTANCE_TABLE: &[u8] = include_bytes!("../tables/solar_transmittance.wgpu.bin");
const IRRADIANCE_TABLE: &[u8] = include_bytes!("../tables/solar_irradiance.wgpu.bin");
const SCATTERING_TABLE: &[u8] = include_bytes!("../tables/solar_scattering.wgpu.bin");
const SINGLE_MIE_SCATTERING_TABLE: &[u8] =
    include_bytes!("../tables/solar_single_mie_scattering.wgpu.bin");

pub struct TableHelpers;

impl TableHelpers {
    pub fn initial_atmosphere_parameters(gpu: &Gpu) -> wgpu::Buffer {
        let params = EarthParameters::new();
        let mut srgb_atmosphere = params.sample(RGB_LAMBDAS);
        srgb_atmosphere.ground_albedo = [0f32, 0f32, 0.04f32, 0f32];
        gpu.push_data(
            "atmosphere-srgb-params-buffer",
            &srgb_atmosphere,
            wgpu::BufferUsage::UNIFORM,
        )
    }

    pub fn initial_textures(
        gpu: &mut Gpu,
    ) -> Result<(wgpu::Texture, wgpu::Texture, wgpu::Texture, wgpu::Texture)> {
        let transmittance_buffer = gpu.push_buffer(
            "atmosphere-transmittance-file-upload-buffer",
            TRANSMITTANCE_TABLE,
            wgpu::BufferUsage::all(),
        );
        let irradiance_buffer = gpu.push_buffer(
            "atmosphere-irradiance-file-upload-buffer",
            IRRADIANCE_TABLE,
            wgpu::BufferUsage::all(),
        );

        let scattering_buffer = gpu.push_buffer(
            "atmosphere-scattering-file-upload-buffer",
            SCATTERING_TABLE,
            wgpu::BufferUsage::all(),
        );
        let single_mie_scattering_buffer = gpu.push_buffer(
            "atmosphere-single-mie-scattering-file-upload-buffer",
            SINGLE_MIE_SCATTERING_TABLE,
            wgpu::BufferUsage::all(),
        );

        let transmittance_texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("atmosphere-transmittance-texture"),
            size: TRANSMITTANCE_EXTENT,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsage::all(),
        });
        let scattering_texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("atmosphere-scattering-texture"),
            size: SCATTERING_EXTENT,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsage::all(),
        });
        let single_mie_scattering_texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("atmosphere-single-mie-scattering-texture"),
            size: SCATTERING_EXTENT,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsage::all(),
        });
        let irradiance_texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("atmosphere-irradiance-texture"),
            size: IRRADIANCE_EXTENT,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsage::all(),
        });

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
                        bytes_per_row: extent.width
                            * texture_format_size(wgpu::TextureFormat::Rgba32Float),
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
            &transmittance_texture,
            TRANSMITTANCE_EXTENT,
        );
        mk_copy(
            &mut encoder,
            &irradiance_buffer,
            &irradiance_texture,
            IRRADIANCE_EXTENT,
        );
        mk_copy(
            &mut encoder,
            &scattering_buffer,
            &scattering_texture,
            SCATTERING_EXTENT,
        );
        mk_copy(
            &mut encoder,
            &single_mie_scattering_buffer,
            &single_mie_scattering_texture,
            SCATTERING_EXTENT,
        );
        gpu.queue_mut().submit(vec![encoder.finish()]);
        gpu.device().poll(wgpu::Maintain::Wait);

        Ok((
            transmittance_texture,
            irradiance_texture,
            scattering_texture,
            single_mie_scattering_texture,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn write_textures(
        transmittance_texture: &wgpu::Texture,
        transmittance_path: &Path,
        irradiance_texture: &wgpu::Texture,
        irradiance_path: &Path,
        scattering_texture: &wgpu::Texture,
        scattering_path: &Path,
        single_mie_scattering_texture: &wgpu::Texture,
        single_mie_scattering_path: &Path,
        gpu: &mut Gpu,
    ) -> Result<()> {
        let transmittance_buf_size =
            u64::from(TRANSMITTANCE_EXTENT.width * TRANSMITTANCE_EXTENT.height * 16);
        let irradiance_buf_size =
            u64::from(IRRADIANCE_EXTENT.width * IRRADIANCE_EXTENT.height * 16);
        let scattering_buf_size = u64::from(
            SCATTERING_EXTENT.width * SCATTERING_EXTENT.height * SCATTERING_EXTENT.depth * 16,
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
            transmittance_texture,
            &transmittance_buffer,
            TRANSMITTANCE_EXTENT,
        );
        mk_copy(
            &mut encoder,
            irradiance_texture,
            &irradiance_buffer,
            IRRADIANCE_EXTENT,
        );
        mk_copy(
            &mut encoder,
            scattering_texture,
            &scattering_buffer,
            SCATTERING_EXTENT,
        );
        mk_copy(
            &mut encoder,
            single_mie_scattering_texture,
            &single_mie_scattering_buffer,
            SCATTERING_EXTENT,
        );
        gpu.queue_mut().submit(vec![encoder.finish()]);
        gpu.device().poll(wgpu::Maintain::Wait);

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
            transmittance_path,
            &transmittance_buffer.slice(..).get_mapped_range().to_owned(),
        )?;
        fs::write(
            irradiance_path,
            &irradiance_buffer.slice(..).get_mapped_range().to_owned(),
        )?;
        fs::write(
            scattering_path,
            &scattering_buffer.slice(..).get_mapped_range().to_owned(),
        )?;
        fs::write(
            single_mie_scattering_path,
            &single_mie_scattering_buffer
                .slice(..)
                .get_mapped_range()
                .to_owned(),
        )?;

        Ok(())
    }
}
