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
mod box_packing;
mod color;
mod font_context;
mod paint_context;
mod text_run;
mod widget;
mod widget_info;
mod widget_vertex;
mod widgets;

pub use crate::{
    box_packing::{PositionH, PositionV},
    color::Color,
    paint_context::PaintContext,
    widget::Widget,
    widget_info::WidgetInfo,
    widget_vertex::WidgetVertex,
    widgets::{
        float_box::FloatBox, label::Label, line_edit::LineEdit, terminal::Terminal,
        text_edit::TextEdit, vertical_box::VerticalBox,
    },
};

use crate::font_context::FontContext;
use commandable::{commandable, Commandable};
use failure::{ensure, Fallible};
use font_common::FontInterface;
use font_ttf::TtfFont;
use gpu::{UploadTracker, GPU};
use log::trace;
use parking_lot::RwLock;
use std::{borrow::Borrow, mem, num::NonZeroU64, ops::Range, sync::Arc};
use winit::event::{KeyboardInput, ModifiersState};

// Drawing UI efficiently:
//
// We have one pipeline for each of the following.
// 1) Draw all widget backgrounds / borders in one pipeline, with depth.
// 2) Draw all images
// 3) Draw all text
//
// Widget upload recurses through the tree of widgets. Each layer gets a 1.0 wide depth slot to
// render into. They may upload vertices to 3 vertex pools, one for each of the above concerns.
// Rendering is done from leaf up, making use of the depth test where possible to avoid overpaint.
// Vertices contain x, y, and z coordinates in screen space, s and t texture coordinates, and an
// index into the widget info buffer. There is one slot in the info buffer per widget where the
// majority of the widget data lives, to save space in vertices.

pub const SANS_FONT_NAME: &str = "sans";
pub const MONO_FONT_NAME: &str = "mono";
const DEJAVU_SANS_REGULAR_TTF_DATA: &[u8] =
    include_bytes!("../../../../assets/font/DejaVuSans.ttf");
const DEJAVU_MONO_REGULAR_TTF_DATA: &[u8] =
    include_bytes!("../../../../assets/font/DejaVuSansMono.ttf");
const FIRA_SANS_REGULAR_TTF_DATA: &[u8] =
    include_bytes!("../../../../assets/font/FiraSans-Regular.ttf");
const FIRA_MONO_REGULAR_TTF_DATA: &[u8] =
    include_bytes!("../../../../assets/font/FiraMono-Regular.ttf");

#[derive(Commandable)]
pub struct WidgetBuffer {
    // Widget state.
    root: Arc<RwLock<FloatBox>>,
    paint_context: PaintContext,

    // The four key buffers.
    widget_info_buffer: Arc<Box<wgpu::Buffer>>,
    background_vertex_buffer: Arc<Box<wgpu::Buffer>>,
    image_vertex_buffer: Arc<Box<wgpu::Buffer>>,
    text_vertex_buffer: Arc<Box<wgpu::Buffer>>,

    // The accumulated bind group for all widget rendering, encompassing everything we uploaded above.
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: Option<wgpu::BindGroup>,
}

#[commandable]
impl WidgetBuffer {
    const MAX_WIDGETS: usize = 512;
    const MAX_TEXT_VERTICES: usize = Self::MAX_WIDGETS * 128 * 6;
    const MAX_BACKGROUND_VERTICES: usize = Self::MAX_WIDGETS * 128 * 6; // note: rounded corners
    const MAX_IMAGE_VERTICES: usize = Self::MAX_WIDGETS * 4 * 6;

    pub fn new(gpu: &mut GPU) -> Fallible<Self> {
        trace!("WidgetBuffer::new");

        let mut paint_context = PaintContext::new(gpu.device());
        let fira_mono = TtfFont::from_bytes(&FIRA_MONO_REGULAR_TTF_DATA)?;
        let fira_sans = TtfFont::from_bytes(&FIRA_SANS_REGULAR_TTF_DATA)?;
        let dejavu_mono = TtfFont::from_bytes(&DEJAVU_MONO_REGULAR_TTF_DATA)?;
        let dejavu_sans = TtfFont::from_bytes(&DEJAVU_SANS_REGULAR_TTF_DATA)?;
        paint_context.add_font("fira-mono", fira_mono.clone());
        paint_context.add_font("fira-sans", fira_sans);
        paint_context.add_font("dejavu-mono", dejavu_mono);
        paint_context.add_font("dejavu-sans", dejavu_sans.clone());
        paint_context.add_font("mono", fira_mono);
        paint_context.add_font("sans", dejavu_sans);

        // Create the core widget info buffer.
        let widget_info_buffer_size =
            (mem::size_of::<WidgetInfo>() * Self::MAX_WIDGETS) as wgpu::BufferAddress;
        let widget_info_buffer = Arc::new(Box::new(gpu.device().create_buffer(
            &wgpu::BufferDescriptor {
                label: Some("widget-info-buffer"),
                size: widget_info_buffer_size,
                usage: wgpu::BufferUsage::COPY_DST | wgpu::BufferUsage::UNIFORM,
                mapped_at_creation: false,
            },
        )));

        // Create the background vertex buffer.
        let background_vertex_buffer_size =
            (mem::size_of::<WidgetVertex>() * Self::MAX_BACKGROUND_VERTICES) as wgpu::BufferAddress;
        let background_vertex_buffer = Arc::new(Box::new(gpu.device().create_buffer(
            &wgpu::BufferDescriptor {
                label: Some("widget-bg-vertex-buffer"),
                size: background_vertex_buffer_size,
                usage: wgpu::BufferUsage::COPY_DST | wgpu::BufferUsage::VERTEX,
                mapped_at_creation: false,
            },
        )));

        // Create the image vertex buffer.
        let image_vertex_buffer_size =
            (mem::size_of::<WidgetVertex>() * Self::MAX_IMAGE_VERTICES) as wgpu::BufferAddress;
        let image_vertex_buffer = Arc::new(Box::new(gpu.device().create_buffer(
            &wgpu::BufferDescriptor {
                label: Some("widget-image-vertex-buffer"),
                size: image_vertex_buffer_size,
                usage: wgpu::BufferUsage::COPY_DST | wgpu::BufferUsage::VERTEX,
                mapped_at_creation: false,
            },
        )));

        // Create the text vertex buffer.
        let text_vertex_buffer_size =
            (mem::size_of::<WidgetVertex>() * Self::MAX_TEXT_VERTICES) as wgpu::BufferAddress;
        let text_vertex_buffer = Arc::new(Box::new(gpu.device().create_buffer(
            &wgpu::BufferDescriptor {
                label: Some("widget-text-vertex-buffer"),
                size: text_vertex_buffer_size,
                usage: wgpu::BufferUsage::COPY_DST | wgpu::BufferUsage::VERTEX,
                mapped_at_creation: false,
            },
        )));

        let bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("widget-bind-group-layout"),
                    entries: &[
                        // widget_info: WidgetInfo[MAX_WIDGETS]
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStage::all(),
                            ty: wgpu::BindingType::UniformBuffer {
                                dynamic: false,
                                min_binding_size: NonZeroU64::new(widget_info_buffer_size),
                            },
                            count: None,
                        },
                        // glyph_sheet: Texture2d[MAX_LAYER]
                        paint_context
                            .font_context
                            .glyph_sheet_texture_layout_entry(1),
                        // glyph_sampler: Sampler
                        paint_context
                            .font_context
                            .glyph_sheet_sampler_layout_entry(2),
                    ],
                });

        Ok(Self {
            root: FloatBox::new(),
            paint_context,

            widget_info_buffer,
            background_vertex_buffer,
            text_vertex_buffer,
            image_vertex_buffer,

            bind_group_layout,
            bind_group: None,
        })
    }

    pub fn root(&self) -> Arc<RwLock<FloatBox>> {
        self.root.clone()
    }

    pub fn add_font<S: Borrow<str> + Into<String>>(
        &mut self,
        font_name: S,
        font: Arc<RwLock<dyn FontInterface>>,
    ) {
        self.paint_context.add_font(font_name, font);
    }

    pub fn font_context(&self) -> &FontContext {
        &self.paint_context.font_context
    }

    pub fn handle_keyboard(&mut self, inputs: &[(KeyboardInput, ModifiersState)]) -> Fallible<()> {
        self.root().write().handle_keyboard(inputs)
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    /// Must only be called after first upload
    pub fn bind_group(&self) -> &wgpu::BindGroup {
        self.bind_group.as_ref().unwrap()
    }

    pub fn background_vertex_buffer(&self) -> wgpu::BufferSlice {
        self.background_vertex_buffer.slice(
            0u64..(mem::size_of::<WidgetVertex>() * self.paint_context.background_pool.len())
                as u64,
        )
    }

    pub fn background_vertex_range(&self) -> Range<u32> {
        0u32..self.paint_context.background_pool.len() as u32
    }

    pub fn text_vertex_buffer(&self) -> wgpu::BufferSlice {
        self.text_vertex_buffer.slice(
            0u64..(mem::size_of::<WidgetVertex>() * self.paint_context.text_pool.len()) as u64,
        )
    }

    pub fn text_vertex_range(&self) -> Range<u32> {
        0u32..self.paint_context.text_pool.len() as u32
    }

    pub fn make_upload_buffer(&mut self, gpu: &GPU, tracker: &mut UploadTracker) -> Fallible<()> {
        self.paint_context.reset_for_frame();
        self.root.read().upload(gpu, &mut self.paint_context)?;

        self.paint_context.font_context.upload(gpu, tracker);

        if !self.paint_context.widget_info_pool.is_empty() {
            ensure!(self.paint_context.widget_info_pool.len() <= Self::MAX_WIDGETS);
            gpu.upload_slice_to(
                "widget-info-upload",
                &self.paint_context.widget_info_pool,
                self.widget_info_buffer.clone(),
                tracker,
            );
        }

        if !self.paint_context.background_pool.is_empty() {
            ensure!(self.paint_context.background_pool.len() <= Self::MAX_BACKGROUND_VERTICES);
            gpu.upload_slice_to(
                "widget-bg-vertex-upload",
                &self.paint_context.background_pool,
                self.background_vertex_buffer.clone(),
                tracker,
            );
        }

        if !self.paint_context.image_pool.is_empty() {
            ensure!(self.paint_context.image_pool.len() <= Self::MAX_IMAGE_VERTICES);
            gpu.upload_slice_to(
                "widget-image-vertex-upload",
                &self.paint_context.image_pool,
                self.image_vertex_buffer.clone(),
                tracker,
            );
        }

        if !self.paint_context.text_pool.is_empty() {
            ensure!(self.paint_context.text_pool.len() <= Self::MAX_TEXT_VERTICES);
            gpu.upload_slice_to(
                "widget-text-vertex-upload",
                &self.paint_context.text_pool,
                self.text_vertex_buffer.clone(),
                tracker,
            );
        }

        self.bind_group = Some(
            gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("widget-bind-group"),
                layout: &self.bind_group_layout,
                entries: &[
                    // widget_info
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(self.widget_info_buffer.slice(..)),
                    },
                    // glyph_sheet_texture: Texture2dArray
                    self.paint_context
                        .font_context
                        .glyph_sheet_texture_binding(1),
                    // glyph_sheet_sampler: Sampler2d
                    self.paint_context
                        .font_context
                        .glyph_sheet_sampler_binding(2),
                ],
            }),
        );

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use winit::{event_loop::EventLoop, window::Window};

    #[test]
    fn test_label_widget() -> Fallible<()> {
        use winit::platform::unix::EventLoopExtUnix;
        let event_loop = EventLoop::<()>::new_any_thread();
        let window = Window::new(&event_loop)?;
        let mut gpu = GPU::new(&window, Default::default())?;

        let mut widgets = WidgetBuffer::new(&mut gpu)?;
        let label = Label::new(
            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789\
            สิบสองกษัตริย์ก่อนหน้าแลถัดไป       สององค์ไซร้โง่เขลาเบาปัญญา\
            Зарегистрируйтесь сейчас на Десятую Международную Конференцию по\
            გთხოვთ ახლავე გაიაროთ რეგისტრაცია Unicode-ის მეათე საერთაშორისო\
            ∮ E⋅da = Q,  n → ∞, ∑ f(i) = ∏ g(i), ∀x∈ℝ: ⌈x⌉ = −⌊−x⌋, α ∧ ¬β = ¬(¬α ∨ β)\
            Οὐχὶ ταὐτὰ παρίσταταί μοι γιγνώσκειν, ὦ ἄνδρες ᾿Αθηναῖοι,\
            ði ıntəˈnæʃənəl fəˈnɛtık əsoʊsiˈeıʃn\
            Y [ˈʏpsilɔn], Yen [jɛn], Yoga [ˈjoːgɑ]",
        )
        .wrapped();
        widgets.root().write().add_child(label);

        let mut tracker = Default::default();
        widgets.make_upload_buffer(&gpu, &mut tracker)?;

        Ok(())
    }
}
