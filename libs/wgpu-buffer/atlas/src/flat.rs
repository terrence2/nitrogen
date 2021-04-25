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
use anyhow::Result;
use geometry::Aabb2;
use gpu::{texture_format_size, Gpu, UploadTracker};
use image::{ImageBuffer, Luma, Pixel, Rgba};
use log::debug;
use std::{marker::PhantomData, mem, num::NonZeroU64, sync::Arc};
use tokio::runtime::Runtime;
use zerocopy::AsBytes;

#[repr(C)]
#[derive(AsBytes, Copy, Clone)]
struct BufferToTextureCopyInfo {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    padding_px: u32,
    px_size: u32,
    border_color: [u8; 4],
}

// Each column indicates the filled height up to the given offset.
#[derive(Debug)]
pub struct Column {
    fill_height: u32,
    x_end: u32,
}

impl Column {
    fn new(fill_height: u32, x_offset: u32) -> Self {
        Self {
            fill_height,
            x_end: x_offset,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum DirtyState {
    Clean,
    RecreateTexture((u32, u32)),
}

// The Frame tells our renderer how to get back to the texture in our eventual Atlas.
#[derive(Copy, Clone, Debug)]
pub struct Frame {
    s0: u32,
    s1: u32,
    t0: u32,
    t1: u32,
}

impl Frame {
    fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            s0: x,
            s1: x + width,
            t0: y + height,
            t1: y,
        }
    }

    pub fn s0(&self, width: u32) -> f32 {
        self.s0 as f32 / width as f32
    }

    pub fn s1(&self, width: u32) -> f32 {
        self.s1 as f32 / width as f32
    }

    pub fn t0(&self, height: u32) -> f32 {
        self.t0 as f32 / height as f32
    }

    pub fn t1(&self, height: u32) -> f32 {
        self.t1 as f32 / height as f32
    }
}

// Trades off pack complexity against efficiency. This packer is designed for online, incremental
// usage, so tries to be faster to pack at the cost of potentially loosing out on easy space wins
// in cases where subsequent items are differently sized or shaped. Most common uses will only
// feed similarly shaped items, so will generally be fine.
//
// For one-shot packing, call `add_image`, record all frames, then call `finish` to get a
// texture out that can be bound.
//
// Assumptions:
//   * Texture format must be a Unorm variety
//   * Only R8Unorm and Rgba8Unorm are currently supported
#[derive(Debug)]
pub struct AtlasPacker<P: Pixel + 'static> {
    // Constant storage info
    name: String,
    fill_color_bytes: [u8; 4],
    initial_width: u32,
    initial_height: u32,
    padding: u32,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsage,

    // Pack state
    width: u32,
    height: u32,
    columns: Vec<Column>,

    // Upload state
    dirty_region: DirtyState,
    texture: Arc<Box<wgpu::Texture>>,
    texture_view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    dump_texture: Option<String>,
    next_texture: Option<Arc<Box<wgpu::Texture>>>,

    // CPU-side list of buffers that need to be blit into the target texture these can either
    // get directly encoded for aligned upload-as-copy, or need to get deferred to a gpu compute
    // pass for unaligned and palettized uploads.
    blit_list: Vec<(wgpu::Buffer, wgpu::Buffer, u32, u32)>,
    upload_unaligned_bind_group_layout: wgpu::BindGroupLayout,
    upload_unaligned_pipeline: wgpu::ComputePipeline,
    unaligned_blit: Vec<(wgpu::BindGroup, u32, u32)>,

    _phantom: PhantomData<P>,
}

impl<P: Pixel + 'static> AtlasPacker<P>
where
    [P::Subpixel]: AsRef<[u8]>,
    P::Subpixel: AsBytes + 'static,
{
    // Note that this much match the work group size in the shader.
    // Columns in the layout must be aligned on these values as well so that texture writes
    // can happen concurrently without stomping on each other.
    const BLOCK_SIZE: u32 = 16;

    // This padding applies all the way around for all images so is effectively 2, except at
    // borders. As such, it is generally good enough for linear filtering in most situations.
    const DEFAULT_PADDING: u32 = 1;

    pub fn new<S: Into<String>>(
        name: S,
        gpu: &Gpu,
        initial_width: u32,
        initial_height: u32,
        fill_color: [u8; 4],
        format: wgpu::TextureFormat,
        filter: wgpu::FilterMode,
    ) -> Result<Self> {
        let usage = wgpu::TextureUsage::SAMPLED
            | wgpu::TextureUsage::COPY_SRC
            | wgpu::TextureUsage::COPY_DST
            | wgpu::TextureUsage::STORAGE;
        assert_eq!(texture_format_size(format) as usize, mem::size_of::<P>());
        let pix_size = mem::size_of::<P>() as u32;
        assert_eq!(
            (initial_width * pix_size) % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT,
            0
        );
        let sampler = gpu.device().create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atlas-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: filter,
            min_filter: filter,
            mipmap_filter: filter,
            lod_min_clamp: 1.0,
            lod_max_clamp: 1.0, // TODO: mipmapping
            anisotropy_clamp: None,
            compare: None,
            border_color: None,
        });
        let texture = Arc::new(Box::new(gpu.device().create_texture(
            &wgpu::TextureDescriptor {
                label: Some("atlas-texture"),
                size: wgpu::Extent3d {
                    width: initial_width,
                    height: initial_height,
                    depth: 1,
                },
                mip_level_count: 1, // TODO: mip-mapping for atlas textures?
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
            },
        )));
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("atlas-texture-view"),
            format: None,
            dimension: None,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            level_count: None, // mip_
            base_array_layer: 0,
            array_layer_count: None,
        });

        let upload_unaligned_bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("atlas-upload-unaligned-bind-group-layout"),
                    entries: &[
                        // 0: Copy Info
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        // 1: Buffer source
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        // 2: Texture target
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::StorageTexture {
                                access: wgpu::StorageTextureAccess::WriteOnly,
                                format,
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                    ],
                });

        let shader = if mem::size_of::<P>() == 4 {
            gpu.create_shader_module(
                "upload_unaligned.comp",
                include_bytes!("../target/upload_unaligned_rgba.comp.spirv"),
            )?
        } else {
            assert_eq!(mem::size_of::<P>(), 1);
            gpu.create_shader_module(
                "upload_unaligned.comp",
                include_bytes!("../target/upload_unaligned_gray.comp.spirv"),
            )?
        };

        let upload_unaligned_pipeline =
            gpu.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("atlas-upload-unaligned-pipeline"),
                    layout: Some(&gpu.device().create_pipeline_layout(
                        &wgpu::PipelineLayoutDescriptor {
                            label: Some("atlas-upload-unaligned-pipeline-layout"),
                            bind_group_layouts: &[&upload_unaligned_bind_group_layout],
                            push_constant_ranges: &[],
                        },
                    )),
                    module: &shader,
                    entry_point: "main",
                });

        Ok(Self {
            name: name.into(),
            fill_color_bytes: fill_color,
            initial_width,
            initial_height,
            format,
            usage: usage | wgpu::TextureUsage::COPY_DST,
            padding: Self::DEFAULT_PADDING,
            width: initial_width,
            height: initial_height,
            columns: vec![Column::new(0, 0)],
            // Note: texture not initialized, but no frames reference it yet.
            dirty_region: DirtyState::Clean,
            texture,
            texture_view,
            sampler,
            dump_texture: None,
            next_texture: None,

            upload_unaligned_bind_group_layout,
            upload_unaligned_pipeline,
            blit_list: Vec::new(),
            unaligned_blit: Vec::new(),

            _phantom: PhantomData::default(),
        })
    }

    pub fn align(v: u32) -> u32 {
        (v + Self::BLOCK_SIZE - 1) & !(Self::BLOCK_SIZE - 1)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn with_padding(mut self, padding: u32) -> Self {
        self.padding = padding;
        self
    }

    pub fn dump(&mut self, path: &str) {
        self.dump_texture = Some(path.to_owned());
    }

    fn do_layout(&mut self, w: u32, h: u32) -> (u32, u32) {
        assert!(w + 2 * self.padding <= self.initial_width);
        assert!(h + 2 * self.padding <= self.initial_height);
        let mut x_column_start = 0;
        let x_last = self.columns.last().unwrap().x_end;

        // Pack into the first segment that can take our height, adjusting the column as necessary.
        let mut position = None;
        let mut adjust = None;
        for (i, c) in self.columns.iter_mut().enumerate() {
            if h + 2 * self.padding <= self.height - c.fill_height {
                if w + 2 * self.padding <= c.x_end - x_column_start {
                    // Fits below this corner, place and expand corner down.
                    position = Some((x_column_start, c.fill_height));
                    adjust = Some((
                        i,
                        Self::align(c.x_end),
                        Self::align(c.fill_height + h + 2 * self.padding),
                    ));
                    break;
                } else if c.x_end == x_last && x_column_start + w < self.width {
                    // Does not fit width-wise, but we can expand since we are the last column.
                    position = Some((x_column_start, c.fill_height));
                    adjust = Some((
                        i,
                        Self::align(x_column_start + w + 2 * self.padding),
                        Self::align(c.fill_height + h + 2 * self.padding),
                    ));
                    break;
                } else {
                    x_column_start = c.x_end;
                }
            } else {
                x_column_start = c.x_end;
            }
        }
        if let Some((x, y)) = position {
            self.assert_non_overlapping(x, y, w, h);
        }
        if let Some((offset, x_end, fill_height)) = adjust {
            self.columns[offset].x_end = x_end;
            self.columns[offset].fill_height = fill_height;
        }

        if position.is_none() {
            // If we did not find a position above our current columns, see if there is room to insert
            // a new column and try there.
            if self.width - x_last > w + 2 * self.padding {
                self.columns.push(Column::new(
                    Self::align(h + 2 * self.padding),
                    x_last + w + 2 * self.padding,
                ));
                position = Some((x_last, 0));
            }
        }

        self.assert_column_constraints();

        if let Some((x, y)) = position {
            (x, y)
        } else {
            // Did not find room in this image, grow and try again.
            self.grow();
            self.do_layout(w, h)
        }
    }

    pub fn push_buffer(
        &mut self,
        buffer: wgpu::Buffer,
        width: u32,
        height: u32,
        gpu: &Gpu,
    ) -> Result<Frame> {
        let (x, y) = self.do_layout(width, height);
        let copy_info = BufferToTextureCopyInfo {
            x,
            y,
            w: width,
            h: height,
            padding_px: self.padding,
            px_size: texture_format_size(self.format),
            border_color: self.fill_color_bytes,
        };
        let copy_buffer = gpu.push_data("atlas-copy-info", &copy_info, wgpu::BufferUsage::UNIFORM);
        self.blit_list.push((copy_buffer, buffer, width, height));
        Ok(Frame::new(
            x + self.padding,
            y + self.padding,
            width,
            height,
        ))
    }

    pub fn push_image(
        &mut self,
        image: &ImageBuffer<P, Vec<P::Subpixel>>,
        gpu: &Gpu,
    ) -> Result<Frame> {
        let img_buffer = gpu.push_buffer(
            "atlas-image-upload",
            image.as_bytes(),
            wgpu::BufferUsage::STORAGE,
        );
        self.push_buffer(img_buffer, image.width(), image.height(), gpu)
    }

    pub fn texture_layout_entry(&self, binding: u32) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStage::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                multisampled: false,
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
            },
            count: None,
        }
    }

    pub fn sampler_layout_entry(&self, binding: u32) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStage::FRAGMENT,
            ty: wgpu::BindingType::Sampler {
                filtering: true,
                comparison: false,
            },
            count: None,
        }
    }

    pub fn texture_binding(&self, binding: u32) -> wgpu::BindGroupEntry {
        wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::TextureView(&self.texture_view),
        }
    }
    pub fn sampler_binding(&self, binding: u32) -> wgpu::BindGroupEntry {
        wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::Sampler(&self.sampler),
        }
    }

    pub fn texture(&self) -> &wgpu::Texture {
        self.texture.as_ref()
    }

    /// Note: panics if no upload has happened.
    pub fn texture_view(&self) -> &wgpu::TextureView {
        &self.texture_view
    }

    /// Upload the current contents to the GPU. Note that this is non-destructive. If needed,
    /// the builder can accumulate more textures and upload again later.
    pub fn make_upload_buffer(
        &mut self,
        gpu: &mut Gpu,
        async_rt: &Runtime,
        tracker: &mut UploadTracker,
    ) -> Result<()> {
        // If we started a texture upload last frame, replace the prior texture with the new.
        // Any glyphs in the new region will have an oob Frame for one frame, but that's better
        // than having the entire glyph texture be noise for one frame.
        if let Some(texture) = &self.next_texture {
            debug!("{} transitioning to new texture", self.name);
            self.texture = texture.to_owned();
            self.texture_view = self.texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("atlas-texture-view"),
                format: None,
                dimension: None,
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                level_count: None, // mip_
                base_array_layer: 0,
                array_layer_count: None,
            });
        }
        self.next_texture = None;

        match self.dirty_region {
            DirtyState::Clean => {}
            DirtyState::RecreateTexture((hi_x, hi_y)) => {
                debug!(
                    "{} upload recreate {}x{} from {}x{}",
                    self.name, self.width, self.height, hi_x, hi_y
                );
                // We are not in upload when we need to resize.
                // When we enter here, the CPU `buffer` is already resized. The width/height fields
                // are updated with the new requested size. We need to copy from 0,0 up to whatever
                // else has been packed this frame, which are tracked in hiX,hiY.
                // We create a fresh binding every frame so that we can drop in a new texture
                // here easily, however, the content is going to take a frame to upload, so we
                // need to actually delay replacing it until the next frame.
                let next_texture = Arc::new(Box::new(gpu.device().create_texture(
                    &wgpu::TextureDescriptor {
                        label: Some("atlas-texture"),
                        size: wgpu::Extent3d {
                            width: self.width,
                            height: self.height,
                            depth: 1,
                        },
                        mip_level_count: 1, // TODO: mip-mapping for atlas textures
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: self.format,
                        usage: self.usage,
                    },
                )));
                tracker.copy_texture_to_texture(
                    self.texture.clone(),
                    0,
                    next_texture.clone(),
                    0,
                    wgpu::Extent3d {
                        width: hi_x,
                        height: hi_y,
                        depth: 1,
                    },
                );
                self.next_texture = Some(next_texture);
            }
        }
        self.dirty_region = DirtyState::Clean;

        // Set up texture blits
        self.unaligned_blit.clear();
        for (copy_buffer, img_buffer, width, height) in self.blit_list.drain(..) {
            let target_texture = if let Some(ref next_texture) = self.next_texture {
                next_texture.clone()
            } else {
                self.texture.clone()
            };
            let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("atlas-upload-unaligned-bind-group"),
                layout: &self.upload_unaligned_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer {
                            buffer: &copy_buffer,
                            offset: 0,
                            size: NonZeroU64::new(mem::size_of::<BufferToTextureCopyInfo>() as u64),
                        },
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Buffer {
                            buffer: &img_buffer,
                            offset: 0,
                            size: NonZeroU64::new(
                                (texture_format_size(self.format) * width * height) as u64,
                            ),
                        },
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&target_texture.create_view(
                            &wgpu::TextureViewDescriptor {
                                label: Some("atlas-texture-view"),
                                format: None,
                                dimension: None,
                                aspect: wgpu::TextureAspect::All,
                                base_mip_level: 0,
                                level_count: None, // mip_
                                base_array_layer: 0,
                                array_layer_count: None,
                            },
                        )),
                    },
                ],
            });
            self.unaligned_blit.push((
                bind_group,
                Self::align(width + 2 * self.padding),
                Self::align(height + 2 * self.padding),
            ));
        }

        if let Some(path_ref) = self.dump_texture.as_ref() {
            let path = path_ref.to_owned();
            let write_img =
                |extent: wgpu::Extent3d, fmt: wgpu::TextureFormat, data: Vec<u8>| match fmt {
                    wgpu::TextureFormat::R8Unorm => {
                        let img =
                            ImageBuffer::<Luma<u8>, _>::from_raw(extent.width, extent.height, data)
                                .expect("built image");
                        println!("writing to {}", path);
                        img.save(path).expect("wrote file");
                    }
                    wgpu::TextureFormat::Rgba8Unorm => {
                        let img =
                            ImageBuffer::<Rgba<u8>, _>::from_raw(extent.width, extent.height, data)
                                .expect("built image");
                        println!("writing to {}", path);
                        img.save(path).expect("wrote file");
                    }
                    _ => panic!("don't know how to dump texture format: {:?}", fmt),
                };
            Gpu::dump_texture(
                &self.texture,
                wgpu::Extent3d {
                    width: self.width,
                    height: self.height,
                    depth: 1,
                },
                self.format,
                async_rt,
                gpu,
                Box::new(write_img),
            )?;
        }
        self.dump_texture = None;

        Ok(())
    }

    pub fn maintain_gpu_resources<'a>(
        &'a self,
        mut cpass: wgpu::ComputePass<'a>,
    ) -> Result<wgpu::ComputePass<'a>> {
        cpass.set_pipeline(&self.upload_unaligned_pipeline);
        for (bind_group, width, height) in &self.unaligned_blit {
            assert!(*width / Self::BLOCK_SIZE > 0);
            assert!(*height / Self::BLOCK_SIZE > 0);
            cpass.set_bind_group(0, bind_group, &[]);
            cpass.dispatch(*width / Self::BLOCK_SIZE, *height / Self::BLOCK_SIZE, 1);
        }
        Ok(cpass)
    }

    /// Upload and then steal the texture. Useful when used as a one-shot atlas.
    pub fn finish(
        mut self,
        gpu: &mut Gpu,
        async_rt: &Runtime,
        tracker: &mut UploadTracker,
    ) -> Result<(Arc<Box<wgpu::Texture>>, wgpu::TextureView, wgpu::Sampler)> {
        // Note: we need to crank make_upload_buffer twice because of the way
        // we defer moving to a new texture to ensure in-flight uploads happen.
        self.make_upload_buffer(gpu, async_rt, tracker)?;

        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("atlas-finish"),
            });
        let cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("atlas-finish-compute-pass"),
        });
        self.maintain_gpu_resources(cpass)?;
        gpu.queue_mut().submit(vec![encoder.finish()]);

        self.make_upload_buffer(gpu, async_rt, tracker)?;

        Ok((self.texture, self.texture_view, self.sampler))
    }

    fn grow(&mut self) {
        debug!(
            "{} grow {}x{} => {}x{}",
            self.name,
            self.width,
            self.height,
            self.width + self.initial_width,
            self.height + self.initial_height
        );
        let prior_width = self.width;
        let prior_height = self.height;
        self.width += self.initial_width;
        self.height += self.initial_height;
        if self.dirty_region == DirtyState::Clean {
            // Note: we don't want to grow the dirty region past the extent of the currently
            // bound texture. The w/h or the DirtyState are the region we copy *from*.
            self.dirty_region = DirtyState::RecreateTexture((prior_width, prior_height));
            debug!(
                "{} set dirty region origin->{}x{}",
                self.name, prior_width, prior_height
            );
        }
    }

    fn assert_non_overlapping(&self, lo_x: u32, lo_y: u32, w: u32, h: u32) {
        let img = Aabb2::new(
            [lo_x + self.padding, lo_y + self.padding],
            [lo_x + w, lo_y + h],
        );
        let mut c_x_start = 0;
        for c in self.columns.iter() {
            let col = Aabb2::new([c_x_start, 0], [c.x_end, c.fill_height]);
            c_x_start = c.x_end;
            assert!(!img.overlaps(&col));
        }
    }

    fn assert_column_constraints(&self) {
        let mut prior = &self.columns[0];
        for c in self.columns.iter().skip(1) {
            assert!(c.x_end > prior.x_end);
            assert!(c.x_end <= self.width);
            assert!(c.fill_height <= self.height);
            prior = c;
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use image::{Rgba, RgbaImage};
    use nitrous::Interpreter;
    use rand::prelude::*;
    use std::{env, time::Duration};
    use tokio::runtime::Runtime;
    use winit::{event_loop::EventLoop, window::Window};

    #[cfg(unix)]
    #[test]
    fn test_random_packing() -> Result<()> {
        use winit::platform::unix::EventLoopExtUnix;
        let event_loop = EventLoop::<()>::new_any_thread();
        let window = Window::new(&event_loop).unwrap();
        let interpreter = Interpreter::new();
        let async_rt = Runtime::new()?;
        let gpu = Gpu::new(&window, Default::default(), &mut interpreter.write())?;

        let mut packer = AtlasPacker::<Rgba<u8>>::new(
            "random_packing",
            &gpu.read(),
            Gpu::stride_for_row_size((1024 + 8) * 4) / 4,
            2048,
            [random(), random(), random(), 255],
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::FilterMode::Linear,
        )?;
        let minimum = 40;
        let maximum = 200;

        for _ in 0..320 {
            let img = RgbaImage::from_pixel(
                thread_rng().gen_range(minimum..maximum),
                thread_rng().gen_range(minimum..maximum),
                *Rgba::from_slice(&[random(), random(), random(), 255]),
            );
            let frame = packer.push_image(&img, &gpu.read())?;
            let w = packer.width();
            let h = packer.height();
            // Frame edges should keep these from ever being full.
            assert!(frame.s0(w) > 0.0);
            assert!(frame.s1(w) > 0.0);
            assert!(frame.s0(w) < 1.0);
            assert!(frame.s1(w) < 1.0);
            assert!(frame.t0(h) > 0.0);
            assert!(frame.t1(h) > 0.0);
            assert!(frame.t0(h) < 1.0);
            assert!(frame.t1(h) < 1.0);
            // Orientation
            assert!(frame.s0(w) < frame.s1(w));
            assert!(frame.t0(h) > frame.t1(h));
        }
        let extent = wgpu::Extent3d {
            width: packer.width(),
            height: packer.height(),
            depth: 1,
        };
        let (texture, _view, _sampler) =
            packer.finish(&mut gpu.write(), &async_rt, &mut Default::default())?;
        if env::var("DUMP") == Ok("1".to_owned()) {
            Gpu::dump_texture(
                &texture,
                extent,
                wgpu::TextureFormat::Rgba8Unorm,
                &async_rt,
                &mut gpu.write(),
                Box::new(move |extent, _fmt, data| {
                    let buffer = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_vec(
                        extent.width,
                        extent.height,
                        data,
                    )
                    .unwrap();
                    buffer
                        .save("../../../__dump__/test_atlas_random_packing.png")
                        .unwrap();
                }),
            )?;
            // Shutting down the async_rt kills all tasks, so give ourself a chance to run.
            std::thread::sleep(Duration::from_secs(1));
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn test_finish() -> Result<()> {
        use winit::platform::unix::EventLoopExtUnix;
        let event_loop = EventLoop::<()>::new_any_thread();
        let window = Window::new(&event_loop).unwrap();
        let async_rt = Runtime::new()?;
        let interpreter = Interpreter::new();
        let gpu = Gpu::new(&window, Default::default(), &mut interpreter.write())?;

        let mut packer = AtlasPacker::<Rgba<u8>>::new(
            "test_finish",
            &gpu.read(),
            256,
            256,
            [0, 0, 0, 0],
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::FilterMode::Linear,
        )?;
        let _ = packer.push_image(
            &RgbaImage::from_pixel(254, 254, *Rgba::from_slice(&[255, 0, 0, 255])),
            &gpu.read(),
        )?;

        let _ = packer.finish(&mut gpu.write(), &async_rt, &mut Default::default());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn test_incremental_upload() -> Result<()> {
        use winit::platform::unix::EventLoopExtUnix;
        let async_rt = Runtime::new()?;
        let event_loop = EventLoop::<()>::new_any_thread();
        let window = Window::new(&event_loop).unwrap();
        let interpreter = Interpreter::new();
        let gpu = Gpu::new(&window, Default::default(), &mut interpreter.write())?;

        let mut packer = AtlasPacker::<Rgba<u8>>::new(
            "test_incremental",
            &gpu.read(),
            256,
            256,
            [0, 0, 0, 0],
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::FilterMode::Linear,
        )?;

        // Base upload
        let _ = packer.push_image(
            &RgbaImage::from_pixel(254, 254, *Rgba::from_slice(&[255, 0, 0, 255])),
            &gpu.read(),
        )?;
        packer.make_upload_buffer(&mut gpu.write(), &async_rt, &mut Default::default())?;
        let _ = packer.texture();

        // Grow
        let _ = packer.push_image(
            &RgbaImage::from_pixel(24, 254, *Rgba::from_slice(&[255, 0, 0, 255])),
            &gpu.read(),
        )?;
        packer.make_upload_buffer(&mut gpu.write(), &async_rt, &mut Default::default())?;
        let _ = packer.texture();

        // Reuse
        let _ = packer.push_image(
            &RgbaImage::from_pixel(24, 254, *Rgba::from_slice(&[255, 0, 0, 255])),
            &gpu.read(),
        )?;
        packer.make_upload_buffer(&mut gpu.write(), &async_rt, &mut Default::default())?;
        let _ = packer.texture();
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    #[should_panic]
    fn test_extreme_width() {
        use winit::platform::unix::EventLoopExtUnix;
        let event_loop = EventLoop::<()>::new_any_thread();
        let window = Window::new(&event_loop).unwrap();
        let interpreter = Interpreter::new();
        let gpu = Gpu::new(&window, Default::default(), &mut interpreter.write()).unwrap();

        let mut packer = AtlasPacker::<Rgba<u8>>::new(
            "test_extreme_width",
            &gpu.read(),
            256,
            256,
            [0, 0, 0, 0],
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::FilterMode::Linear,
        )
        .unwrap();
        let img = RgbaImage::from_pixel(255, 24, *Rgba::from_slice(&[255, 0, 0, 1]));
        packer.push_image(&img, &gpu.read()).unwrap();
    }

    #[cfg(unix)]
    #[test]
    #[should_panic]
    fn test_extreme_height() {
        use winit::platform::unix::EventLoopExtUnix;
        let event_loop = EventLoop::<()>::new_any_thread();
        let window = Window::new(&event_loop).unwrap();
        let interpreter = Interpreter::new();
        let gpu = Gpu::new(&window, Default::default(), &mut interpreter.write()).unwrap();

        let mut packer = AtlasPacker::<Rgba<u8>>::new(
            "test_extreme_height",
            &gpu.read(),
            256,
            256,
            [0, 0, 0, 0],
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::FilterMode::Linear,
        )
        .unwrap();
        let img = RgbaImage::from_pixel(24, 255, *Rgba::from_slice(&[255, 0, 0, 1]));
        packer.push_image(&img, &gpu.read()).unwrap();
    }
}
