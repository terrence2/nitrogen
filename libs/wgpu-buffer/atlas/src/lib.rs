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
use gpu::{texture_format_size, GPU};
use image::{GenericImage, ImageBuffer, Pixel};
use std::{mem, num::NonZeroU32};

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
pub struct TexCoord {
    s: f32,
    t: f32,
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
#[derive(Debug)]
pub struct Frame {
    coord0: TexCoord,
    coord1: TexCoord,
    offset: f32,
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

// Trades off pack complexity against efficiency. This packer is designed for online usage
// so tries to be faster to pack at the cost of potentially loosing out on easy space wins
// in cases where subsequent items are differently sized or shaped. Most common uses will only
// feed similarly shaped items, so will generally be fine.
// FIXME: padding is doubled at non-borders
pub struct AtlasPacker<P: Pixel + 'static> {
    // Storage info
    fill_color: P,
    width: u32,
    height: u32,
    padding: u32,

    // Pack state
    buffer_offset: usize,
    buffers: Vec<ImageBuffer<P, Vec<P::Subpixel>>>,
    column_sets: Vec<Vec<Column>>,
}

impl<P: Pixel + 'static> AtlasPacker<P>
where
    [P::Subpixel]: AsRef<[u8]>,
    P::Subpixel: 'static,
{
    pub fn new(width: u32, height: u32, fill_color: P) -> Self {
        let pix_size = mem::size_of::<P>() as u32;
        assert_eq!((width * pix_size) % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT, 0);
        let mut packer = Self {
            fill_color,
            width,
            height,
            padding: 1,
            buffers: vec![],
            buffer_offset: 0,
            column_sets: vec![],
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
        let w = image.width() + self.padding;
        let h = image.height() + self.padding;
        assert!(w < self.width);
        assert!(h < self.height);
        let mut x_column_start = 0;
        let x_last = self.column_sets[self.buffer_offset].last().unwrap().x_end;

        // Pack into the first segment that can take our height, adjusting the column as necessary.
        let mut position = None;
        for c in self.column_sets[self.buffer_offset].iter_mut() {
            if h < self.height - c.fill_height {
                if w < c.x_end - x_column_start {
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

    pub fn buffers(&self) -> &[ImageBuffer<P, Vec<P::Subpixel>>] {
        &self.buffers
    }

    /// Upload the current contents to the GPU. Note that this is non-destructive. If needed,
    /// the builder can accumulate more textures and upload again later.
    pub fn upload(
        &self,
        gpu: &mut GPU,
        format: wgpu::TextureFormat,
        usage: wgpu::TextureUsage,
    ) -> wgpu::TextureView {
        assert_eq!(texture_format_size(format) as usize, mem::size_of::<P>());
        let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("atlas-texture"),
            size: wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth: 1,
            },
            mip_level_count: 1, // TODO: mip-mapping for atlas textures
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: usage | wgpu::TextureUsage::COPY_DST,
        });
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("atlas-texture-view"),
            format: None,
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            level_count: None, // mip_
            base_array_layer: 0,
            array_layer_count: NonZeroU32::new(self.buffer_offset as u32),
        });

        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("shape-chunk-texture-atlas-uploader-command-encoder"),
            });
        for (i, image) in self.buffers.iter().enumerate() {
            let buffer = gpu.push_buffer(
                "atlas-upload-buffer",
                &image.as_flat_samples().to_vec::<u8>().samples,
                wgpu::BufferUsage::COPY_SRC,
            );
            encoder.copy_buffer_to_texture(
                wgpu::BufferCopyView {
                    buffer: &buffer,
                    layout: wgpu::TextureDataLayout {
                        offset: 0,
                        bytes_per_row: image.width() * 4,
                        rows_per_image: image.height(),
                    },
                },
                wgpu::TextureCopyView {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: i as u32,
                    },
                },
                wgpu::Extent3d {
                    width: image.width(),
                    height: image.height(),
                    depth: 1,
                },
            );
        }
        gpu.queue_mut().submit(vec![encoder.finish()]);

        texture_view
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
                assert!(c.fill_height < self.height);
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
    fn test_upload() {
        let mut packer = AtlasPacker::<Rgba<u8>>::new(256, 256, *Rgba::from_slice(&[0; 4]));
        let _ = packer.push_image(&RgbaImage::from_pixel(
            254,
            254,
            *Rgba::from_slice(&[255, 0, 0, 1]),
        ));

        use winit::platform::unix::EventLoopExtUnix;
        let event_loop = EventLoop::<()>::new_any_thread();
        let window = Window::new(&event_loop).unwrap();
        let mut gpu = GPU::new(&window, Default::default()).unwrap();

        let _ = packer.upload(
            &mut gpu,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsage::SAMPLED,
        );
    }

    #[test]
    #[should_panic]
    fn test_extreme_width() {
        let mut packer = AtlasPacker::<Rgba<u8>>::new(256, 256, *Rgba::from_slice(&[0; 4]));
        let img = RgbaImage::from_pixel(255, 24, *Rgba::from_slice(&[255, 0, 0, 1]));
        packer.push_image(&img);
    }

    #[test]
    #[should_panic]
    fn test_extreme_height() {
        let mut packer = AtlasPacker::<Rgba<u8>>::new(256, 256, *Rgba::from_slice(&[0; 4]));
        let img = RgbaImage::from_pixel(24, 255, *Rgba::from_slice(&[255, 0, 0, 1]));
        packer.push_image(&img);
    }
}
