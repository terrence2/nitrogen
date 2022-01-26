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
pub use window::DisplayConfig;

// Note: re-export for use by FrameGraph when it is instantiated in other crates.
pub use wgpu;
pub use winit::dpi::{LogicalSize, PhysicalSize};

use anyhow::{anyhow, bail, Result};
use bevy_ecs::prelude::*;
use futures::executor::block_on;
use log::{info, trace};
use nitrous_injector::{inject_nitrous_module, method, NitrousModule};
use runtime::{Extension, FrameStage, Runtime};
use std::{borrow::Cow, fmt::Debug, fs, mem, num::NonZeroU32, path::PathBuf, ptr, sync::Arc};
use wgpu::util::DeviceExt;
use window::Window;
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
    depth_texture: wgpu::TextureView,

    // Render extent is usually decoupled from
    render_extent: wgpu::Extent3d,

    config: RenderConfig,
    frame_count: usize,
}

impl Extension for Gpu {
    fn init(runtime: &mut Runtime) -> Result<()> {
        let gpu = Self::new(runtime.resource::<Window>(), Default::default())?;
        runtime.insert_module("gpu", gpu);
        runtime
            .frame_stage_mut(FrameStage::HandleDisplayChange)
            .add_system(Self::sys_handle_display_config_change);
        runtime.insert_resource(UploadTracker::default());

        // FIXME: Once we've tied into this all...
        // runtime
        //     .frame_stage_mut(FrameStage::PostRender)
        //     .add_system(Self::sys_clear_upload_tracker);
        Ok(())
    }
}

#[inject_nitrous_module]
impl Gpu {
    pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
    pub const SCREEN_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

    pub fn new(window: &Window, config: RenderConfig) -> Result<Self> {
        block_on(Self::new_async(window, config))
    }

    pub async fn new_async(window: &Window, config: RenderConfig) -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::Backends::PRIMARY);
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
            })
            .await
            .ok_or_else(|| anyhow!("no suitable graphics adapter"))?;

        let surface = { unsafe { instance.create_surface(window.os_window()) } };

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

        let physical_size = window.physical_size();
        let sc_desc = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: Self::SCREEN_FORMAT,
            width: physical_size.width,
            height: physical_size.height,
            present_mode: config.present_mode,
        };
        surface.configure(&device, &sc_desc);
        let depth_texture = Self::create_depth_texture(&device, &sc_desc);

        let render_size = window.render_extent();
        let render_extent = wgpu::Extent3d {
            width: render_size.width,
            height: render_size.height,
            depth_or_array_layers: 1,
        };

        Ok(Self {
            _instance: instance,
            surface,
            adapter,
            device,
            queue,
            depth_texture,
            render_extent,
            config,
            frame_count: 0,
        })
    }

    #[cfg(unix)]
    pub fn for_test_unix() -> Result<Runtime> {
        let mut runtime = input::InputController::for_test_unix()?;
        runtime
            .insert_resource(window::DisplayOpts::default())
            .load_extension::<Window>()?
            .load_extension::<Gpu>()?;
        Ok(runtime)
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
            "{:^6} - DEPTH_CLIP_CONTROL\n",
            f.contains(wgpu::Features::DEPTH_CLIP_CONTROL)
        ) + &format!(
            "{:^6} - TEXTURE_COMPRESSION_BC\n",
            f.contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
        ) + &format!(
            "{:^6} - INDIRECT_FIRST_INSTANCE\n",
            f.contains(wgpu::Features::INDIRECT_FIRST_INSTANCE)
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
            "{:^6} - TEXTURE_BINDING_ARRAY\n",
            f.contains(wgpu::Features::TEXTURE_BINDING_ARRAY)
        ) + &format!(
            "{:^6} - BUFFER_BINDING_ARRAY\n",
            f.contains(wgpu::Features::BUFFER_BINDING_ARRAY)
        ) + &format!(
            "{:^6} - BUFFER_BINDING_ARRAY\n",
            f.contains(wgpu::Features::BUFFER_BINDING_ARRAY)
        ) + &format!(
            "{:^6} - STORAGE_RESOURCE_BINDING_ARRAY\n",
            f.contains(wgpu::Features::STORAGE_RESOURCE_BINDING_ARRAY)
        ) + &format!(
            "{:^6} - SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING\n",
            f.contains(
                wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING
            )
        ) + &format!(
            "{:^6} - UNIFORM_BUFFER_AND_STORAGE_TEXTURE_ARRAY_NON_UNIFORM_INDEXING\n",
            f.contains(
                wgpu::Features::UNIFORM_BUFFER_AND_STORAGE_TEXTURE_ARRAY_NON_UNIFORM_INDEXING
            )
        ) + &format!(
            "{:^6} - PARTIALLY_BOUND_BINDING_ARRAY\n",
            f.contains(wgpu::Features::PARTIALLY_BOUND_BINDING_ARRAY)
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
            "{:^6} - POLYGON_MODE_LINE\n",
            f.contains(wgpu::Features::POLYGON_MODE_LINE)
        ) + &format!(
            "{:^6} - POLYGON_MODE_POINT\n",
            f.contains(wgpu::Features::POLYGON_MODE_POINT)
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
        ) + &format!(
            "{:^6} - CONSERVATIVE_RASTERIZATION\n",
            f.contains(wgpu::Features::CONSERVATIVE_RASTERIZATION)
        ) + &format!(
            "{:^6} - VERTEX_WRITABLE_STORAGE\n",
            f.contains(wgpu::Features::VERTEX_WRITABLE_STORAGE)
        ) + &format!(
            "{:^6} - CLEAR_COMMANDS\n",
            f.contains(wgpu::Features::CLEAR_COMMANDS)
        ) + &format!(
            "{:^6} - SPIRV_SHADER_PASSTHROUGH\n",
            f.contains(wgpu::Features::SPIRV_SHADER_PASSTHROUGH)
        ) + &format!(
            "{:^6} - SHADER_PRIMITIVE_INDEX\n",
            f.contains(wgpu::Features::SHADER_PRIMITIVE_INDEX)
        ) + &format!("{:^6} - MULTIVIEW\n", f.contains(wgpu::Features::MULTIVIEW))
            + &format!(
                "{:^6} - TEXTURE_FORMAT_16BIT_NORM\n",
                f.contains(wgpu::Features::TEXTURE_FORMAT_16BIT_NORM)
            )
    }

    fn create_depth_texture(
        device: &wgpu::Device,
        sc_desc: &wgpu::SurfaceConfiguration,
    ) -> wgpu::TextureView {
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth-texture"),
            size: wgpu::Extent3d {
                width: sc_desc.width,
                height: sc_desc.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        });
        depth_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("depth-texture-view"),
            format: Some(Self::DEPTH_FORMAT),
            dimension: Some(wgpu::TextureViewDimension::D2),
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        })
    }

    pub fn sys_handle_display_config_change(
        updated_config: Res<Option<DisplayConfig>>,
        mut gpu: ResMut<Gpu>,
    ) {
        if let Some(config) = updated_config.as_ref() {
            gpu.on_display_config_changed(config)
                .expect("Gpu::on_display_config_changed");
        }
    }

    pub fn on_display_config_changed(&mut self, config: &DisplayConfig) -> Result<()> {
        info!(
            "window config changed {}x{}",
            config.window_size().width,
            config.window_size().height
        );

        // Recreate the screen resources for the new window size.
        let sc_desc = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: Self::SCREEN_FORMAT,
            width: config.window_size().width,
            height: config.window_size().height,
            present_mode: self.config.present_mode,
        };
        self.surface.configure(&self.device, &sc_desc);
        self.depth_texture = Self::create_depth_texture(&self.device, &sc_desc);

        // Check if our render extent has changed and re-broadcast
        let extent = config.render_extent();
        if self.render_extent.width != extent.width || self.render_extent.height != extent.height {
            self.render_extent = wgpu::Extent3d {
                width: extent.width,
                height: extent.height,
                depth_or_array_layers: 1,
            };
        }

        Ok(())
    }

    // pub fn register_render_extent_change_receiver<T: RenderExtentChangeReceiver>(
    //     &mut self,
    //     observer: Arc<RwLock<T>>,
    // ) {
    //     self.render_extent_change_receivers.push(observer);
    // }

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

    pub fn get_next_framebuffer(&mut self) -> Result<Option<wgpu::SurfaceTexture>> {
        self.frame_count += 1;
        match self.surface.get_current_texture() {
            Ok(frame) => Ok(Some(frame)),
            Err(wgpu::SurfaceError::Timeout) => bail!("Timeout: gpu is locked up"),
            Err(wgpu::SurfaceError::OutOfMemory) => bail!("OOM: gpu is out of memory"),
            Err(wgpu::SurfaceError::Lost) => bail!("Lost: our context wondered off"),
            Err(wgpu::SurfaceError::Outdated) => {
                info!("GPU: swap chain outdated, must be recreated");
                Ok(None)
            }
        }
    }

    pub fn depth_attachment(&self) -> &wgpu::TextureView {
        &self.depth_texture
    }

    pub fn depth_stencil_attachment(&self) -> wgpu::RenderPassDepthStencilAttachment {
        wgpu::RenderPassDepthStencilAttachment {
            view: &self.depth_texture,
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

    pub fn color_attachment(view: &wgpu::TextureView) -> wgpu::RenderPassColorAttachment {
        wgpu::RenderPassColorAttachment {
            view,
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
        usage: wgpu::BufferUsages,
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
        usage: wgpu::BufferUsages,
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
        usage: wgpu::BufferUsages,
    ) -> wgpu::Buffer {
        self.maybe_push_buffer(label, data, usage)
            .expect("push non-empty buffer")
    }

    /// Push `data` to the GPU and return a new buffer. Panics if data is empty.
    pub fn push_slice<T: AsBytes>(
        &self,
        label: &'static str,
        data: &[T],
        usage: wgpu::BufferUsages,
    ) -> wgpu::Buffer {
        self.maybe_push_slice(label, data, usage)
            .expect("push non-empty slice")
    }

    /// Push `data` to the GPU and return a new buffer. Panics if data is empty.
    pub fn push_data<T: AsBytes>(
        &self,
        label: &'static str,
        data: &T,
        usage: wgpu::BufferUsages,
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
        if let Some(source) = self.maybe_push_slice(label, data, wgpu::BufferUsages::COPY_SRC) {
            tracker.upload(source, target, mem::size_of::<T>() * data.len());
        }
    }

    // Copy of the old util method that went away.
    fn make_spirv(data: &[u8]) -> wgpu::ShaderSource {
        const MAGIC_NUMBER: u32 = 0x0723_0203;

        assert_eq!(
            data.len() % mem::size_of::<u32>(),
            0,
            "data size is not a multiple of 4"
        );

        //If the data happens to be aligned, directly use the byte array,
        // otherwise copy the byte array in an owned vector and use that instead.
        let words = if data.as_ptr().align_offset(mem::align_of::<u32>()) == 0 {
            let (pre, words, post) = unsafe { data.align_to::<u32>() };
            debug_assert!(pre.is_empty());
            debug_assert!(post.is_empty());
            Cow::from(words)
        } else {
            let mut words = vec![0u32; data.len() / mem::size_of::<u32>()];
            unsafe {
                ptr::copy_nonoverlapping(data.as_ptr(), words.as_mut_ptr() as *mut u8, data.len());
            }
            Cow::from(words)
        };

        assert_eq!(
            words[0], MAGIC_NUMBER,
            "wrong magic word {:x}. Make sure you are using a binary SPIRV file.",
            words[0]
        );
        wgpu::ShaderSource::SpirV(words)
    }

    pub fn create_shader_module(&self, name: &str, spirv: &[u8]) -> Result<wgpu::ShaderModule> {
        let spirv_words = Self::make_spirv(spirv);
        Ok(self
            .device
            .create_shader_module(&wgpu::ShaderModuleDescriptor {
                label: Some(name),
                source: spirv_words,
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
            usage: wgpu::BufferUsages::all(),
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
            wgpu::ImageCopyTexture {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &download_buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: NonZeroU32::new(bytes_per_row),
                    rows_per_image: NonZeroU32::new(extent.height),
                },
            },
            extent,
        );
        gpu.queue_mut().submit(vec![encoder.finish()]);
        gpu.device().poll(wgpu::Maintain::Wait);
        let reader = download_buffer.slice(..).map_async(wgpu::MapMode::Read);
        gpu.device().poll(wgpu::Maintain::Wait);
        block_on(reader)?;
        let raw = download_buffer.slice(..).get_mapped_range().to_owned();
        callback(extent, format, raw);
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

    #[cfg(unix)]
    #[test]
    fn test_create() -> Result<()> {
        let runtime = Gpu::for_test_unix()?;
        assert!(runtime.resource::<Gpu>().render_extent().width > 0);
        Ok(())
    }
}
