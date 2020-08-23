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
mod font_interface;
mod glyph_frame;

pub use crate::{font_interface::FontInterface, glyph_frame::GlyphFrame};

use failure::Fallible;
use gpu::GPU;
use image::{ImageBuffer, Luma};

pub const ROW_ALIGNMENT: u32 = 256;

pub fn upload_texture_luma(
    name: &str,
    image_buf: ImageBuffer<Luma<u8>, Vec<u8>>,
    gpu: &mut GPU,
) -> Fallible<wgpu::TextureView> {
    let image_dim = image_buf.dimensions();
    let extent = wgpu::Extent3d {
        width: image_dim.0,
        height: image_dim.1,
        depth: 1,
    };
    assert_eq!(extent.width % 256, 0);
    let image_data = image_buf.into_raw();

    let transfer_buffer = gpu.push_buffer(
        "glyph-cache-transfer-buffer",
        &image_data,
        wgpu::BufferUsage::all(),
    );
    let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("glyph-cache-texture"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsage::all(),
    });
    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("glyph-cache-command-encoder"),
        });
    encoder.copy_buffer_to_texture(
        wgpu::BufferCopyView {
            buffer: &transfer_buffer,
            layout: wgpu::TextureDataLayout {
                offset: 0,
                bytes_per_row: extent.width,
                rows_per_image: extent.height,
            },
        },
        wgpu::TextureCopyView {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
        },
        extent,
    );
    gpu.queue_mut().submit(vec![encoder.finish()]);

    // FIXME: we need to track usage of this... it should only be startup.
    //        If so, can we aggregate these into a single wait or something?
    gpu.device().poll(wgpu::Maintain::Wait);

    let texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some(name),
        format: None,
        dimension: None,
        aspect: wgpu::TextureAspect::All,
        base_mip_level: 0,
        level_count: None,
        base_array_layer: 0,
        array_layer_count: None,
    });

    Ok(texture_view)
}
