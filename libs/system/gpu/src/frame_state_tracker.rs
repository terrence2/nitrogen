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
use std::sync::Arc;

pub struct CopyBufferToTextureDescriptor {
    source: wgpu::Buffer,
    target: Arc<Box<wgpu::Texture>>,
    target_extent: wgpu::Extent3d,
    target_element_size: u32,
    target_array_layer: u32,
}

impl CopyBufferToTextureDescriptor {
    pub fn new(
        source: wgpu::Buffer,
        target: Arc<Box<wgpu::Texture>>,
        target_extent: wgpu::Extent3d,
        target_element_size: u32,
        target_array_layer: u32,
    ) -> Self {
        Self {
            source,
            target,
            target_extent,
            target_element_size,
            target_array_layer,
        }
    }
}

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
}

// Note: still quite limited; just precompute without dependencies.
pub struct FrameStateTracker {
    b2b_uploads: Vec<CopyBufferToBufferDescriptor>,
    b2t_uploads: Vec<CopyBufferToTextureDescriptor>,
}

impl Default for FrameStateTracker {
    fn default() -> Self {
        Self {
            b2b_uploads: Vec::new(),
            b2t_uploads: Vec::new(),
        }
    }
}

impl FrameStateTracker {
    pub fn reset(&mut self) {
        self.b2b_uploads.clear();
        self.b2t_uploads.clear();
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

    pub fn upload_to_texture(
        &mut self,
        source: wgpu::Buffer,
        target: Arc<Box<wgpu::Texture>>,
        target_extent: wgpu::Extent3d,
        target_format: wgpu::TextureFormat,
        target_array_layer: u32,
    ) {
        self.b2t_uploads.push(CopyBufferToTextureDescriptor::new(
            source,
            target,
            target_extent,
            texture_format_size(target_format),
            target_array_layer,
        ));
    }

    pub fn dispatch_uploads(&self, encoder: &mut wgpu::CommandEncoder) {
        for desc in self.b2b_uploads.iter() {
            encoder.copy_buffer_to_buffer(
                &desc.source,
                desc.source_offset,
                &desc.destination,
                desc.destination_offset,
                desc.copy_size,
            );
        }

        for desc in self.b2t_uploads.iter() {
            encoder.copy_buffer_to_texture(
                wgpu::BufferCopyView {
                    buffer: &desc.source,
                    offset: 0,
                    bytes_per_row: desc.target_extent.width * desc.target_element_size,
                    rows_per_image: desc.target_extent.height,
                },
                wgpu::TextureCopyView {
                    texture: &desc.target,
                    mip_level: 0, // TODO: need to scale extent appropriately
                    array_layer: desc.target_array_layer,
                    origin: wgpu::Origin3d::ZERO,
                },
                desc.target_extent,
            );
        }
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
    }
}
