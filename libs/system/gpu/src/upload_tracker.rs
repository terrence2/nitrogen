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
    pub layout: wgpu::ImageDataLayout,
}

#[derive(Debug)]
pub struct ArcTextureCopyView {
    pub texture: Arc<wgpu::Texture>,
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
    destination: Arc<wgpu::Buffer>,
    destination_offset: wgpu::BufferAddress,
    copy_size: wgpu::BufferAddress,
}

impl CopyBufferToBufferDescriptor {
    pub fn new(
        source: wgpu::Buffer,
        destination: Arc<wgpu::Buffer>,
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
        destination: Arc<wgpu::Buffer>,
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
    source: Arc<wgpu::Texture>,
    source_layer: u32,
    target: Arc<wgpu::Texture>,
    target_layer: u32,
    size: wgpu::Extent3d,
}

impl CopyTextureToTextureDescriptor {
    pub fn new(
        source: Arc<wgpu::Texture>,
        source_layer: u32,
        target: Arc<wgpu::Texture>,
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
        destination: Arc<wgpu::Buffer>,
        copy_size: usize,
    ) {
        assert!(copy_size < wgpu::BufferAddress::MAX as usize);
        self.upload_ba(source, destination, copy_size as wgpu::BufferAddress);
    }

    pub fn upload_ba(
        &mut self,
        source: wgpu::Buffer,
        destination: Arc<wgpu::Buffer>,
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
        target_array: Arc<wgpu::Buffer>,
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
        source: Arc<wgpu::Texture>,
        source_layer: u32,
        target: Arc<wgpu::Texture>,
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

    pub fn dispatch_uploads_until_empty(&mut self, encoder: &mut wgpu::CommandEncoder) {
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
                wgpu::ImageCopyTexture {
                    texture: &desc.source,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: desc.source_layer,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::ImageCopyTexture {
                    texture: &desc.target,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: desc.target_layer,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: desc.size.width,
                    height: desc.size.height,
                    depth_or_array_layers: 1,
                },
            )
        }

        for desc in self.copy_owned_buffer_to_arc_texture.drain(..) {
            encoder.copy_buffer_to_texture(
                wgpu::ImageCopyBuffer {
                    buffer: &desc.buffer.buffer,
                    layout: desc.buffer.layout,
                },
                wgpu::ImageCopyTexture {
                    texture: &desc.texture.texture,
                    mip_level: desc.texture.mip_level,
                    origin: desc.texture.origin,
                    aspect: wgpu::TextureAspect::All,
                },
                desc.extent,
            );
        }
    }

    pub fn dispatch_uploads(mut self, encoder: &mut wgpu::CommandEncoder) {
        self.dispatch_uploads_until_empty(encoder)
    }
}

pub fn texture_format_sample_type(texture_format: wgpu::TextureFormat) -> wgpu::TextureSampleType {
    let info = texture_format.describe();
    info.sample_type
}

pub fn texture_format_size(texture_format: wgpu::TextureFormat) -> u32 {
    let info = texture_format.describe();
    info.block_size as u32
}
