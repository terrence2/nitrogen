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
mod frame_graph;
mod upload_tracker;

pub use crate::upload_tracker::{texture_format_size, UploadTracker};

// Note: re-export for use by FrameGraph when it is instantiated in other crates.
pub use wgpu;

use failure::{bail, err_msg, Fallible};
use futures::executor::block_on;
use input::InputSystem;
use log::{info, trace};
use std::{fs, mem, path::PathBuf, sync::Arc};
use tokio::runtime::Runtime;
use wgpu::util::DeviceExt;
use winit::dpi::PhysicalSize;
use zerocopy::{AsBytes, FromBytes};

#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Debug)]
pub struct DrawIndirectCommand {
    pub vertex_count: u32,
    pub instance_count: u32,
    pub first_vertex: u32,
    pub first_instance: u32,
}

pub struct GPUConfig {
    present_mode: wgpu::PresentMode,
}
impl Default for GPUConfig {
    fn default() -> Self {
        Self {
            present_mode: wgpu::PresentMode::Mailbox,
        }
    }
}

pub struct GPU {
    surface: wgpu::Surface,
    _adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    swap_chain: wgpu::SwapChain,
    depth_texture: wgpu::TextureView,

    config: GPUConfig,
    size: PhysicalSize,
}

impl GPU {
    pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
    pub const SCREEN_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

    pub fn aspect_ratio(&self) -> f64 {
        self.size.height.floor() / self.size.width.floor()
    }

    pub fn aspect_ratio_f32(&self) -> f32 {
        (self.size.height.floor() / self.size.width.floor()) as f32
    }

    pub fn physical_size(&self) -> PhysicalSize {
        self.size
    }

    pub fn new(input: &InputSystem, config: GPUConfig) -> Fallible<Self> {
        block_on(Self::new_async(input, config))
    }

    pub async fn new_async(input: &InputSystem, config: GPUConfig) -> Fallible<Self> {
        input.window().set_title("Nitrogen");
        let instance = wgpu::Instance::new(wgpu::BackendBit::PRIMARY);

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
            })
            .await
            .ok_or_else(|| err_msg("no suitable graphics adapter"))?;

        let surface = unsafe { instance.create_surface(input.window()) };

        let trace_path = PathBuf::from("api_tracing.txt");
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    features: adapter.features(),
                    limits: adapter.limits(),
                    // TODO: make this configurable?
                    shader_validation: true,
                },
                Some(&trace_path),
            )
            .await?;

        let size = input
            .window()
            .inner_size()
            .to_physical(input.window().hidpi_factor());
        let sc_desc = wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            format: Self::SCREEN_FORMAT,
            width: size.width.floor() as u32,
            height: size.height.floor() as u32,
            present_mode: config.present_mode,
        };
        let swap_chain = device.create_swap_chain(&surface, &sc_desc);
        let depth_texture = Self::create_depth_texture(&device, &sc_desc);

        Ok(Self {
            surface,
            _adapter: adapter,
            device,
            queue,
            swap_chain,
            depth_texture,
            config,
            size,
        })
    }

    fn create_depth_texture(
        device: &wgpu::Device,
        sc_desc: &wgpu::SwapChainDescriptor,
    ) -> wgpu::TextureView {
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth-texture"),
            size: wgpu::Extent3d {
                width: sc_desc.width,
                height: sc_desc.height,
                depth: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEPTH_FORMAT,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
        });
        depth_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("depth-texture-view"),
            format: Some(Self::DEPTH_FORMAT),
            dimension: Some(wgpu::TextureViewDimension::D2),
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        })
    }

    pub fn attachment_extent(&self) -> wgpu::Extent3d {
        wgpu::Extent3d {
            width: self.size.width.floor() as u32,
            height: self.size.height.floor() as u32,
            depth: 1,
        }
    }

    pub fn note_resize(&mut self, input: &InputSystem) {
        self.size = input
            .window()
            .inner_size()
            .to_physical(input.window().hidpi_factor());
        let sc_desc = wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            format: Self::SCREEN_FORMAT,
            width: self.size.width.floor() as u32,
            height: self.size.height.floor() as u32,
            present_mode: self.config.present_mode,
        };
        self.swap_chain = self.device.create_swap_chain(&self.surface, &sc_desc);
        self.depth_texture = Self::create_depth_texture(&self.device, &sc_desc);
    }

    pub fn get_next_framebuffer(&mut self) -> Fallible<Option<wgpu::SwapChainFrame>> {
        match self.swap_chain.get_current_frame() {
            Ok(frame) => Ok(Some(frame)),
            Err(wgpu::SwapChainError::Timeout) => bail!("Timeout: gpu is locked up"),
            Err(wgpu::SwapChainError::OutOfMemory) => bail!("OOM: gpu is out of memory"),
            Err(wgpu::SwapChainError::Lost) => bail!("Lost: our context wondered off"),
            Err(wgpu::SwapChainError::Outdated) => {
                info!("GPU: context outdated, recreating");
                Ok(None)
            }
        }
        //.map_err(|e| err_msg(format!("failed to get next swap chain image: {}", e)))?)
    }

    pub fn depth_attachment(&self) -> &wgpu::TextureView {
        &self.depth_texture
    }

    pub fn depth_stencil_attachment(&self) -> wgpu::RenderPassDepthStencilAttachmentDescriptor {
        wgpu::RenderPassDepthStencilAttachmentDescriptor {
            attachment: &self.depth_texture,
            depth_ops: Some(wgpu::Operations {
                // Note: clear to *behind* the plane so that our skybox raytrace pass can check
                // for pixels that have not yet been set.
                load: wgpu::LoadOp::Clear(-1f32),
                store: true,
            }),
            stencil_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(0),
                store: true,
            }),
        }
    }

    pub fn color_attachment(
        attachment: &wgpu::TextureView,
    ) -> wgpu::RenderPassColorAttachmentDescriptor {
        wgpu::RenderPassColorAttachmentDescriptor {
            attachment,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::RED),
                store: true,
            },
        }
    }

    pub fn maybe_push_buffer(
        &self,
        label: &'static str,
        data: &[u8],
        usage: wgpu::BufferUsage,
    ) -> Option<wgpu::Buffer> {
        if data.is_empty() {
            return None;
        }
        let size = data.len() as wgpu::BufferAddress;
        trace!("uploading {} with {} bytes", label, size);
        Some(
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: data,
                    usage,
                }),
        )
    }

    pub fn maybe_push_slice<T: AsBytes>(
        &self,
        label: &'static str,
        data: &[T],
        usage: wgpu::BufferUsage,
    ) -> Option<wgpu::Buffer> {
        if data.is_empty() {
            return None;
        }
        let size = (mem::size_of::<T>() * data.len()) as wgpu::BufferAddress;
        trace!("uploading {} with {} bytes", label, size);
        Some(
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: data.as_bytes(),
                    usage,
                }),
        )
    }

    pub fn push_buffer(
        &self,
        label: &'static str,
        data: &[u8],
        usage: wgpu::BufferUsage,
    ) -> wgpu::Buffer {
        self.maybe_push_buffer(label, data, usage)
            .expect("push non-empty buffer")
    }

    pub fn push_slice<T: AsBytes>(
        &self,
        label: &'static str,
        data: &[T],
        usage: wgpu::BufferUsage,
    ) -> wgpu::Buffer {
        self.maybe_push_slice(label, data, usage)
            .expect("push non-empty slice")
    }

    pub fn push_data<T: AsBytes>(
        &self,
        label: &'static str,
        data: &T,
        usage: wgpu::BufferUsage,
    ) -> wgpu::Buffer {
        let size = mem::size_of::<T>() as wgpu::BufferAddress;
        trace!("uploading {} with {} bytes", label, size);
        self.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: data.as_bytes(),
                usage,
            })
    }

    pub fn upload_slice_to<T: AsBytes>(
        &self,
        label: &'static str,
        data: &[T],
        target: Arc<Box<wgpu::Buffer>>,
        tracker: &mut UploadTracker,
    ) {
        if let Some(source) = self.maybe_push_slice(label, data, wgpu::BufferUsage::COPY_SRC) {
            tracker.upload(source, target, mem::size_of::<T>() * data.len());
        }
    }

    pub fn create_shader_module(&self, spirv: &[u8]) -> Fallible<wgpu::ShaderModule> {
        let spirv_words = wgpu::util::make_spirv(spirv);
        Ok(self.device.create_shader_module(spirv_words))
    }

    pub fn stride_for_row_size(size: u32) -> u32 {
        (size + wgpu::COPY_BYTES_PER_ROW_ALIGNMENT - 1) / wgpu::COPY_BYTES_PER_ROW_ALIGNMENT
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT
    }

    pub fn dump_texture(
        texture: &wgpu::Texture,
        extent: wgpu::Extent3d,
        format: wgpu::TextureFormat,
        async_rt: &mut Runtime,
        gpu: &mut GPU,
        callback: Box<
            dyn FnOnce(wgpu::Extent3d, wgpu::TextureFormat, Vec<u8>) + Send + Sync + 'static,
        >,
    ) -> Fallible<()> {
        let _ = fs::create_dir("__dump__");
        let sample_size = texture_format_size(format);
        let bytes_per_row = Self::stride_for_row_size(extent.width * sample_size);
        let buf_size = u64::from(bytes_per_row * extent.height);
        let download_buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("dump-download-buffer"),
            size: buf_size,
            usage: wgpu::BufferUsage::all(),
            mapped_at_creation: false,
        });
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("dump-download-command-encoder"),
            });
        println!(
            "dumping texture: fmt-size: {}, byes-per-row: {}, stride: {}",
            sample_size,
            extent.width * sample_size,
            bytes_per_row
        );
        encoder.copy_texture_to_buffer(
            wgpu::TextureCopyView {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            wgpu::BufferCopyView {
                buffer: &download_buffer,
                layout: wgpu::TextureDataLayout {
                    offset: 0,
                    bytes_per_row,
                    rows_per_image: extent.height,
                },
            },
            extent,
        );
        gpu.queue_mut().submit(vec![encoder.finish()]);
        gpu.device().poll(wgpu::Maintain::Wait);
        let reader = download_buffer.slice(..).map_async(wgpu::MapMode::Read);
        gpu.device().poll(wgpu::Maintain::Wait);
        async_rt.spawn(async move {
            reader.await.unwrap();
            let raw = download_buffer.slice(..).get_mapped_range().to_owned();
            callback(extent, format, raw);
        });
        Ok(())
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn device_mut(&mut self) -> &mut wgpu::Device {
        &mut self.device
    }

    pub fn queue_mut(&mut self) -> &mut wgpu::Queue {
        &mut self.queue
    }

    pub fn device_and_queue_mut(&mut self) -> (&mut wgpu::Device, &mut wgpu::Queue) {
        (&mut self.device, &mut self.queue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create() -> Fallible<()> {
        let input = InputSystem::new(vec![])?;
        let _gpu = GPU::new(&input, Default::default())?;
        Ok(())
    }
}
