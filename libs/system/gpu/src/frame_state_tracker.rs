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
use crate::texture_format_size;
use std::sync::Arc;

pub struct CopyBufferToTextureDescriptor {
    pub(crate) source: wgpu::Buffer,
    pub(crate) target: wgpu::Texture,
    pub(crate) target_extent: wgpu::Extent3d,
    pub(crate) target_element_size: u32,
    pub(crate) target_array_layer: u32,
}

impl CopyBufferToTextureDescriptor {
    pub fn new(
        source: wgpu::Buffer,
        target: wgpu::Texture,
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
    pub(crate) source: wgpu::Buffer,
    pub(crate) source_offset: wgpu::BufferAddress,
    pub(crate) destination: Arc<Box<wgpu::Buffer>>,
    pub(crate) destination_offset: wgpu::BufferAddress,
    pub(crate) copy_size: wgpu::BufferAddress,
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
        target: wgpu::Texture,
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

    pub fn drain_b2b_uploads(&mut self) -> std::vec::Drain<CopyBufferToBufferDescriptor> {
        self.b2b_uploads.drain(..)
    }
}
