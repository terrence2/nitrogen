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
mod region;
mod text_run;
mod widget;
mod widget_info;
mod widget_vertex;
mod widgets;

pub use crate::{
    box_packing::{PositionH, PositionV},
    color::Color,
    paint_context::PaintContext,
    region::{Border, Extent, Position, Region},
    widget::{Labeled, Widget},
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
use std::{borrow::Borrow, mem, num::NonZeroU64, ops::Range, path::Path, sync::Arc, time::Instant};
use window::{
    size::{AbsSize, Size},
    Window,
};

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
    cursor_position: Position<AbsSize>,

    // Auto-inserted widgets.
    terminal: Arc<RwLock<Terminal>>,
    show_terminal: bool,

    // The four key buffers.
    widget_info_buffer: Arc<wgpu::Buffer>,
    background_vertex_buffer: Arc<wgpu::Buffer>,
    image_vertex_buffer: Arc<wgpu::Buffer>,
    text_vertex_buffer: Arc<wgpu::Buffer>,

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

    pub fn new(
        mapper: Arc<RwLock<EventMapper>>,
        gpu: &mut Gpu,
        interpreter: &mut Interpreter,
        state_dir: &Path,
    ) -> Result<Arc<RwLock<Self>>> {
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
        let widget_info_buffer = Arc::new(gpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("widget-info-buffer"),
            size: widget_info_buffer_size,
            usage: wgpu::BufferUsage::COPY_DST | wgpu::BufferUsage::UNIFORM,
            mapped_at_creation: false,
        }));

        // Create the background vertex buffer.
        let background_vertex_buffer_size =
            (mem::size_of::<WidgetVertex>() * Self::MAX_BACKGROUND_VERTICES) as wgpu::BufferAddress;
        let background_vertex_buffer =
            Arc::new(gpu.device().create_buffer(&wgpu::BufferDescriptor {
                label: Some("widget-bg-vertex-buffer"),
                size: background_vertex_buffer_size,
                usage: wgpu::BufferUsage::COPY_DST | wgpu::BufferUsage::VERTEX,
                mapped_at_creation: false,
            }));

        // Create the image vertex buffer.
        let image_vertex_buffer_size =
            (mem::size_of::<WidgetVertex>() * Self::MAX_IMAGE_VERTICES) as wgpu::BufferAddress;
        let image_vertex_buffer = Arc::new(gpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("widget-image-vertex-buffer"),
            size: image_vertex_buffer_size,
            usage: wgpu::BufferUsage::COPY_DST | wgpu::BufferUsage::VERTEX,
            mapped_at_creation: false,
        }));

        // Create the text vertex buffer.
        let text_vertex_buffer_size =
            (mem::size_of::<WidgetVertex>() * Self::MAX_TEXT_VERTICES) as wgpu::BufferAddress;
        let text_vertex_buffer = Arc::new(gpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("widget-text-vertex-buffer"),
            size: text_vertex_buffer_size,
            usage: wgpu::BufferUsage::COPY_DST | wgpu::BufferUsage::VERTEX,
            mapped_at_creation: false,
        }));

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
        let terminal = Terminal::new(&paint_context.font_context, state_dir)?
            .with_visible(false)
            .wrapped();
        root.write().add_child("mapper", mapper);
        root.write().add_child("terminal", terminal.clone());

        let widget = Arc::new(RwLock::new(Self {
            root,
            paint_context,
            keyboard_focus: "mapper".to_owned(),
            cursor_position: Position::origin(),

            terminal,
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

    pub fn root_container(&self) -> Arc<RwLock<FloatBox>> {
        self.root.clone()
    }

    #[method]
    pub fn root(&self) -> Value {
        Value::Module(self.root.clone())
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
    pub fn font_id_for_name(&self, font_name: &str) -> Value {
        self.paint_context
            .font_context
            .font_id_for_name(font_name)
            .as_value()
    }

    #[method]
    pub fn dump_glyphs(&mut self) -> Result<()> {
        self.paint_context.dump_glyphs()
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

    pub fn toggle_terminal(&mut self) {
        match self.show_terminal {
            true => self.hide_terminal(true),
            false => self.show_terminal(true),
        }
    }

    #[method]
    pub fn show_terminal(&mut self, _pressed: bool) {
        self.show_terminal = true;
        self.set_keyboard_focus("terminal");
        self.terminal.write().set_visible(true);
    }

    #[method]
    pub fn hide_terminal(&mut self, _pressed: bool) {
        self.show_terminal = false;
        self.set_keyboard_focus("mapper");
        self.terminal.write().set_visible(false);
    }

    pub fn track_state_changes(
        &mut self,
        now: Instant,
        events: &[GenericEvent],
        win: &Window,
        interpreter: Interpreter,
    ) -> Result<()> {
        for event in events {
            if let GenericEvent::KeyboardKey {
                virtual_keycode,
                press_state,
                modifiers_state,
                ..
            } = event
            {
                if self.show_terminal && *virtual_keycode == VirtualKeyCode::Escape
                    || *virtual_keycode == VirtualKeyCode::Grave
                        && *modifiers_state == ModifiersState::SHIFT
                        && *press_state == ElementState::Pressed
                {
                    self.toggle_terminal();
                    continue;
                }
            }
            if let GenericEvent::CursorMove { pixel_position, .. } = event {
                let (x, y) = *pixel_position;
                self.cursor_position = Position::new(
                    AbsSize::from_px(x as f32),
                    AbsSize::from_px(win.height() as f32 - y as f32),
                );
            }
            self.root_container().write().handle_event(
                now,
                event,
                &self.keyboard_focus,
                self.cursor_position,
                interpreter.clone(),
            )?;
        }

        // Perform recursive layout algorithm against retained state.
        self.root.write().layout(
            now,
            Region::new(
                Position::origin(),
                Extent::new(Size::from_percent(100.), Size::from_percent(100.)),
            ),
            win,
            &mut self.paint_context.font_context,
        )?;

        Ok(())
    }

    pub fn ensure_uploaded(
        &mut self,
        now: Instant,
        gpu: &mut Gpu,
        win: &Window,
        tracker: &mut UploadTracker,
    ) -> Result<()> {
        // Draw into the paint context.
        self.paint_context.reset_for_frame();
        self.root
            .read()
            .upload(now, win, gpu, &mut self.paint_context)?;

        // Upload: copy all of the CPU paint context to the GPU buffers we maintain.
        self.paint_context.make_upload_buffer(gpu, tracker)?;

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

        // FIXME: We should only need a new bind group if the underlying texture
        // FIXME: atlas grew and we have a new texture reference, not every frame.
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

    pub fn maintain_font_atlas(
        &self,
        encoder: wgpu::CommandEncoder,
    ) -> Result<wgpu::CommandEncoder> {
        self.paint_context.maintain_font_atlas(encoder)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use gpu::TestResources;
    use std::env::current_dir;

    #[test]
    fn test_label_widget() -> Result<()> {
        let TestResources {
            window,
            gpu,
            mut interpreter,
            ..
        } = Gpu::for_test_unix()?;
        let mapper = EventMapper::new(&mut interpreter);

        let widgets = WidgetBuffer::new(
            mapper,
            &mut gpu.write(),
            &mut interpreter,
            &(current_dir()?.join("__dump__")),
        )?;
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
        widgets
            .read()
            .root_container()
            .write()
            .add_child("label", label);

        widgets
            .write()
            .track_state_changes(Instant::now(), &[], &window.read(), interpreter)?;

        let mut tracker = Default::default();
        widgets.write().ensure_uploaded(
            Instant::now(),
            &mut gpu.write(),
            &window.read(),
            &mut tracker,
        )?;

        Ok(())
    }
}
