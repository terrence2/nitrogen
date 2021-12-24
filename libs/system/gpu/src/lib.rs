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
mod detail;
mod frame_graph;
mod upload_tracker;

pub use crate::{
    detail::{CpuDetailLevel, DetailLevelOpts, GpuDetailLevel},
    upload_tracker::{
        texture_format_sample_type, texture_format_size, ArcTextureCopyView, OwnedBufferCopyView,
        UploadTracker,
    },
};

// Note: re-export for use by FrameGraph when it is instantiated in other crates.
pub use wgpu;
pub use winit::dpi::{LogicalSize, PhysicalSize};

use anyhow::{anyhow, bail, Result};
use futures::executor::block_on;
use input::InputController;
use log::{info, trace};
use nitrous::{Interpreter, Value};
use nitrous_injector::{inject_nitrous_module, method, NitrousModule};
use parking_lot::RwLock;
use std::{fmt::Debug, fs, mem, path::PathBuf, sync::Arc};
use tokio::runtime::Runtime;
use wgpu::util::DeviceExt;
use window::{DisplayConfig, DisplayConfigChangeReceiver, Window};
use zerocopy::AsBytes;

#[derive(Debug)]
pub struct RenderConfig {
    present_mode: wgpu::PresentMode,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            present_mode: wgpu::PresentMode::Mailbox,
        }
    }
}

pub struct TestResources {
    pub window: Arc<RwLock<Window>>,
    pub gpu: Arc<RwLock<Gpu>>,
    pub async_rt: Runtime,
    pub interpreter: Interpreter,
    pub input: InputController,
}

/// Implement this and register with the gpu instance to get resize notifications.
pub trait RenderExtentChangeReceiver: Debug + Send + Sync + 'static {
    fn on_render_extent_changed(&mut self, gpu: &Gpu) -> Result<()>;
}

#[derive(Debug, NitrousModule)]
pub struct Gpu {
    _instance: wgpu::Instance,
    surface: wgpu::Surface,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,

    // Render extent doesn't necessarily match window size, so most resources don't
    // actually need to get re-created in most window size change cases. However, the
    // backing screen resources that we eventually project into each frame do need
    // to follow the window size exactly.

    // The swap chain and depth buffer need to follow window size changes exactly.
    swap_chain: wgpu::SwapChain,
    depth_texture: wgpu::TextureView,

    // Render extent is usually decoupled from
    render_extent: wgpu::Extent3d,
    render_extent_change_receivers: Vec<Arc<RwLock<dyn RenderExtentChangeReceiver>>>,

    config: RenderConfig,
    frame_count: usize,
}

#[inject_nitrous_module]
impl Gpu {
    pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
    pub const SCREEN_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

    pub fn new(
        win: &mut Window,
        config: RenderConfig,
        interpreter: &mut Interpreter,
    ) -> Result<Arc<RwLock<Self>>> {
        block_on(Self::new_async(win, config, interpreter))
    }

    pub async fn new_async(
        win: &mut Window,
        config: RenderConfig,
        interpreter: &mut Interpreter,
    ) -> Result<Arc<RwLock<Self>>> {
        let instance = wgpu::Instance::new(wgpu::BackendBit::PRIMARY);
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
            })
            .await
            .ok_or_else(|| anyhow!("no suitable graphics adapter"))?;

        let surface = { unsafe { instance.create_surface(win.os_window()) } };

        let trace_path = PathBuf::from("api_tracing.txt");
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    features: adapter.features(),
                    limits: adapter.limits(),
                },
                Some(&trace_path),
            )
            .await?;

        let physical_size = win.physical_size();
        let sc_desc = wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::RENDER_ATTACHMENT,
            format: Self::SCREEN_FORMAT,
            width: physical_size.width,
            height: physical_size.height,
            present_mode: config.present_mode,
        };
        let swap_chain = device.create_swap_chain(&surface, &sc_desc);
        let depth_texture = Self::create_depth_texture(&device, &sc_desc);

        let render_size = win.render_extent();
        let render_extent = wgpu::Extent3d {
            width: render_size.width,
            height: render_size.height,
            depth: 1,
        };

        let gpu = Arc::new(RwLock::new(Self {
            _instance: instance,
            surface,
            adapter,
            device,
            queue,
            swap_chain,
            depth_texture,
            render_extent,
            render_extent_change_receivers: Vec::new(),
            config,
            frame_count: 0,
        }));
        interpreter.put_global("gpu", Value::Module(gpu.clone()));
        win.register_display_config_change_receiver(gpu.clone());
        Ok(gpu)
    }

    #[cfg(unix)]
    pub fn for_test_unix() -> Result<TestResources> {
        let (os_window, mut input) = input::InputController::for_test_unix()?;
        let mut interpreter = Interpreter::default();
        let window = Window::new(
            os_window,
            &mut input,
            DisplayConfig::for_test(),
            &mut interpreter,
        )?;
        let gpu = Self::new(&mut window.write(), Default::default(), &mut interpreter)?;
        let async_rt = Runtime::new()?;
        Ok(TestResources {
            gpu,
            window,
            async_rt,
            interpreter,
            input,
        })
    }

    #[method]
    pub fn info(&self) -> String {
        let info = self.adapter.get_info();
        format!(
            "Name: {}\nVendor: {}\nDevice: {:?} {}\nBackend: {:?}",
            info.name, info.vendor, info.device_type, info.device, info.backend
        )
    }

    #[method]
    pub fn name(&self) -> String {
        self.adapter.get_info().name
    }

    #[method]
    pub fn vendor_id(&self) -> String {
        self.adapter.get_info().vendor.to_string()
    }

    #[method]
    pub fn device_id(&self) -> String {
        self.adapter.get_info().device.to_string()
    }

    #[method]
    pub fn device_type(&self) -> String {
        format!("{:?}", self.adapter.get_info().device_type)
    }

    #[method]
    pub fn backend(&self) -> String {
        format!("{:?}", self.adapter.get_info().backend)
    }

    #[method]
    pub fn limits(&self) -> String {
        format!("{:#?}", self.adapter.limits())
    }

    #[method]
    pub fn features(&self) -> String {
        let f = self.adapter.features();
        format!(
            "{:^6} - DEPTH_CLAMPING\n",
            f.contains(wgpu::Features::DEPTH_CLAMPING)
        ) + &format!(
            "{:^6} - TEXTURE_COMPRESSION_BC\n",
            f.contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
        ) + &format!(
            "{:^6} - TIMESTAMP_QUERY\n",
            f.contains(wgpu::Features::TIMESTAMP_QUERY)
        ) + &format!(
            "{:^6} - PIPELINE_STATISTICS_QUERY\n",
            f.contains(wgpu::Features::PIPELINE_STATISTICS_QUERY)
        ) + &format!(
            "{:^6} - MAPPABLE_PRIMARY_BUFFERS\n",
            f.contains(wgpu::Features::MAPPABLE_PRIMARY_BUFFERS)
        ) + &format!(
            "{:^6} - SAMPLED_TEXTURE_BINDING_ARRAY\n",
            f.contains(wgpu::Features::SAMPLED_TEXTURE_BINDING_ARRAY)
        ) + &format!(
            "{:^6} - SAMPLED_TEXTURE_ARRAY_DYNAMIC_INDEXING\n",
            f.contains(wgpu::Features::SAMPLED_TEXTURE_ARRAY_DYNAMIC_INDEXING)
        ) + &format!(
            "{:^6} - SAMPLED_TEXTURE_ARRAY_NON_UNIFORM_INDEXING\n",
            f.contains(wgpu::Features::SAMPLED_TEXTURE_ARRAY_NON_UNIFORM_INDEXING)
        ) + &format!(
            "{:^6} - UNSIZED_BINDING_ARRAY\n",
            f.contains(wgpu::Features::UNSIZED_BINDING_ARRAY)
        ) + &format!(
            "{:^6} - MULTI_DRAW_INDIRECT\n",
            f.contains(wgpu::Features::MULTI_DRAW_INDIRECT)
        ) + &format!(
            "{:^6} - MULTI_DRAW_INDIRECT_COUNT\n",
            f.contains(wgpu::Features::MULTI_DRAW_INDIRECT_COUNT)
        ) + &format!(
            "{:^6} - PUSH_CONSTANTS\n",
            f.contains(wgpu::Features::PUSH_CONSTANTS)
        ) + &format!(
            "{:^6} - ADDRESS_MODE_CLAMP_TO_BORDER\n",
            f.contains(wgpu::Features::ADDRESS_MODE_CLAMP_TO_BORDER)
        ) + &format!(
            "{:^6} - NON_FILL_POLYGON_MODE\n",
            f.contains(wgpu::Features::NON_FILL_POLYGON_MODE)
        ) + &format!(
            "{:^6} - TEXTURE_COMPRESSION_ETC2\n",
            f.contains(wgpu::Features::TEXTURE_COMPRESSION_ETC2)
        ) + &format!(
            "{:^6} - TEXTURE_COMPRESSION_ASTC_LDR\n",
            f.contains(wgpu::Features::TEXTURE_COMPRESSION_ASTC_LDR)
        ) + &format!(
            "{:^6} - TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES\n",
            f.contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
        ) + &format!(
            "{:^6} - SHADER_FLOAT64\n",
            f.contains(wgpu::Features::SHADER_FLOAT64)
        ) + &format!(
            "{:^6} - VERTEX_ATTRIBUTE_64BIT\n",
            f.contains(wgpu::Features::VERTEX_ATTRIBUTE_64BIT)
        )
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
            usage: wgpu::TextureUsage::RENDER_ATTACHMENT,
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

    pub fn register_render_extent_change_receiver<T: RenderExtentChangeReceiver>(
        &mut self,
        observer: Arc<RwLock<T>>,
    ) {
        self.render_extent_change_receivers.push(observer);
    }

    pub fn attachment_extent(&self) -> wgpu::Extent3d {
        self.render_extent
    }

    pub fn render_extent(&self) -> wgpu::Extent3d {
        self.render_extent
    }

    #[method]
    pub fn frame_count(&self) -> i64 {
        self.frame_count as i64
    }

    pub fn get_next_framebuffer(&mut self) -> Result<Option<wgpu::SwapChainFrame>> {
        self.frame_count += 1;
        match self.swap_chain.get_current_frame() {
            Ok(frame) => Ok(Some(frame)),
            Err(wgpu::SwapChainError::Timeout) => bail!("Timeout: gpu is locked up"),
            Err(wgpu::SwapChainError::OutOfMemory) => bail!("OOM: gpu is out of memory"),
            Err(wgpu::SwapChainError::Lost) => bail!("Lost: our context wondered off"),
            Err(wgpu::SwapChainError::Outdated) => {
                info!("GPU: swap chain outdated, must be recreated");
                Ok(None)
            }
        }
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

    /// Push `data` to the GPU and return a new buffer. Returns None if data is empty
    /// instead of crashing.
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

    /// Push `data` to the GPU and return a new buffer. Returns None if data is empty
    /// instead of crashing.
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

    /// Push `data` to the GPU and return a new buffer. Panics if data is empty.
    pub fn push_buffer(
        &self,
        label: &'static str,
        data: &[u8],
        usage: wgpu::BufferUsage,
    ) -> wgpu::Buffer {
        self.maybe_push_buffer(label, data, usage)
            .expect("push non-empty buffer")
    }

    /// Push `data` to the GPU and return a new buffer. Panics if data is empty.
    pub fn push_slice<T: AsBytes>(
        &self,
        label: &'static str,
        data: &[T],
        usage: wgpu::BufferUsage,
    ) -> wgpu::Buffer {
        self.maybe_push_slice(label, data, usage)
            .expect("push non-empty slice")
    }

    /// Push `data` to the GPU and return a new buffer. Panics if data is empty.
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

    /// Push `data` to the GPU and copy it to `target`. Does nothing if data is empty.
    /// The copy appears to currently be fenced with respect to usages of the target,
    /// but this is not specified as of the time of writing. This is optimized under
    /// the hood and is supposed to be, I think, faster than creating a new bind group.
    pub fn upload_slice_to<T: AsBytes>(
        &self,
        label: &'static str,
        data: &[T],
        target: Arc<wgpu::Buffer>,
        tracker: &mut UploadTracker,
    ) {
        if let Some(source) = self.maybe_push_slice(label, data, wgpu::BufferUsage::COPY_SRC) {
            tracker.upload(source, target, mem::size_of::<T>() * data.len());
        }
    }

    pub fn create_shader_module(&self, name: &str, spirv: &[u8]) -> Result<wgpu::ShaderModule> {
        let spirv_words = wgpu::util::make_spirv(spirv);
        Ok(self
            .device
            .create_shader_module(&wgpu::ShaderModuleDescriptor {
                label: Some(name),
                source: spirv_words,
                // FIXME: make configurable?
                flags: wgpu::ShaderFlags::VALIDATION,
            }))
    }

    pub const fn stride_for_row_size(size_in_bytes: u32) -> u32 {
        (size_in_bytes + wgpu::COPY_BYTES_PER_ROW_ALIGNMENT - 1)
            / wgpu::COPY_BYTES_PER_ROW_ALIGNMENT
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT
    }

    pub fn dump_texture(
        texture: &wgpu::Texture,
        extent: wgpu::Extent3d,
        format: wgpu::TextureFormat,
        gpu: &mut Gpu,
        callback: Box<
            dyn FnOnce(wgpu::Extent3d, wgpu::TextureFormat, Vec<u8>) + Send + Sync + 'static,
        >,
    ) -> Result<()> {
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
        tokio::spawn(async move {
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

impl DisplayConfigChangeReceiver for Gpu {
    fn on_display_config_changed(&mut self, config: &DisplayConfig) -> Result<()> {
        info!(
            "window config changed {}x{}",
            config.window_size().width,
            config.window_size().height
        );

        // Recreate the screen resources for the new window size.
        let sc_desc = wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::RENDER_ATTACHMENT,
            format: Self::SCREEN_FORMAT,
            width: config.window_size().width,
            height: config.window_size().height,
            present_mode: self.config.present_mode,
        };
        self.swap_chain = self.device.create_swap_chain(&self.surface, &sc_desc);
        self.depth_texture = Self::create_depth_texture(&self.device, &sc_desc);

        // Check if our render extent has changed and re-broadcast
        let extent = config.render_extent();
        if self.render_extent.width != extent.width || self.render_extent.height != extent.height {
            self.render_extent = wgpu::Extent3d {
                width: extent.width,
                height: extent.height,
                depth: 1,
            };
            for module in &self.render_extent_change_receivers {
                module.write().on_render_extent_changed(self)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn test_create() -> Result<()> {
        let _ = Gpu::for_test_unix()?;
        Ok(())
    }
}
