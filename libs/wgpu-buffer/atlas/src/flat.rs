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
use geometry::Aabb;
use gpu::{texture_format_size, ArcTextureCopyView, Gpu, OwnedBufferCopyView, UploadTracker};
use image::{ImageBuffer, Luma, Pixel, Rgba};
use log::debug;
use std::{marker::PhantomData, mem, num::NonZeroU32, path::PathBuf, sync::Arc};
use wgpu::Origin3d;
use zerocopy::{AsBytes, FromBytes};

#[repr(C)]
#[derive(AsBytes, FromBytes, Clone, Copy, Debug)]
pub struct BlitVertex {
    _pos: [f32; 2],
    _tc: [f32; 2],
}

impl BlitVertex {
    pub fn new(pos: [f32; 2], tc: [f32; 2]) -> Self {
        Self { _pos: pos, _tc: tc }
    }

    pub fn buffer(
        gpu: &Gpu,
        (x, y): (u32, u32),
        (w, h): (u32, u32),
        (width, height): (u32, u32),
    ) -> wgpu::Buffer {
        let x0 = (x as f32 / width as f32) * 2. - 1.;
        let x1 = ((x + w) as f32 / width as f32) * 2. - 1.;
        let y0 = 1. - (y as f32 / height as f32) * 2.;
        let y1 = 1. - ((y + h) as f32 / height as f32) * 2.;
        let vertices = vec![
            Self::new([x0, y1], [0., 1.]),
            Self::new([x0, y0], [0., 0.]),
            Self::new([x1, y1], [1., 1.]),
            Self::new([x1, y0], [1., 0.]),
        ];
        gpu.push_slice("blit-vertices", &vertices, wgpu::BufferUsages::VERTEX)
    }

    pub fn descriptor() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 4 * 2,
                    shader_location: 1,
                },
            ],
        }
    }
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

    pub fn raw_base(&self) -> (u32, u32) {
        (self.s0, self.t0)
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

#[derive(Debug)]
struct BlitItem {
    img_buffer: wgpu::Buffer,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    stride_bytes: u32,
}

impl BlitItem {
    fn new(
        img_buffer: wgpu::Buffer,
        (x, y): (u32, u32),
        (width, height, stride_bytes): (u32, u32, u32),
    ) -> Self {
        Self {
            img_buffer,
            x,
            y,
            width,
            height,
            stride_bytes,
        }
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
    initial_width: u32,
    initial_height: u32,
    padding: u32,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,

    // Pack state
    width: u32,
    height: u32,
    columns: Vec<Column>,

    // Upload state
    dirty_region: DirtyState,
    texture: Arc<wgpu::Texture>,
    texture_view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    dump_texture: Option<PathBuf>,
    next_texture: Option<Arc<wgpu::Texture>>,

    // CPU-side list of buffers that need to be blit into the target texture these can either
    // get directly encoded for aligned upload-as-copy, or need to get deferred to a gpu compute
    // pass for unaligned and palettized uploads.
    blit_list: Vec<BlitItem>,
    unaligned_blit_bind_group_layout: wgpu::BindGroupLayout,
    unaligned_blit_texture_sampler: wgpu::Sampler,
    unaligned_blit_pipeline: wgpu::RenderPipeline,
    unaligned_blit: Vec<(wgpu::BindGroup, wgpu::Buffer)>,

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
        format: wgpu::TextureFormat,
        filter: wgpu::FilterMode,
    ) -> Result<Self> {
        let usage = wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT;
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
        let texture = Arc::new(gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("atlas-texture"),
            size: wgpu::Extent3d {
                width: initial_width,
                height: initial_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1, // TODO: mip-mapping for atlas textures?
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
        }));
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("atlas-texture-view"),
            format: None,
            dimension: None,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None, // mip_
            base_array_layer: 0,
            array_layer_count: None,
        });
        // Note: should be straight pixel-to-pixel copy so no filter
        let unaligned_blit_texture_sampler =
            gpu.device().create_sampler(&wgpu::SamplerDescriptor {
                label: Some("unaligned-blit-sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                mipmap_filter: wgpu::FilterMode::Nearest,
                lod_min_clamp: 0.0,
                lod_max_clamp: 0.0,
                compare: None,
                anisotropy_clamp: None,
                border_color: None,
            });

        let upload_unaligned_bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("atlas-upload-unaligned-bind-group-layout"),
                    entries: &[
                        // Texture Source
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        // Sampler
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                            count: None,
                        },
                    ],
                });

        let unaligned_blit_pipeline =
            gpu.device()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("atlas-unaligned-blit-pipeline"),
                    layout: Some(&gpu.device().create_pipeline_layout(
                        &wgpu::PipelineLayoutDescriptor {
                            label: Some("atlas-unaligned-blit-pipeline-layout"),
                            push_constant_ranges: &[],
                            bind_group_layouts: &[&upload_unaligned_bind_group_layout],
                        },
                    )),
                    vertex: wgpu::VertexState {
                        module: &gpu.create_shader_module(
                            "unaligned_blit.vert",
                            include_bytes!("../target/unaligned_blit.vert.spirv"),
                        )?,
                        entry_point: "main",
                        buffers: &[BlitVertex::descriptor()],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &gpu.create_shader_module(
                            "unaligned_blit.frag",
                            include_bytes!("../target/unaligned_blit.frag.spirv"),
                        )?,
                        entry_point: "main",
                        targets: &[wgpu::ColorTargetState {
                            format,
                            blend: None,
                            write_mask: wgpu::ColorWrites::all(),
                        }],
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleStrip,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Cw,
                        cull_mode: Some(wgpu::Face::Back),
                        unclipped_depth: false,
                        polygon_mode: wgpu::PolygonMode::Fill,
                        conservative: false,
                    },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState {
                        count: 1,
                        mask: !0,
                        alpha_to_coverage_enabled: false,
                    },
                    multiview: None,
                });

        Ok(Self {
            name: name.into(),
            initial_width,
            initial_height,
            format,
            usage,
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

            unaligned_blit_bind_group_layout: upload_unaligned_bind_group_layout,
            unaligned_blit_texture_sampler,
            unaligned_blit_pipeline,
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

    pub fn atlas_size(&self) -> usize {
        self.width as usize * self.height as usize * mem::size_of::<P>()
    }

    pub fn with_padding(mut self, padding: u32) -> Self {
        self.padding = padding;
        self
    }

    pub fn dump(&mut self, path: PathBuf) {
        self.dump_texture = Some(path);
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
        img_buffer: wgpu::Buffer,
        width: u32,
        height: u32,
        stride_bytes: u32,
    ) -> Result<Frame> {
        let (x, y) = self.do_layout(width, height);
        self.blit_list.push(BlitItem::new(
            img_buffer,
            (x + self.padding, y + self.padding),
            (width, height, stride_bytes),
        ));
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
        let native_stride = image.width() * mem::size_of::<P>() as u32;
        let upload_stride = Gpu::stride_for_row_size(native_stride);
        if upload_stride == native_stride {
            return self.push_aligned_image(image, gpu);
        }
        let upload_width = upload_stride / mem::size_of::<P>() as u32;
        let mut upload_img = ImageBuffer::new(upload_width, image.height());
        for (x, y, p) in image.enumerate_pixels() {
            *upload_img.get_pixel_mut(x, y) = *p;
        }
        let img_buffer = gpu.push_buffer(
            "atlas-image-upload",
            upload_img.as_bytes(),
            wgpu::BufferUsages::COPY_SRC,
        );
        self.push_buffer(img_buffer, image.width(), image.height(), upload_stride)
    }

    pub fn push_aligned_image(
        &mut self,
        image: &ImageBuffer<P, Vec<P::Subpixel>>,
        gpu: &Gpu,
    ) -> Result<Frame> {
        let native_stride = image.width() * mem::size_of::<P>() as u32;
        let upload_stride = Gpu::stride_for_row_size(native_stride);
        assert_eq!(native_stride % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT, 0);
        assert_eq!(native_stride, upload_stride);
        let img_buffer = gpu.push_buffer(
            "atlas-image-upload",
            image.as_bytes(),
            wgpu::BufferUsages::COPY_SRC,
        );
        self.push_buffer(img_buffer, image.width(), image.height(), upload_stride)
    }

    pub fn texture_layout_entry(&self, binding: u32) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
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
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
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

    pub fn texture_view(&self) -> &wgpu::TextureView {
        &self.texture_view
    }

    /// Upload the current contents to the GPU. Note that this is non-destructive. If needed,
    /// the builder can accumulate more textures and upload again later.
    pub fn make_upload_buffer(&mut self, gpu: &Gpu, tracker: &UploadTracker) -> Result<()> {
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
                mip_level_count: None, // mip_
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
                let next_texture =
                    Arc::new(gpu.device().create_texture(&wgpu::TextureDescriptor {
                        label: Some("atlas-texture"),
                        size: wgpu::Extent3d {
                            width: self.width,
                            height: self.height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1, // TODO: mip-mapping for atlas textures
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: self.format,
                        usage: self.usage,
                    }));
                tracker.copy_texture_to_texture(
                    self.texture.clone(),
                    0,
                    next_texture.clone(),
                    0,
                    wgpu::Extent3d {
                        width: hi_x,
                        height: hi_y,
                        depth_or_array_layers: 1,
                    },
                );
                self.next_texture = Some(next_texture);
            }
        }
        self.dirty_region = DirtyState::Clean;

        // Set up texture blits
        self.unaligned_blit.clear();
        for item in self.blit_list.drain(..) {
            let img_extent = wgpu::Extent3d {
                width: item.width,
                height: item.height,
                depth_or_array_layers: 1,
            };
            let img_texture = Arc::new(gpu.device().create_texture(&wgpu::TextureDescriptor {
                label: Some("atlas-img-upload-texture"),
                size: img_extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            }));
            tracker.copy_owned_buffer_to_arc_texture(
                OwnedBufferCopyView {
                    buffer: item.img_buffer,
                    layout: wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: NonZeroU32::new(item.stride_bytes),
                        rows_per_image: NonZeroU32::new(item.height),
                    },
                },
                ArcTextureCopyView {
                    texture: img_texture.clone(),
                    mip_level: 0,
                    origin: Origin3d::ZERO,
                },
                img_extent,
            );
            let img_view = img_texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("atlas-img-upload-view"),
                format: None,
                dimension: None,
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                mip_level_count: None,
                base_array_layer: 0,
                array_layer_count: None,
            });
            let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("atlas-upload-unaligned-bind-group"),
                layout: &self.unaligned_blit_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&img_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(
                            &self.unaligned_blit_texture_sampler,
                        ),
                    },
                ],
            });
            let vertex_buffer = BlitVertex::buffer(
                gpu,
                (item.x, item.y),
                (item.width, item.height),
                (self.width, self.height),
            );
            self.unaligned_blit.push((bind_group, vertex_buffer));
        }

        if let Some(path_ref) = self.dump_texture.as_ref() {
            let path = path_ref.to_owned();
            let write_img =
                |extent: wgpu::Extent3d, fmt: wgpu::TextureFormat, data: Vec<u8>| match fmt {
                    wgpu::TextureFormat::R8Unorm => {
                        let img =
                            ImageBuffer::<Luma<u8>, _>::from_raw(extent.width, extent.height, data)
                                .expect("built image");
                        println!("writing to {}", path.to_string_lossy());
                        img.save(path).expect("wrote file");
                    }
                    wgpu::TextureFormat::Rgba8Unorm => {
                        let img =
                            ImageBuffer::<Rgba<u8>, _>::from_raw(extent.width, extent.height, data)
                                .expect("built image");
                        println!("writing to {}", path.to_string_lossy());
                        img.save(path).expect("wrote file");
                    }
                    _ => panic!("don't know how to dump texture format: {:?}", fmt),
                };
            // Gpu::dump_texture(
            //     &self.texture,
            //     wgpu::Extent3d {
            //         width: self.width,
            //         height: self.height,
            //         depth_or_array_layers: 1,
            //     },
            //     self.format,
            //     gpu,
            //     Box::new(write_img),
            // )?;
        }
        self.dump_texture = None;

        Ok(())
    }

    pub fn maintain_gpu_resources(&self, encoder: &mut wgpu::CommandEncoder) {
        let target_texture = if let Some(ref next_texture) = self.next_texture {
            next_texture.clone()
        } else {
            self.texture.clone()
        };
        let target_view = target_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("atlas-texture-view"),
            format: None,
            dimension: None,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None, // mip_
            base_array_layer: 0,
            array_layer_count: None,
        });
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("atlas-finish-render-pass"),
            color_attachments: &[wgpu::RenderPassColorAttachment {
                view: &target_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: true,
                },
            }],
            depth_stencil_attachment: None,
        });
        rpass.set_pipeline(&self.unaligned_blit_pipeline);
        for (bind_group, vertex_buffer) in &self.unaligned_blit {
            rpass.set_bind_group(0, bind_group, &[]);
            rpass.set_vertex_buffer(0, vertex_buffer.slice(..));
            rpass.draw(0..4, 0..1);
        }
    }

    /// Upload and then steal the texture. Useful when used as a one-shot atlas.
    pub fn finish(
        mut self,
        gpu: &mut Gpu,
        tracker: &mut UploadTracker,
    ) -> Result<(Arc<wgpu::Texture>, wgpu::TextureView, wgpu::Sampler)> {
        // Note: we need to crank make_upload_buffer twice because of the way
        // we defer moving to a new texture to ensure in-flight uploads happen.
        self.make_upload_buffer(gpu, tracker)?;

        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("atlas-finish"),
            });
        tracker.dispatch_uploads_until_empty(&mut encoder);
        self.maintain_gpu_resources(&mut encoder);
        gpu.queue_mut().submit(vec![encoder.finish()]);

        self.make_upload_buffer(gpu, tracker)?;

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
        let img = Aabb::new(
            [lo_x + self.padding, lo_y + self.padding],
            [lo_x + w, lo_y + h],
        );
        let mut c_x_start = 0;
        for c in self.columns.iter() {
            let col = Aabb::new([c_x_start, 0], [c.x_end, c.fill_height]);
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
    use image::{GrayImage, Luma, Rgba, RgbaImage};
    use rand::prelude::*;
    use std::{env, time::Duration};

    #[cfg(unix)]
    #[test]
    fn test_random_packing() -> Result<()> {
        let mut runtime = Gpu::for_test_unix()?;
        let mut gpu = runtime.resource_mut::<Gpu>();

        let mut packer = AtlasPacker::<Rgba<u8>>::new(
            "random_packing",
            &gpu,
            Gpu::stride_for_row_size((1024 + 8) * 4) / 4,
            2048,
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
            let frame = packer.push_image(&img, &gpu)?;
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
            depth_or_array_layers: 1,
        };
        let (texture, _view, _sampler) = packer.finish(&mut gpu, &mut Default::default())?;
        if env::var("DUMP") == Ok("1".to_owned()) {
            Gpu::dump_texture(
                &texture,
                extent,
                wgpu::TextureFormat::Rgba8Unorm,
                &mut gpu,
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
            // Shutting down tokio kills all tasks, so give ourself a chance to run.
            std::thread::sleep(Duration::from_secs(1));
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn test_finish() -> Result<()> {
        let mut runtime = Gpu::for_test_unix()?;
        let mut gpu = runtime.resource_mut::<Gpu>();

        let mut packer = AtlasPacker::<Rgba<u8>>::new(
            "test_finish",
            &gpu,
            256,
            256,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::FilterMode::Linear,
        )?;
        let _ = packer.push_image(
            &RgbaImage::from_pixel(254, 254, *Rgba::from_slice(&[255, 0, 0, 255])),
            &gpu,
        )?;

        let _ = packer.finish(&mut gpu, &mut Default::default());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn test_grayscale() -> Result<()> {
        let mut runtime = Gpu::for_test_unix()?;
        let mut gpu = runtime.resource_mut::<Gpu>();

        let mut packer = AtlasPacker::<Luma<u8>>::new(
            "test_grayscale",
            &gpu,
            256,
            256,
            wgpu::TextureFormat::R8Unorm,
            wgpu::FilterMode::Linear,
        )?;
        let _ = packer.push_image(
            &GrayImage::from_pixel(254, 254, *Luma::from_slice(&[255])),
            &gpu,
        )?;

        let _ = packer.finish(&mut gpu, &mut Default::default());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn test_incremental_upload() -> Result<()> {
        let runtime = Gpu::for_test_unix()?;
        let gpu = runtime.resource::<Gpu>();

        let mut packer = AtlasPacker::<Rgba<u8>>::new(
            "test_incremental",
            gpu,
            256,
            256,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::FilterMode::Linear,
        )?;

        // Base upload
        let _ = packer.push_image(
            &RgbaImage::from_pixel(254, 254, *Rgba::from_slice(&[255, 0, 0, 255])),
            gpu,
        )?;
        packer.make_upload_buffer(gpu, &Default::default())?;
        let _ = packer.texture();

        // Grow
        let _ = packer.push_image(
            &RgbaImage::from_pixel(24, 254, *Rgba::from_slice(&[255, 0, 0, 255])),
            gpu,
        )?;
        packer.make_upload_buffer(gpu, &Default::default())?;
        let _ = packer.texture();

        // Reuse
        let _ = packer.push_image(
            &RgbaImage::from_pixel(24, 254, *Rgba::from_slice(&[255, 0, 0, 255])),
            gpu,
        )?;
        packer.make_upload_buffer(gpu, &Default::default())?;
        let _ = packer.texture();
        Ok(())
    }
}
