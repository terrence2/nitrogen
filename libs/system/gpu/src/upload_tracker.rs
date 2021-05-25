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
use crate::Gpu;
use std::{mem, sync::Arc};

#[derive(Debug)]
pub struct OwnedBufferCopyView {
    pub buffer: wgpu::Buffer,
    pub layout: wgpu::TextureDataLayout,
}

#[derive(Debug)]
pub struct ArcTextureCopyView {
    pub texture: Arc<Box<wgpu::Texture>>,
    pub mip_level: u32,
    pub origin: wgpu::Origin3d,
}

#[derive(Debug)]
pub struct CopyOwnedBufferToArcTextureDescriptor {
    buffer: OwnedBufferCopyView,
    texture: ArcTextureCopyView,
    extent: wgpu::Extent3d,
}

impl CopyOwnedBufferToArcTextureDescriptor {
    pub fn new(
        buffer: OwnedBufferCopyView,
        texture: ArcTextureCopyView,
        extent: wgpu::Extent3d,
    ) -> Self {
        Self {
            buffer,
            texture,
            extent,
        }
    }
}

#[derive(Debug)]
pub struct CopyBufferToBufferDescriptor {
    source: wgpu::Buffer,
    source_offset: wgpu::BufferAddress,
    destination: Arc<Box<wgpu::Buffer>>,
    destination_offset: wgpu::BufferAddress,
    copy_size: wgpu::BufferAddress,
}

impl CopyBufferToBufferDescriptor {
    pub fn new(
        source: wgpu::Buffer,
        destination: Arc<Box<wgpu::Buffer>>,
        copy_size: wgpu::BufferAddress,
    ) -> Self {
        Self {
            source,
            source_offset: 0,
            destination,
            destination_offset: 0,
            copy_size,
        }
    }

    pub fn new_raw(
        source: wgpu::Buffer,
        source_offset: wgpu::BufferAddress,
        destination: Arc<Box<wgpu::Buffer>>,
        destination_offset: wgpu::BufferAddress,
        copy_size: wgpu::BufferAddress,
    ) -> Self {
        Self {
            source,
            source_offset,
            destination,
            destination_offset,
            copy_size,
        }
    }
}

#[derive(Debug)]
pub struct CopyTextureToTextureDescriptor {
    source: Arc<Box<wgpu::Texture>>,
    source_layer: u32,
    target: Arc<Box<wgpu::Texture>>,
    target_layer: u32,
    size: wgpu::Extent3d,
}

impl CopyTextureToTextureDescriptor {
    pub fn new(
        source: Arc<Box<wgpu::Texture>>,
        source_layer: u32,
        target: Arc<Box<wgpu::Texture>>,
        target_layer: u32,
        size: wgpu::Extent3d,
    ) -> Self {
        Self {
            source,
            source_layer,
            target,
            target_layer,
            size,
        }
    }
}

// Note: still quite limited; just precompute without dependencies.
#[derive(Debug, Default)]
pub struct UploadTracker {
    b2b_uploads: Vec<CopyBufferToBufferDescriptor>,
    t2t_uploads: Vec<CopyTextureToTextureDescriptor>,
    copy_owned_buffer_to_arc_texture: Vec<CopyOwnedBufferToArcTextureDescriptor>,
}

impl UploadTracker {
    pub fn new() -> Self {
        Self {
            b2b_uploads: vec![],
            t2t_uploads: vec![],
            copy_owned_buffer_to_arc_texture: vec![],
        }
    }

    pub fn upload(
        &mut self,
        source: wgpu::Buffer,
        destination: Arc<Box<wgpu::Buffer>>,
        copy_size: usize,
    ) {
        assert!(copy_size < wgpu::BufferAddress::MAX as usize);
        self.upload_ba(source, destination, copy_size as wgpu::BufferAddress);
    }

    pub fn upload_ba(
        &mut self,
        source: wgpu::Buffer,
        destination: Arc<Box<wgpu::Buffer>>,
        copy_size: wgpu::BufferAddress,
    ) {
        self.b2b_uploads.push(CopyBufferToBufferDescriptor::new(
            source,
            destination,
            copy_size,
        ));
    }

    pub fn upload_to_array_element<T: Sized>(
        &mut self,
        source: wgpu::Buffer,
        target_array: Arc<Box<wgpu::Buffer>>,
        array_offset: usize,
    ) {
        self.b2b_uploads.push(CopyBufferToBufferDescriptor::new_raw(
            source,
            0,
            target_array,
            (mem::size_of::<T>() * array_offset) as wgpu::BufferAddress,
            mem::size_of::<T>() as wgpu::BufferAddress,
        ));
    }

    pub fn copy_owned_buffer_to_arc_texture(
        &mut self,
        buffer: OwnedBufferCopyView,
        texture: ArcTextureCopyView,
        extent: wgpu::Extent3d,
    ) {
        self.copy_owned_buffer_to_arc_texture
            .push(CopyOwnedBufferToArcTextureDescriptor::new(
                buffer, texture, extent,
            ));
    }

    pub fn copy_texture_to_texture(
        &mut self,
        source: Arc<Box<wgpu::Texture>>,
        source_layer: u32,
        target: Arc<Box<wgpu::Texture>>,
        target_layer: u32,
        size: wgpu::Extent3d,
    ) {
        self.t2t_uploads.push(CopyTextureToTextureDescriptor::new(
            source,
            source_layer,
            target,
            target_layer,
            size,
        ));
    }

    pub fn dispatch_uploads_one_shot(self, gpu: &mut Gpu) {
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("upload-t2-info"),
            });
        self.dispatch_uploads(&mut encoder);
        gpu.queue_mut().submit(vec![encoder.finish()]);
    }

    pub fn dispatch_uploads(mut self, encoder: &mut wgpu::CommandEncoder) {
        for desc in self.b2b_uploads.drain(..) {
            encoder.copy_buffer_to_buffer(
                &desc.source,
                desc.source_offset,
                &desc.destination,
                desc.destination_offset,
                desc.copy_size,
            );
        }

        for desc in self.t2t_uploads.drain(..) {
            encoder.copy_texture_to_texture(
                wgpu::TextureCopyView {
                    texture: &desc.source,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: desc.source_layer,
                    },
                },
                wgpu::TextureCopyView {
                    texture: &desc.target,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: desc.target_layer,
                    },
                },
                wgpu::Extent3d {
                    width: desc.size.width,
                    height: desc.size.height,
                    depth: 1,
                },
            )
        }

        for desc in self.copy_owned_buffer_to_arc_texture.drain(..) {
            encoder.copy_buffer_to_texture(
                wgpu::BufferCopyView {
                    buffer: &desc.buffer.buffer,
                    layout: desc.buffer.layout,
                },
                wgpu::TextureCopyView {
                    texture: &desc.texture.texture,
                    mip_level: desc.texture.mip_level,
                    origin: desc.texture.origin,
                },
                desc.extent,
            );
        }
    }
}

pub fn texture_format_sample_type(texture_format: wgpu::TextureFormat) -> wgpu::TextureSampleType {
    match texture_format {
        wgpu::TextureFormat::R8Unorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::R8Snorm => wgpu::TextureSampleType::Sint,
        wgpu::TextureFormat::R8Uint => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::R8Sint => wgpu::TextureSampleType::Sint,
        wgpu::TextureFormat::R16Uint => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::R16Sint => wgpu::TextureSampleType::Sint,
        wgpu::TextureFormat::R16Float => wgpu::TextureSampleType::Float { filterable: true },
        wgpu::TextureFormat::Rg8Unorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Rg8Snorm => wgpu::TextureSampleType::Sint,
        wgpu::TextureFormat::Rg8Uint => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Rg8Sint => wgpu::TextureSampleType::Sint,
        wgpu::TextureFormat::R32Uint => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::R32Sint => wgpu::TextureSampleType::Sint,
        wgpu::TextureFormat::R32Float => wgpu::TextureSampleType::Float { filterable: true },
        wgpu::TextureFormat::Rg16Uint => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Rg16Sint => wgpu::TextureSampleType::Sint,
        wgpu::TextureFormat::Rg16Float => wgpu::TextureSampleType::Float { filterable: true },
        wgpu::TextureFormat::Rgba8Unorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Rgba8UnormSrgb => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Rgba8Snorm => wgpu::TextureSampleType::Sint,
        wgpu::TextureFormat::Rgba8Uint => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Rgba8Sint => wgpu::TextureSampleType::Sint,
        wgpu::TextureFormat::Bgra8Unorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Bgra8UnormSrgb => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Rgb10a2Unorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Rg11b10Float => wgpu::TextureSampleType::Float { filterable: true },
        wgpu::TextureFormat::Rg32Uint => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Rg32Sint => wgpu::TextureSampleType::Sint,
        wgpu::TextureFormat::Rg32Float => wgpu::TextureSampleType::Float { filterable: true },
        wgpu::TextureFormat::Rgba16Uint => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Rgba16Sint => wgpu::TextureSampleType::Sint,
        wgpu::TextureFormat::Rgba16Float => wgpu::TextureSampleType::Float { filterable: true },
        wgpu::TextureFormat::Rgba32Uint => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Rgba32Sint => wgpu::TextureSampleType::Sint,
        wgpu::TextureFormat::Rgba32Float => wgpu::TextureSampleType::Float { filterable: true },
        wgpu::TextureFormat::Depth32Float => wgpu::TextureSampleType::Depth,
        wgpu::TextureFormat::Depth24Plus => wgpu::TextureSampleType::Depth,
        wgpu::TextureFormat::Depth24PlusStencil8 => wgpu::TextureSampleType::Depth,
        // Compressed textures usable with `TEXTURE_COMPRESSION_BC` feature.
        wgpu::TextureFormat::Bc1RgbaUnorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Bc1RgbaUnormSrgb => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Bc2RgbaUnorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Bc2RgbaUnormSrgb => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Bc3RgbaUnorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Bc3RgbaUnormSrgb => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Bc4RUnorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Bc4RSnorm => wgpu::TextureSampleType::Sint,
        wgpu::TextureFormat::Bc5RgUnorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Bc5RgSnorm => wgpu::TextureSampleType::Sint,
        wgpu::TextureFormat::Bc6hRgbUfloat => wgpu::TextureSampleType::Float { filterable: true },
        wgpu::TextureFormat::Bc6hRgbSfloat => wgpu::TextureSampleType::Float { filterable: true },
        wgpu::TextureFormat::Bc7RgbaUnorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Bc7RgbaUnormSrgb => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Etc2RgbUnorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Etc2RgbUnormSrgb => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Etc2RgbA1Unorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Etc2RgbA1UnormSrgb => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Etc2RgbA8Unorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Etc2RgbA8UnormSrgb => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::EacRUnorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::EacRSnorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::EtcRgUnorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::EtcRgSnorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc4x4RgbaUnorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc4x4RgbaUnormSrgb => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc5x4RgbaUnorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc5x4RgbaUnormSrgb => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc5x5RgbaUnorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc5x5RgbaUnormSrgb => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc6x5RgbaUnorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc6x5RgbaUnormSrgb => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc6x6RgbaUnorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc6x6RgbaUnormSrgb => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc8x5RgbaUnorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc8x5RgbaUnormSrgb => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc8x6RgbaUnorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc8x6RgbaUnormSrgb => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc10x5RgbaUnorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc10x5RgbaUnormSrgb => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc10x6RgbaUnorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc10x6RgbaUnormSrgb => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc8x8RgbaUnorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc8x8RgbaUnormSrgb => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc10x8RgbaUnorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc10x8RgbaUnormSrgb => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc10x10RgbaUnorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc10x10RgbaUnormSrgb => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc12x10RgbaUnorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc12x10RgbaUnormSrgb => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc12x12RgbaUnorm => wgpu::TextureSampleType::Uint,
        wgpu::TextureFormat::Astc12x12RgbaUnormSrgb => wgpu::TextureSampleType::Uint,
    }
}

pub fn texture_format_size(texture_format: wgpu::TextureFormat) -> u32 {
    match texture_format {
        wgpu::TextureFormat::R8Unorm => 1,
        wgpu::TextureFormat::R8Snorm => 1,
        wgpu::TextureFormat::R8Uint => 1,
        wgpu::TextureFormat::R8Sint => 1,
        wgpu::TextureFormat::R16Uint => 2,
        wgpu::TextureFormat::R16Sint => 2,
        wgpu::TextureFormat::R16Float => 2,
        wgpu::TextureFormat::Rg8Unorm => 2,
        wgpu::TextureFormat::Rg8Snorm => 2,
        wgpu::TextureFormat::Rg8Uint => 2,
        wgpu::TextureFormat::Rg8Sint => 2,
        wgpu::TextureFormat::R32Uint => 4,
        wgpu::TextureFormat::R32Sint => 4,
        wgpu::TextureFormat::R32Float => 4,
        wgpu::TextureFormat::Rg16Uint => 4,
        wgpu::TextureFormat::Rg16Sint => 4,
        wgpu::TextureFormat::Rg16Float => 4,
        wgpu::TextureFormat::Rgba8Unorm => 4,
        wgpu::TextureFormat::Rgba8UnormSrgb => 4,
        wgpu::TextureFormat::Rgba8Snorm => 4,
        wgpu::TextureFormat::Rgba8Uint => 4,
        wgpu::TextureFormat::Rgba8Sint => 4,
        wgpu::TextureFormat::Bgra8Unorm => 4,
        wgpu::TextureFormat::Bgra8UnormSrgb => 4,
        wgpu::TextureFormat::Rgb10a2Unorm => 4,
        wgpu::TextureFormat::Rg11b10Float => 4,
        wgpu::TextureFormat::Rg32Uint => 8,
        wgpu::TextureFormat::Rg32Sint => 8,
        wgpu::TextureFormat::Rg32Float => 8,
        wgpu::TextureFormat::Rgba16Uint => 8,
        wgpu::TextureFormat::Rgba16Sint => 8,
        wgpu::TextureFormat::Rgba16Float => 8,
        wgpu::TextureFormat::Rgba32Uint => 16,
        wgpu::TextureFormat::Rgba32Sint => 16,
        wgpu::TextureFormat::Rgba32Float => 16,
        wgpu::TextureFormat::Depth32Float => 4,
        wgpu::TextureFormat::Depth24Plus => 4,
        wgpu::TextureFormat::Depth24PlusStencil8 => 4,
        wgpu::TextureFormat::Bc1RgbaUnorm => 4,
        wgpu::TextureFormat::Bc1RgbaUnormSrgb => 4,
        wgpu::TextureFormat::Bc2RgbaUnorm => 4,
        wgpu::TextureFormat::Bc2RgbaUnormSrgb => 4,
        wgpu::TextureFormat::Bc3RgbaUnorm => 4,
        wgpu::TextureFormat::Bc3RgbaUnormSrgb => 4,
        wgpu::TextureFormat::Bc4RUnorm => 1,
        wgpu::TextureFormat::Bc4RSnorm => 1,
        wgpu::TextureFormat::Bc5RgUnorm => 2,
        wgpu::TextureFormat::Bc5RgSnorm => 2,
        wgpu::TextureFormat::Bc6hRgbUfloat => 16,
        wgpu::TextureFormat::Bc6hRgbSfloat => 16,
        wgpu::TextureFormat::Bc7RgbaUnorm => 4,
        wgpu::TextureFormat::Bc7RgbaUnormSrgb => 4,
        wgpu::TextureFormat::Etc2RgbUnorm => 4,
        wgpu::TextureFormat::Etc2RgbUnormSrgb => 4,
        wgpu::TextureFormat::Etc2RgbA1Unorm => 1,
        wgpu::TextureFormat::Etc2RgbA1UnormSrgb => 1,
        wgpu::TextureFormat::Etc2RgbA8Unorm => 4,
        wgpu::TextureFormat::Etc2RgbA8UnormSrgb => 4,
        wgpu::TextureFormat::EacRUnorm => 1,
        wgpu::TextureFormat::EacRSnorm => 1,
        wgpu::TextureFormat::EtcRgUnorm => 2,
        wgpu::TextureFormat::EtcRgSnorm => 2,
        wgpu::TextureFormat::Astc4x4RgbaUnorm => 4,
        wgpu::TextureFormat::Astc4x4RgbaUnormSrgb => 4,
        wgpu::TextureFormat::Astc5x4RgbaUnorm => 4,
        wgpu::TextureFormat::Astc5x4RgbaUnormSrgb => 4,
        wgpu::TextureFormat::Astc5x5RgbaUnorm => 4,
        wgpu::TextureFormat::Astc5x5RgbaUnormSrgb => 4,
        wgpu::TextureFormat::Astc6x5RgbaUnorm => 4,
        wgpu::TextureFormat::Astc6x5RgbaUnormSrgb => 4,
        wgpu::TextureFormat::Astc6x6RgbaUnorm => 4,
        wgpu::TextureFormat::Astc6x6RgbaUnormSrgb => 4,
        wgpu::TextureFormat::Astc8x5RgbaUnorm => 4,
        wgpu::TextureFormat::Astc8x5RgbaUnormSrgb => 4,
        wgpu::TextureFormat::Astc8x6RgbaUnorm => 4,
        wgpu::TextureFormat::Astc8x6RgbaUnormSrgb => 4,
        wgpu::TextureFormat::Astc10x5RgbaUnorm => 4,
        wgpu::TextureFormat::Astc10x5RgbaUnormSrgb => 4,
        wgpu::TextureFormat::Astc10x6RgbaUnorm => 4,
        wgpu::TextureFormat::Astc10x6RgbaUnormSrgb => 4,
        wgpu::TextureFormat::Astc8x8RgbaUnorm => 4,
        wgpu::TextureFormat::Astc8x8RgbaUnormSrgb => 4,
        wgpu::TextureFormat::Astc10x8RgbaUnorm => 4,
        wgpu::TextureFormat::Astc10x8RgbaUnormSrgb => 4,
        wgpu::TextureFormat::Astc10x10RgbaUnorm => 4,
        wgpu::TextureFormat::Astc10x10RgbaUnormSrgb => 4,
        wgpu::TextureFormat::Astc12x10RgbaUnorm => 4,
        wgpu::TextureFormat::Astc12x10RgbaUnormSrgb => 4,
        wgpu::TextureFormat::Astc12x12RgbaUnorm => 4,
        wgpu::TextureFormat::Astc12x12RgbaUnormSrgb => 4,
    }
}
