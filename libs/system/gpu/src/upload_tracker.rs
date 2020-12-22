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
use std::{mem, sync::Arc};

#[derive(Debug)]
pub struct CopyBufferToTextureDescriptor {
    source: wgpu::Buffer,
    target: Arc<Box<wgpu::Texture>>,
    target_extent: wgpu::Extent3d,
    target_element_size: u32,
    target_array_layer: u32,
    array_layer_count: u32,
}

impl CopyBufferToTextureDescriptor {
    pub fn new(
        source: wgpu::Buffer,
        target: Arc<Box<wgpu::Texture>>,
        target_extent: wgpu::Extent3d,
        target_element_size: u32,
        target_array_layer: u32,
        array_layer_count: u32,
    ) -> Self {
        Self {
            source,
            target,
            target_extent,
            target_element_size,
            target_array_layer,
            array_layer_count,
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
pub struct UploadTracker {
    b2b_uploads: Vec<CopyBufferToBufferDescriptor>,
    b2t_uploads: Vec<CopyBufferToTextureDescriptor>,
    t2t_uploads: Vec<CopyTextureToTextureDescriptor>,
}

impl Default for UploadTracker {
    fn default() -> Self {
        Self {
            b2b_uploads: Vec::new(),
            b2t_uploads: Vec::new(),
            t2t_uploads: Vec::new(),
        }
    }
}

impl UploadTracker {
    pub fn reset(&mut self) {
        self.b2b_uploads.clear();
        self.b2t_uploads.clear();
        self.t2t_uploads.clear();
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

    pub fn upload_to_texture(
        &mut self,
        source: wgpu::Buffer,
        target: Arc<Box<wgpu::Texture>>,
        target_extent: wgpu::Extent3d,
        target_format: wgpu::TextureFormat,
        target_array_layer: u32,
        array_layer_count: u32,
    ) {
        let target_element_size = texture_format_size(target_format);
        assert_eq!(
            target_extent.width * target_element_size % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT,
            0
        );
        self.b2t_uploads.push(CopyBufferToTextureDescriptor::new(
            source,
            target,
            target_extent,
            target_element_size,
            target_array_layer,
            array_layer_count,
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

        for desc in self.b2t_uploads.drain(..) {
            encoder.copy_buffer_to_texture(
                wgpu::BufferCopyView {
                    buffer: &desc.source,
                    layout: wgpu::TextureDataLayout {
                        offset: 0,
                        bytes_per_row: desc.target_extent.width * desc.target_element_size,
                        rows_per_image: desc.target_extent.height,
                    },
                },
                wgpu::TextureCopyView {
                    texture: &desc.target,
                    mip_level: 0, // TODO: need to scale extent appropriately
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: desc.target_array_layer,
                    },
                },
                wgpu::Extent3d {
                    width: desc.target_extent.width,
                    height: desc.target_extent.height,
                    depth: desc.array_layer_count,
                },
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
    }
}

pub fn texture_format_component_type(
    texture_format: wgpu::TextureFormat,
) -> wgpu::TextureComponentType {
    match texture_format {
        wgpu::TextureFormat::R8Unorm => wgpu::TextureComponentType::Uint,
        wgpu::TextureFormat::R8Snorm => wgpu::TextureComponentType::Sint,
        wgpu::TextureFormat::R8Uint => wgpu::TextureComponentType::Uint,
        wgpu::TextureFormat::R8Sint => wgpu::TextureComponentType::Sint,
        wgpu::TextureFormat::R16Uint => wgpu::TextureComponentType::Uint,
        wgpu::TextureFormat::R16Sint => wgpu::TextureComponentType::Sint,
        wgpu::TextureFormat::R16Float => wgpu::TextureComponentType::Float,
        wgpu::TextureFormat::Rg8Unorm => wgpu::TextureComponentType::Uint,
        wgpu::TextureFormat::Rg8Snorm => wgpu::TextureComponentType::Sint,
        wgpu::TextureFormat::Rg8Uint => wgpu::TextureComponentType::Uint,
        wgpu::TextureFormat::Rg8Sint => wgpu::TextureComponentType::Sint,
        wgpu::TextureFormat::R32Uint => wgpu::TextureComponentType::Uint,
        wgpu::TextureFormat::R32Sint => wgpu::TextureComponentType::Sint,
        wgpu::TextureFormat::R32Float => wgpu::TextureComponentType::Float,
        wgpu::TextureFormat::Rg16Uint => wgpu::TextureComponentType::Uint,
        wgpu::TextureFormat::Rg16Sint => wgpu::TextureComponentType::Sint,
        wgpu::TextureFormat::Rg16Float => wgpu::TextureComponentType::Float,
        wgpu::TextureFormat::Rgba8Unorm => wgpu::TextureComponentType::Uint,
        wgpu::TextureFormat::Rgba8UnormSrgb => wgpu::TextureComponentType::Uint,
        wgpu::TextureFormat::Rgba8Snorm => wgpu::TextureComponentType::Sint,
        wgpu::TextureFormat::Rgba8Uint => wgpu::TextureComponentType::Uint,
        wgpu::TextureFormat::Rgba8Sint => wgpu::TextureComponentType::Sint,
        wgpu::TextureFormat::Bgra8Unorm => wgpu::TextureComponentType::Uint,
        wgpu::TextureFormat::Bgra8UnormSrgb => wgpu::TextureComponentType::Uint,
        wgpu::TextureFormat::Rgb10a2Unorm => wgpu::TextureComponentType::Uint,
        wgpu::TextureFormat::Rg11b10Float => wgpu::TextureComponentType::Float,
        wgpu::TextureFormat::Rg32Uint => wgpu::TextureComponentType::Uint,
        wgpu::TextureFormat::Rg32Sint => wgpu::TextureComponentType::Sint,
        wgpu::TextureFormat::Rg32Float => wgpu::TextureComponentType::Float,
        wgpu::TextureFormat::Rgba16Uint => wgpu::TextureComponentType::Uint,
        wgpu::TextureFormat::Rgba16Sint => wgpu::TextureComponentType::Sint,
        wgpu::TextureFormat::Rgba16Float => wgpu::TextureComponentType::Float,
        wgpu::TextureFormat::Rgba32Uint => wgpu::TextureComponentType::Uint,
        wgpu::TextureFormat::Rgba32Sint => wgpu::TextureComponentType::Sint,
        wgpu::TextureFormat::Rgba32Float => wgpu::TextureComponentType::Float,
        wgpu::TextureFormat::Depth32Float => wgpu::TextureComponentType::Float,
        wgpu::TextureFormat::Depth24Plus => wgpu::TextureComponentType::Uint,
        wgpu::TextureFormat::Depth24PlusStencil8 => wgpu::TextureComponentType::Uint,
        wgpu::TextureFormat::Bc1RgbaUnorm => wgpu::TextureComponentType::Uint,
        wgpu::TextureFormat::Bc1RgbaUnormSrgb => wgpu::TextureComponentType::Uint,
        wgpu::TextureFormat::Bc2RgbaUnorm => wgpu::TextureComponentType::Uint,
        wgpu::TextureFormat::Bc2RgbaUnormSrgb => wgpu::TextureComponentType::Uint,
        wgpu::TextureFormat::Bc3RgbaUnorm => wgpu::TextureComponentType::Uint,
        wgpu::TextureFormat::Bc3RgbaUnormSrgb => wgpu::TextureComponentType::Uint,
        wgpu::TextureFormat::Bc4RUnorm => wgpu::TextureComponentType::Uint,
        wgpu::TextureFormat::Bc4RSnorm => wgpu::TextureComponentType::Sint,
        wgpu::TextureFormat::Bc5RgUnorm => wgpu::TextureComponentType::Uint,
        wgpu::TextureFormat::Bc5RgSnorm => wgpu::TextureComponentType::Sint,
        wgpu::TextureFormat::Bc6hRgbUfloat => wgpu::TextureComponentType::Float,
        wgpu::TextureFormat::Bc6hRgbSfloat => wgpu::TextureComponentType::Float,
        wgpu::TextureFormat::Bc7RgbaUnorm => wgpu::TextureComponentType::Uint,
        wgpu::TextureFormat::Bc7RgbaUnormSrgb => wgpu::TextureComponentType::Uint,
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
    }
}
