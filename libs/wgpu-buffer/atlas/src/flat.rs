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
use gpu::{texture_format_size, UploadTracker, GPU};
use image::{GenericImage, ImageBuffer, Pixel};
use std::{mem, sync::Arc};

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

#[derive(Debug)]
enum DirtyState {
    Clean,
    Dirty(((u32, u32), (u32, u32))),
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
    fn new(x: u32, y: u32, width: u32, height: u32, _img_width: u32, _img_height: u32) -> Self {
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
#[derive(Debug)]
pub struct AtlasPacker<P: Pixel + 'static> {
    // Constant storage info
    fill_color: P,
    initial_width: u32,
    initial_height: u32,
    padding: u32,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsage,

    // Pack state
    width: u32,
    height: u32,
    buffer: ImageBuffer<P, Vec<P::Subpixel>>,
    columns: Vec<Column>,

    // Upload state
    dirty_region: DirtyState,
    texture: Arc<Box<wgpu::Texture>>,
    texture_view: wgpu::TextureView,
    sampler: wgpu::Sampler,
}

impl<P: Pixel + 'static> AtlasPacker<P>
where
    [P::Subpixel]: AsRef<[u8]>,
    P::Subpixel: 'static,
{
    pub fn new(
        device: &wgpu::Device,
        initial_width: u32,
        initial_height: u32,
        fill_color: P,
        format: wgpu::TextureFormat,
        mut usage: wgpu::TextureUsage,
        filter: wgpu::FilterMode,
    ) -> Self {
        usage |= wgpu::TextureUsage::COPY_DST;
        assert_eq!(texture_format_size(format) as usize, mem::size_of::<P>());
        let pix_size = mem::size_of::<P>() as u32;
        assert_eq!(
            (initial_width * pix_size) % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT,
            0
        );
        let buffer = ImageBuffer::<P, Vec<P::Subpixel>>::from_pixel(
            initial_width,
            initial_height,
            fill_color,
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
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
        let texture = Arc::new(Box::new(device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atlas-texture"),
            size: wgpu::Extent3d {
                width: initial_width,
                height: initial_height,
                depth: 1,
            },
            mip_level_count: 1, // TODO: mip-mapping for atlas textures
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
        })));
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
        Self {
            fill_color,
            initial_width,
            initial_height,
            format,
            usage: usage | wgpu::TextureUsage::COPY_DST,
            padding: 1,
            width: initial_width,
            height: initial_height,
            buffer,
            columns: vec![Column::new(0, 0)],
            // Note: texture not initialized, but no frames reference it yet.
            dirty_region: DirtyState::Clean,
            texture,
            texture_view,
            sampler,
        }
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

    pub fn push_image(&mut self, image: &ImageBuffer<P, Vec<P::Subpixel>>) -> Result<Frame> {
        let w = image.width() + self.padding;
        let h = image.height() + self.padding;
        assert!(w < self.initial_width);
        assert!(h < self.initial_height);
        let mut x_column_start = 0;
        let x_last = self.columns.last().unwrap().x_end;

        // Pack into the first segment that can take our height, adjusting the column as necessary.
        let mut position = None;
        for c in self.columns.iter_mut() {
            if h + self.padding <= self.height - c.fill_height {
                if w + self.padding <= c.x_end - x_column_start {
                    // Fits above this corner, place and expand corner up.
                    position = Some((x_column_start, c.fill_height));
                    c.fill_height += h;
                    break;
                } else if c.x_end == x_last && x_column_start + w < self.width {
                    // Does not fit width-wise, but we can expand since we are the last column.
                    position = Some((x_column_start, c.fill_height));
                    c.x_end = x_column_start + w;
                    c.fill_height += h;
                    break;
                }
            } else {
                x_column_start = c.x_end;
            }
        }

        if position.is_none() {
            // If we did not find a position above our current columns, see if there is room to insert
            // a new column and try there.
            if self.width - x_last > w {
                self.columns.push(Column::new(h, x_last + w));
                position = Some((x_last, 0));
            }
        }

        self.assert_column_constraints();

        Ok(if let Some((x, y)) = position {
            self.mark_dirty_region(x, y, w + self.padding, h + self.padding);
            self.blit(image, x + self.padding, y + self.padding)?;
            Frame::new(
                x + self.padding,
                y + self.padding,
                image.width(),
                image.height(),
                self.width,
                self.height,
            )
        } else {
            // Did not find room in this image, try the next one.
            self.grow()?;
            self.push_image(image)?
        })
    }

    fn mark_dirty_region(&mut self, x: u32, y: u32, w: u32, h: u32) {
        self.dirty_region = match self.dirty_region {
            DirtyState::Clean => DirtyState::Dirty(((x, y), (x + w, y + h))),
            DirtyState::Dirty(((lo_x, lo_y), (hi_x, hi_y))) => DirtyState::Dirty((
                (lo_x.min(x), lo_y.min(y)),
                (hi_x.max(x + w), hi_y.max(y + h)),
            )),
            DirtyState::RecreateTexture((hi_x, hi_y)) => {
                DirtyState::RecreateTexture((hi_x.max(x + w), hi_y.max(y + h)))
            }
        };
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

    pub fn buffer(&self) -> &ImageBuffer<P, Vec<P::Subpixel>> {
        &self.buffer
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
    pub fn upload(&mut self, gpu: &GPU, tracker: &mut UploadTracker) {
        match self.dirty_region {
            DirtyState::Clean => {}
            DirtyState::Dirty(((mut lo_x, lo_y), (hi_x, hi_y))) => {
                // Note: even sub-region uploads need to obey row stride constraints.
                // Note: this cannot overflow width because full width is aligned to row stride and
                //       we never overflow our width when dirtying.
                // Note: we need to adjust lo_x to account for our alignment.
                let upload_width =
                    GPU::stride_for_row_size((hi_x - lo_x) * mem::size_of::<P>() as u32)
                        / mem::size_of::<P>() as u32;
                if lo_x + upload_width >= self.width {
                    lo_x = self.width - upload_width;
                }

                let contiguous = self
                    .buffer
                    .sub_image(lo_x, lo_y, upload_width, hi_y - lo_y)
                    .to_image();
                let buffer = gpu.push_buffer(
                    "atlas-upload-buffer",
                    &contiguous.as_flat_samples().to_vec::<u8>().samples,
                    wgpu::BufferUsage::COPY_SRC,
                );
                tracker.upload_to_texture(
                    buffer,
                    self.texture.clone(),
                    wgpu::Extent3d {
                        width: upload_width,
                        height: hi_y - lo_y,
                        depth: 1,
                    },
                    self.format,
                    1,
                    wgpu::Origin3d {
                        x: lo_x,
                        y: lo_y,
                        z: 0,
                    },
                );
            }
            DirtyState::RecreateTexture((hi_x, hi_y)) => {
                // We are not in upload when we need to resize.
                // When we enter here, the CPU `buffer` is already resized. The width/height fields
                // are updated with the new requested size. We need to copy from 0,0 up to whatever
                // else has been packed this frame, which are tracked in hiX,hiY.
                self.texture = Arc::new(Box::new(gpu.device().create_texture(
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

                let upload_width = GPU::stride_for_row_size(hi_x * mem::size_of::<P>() as u32)
                    / mem::size_of::<P>() as u32;
                let contiguous = self.buffer.sub_image(0, 0, upload_width, hi_y).to_image();
                let buffer = gpu.push_buffer(
                    "atlas-upload-buffer",
                    &contiguous.as_flat_samples().to_vec::<u8>().samples,
                    wgpu::BufferUsage::COPY_SRC,
                );
                tracker.upload_to_texture(
                    buffer,
                    self.texture.clone(),
                    wgpu::Extent3d {
                        width: upload_width,
                        height: hi_y,
                        depth: 1,
                    },
                    self.format,
                    1,
                    wgpu::Origin3d::ZERO,
                );
            }
        }
        self.dirty_region = DirtyState::Clean;
    }

    /// Upload and then steal the texture. Useful when used as a one-shot atlas.
    pub fn finish(
        mut self,
        gpu: &GPU,
        tracker: &mut UploadTracker,
    ) -> (wgpu::TextureView, wgpu::Sampler) {
        self.upload(gpu, tracker);
        (self.texture_view, self.sampler)
    }

    fn grow(&mut self) -> Result<()> {
        // panic!("Cannot safely grow");
        self.width += self.initial_width;
        self.height += self.initial_height;
        let mut next_buffer = ImageBuffer::<P, Vec<P::Subpixel>>::from_pixel(
            self.width,
            self.height,
            self.fill_color,
        );
        next_buffer.copy_from(&self.buffer, 0, 0)?;
        self.dirty_region =
            DirtyState::RecreateTexture((self.buffer.width(), self.buffer.height()));
        self.buffer = next_buffer;
        Ok(())
    }

    fn blit(&mut self, other: &ImageBuffer<P, Vec<P::Subpixel>>, x: u32, y: u32) -> Result<()> {
        self.buffer.copy_from(other, x, y)?;
        Ok(())
    }

    fn assert_column_constraints(&self) {
        let mut prior = &self.columns[0];
        for c in self.columns.iter().skip(1) {
            assert!(c.x_end > prior.x_end);
            assert!(c.x_end < self.width);
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
    use std::env;
    use winit::{event_loop::EventLoop, window::Window};

    #[cfg(unix)]
    #[test]
    fn test_random_packing() -> Result<()> {
        use winit::platform::unix::EventLoopExtUnix;
        let event_loop = EventLoop::<()>::new_any_thread();
        let window = Window::new(&event_loop).unwrap();
        let interpreter = Interpreter::new();
        let gpu = GPU::new(&window, Default::default(), &mut interpreter.write())?;

        let mut packer = AtlasPacker::<Rgba<u8>>::new(
            gpu.read().device(),
            GPU::stride_for_row_size((1024 + 8) * 4) / 4,
            2048,
            *Rgba::from_slice(&[random(), random(), random(), 255]),
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsage::SAMPLED,
            wgpu::FilterMode::Linear,
        );
        let minimum = 40;
        let maximum = 200;

        for _ in 0..320 {
            let img = RgbaImage::from_pixel(
                thread_rng().gen_range(minimum..maximum),
                thread_rng().gen_range(minimum..maximum),
                *Rgba::from_slice(&[random(), random(), random(), 255]),
            );
            let frame = packer.push_image(&img)?;
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
        if env::var("DUMP") == Ok("1".to_owned()) {
            packer
                .buffer()
                .save("../../../__dump__/test_atlas_random_packing.png")
                .unwrap();
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn test_finish() -> Result<()> {
        use winit::platform::unix::EventLoopExtUnix;
        let event_loop = EventLoop::<()>::new_any_thread();
        let window = Window::new(&event_loop).unwrap();
        let interpreter = Interpreter::new();
        let gpu = GPU::new(&window, Default::default(), &mut interpreter.write())?;

        let mut packer = AtlasPacker::<Rgba<u8>>::new(
            gpu.read().device(),
            256,
            256,
            *Rgba::from_slice(&[0; 4]),
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsage::SAMPLED,
            wgpu::FilterMode::Linear,
        );
        let _ = packer.push_image(&RgbaImage::from_pixel(
            254,
            254,
            *Rgba::from_slice(&[255, 0, 0, 255]),
        ))?;
        if env::var("DUMP") == Ok("1".to_owned()) {
            packer
                .buffer()
                .save("../../../__dump__/test_atlas_upload.png")
                .unwrap();
        }

        let _ = packer.finish(&gpu.read(), &mut Default::default());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn test_incremental_upload() -> Result<()> {
        use winit::platform::unix::EventLoopExtUnix;
        let event_loop = EventLoop::<()>::new_any_thread();
        let window = Window::new(&event_loop).unwrap();
        let interpreter = Interpreter::new();
        let gpu = GPU::new(&window, Default::default(), &mut interpreter.write())?;

        let mut packer = AtlasPacker::<Rgba<u8>>::new(
            gpu.read().device(),
            256,
            256,
            *Rgba::from_slice(&[0; 4]),
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsage::SAMPLED,
            wgpu::FilterMode::Linear,
        );

        // Base upload
        let _ = packer.push_image(&RgbaImage::from_pixel(
            254,
            254,
            *Rgba::from_slice(&[255, 0, 0, 255]),
        ))?;
        packer.upload(&gpu.read(), &mut Default::default());
        let _ = packer.texture();

        // Grow
        let _ = packer.push_image(&RgbaImage::from_pixel(
            24,
            254,
            *Rgba::from_slice(&[255, 0, 0, 255]),
        ))?;
        packer.upload(&gpu.read(), &mut Default::default());
        let _ = packer.texture();

        // Reuse
        let _ = packer.push_image(&RgbaImage::from_pixel(
            24,
            254,
            *Rgba::from_slice(&[255, 0, 0, 255]),
        ))?;
        packer.upload(&gpu.read(), &mut Default::default());
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
        let gpu = GPU::new(&window, Default::default(), &mut interpreter.write()).unwrap();

        let mut packer = AtlasPacker::<Rgba<u8>>::new(
            gpu.read().device(),
            256,
            256,
            *Rgba::from_slice(&[0; 4]),
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsage::SAMPLED,
            wgpu::FilterMode::Linear,
        );
        let img = RgbaImage::from_pixel(255, 24, *Rgba::from_slice(&[255, 0, 0, 1]));
        packer.push_image(&img).unwrap();
    }

    #[cfg(unix)]
    #[test]
    #[should_panic]
    fn test_extreme_height() {
        use winit::platform::unix::EventLoopExtUnix;
        let event_loop = EventLoop::<()>::new_any_thread();
        let window = Window::new(&event_loop).unwrap();
        let interpreter = Interpreter::new();
        let gpu = GPU::new(&window, Default::default(), &mut interpreter.write()).unwrap();

        let mut packer = AtlasPacker::<Rgba<u8>>::new(
            gpu.read().device(),
            256,
            256,
            *Rgba::from_slice(&[0; 4]),
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsage::SAMPLED,
            wgpu::FilterMode::Linear,
        );
        let img = RgbaImage::from_pixel(24, 255, *Rgba::from_slice(&[255, 0, 0, 1]));
        packer.push_image(&img).unwrap();
    }
}
