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
mod size;
mod text_run;
mod widget;
mod widget_info;
mod widget_vertex;
mod widgets;

pub use crate::{
    box_packing::{PositionH, PositionV},
    color::Color,
    paint_context::PaintContext,
    size::{Border, Extent, LeftBound, Position, Size},
    widget::Widget,
    widget_info::WidgetInfo,
    widget_vertex::WidgetVertex,
    widgets::{
        button::Button,
        event_mapper::{Bindings, EventMapper},
        expander::Expander,
        float_box::FloatBox,
        label::Label,
        line_edit::LineEdit,
        terminal::Terminal,
        text_edit::TextEdit,
        vertical_box::VerticalBox,
    },
};

use crate::font_context::FontContext;
use anyhow::{ensure, Result};
use font_common::{FontAdvance, FontInterface};
use font_ttf::TtfFont;
use gpu::{Gpu, UploadTracker};
use input::{ElementState, GenericEvent, ModifiersState, VirtualKeyCode};
use log::trace;
use nitrous::{Interpreter, Value};
use nitrous_injector::{inject_nitrous_module, method, NitrousModule};
use parking_lot::RwLock;
use std::{borrow::Borrow, mem, num::NonZeroU64, ops::Range, sync::Arc};
use tokio::runtime::Runtime;

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

#[derive(Debug, NitrousModule)]
pub struct WidgetBuffer {
    // Widget state.
    root: Arc<RwLock<FloatBox>>,
    paint_context: PaintContext,
    keyboard_focus: String,
    cursor_position: Position<Size>,

    // Auto-inserted widgets.
    terminal: Arc<RwLock<Terminal>>,
    mapper: Arc<RwLock<EventMapper>>,
    show_terminal: bool,

    // The four key buffers.
    widget_info_buffer: Arc<Box<wgpu::Buffer>>,
    background_vertex_buffer: Arc<Box<wgpu::Buffer>>,
    image_vertex_buffer: Arc<Box<wgpu::Buffer>>,
    text_vertex_buffer: Arc<Box<wgpu::Buffer>>,

    // The accumulated bind group for all widget rendering, encompassing everything we uploaded above.
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: Option<wgpu::BindGroup>,
}

#[inject_nitrous_module]
impl WidgetBuffer {
    const MAX_WIDGETS: usize = 512;
    const MAX_TEXT_VERTICES: usize = Self::MAX_WIDGETS * 128 * 6;
    const MAX_BACKGROUND_VERTICES: usize = Self::MAX_WIDGETS * 128 * 6; // note: rounded corners
    const MAX_IMAGE_VERTICES: usize = Self::MAX_WIDGETS * 4 * 6;

    pub fn new(gpu: &mut Gpu, interpreter: &mut Interpreter) -> Result<Arc<RwLock<Self>>> {
        trace!("WidgetBuffer::new");

        let mut paint_context = PaintContext::new(gpu)?;
        let fira_mono = TtfFont::from_bytes(FIRA_MONO_REGULAR_TTF_DATA, FontAdvance::Mono)?;
        let fira_sans = TtfFont::from_bytes(FIRA_SANS_REGULAR_TTF_DATA, FontAdvance::Sans)?;
        let dejavu_mono = TtfFont::from_bytes(DEJAVU_MONO_REGULAR_TTF_DATA, FontAdvance::Mono)?;
        let dejavu_sans = TtfFont::from_bytes(DEJAVU_SANS_REGULAR_TTF_DATA, FontAdvance::Sans)?;
        paint_context.add_font("dejavu-sans", dejavu_sans.clone());
        paint_context.add_font("dejavu-mono", dejavu_mono);
        paint_context.add_font("fira-sans", fira_sans);
        paint_context.add_font("fira-mono", fira_mono.clone());
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
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
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

        let root = FloatBox::new();
        let mapper = EventMapper::new(interpreter);
        let terminal = Terminal::new(&paint_context.font_context)
            .with_visible(false)
            .wrapped();
        root.write().add_child("mapper", mapper.clone());
        root.write().add_child("terminal", terminal.clone());

        let widget = Arc::new(RwLock::new(Self {
            root,
            paint_context,
            keyboard_focus: "mapper".to_owned(),
            cursor_position: Position::origin(),

            terminal,
            mapper,
            show_terminal: false,

            widget_info_buffer,
            background_vertex_buffer,
            text_vertex_buffer,
            image_vertex_buffer,

            bind_group_layout,
            bind_group: None,
        }));

        interpreter.put_global("widget", Value::Module(widget.clone()));

        Ok(widget)
    }

    pub fn root(&self) -> Arc<RwLock<FloatBox>> {
        self.root.clone()
    }

    pub fn set_keyboard_focus(&mut self, name: &str) {
        self.keyboard_focus = name.to_owned();
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

    #[method]
    pub fn dump_glyphs(&mut self) {
        self.paint_context.dump_glyphs();
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

    pub fn layout_for_frame(&mut self, gpu: &mut Gpu) -> Result<()> {
        self.root.write().layout(
            gpu,
            Position::origin(),
            Extent::new(Size::from_percent(100.), Size::from_percent(100.)),
            &mut self.paint_context.font_context,
        )?;
        Ok(())
    }

    pub fn handle_events(
        &mut self,
        events: &[GenericEvent],
        interpreter: Arc<RwLock<Interpreter>>,
    ) -> Result<()> {
        for event in events {
            if let GenericEvent::KeyboardKey {
                virtual_keycode,
                press_state,
                modifiers_state,
                ..
            } = event
            {
                if *virtual_keycode == VirtualKeyCode::Grave
                    && *modifiers_state == ModifiersState::SHIFT
                    && *press_state == ElementState::Pressed
                {
                    self.show_terminal = !self.show_terminal;
                    self.set_keyboard_focus(if self.show_terminal {
                        "terminal"
                    } else {
                        "mapper"
                    });
                    self.terminal.write().set_visible(self.show_terminal);
                    continue;
                }
            }
            if let GenericEvent::CursorMove { pixel_position, .. } = event {
                let (x, y) = *pixel_position;
                self.cursor_position =
                    Position::new(Size::from_px(x as f32), Size::from_px(y as f32));
            }
            self.root()
                .write()
                .handle_event(event, &self.keyboard_focus, interpreter.clone())?;
        }
        Ok(())
    }

    pub fn make_upload_buffer(
        &mut self,
        gpu: &mut Gpu,
        async_rt: &Runtime,
        tracker: &mut UploadTracker,
    ) -> Result<()> {
        self.paint_context.reset_for_frame();
        self.root.read().upload(gpu, &mut self.paint_context)?;

        self.paint_context
            .make_upload_buffer(gpu, async_rt, tracker)?;

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
                        resource: wgpu::BindingResource::Buffer {
                            buffer: &self.widget_info_buffer,
                            offset: 0,
                            size: None,
                        },
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

    pub fn maintain_font_atlas<'a>(
        &'a self,
        cpass: wgpu::ComputePass<'a>,
    ) -> Result<wgpu::ComputePass<'a>> {
        self.paint_context.maintain_font_atlas(cpass)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use tokio::runtime::Runtime;
    use winit::{event_loop::EventLoop, window::Window};

    #[test]
    fn test_label_widget() -> Result<()> {
        use winit::platform::unix::EventLoopExtUnix;
        let event_loop = EventLoop::<()>::new_any_thread();
        let window = Window::new(&event_loop)?;
        let async_rt = Runtime::new()?;
        let interpreter = Interpreter::new();
        let gpu = Gpu::new(window, Default::default(), &mut interpreter.write())?;

        let widgets = WidgetBuffer::new(&mut gpu.write(), &mut interpreter.write())?;
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
        widgets.read().root().write().add_child("label", label);

        let mut tracker = Default::default();
        widgets
            .write()
            .make_upload_buffer(&mut gpu.write(), &async_rt, &mut tracker)?;

        Ok(())
    }
}
