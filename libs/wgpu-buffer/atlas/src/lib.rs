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
use gpu::{texture_format_component_type, texture_format_size, UploadTracker, GPU};
use image::{GenericImage, ImageBuffer, Pixel};
use std::{mem, num::NonZeroU32, sync::Arc};

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

#[derive(Copy, Clone, Debug)]
pub struct TexCoord {
    pub s: f32,
    pub t: f32,
}

impl TexCoord {
    fn new(x: u32, y: u32, img_width: u32, img_height: u32) -> Self {
        Self {
            s: x as f32 / img_width as f32,
            t: (img_height - y) as f32 / img_height as f32,
        }
    }
}

// The Frame tells our renderer how to get back to the texture in our eventual Atlas.
#[derive(Copy, Clone, Debug)]
pub struct Frame {
    pub coord0: TexCoord,
    pub coord1: TexCoord,
    pub offset: f32,
}

impl Frame {
    fn new(
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        offset: usize,
        img_width: u32,
        img_height: u32,
    ) -> Self {
        assert!(offset < 2u32.pow(std::f32::MANTISSA_DIGITS) as usize);
        Self {
            coord0: TexCoord::new(x, y, img_width, img_height),
            coord1: TexCoord::new(x + width, y + height, img_width, img_height),
            offset: offset as f32,
        }
    }
}

// Trades off pack complexity against efficiency. This packer is designed for online, incremental
// usage, so tries to be faster to pack at the cost of potentially loosing out on easy space wins
// in cases where subsequent items are differently sized or shaped. Most common uses will only
// feed similarly shaped items, so will generally be fine.
pub struct AtlasPacker<P: Pixel + 'static> {
    // Constant storage info
    fill_color: P,
    width: u32,
    height: u32,
    padding: u32,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsage,

    // Pack state
    buffer_offset: usize,
    buffers: Vec<ImageBuffer<P, Vec<P::Subpixel>>>,
    column_sets: Vec<Vec<Column>>,

    // Upload state
    dirty: bool,
    last_uploaded_offset: usize,
    texture_capacity: u32,
    texture: Option<Arc<Box<wgpu::Texture>>>,
    texture_view: Option<wgpu::TextureView>,
    sampler: wgpu::Sampler,
}

impl<P: Pixel + 'static> AtlasPacker<P>
where
    [P::Subpixel]: AsRef<[u8]>,
    P::Subpixel: 'static,
{
    const MAX_LAYERS: u32 = 32;

    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        fill_color: P,
        format: wgpu::TextureFormat,
        usage: wgpu::TextureUsage,
        filter: wgpu::FilterMode,
    ) -> Self {
        assert_eq!(texture_format_size(format) as usize, mem::size_of::<P>());
        let pix_size = mem::size_of::<P>() as u32;
        assert_eq!((width * pix_size) % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT, 0);
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
        });
        let mut packer = Self {
            fill_color,
            width,
            height,
            format,
            usage: usage | wgpu::TextureUsage::COPY_DST,
            padding: 1,
            buffers: vec![],
            buffer_offset: 0,
            column_sets: vec![],
            dirty: true,
            last_uploaded_offset: 0,
            texture_capacity: 0,
            texture: None,
            texture_view: None,
            sampler,
        };
        packer.add_plane();
        packer.buffer_offset = 0;
        packer
    }

    pub fn with_padding(mut self, padding: u32) -> Self {
        self.padding = padding;
        self
    }

    pub fn push_image(&mut self, image: &ImageBuffer<P, Vec<P::Subpixel>>) -> Frame {
        self.dirty = true;

        let w = image.width() + self.padding;
        let h = image.height() + self.padding;
        assert!(w < self.width);
        assert!(h < self.height);
        let mut x_column_start = 0;
        let x_last = self.column_sets[self.buffer_offset].last().unwrap().x_end;

        // Pack into the first segment that can take our height, adjusting the column as necessary.
        let mut position = None;
        for c in self.column_sets[self.buffer_offset].iter_mut() {
            if h <= self.height - c.fill_height {
                if w <= c.x_end - x_column_start {
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
                self.column_sets[self.buffer_offset].push(Column::new(h, x_last + w));
                position = Some((x_last, 0));
            }
        }

        self.assert_column_constraints();

        if let Some((x, y)) = position {
            self.blit(image, x + self.padding, y + self.padding);
            Frame::new(
                x + self.padding,
                y + self.padding,
                image.width(),
                image.height(),
                self.buffer_offset,
                self.width,
                self.height,
            )
        } else {
            // Did not find room in this image, try the next one.
            self.add_plane();
            self.push_image(image)
        }
    }

    pub fn images(&self) -> &[ImageBuffer<P, Vec<P::Subpixel>>] {
        &self.buffers
    }

    pub fn texture_layout_entry(&self, binding: u32) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStage::FRAGMENT,
            ty: wgpu::BindingType::SampledTexture {
                dimension: wgpu::TextureViewDimension::D2Array,
                component_type: texture_format_component_type(self.format),
                multisampled: false,
            },
            count: NonZeroU32::new(Self::MAX_LAYERS),
        }
    }

    pub fn sampler_layout_entry(&self, binding: u32) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStage::FRAGMENT,
            ty: wgpu::BindingType::Sampler { comparison: false },
            count: None,
        }
    }

    pub fn texture_binding(&self, binding: u32) -> wgpu::BindGroupEntry {
        wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::TextureView(self.texture_view.as_ref().unwrap()),
        }
    }
    pub fn sampler_binding(&self, binding: u32) -> wgpu::BindGroupEntry {
        wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::Sampler(&self.sampler),
        }
    }

    /// Note: panics if no upload has happened.
    pub fn texture(&self) -> &wgpu::Texture {
        self.texture.as_ref().unwrap()
    }

    /// Note: panics if no upload has happened.
    pub fn texture_view(&self) -> &wgpu::TextureView {
        self.texture_view.as_ref().unwrap()
    }

    /// Upload the current contents to the GPU. Note that this is non-destructive. If needed,
    /// the builder can accumulate more textures and upload again later.
    pub fn upload(&mut self, gpu: &GPU, tracker: &mut UploadTracker) {
        if !self.dirty && self.texture.is_some() {
            return;
        }

        if self.texture.is_none() {
            assert!(self.texture_view.is_none());
            assert_eq!(self.texture_capacity, 0);
            assert_eq!(self.last_uploaded_offset, 0);

            self.last_uploaded_offset = self.buffer_offset;
            self.texture_capacity = self.buffer_offset as u32 + 1;
            self.texture = Some(Arc::new(Box::new(gpu.device().create_texture(
                &wgpu::TextureDescriptor {
                    label: Some("atlas-texture"),
                    size: wgpu::Extent3d {
                        width: self.width,
                        height: self.height,
                        depth: self.texture_capacity,
                    },
                    mip_level_count: 1, // TODO: mip-mapping for atlas textures
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: self.format,
                    usage: self.usage,
                },
            ))));
            self.texture_view = Some(self.texture.as_ref().unwrap().create_view(
                &wgpu::TextureViewDescriptor {
                    label: Some("atlas-texture-view"),
                    format: None,
                    dimension: Some(wgpu::TextureViewDimension::D2Array),
                    aspect: wgpu::TextureAspect::All,
                    base_mip_level: 0,
                    level_count: None, // mip_
                    base_array_layer: 0,
                    array_layer_count: NonZeroU32::new(self.texture_capacity),
                },
            ));
            for (i, image) in self.buffers.iter().enumerate() {
                let buffer = gpu.push_buffer(
                    "atlas-upload-buffer",
                    &image.as_flat_samples().to_vec::<u8>().samples,
                    wgpu::BufferUsage::COPY_SRC,
                );
                tracker.upload_to_texture(
                    buffer,
                    self.texture.clone().unwrap(),
                    wgpu::Extent3d {
                        width: image.width(),
                        height: image.height(),
                        depth: 1,
                    },
                    self.format,
                    i as u32,
                    1,
                );
            }
        } else if self.last_uploaded_offset == self.buffer_offset {
            // Common case: we have not allocated a new buffer since our last upload.
            // Note: dirty bit check above filters out the case where we have not added items.
            // No need to re-create texture, just upload the single last texture.
            let image = &self.buffers[self.buffer_offset];
            let buffer = gpu.push_buffer(
                "atlas-upload-buffer",
                &image.as_flat_samples().to_vec::<u8>().samples,
                wgpu::BufferUsage::COPY_SRC,
            );
            tracker.upload_to_texture(
                buffer,
                self.texture.clone().unwrap(),
                wgpu::Extent3d {
                    width: image.width(),
                    height: image.height(),
                    depth: 1,
                },
                self.format,
                self.last_uploaded_offset as u32,
                1,
            );
        } else {
            // Otherwise, our texture is too small. We need to re-allocate, do a GPU-GPU copy of
            // everything up to last_uploaded_offset, then copy from last_uploaded to size from
            // the CPU to GPU.
            assert!((self.texture_capacity as usize) < self.buffer_offset + 1);
            let next_texture_capacity = self.buffer_offset as u32 + 1;
            let next_texture = Arc::new(Box::new(gpu.device().create_texture(
                &wgpu::TextureDescriptor {
                    label: Some("atlas-texture"),
                    size: wgpu::Extent3d {
                        width: self.width,
                        height: self.height,
                        depth: next_texture_capacity,
                    },
                    mip_level_count: 1, // TODO: mip-mapping for atlas textures
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: self.format,
                    usage: self.usage,
                },
            )));
            let next_texture_view = next_texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("atlas-texture-view"),
                format: None,
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                level_count: None, // mip_
                base_array_layer: 0,
                array_layer_count: NonZeroU32::new(next_texture_capacity),
            });
            for i in 0..self.last_uploaded_offset {
                tracker.copy_texture_to_texture(
                    self.texture.clone().unwrap(),
                    i as u32,
                    next_texture.clone(),
                    i as u32,
                    wgpu::Extent3d {
                        width: self.width,
                        height: self.height,
                        depth: 1,
                    },
                );
            }
            for i in self.last_uploaded_offset..self.buffer_offset {
                let image = &self.buffers[i];
                let buffer = gpu.push_buffer(
                    "atlas-upload-buffer",
                    &image.as_flat_samples().to_vec::<u8>().samples,
                    wgpu::BufferUsage::COPY_SRC,
                );
                tracker.upload_to_texture(
                    buffer,
                    self.texture.clone().unwrap(),
                    wgpu::Extent3d {
                        width: image.width(),
                        height: image.height(),
                        depth: 1,
                    },
                    self.format,
                    i as u32,
                    1,
                );
            }
            self.texture = Some(next_texture);
            self.texture_view = Some(next_texture_view);
            self.texture_capacity = next_texture_capacity;
            self.last_uploaded_offset = self.buffer_offset;
        }
        self.dirty = false;
    }

    /// Upload and then steal the texture. Useful when used as a one-shot atlas.
    pub fn finish(
        mut self,
        gpu: &GPU,
        tracker: &mut UploadTracker,
    ) -> (wgpu::TextureView, wgpu::Sampler) {
        self.upload(gpu, tracker);
        (self.texture_view.unwrap(), self.sampler)
    }

    fn add_plane(&mut self) {
        self.buffers.push(ImageBuffer::from_pixel(
            self.width,
            self.height,
            self.fill_color,
        ));
        self.column_sets.push(vec![Column::new(1, 1)]);
        self.buffer_offset += 1;
    }

    fn blit(&mut self, other: &ImageBuffer<P, Vec<P::Subpixel>>, x: u32, y: u32) {
        self.buffers[self.buffer_offset].copy_from(other, x, y);
    }

    fn assert_column_constraints(&self) {
        for columns in &self.column_sets {
            let mut prior = &columns[0];
            for c in columns.iter().skip(1) {
                assert!(c.x_end > prior.x_end);
                assert!(c.x_end < self.width);
                assert!(c.fill_height <= self.height);
                prior = c;
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use image::{Rgba, RgbaImage};
    use rand::prelude::*;
    use std::env;
    use winit::{event_loop::EventLoop, window::Window};

    #[test]
    fn test_random_packing() {
        let mut packer = AtlasPacker::<Rgba<u8>>::new(
            GPU::stride_for_row_size((1024 + 8) * 3),
            2048,
            *Rgba::from_slice(&[random(), random(), random(), 255]),
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsage::SAMPLED,
        );
        let minimum = 40;
        let maximum = 200;

        for _ in 0..320 {
            let img = RgbaImage::from_pixel(
                thread_rng().gen_range(minimum, maximum),
                thread_rng().gen_range(minimum, maximum),
                *Rgba::from_slice(&[random(), random(), random(), 255]),
            );
            let frame = packer.push_image(&img);
            // Frame edges should keep these from ever being full.
            assert!(frame.coord0.s > 0.0);
            assert!(frame.coord0.s < 1.0);
            assert!(frame.coord0.t > 0.0);
            assert!(frame.coord0.t < 1.0);
            // Orientation
            assert!(frame.coord0.s < frame.coord1.s);
            assert!(frame.coord0.t > frame.coord1.t);
        }
        if env::var("DUMP") == Ok("1".to_owned()) {
            for (i, buf) in packer.buffers().iter().enumerate() {
                buf.save(&format!(
                    "../../../__dump__/test_atlas_random_packing{}.png",
                    i
                ))
                .unwrap();
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn test_finish() {
        let mut packer = AtlasPacker::<Rgba<u8>>::new(
            256,
            256,
            *Rgba::from_slice(&[0; 4]),
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsage::SAMPLED,
        );
        let _ = packer.push_image(&RgbaImage::from_pixel(
            254,
            254,
            *Rgba::from_slice(&[255, 0, 0, 255]),
        ));
        if env::var("DUMP") == Ok("1".to_owned()) {
            for (i, buf) in packer.buffers().iter().enumerate() {
                buf.save(&format!("../../../__dump__/test_atlas_upload{}.png", i))
                    .unwrap();
            }
        }

        use winit::platform::unix::EventLoopExtUnix;
        let event_loop = EventLoop::<()>::new_any_thread();
        let window = Window::new(&event_loop).unwrap();
        let mut gpu = GPU::new(&window, Default::default()).unwrap();

        let _ = packer.finish(&mut gpu);
    }

    #[cfg(unix)]
    #[test]
    fn test_incremental_upload() {
        use winit::platform::unix::EventLoopExtUnix;
        let event_loop = EventLoop::<()>::new_any_thread();
        let window = Window::new(&event_loop).unwrap();
        let mut gpu = GPU::new(&window, Default::default()).unwrap();

        let mut packer = AtlasPacker::<Rgba<u8>>::new(
            256,
            256,
            *Rgba::from_slice(&[0; 4]),
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsage::SAMPLED,
        );

        // Base upload
        let _ = packer.push_image(&RgbaImage::from_pixel(
            254,
            254,
            *Rgba::from_slice(&[255, 0, 0, 255]),
        ));
        packer.upload(&mut gpu);
        let _ = packer.texture();

        // Grow
        let _ = packer.push_image(&RgbaImage::from_pixel(
            24,
            254,
            *Rgba::from_slice(&[255, 0, 0, 255]),
        ));
        packer.upload(&mut gpu);
        let _ = packer.texture();

        // Reuse
        let _ = packer.push_image(&RgbaImage::from_pixel(
            24,
            254,
            *Rgba::from_slice(&[255, 0, 0, 255]),
        ));
        packer.upload(&mut gpu);
        let _ = packer.texture();
    }

    #[test]
    #[should_panic]
    fn test_extreme_width() {
        let mut packer = AtlasPacker::<Rgba<u8>>::new(
            256,
            256,
            *Rgba::from_slice(&[0; 4]),
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsage::SAMPLED,
        );
        let img = RgbaImage::from_pixel(255, 24, *Rgba::from_slice(&[255, 0, 0, 1]));
        packer.push_image(&img);
    }

    #[test]
    #[should_panic]
    fn test_extreme_height() {
        let mut packer = AtlasPacker::<Rgba<u8>>::new(
            256,
            256,
            *Rgba::from_slice(&[0; 4]),
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsage::SAMPLED,
        );
        let img = RgbaImage::from_pixel(24, 255, *Rgba::from_slice(&[255, 0, 0, 1]));
        packer.push_image(&img);
    }
}
