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
    widget::{Labeled, Widget, WidgetFocus},
    widget_info::WidgetInfo,
    widget_vertex::WidgetVertex,
    widgets::{
        button::Button, expander::Expander, float_box::FloatBox, label::Label, line_edit::LineEdit,
        terminal::Terminal, text_edit::TextEdit, vertical_box::VerticalBox,
    },
};

use crate::font_context::FontContext;
use animate::TimeStep;
use anyhow::{ensure, Result};
use bevy_ecs::prelude::*;
use font_common::{FontAdvance, FontInterface};
use font_ttf::TtfFont;
use gpu::Gpu;
use input::{ElementState, InputEvent, InputEventVec, InputFocus, ModifiersState, VirtualKeyCode};
use log::{error, trace};
use nitrous::{inject_nitrous_resource, method, HeapMut, NitrousResource, Value};
use parking_lot::RwLock;
use platform_dirs::AppDirs;
use runtime::{Extension, FrameStage, Runtime, ScriptCompletions, ScriptHerder, SimStage};
use std::{
    borrow::Borrow, marker::PhantomData, mem, num::NonZeroU64, ops::Range, path::Path, sync::Arc,
    time::Instant,
};
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

#[derive(Debug, NitrousResource)]
pub struct WidgetBuffer<T>
where
    T: InputFocus,
{
    // Widget state.
    root: Arc<RwLock<FloatBox>>,
    paint_context: PaintContext,
    cursor_position: Position<AbsSize>,

    // Auto-inserted widgets.
    terminal: Arc<RwLock<Terminal>>,
    request_toggle_terminal: bool,
    show_terminal: bool,

    // The four key buffers.
    widget_info_buffer: Arc<wgpu::Buffer>,
    background_vertex_buffer: Arc<wgpu::Buffer>,
    image_vertex_buffer: Arc<wgpu::Buffer>,
    text_vertex_buffer: Arc<wgpu::Buffer>,

    // The accumulated bind group for all widget rendering, encompassing everything we uploaded above.
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: Option<wgpu::BindGroup>,

    phantom: PhantomData<T>,
}

impl<T> Extension for WidgetBuffer<T>
where
    T: InputFocus,
{
    fn init(runtime: &mut Runtime) -> Result<()> {
        let state_dir = runtime.resource::<AppDirs>().state_dir.clone();
        let widget = WidgetBuffer::<T>::new(&mut runtime.resource_mut::<Gpu>(), &state_dir)?;
        runtime.insert_named_resource("widget", widget);

        runtime
            .sim_stage_mut(SimStage::HandleInput)
            .add_system(Self::sys_handle_terminal_events.exclusive_system());
        runtime.sim_stage_mut(SimStage::HandleInput).add_system(
            Self::sys_handle_toggle_terminal.label("WidgetBuffer::sys_handle_toggle_terminal"),
        );
        runtime.sim_stage_mut(SimStage::HandleInput).add_system(
            Self::sys_handle_input_events
                .label("WidgetBuffer::sys_handle_input_events")
                .before("WidgetBuffer::sys_handle_toggle_terminal"),
        );
        runtime
            .sim_stage_mut(SimStage::PostScript)
            .add_system(Self::sys_report_script_completions);

        runtime
            .frame_stage_mut(FrameStage::TrackStateChanges)
            .add_system(Self::sys_track_state_changes);
        runtime.frame_stage_mut(FrameStage::Render).add_system(
            Self::sys_ensure_uploaded
                .label("WidgetBuffer::sys_ensure_uploaded")
                .before("UiRenderPass")
                .after("WidgetBuffer::maintain_font_atlas"),
        );
        runtime.frame_stage_mut(FrameStage::Render).add_system(
            Self::sys_maintain_font_atlas
                .before("UiRenderPass")
                .label("WidgetBuffer::maintain_font_atlas"),
        );
        runtime
            .frame_stage_mut(FrameStage::FrameEnd)
            .add_system(Self::sys_handle_dump_texture);
        Ok(())
    }
}

#[inject_nitrous_resource]
impl<T> WidgetBuffer<T>
where
    T: InputFocus,
{
    const MAX_WIDGETS: usize = 512;
    const MAX_TEXT_VERTICES: usize = Self::MAX_WIDGETS * 128 * 6;
    const MAX_BACKGROUND_VERTICES: usize = Self::MAX_WIDGETS * 128 * 6; // note: rounded corners
    const MAX_IMAGE_VERTICES: usize = Self::MAX_WIDGETS * 4 * 6;

    pub fn new(gpu: &mut Gpu, state_dir: &Path) -> Result<Self> {
        trace!("WidgetBuffer::new");

        let mut paint_context = PaintContext::new(gpu);
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
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: false,
        }));

        // Create the background vertex buffer.
        let background_vertex_buffer_size =
            (mem::size_of::<WidgetVertex>() * Self::MAX_BACKGROUND_VERTICES) as wgpu::BufferAddress;
        let background_vertex_buffer =
            Arc::new(gpu.device().create_buffer(&wgpu::BufferDescriptor {
                label: Some("widget-bg-vertex-buffer"),
                size: background_vertex_buffer_size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::VERTEX,
                mapped_at_creation: false,
            }));

        // Create the image vertex buffer.
        let image_vertex_buffer_size =
            (mem::size_of::<WidgetVertex>() * Self::MAX_IMAGE_VERTICES) as wgpu::BufferAddress;
        let image_vertex_buffer = Arc::new(gpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("widget-image-vertex-buffer"),
            size: image_vertex_buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::VERTEX,
            mapped_at_creation: false,
        }));

        // Create the text vertex buffer.
        let text_vertex_buffer_size =
            (mem::size_of::<WidgetVertex>() * Self::MAX_TEXT_VERTICES) as wgpu::BufferAddress;
        let text_vertex_buffer = Arc::new(gpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("widget-text-vertex-buffer"),
            size: text_vertex_buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::VERTEX,
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
                            visibility: wgpu::ShaderStages::all(),
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
        root.write().add_child("terminal", terminal.clone());

        Ok(Self {
            root,
            paint_context,
            cursor_position: Position::origin(),

            terminal,
            request_toggle_terminal: false,
            show_terminal: false,

            widget_info_buffer,
            background_vertex_buffer,
            text_vertex_buffer,
            image_vertex_buffer,

            bind_group_layout,
            bind_group: None,

            phantom: PhantomData::default(),
        })
    }

    pub fn root_container(&self) -> Arc<RwLock<FloatBox>> {
        self.root.clone()
    }

    // #[method]
    // pub fn root(&self) -> Value {
    //     Value::Module(self.root.clone())
    // }

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

    #[method]
    pub fn toggle_terminal(&mut self, pressed: bool) {
        if pressed {
            self.request_toggle_terminal = true;
        }
    }

    // Since terminal-active mode consumes all keys instead of our bindings,
    // we have to handle toggling as a special case.
    fn is_toggle_terminal_event(&self, event: &InputEvent) -> bool {
        if let InputEvent::KeyboardKey {
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
                return true;
            }
        }
        false
    }

    pub fn sys_handle_toggle_terminal(
        events: Res<InputEventVec>,
        mut input_focus: ResMut<T>,
        mut widgets: ResMut<WidgetBuffer<T>>,
    ) {
        if events
            .iter()
            .any(|event| widgets.is_toggle_terminal_event(event))
        {
            widgets.request_toggle_terminal = true;
        }

        if widgets.request_toggle_terminal {
            widgets.request_toggle_terminal = false;
            input_focus.toggle_terminal();
            widgets.show_terminal = !widgets.show_terminal;
            widgets.terminal.write().set_visible(widgets.show_terminal);
        }
    }

    pub fn sys_handle_input_events(
        events: Res<InputEventVec>,
        input_focus: Res<T>,
        window: Res<Window>,
        mut herder: ResMut<ScriptHerder>,
        mut widgets: ResMut<WidgetBuffer<T>>,
    ) {
        widgets
            .handle_events(&events, *input_focus, &mut herder, &window)
            .map_err(|e| {
                error!("handle_input_events: {}\n{}", e, e.backtrace());
                e
            })
            .ok();
    }

    fn handle_events(
        &mut self,
        events: &[InputEvent],
        focus: T,
        herder: &mut ScriptHerder,
        win: &Window,
    ) -> Result<()> {
        for event in events {
            if let InputEvent::CursorMove { pixel_position, .. } = event {
                let (x, y) = *pixel_position;
                self.cursor_position = Position::new(
                    AbsSize::from_px(x as f32),
                    AbsSize::from_px(win.height() as f32 - y as f32),
                );
            }
            self.root_container().write().handle_event(
                event,
                if focus.is_terminal_focused() {
                    WidgetFocus::Terminal
                } else {
                    WidgetFocus::Game
                },
                self.cursor_position,
                herder,
            )?;
        }
        Ok(())
    }

    fn sys_handle_terminal_events(world: &mut World) {
        if world.get_resource_mut::<T>().unwrap().is_terminal_focused() {
            let events = world.get_resource::<InputEventVec>().unwrap().to_owned();
            world.resource_scope(|world, widgets: Mut<WidgetBuffer<T>>| {
                for event in events {
                    widgets
                        .terminal
                        .write()
                        .handle_terminal_events(&event, HeapMut::wrap(world))
                        .ok();
                }
            })
        }
    }

    fn sys_report_script_completions(
        widgets: Res<WidgetBuffer<T>>,
        completions: Res<ScriptCompletions>,
    ) {
        widgets
            .terminal
            .write()
            .report_script_completions(&completions);
    }

    fn sys_track_state_changes(
        step: Res<TimeStep>,
        window: Res<Window>,
        mut widgets: ResMut<WidgetBuffer<T>>,
    ) {
        widgets
            .track_state_changes(*step.now(), &window)
            .expect("Widgets::track_state_changes");
    }

    pub fn track_state_changes(&mut self, now: Instant, win: &Window) -> Result<()> {
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

    pub fn sys_ensure_uploaded(
        mut widget: ResMut<WidgetBuffer<T>>,
        timestep: Res<TimeStep>,
        gpu: Res<Gpu>,
        window: Res<Window>,
        maybe_encoder: ResMut<Option<wgpu::CommandEncoder>>,
    ) {
        if let Some(encoder) = maybe_encoder.into_inner() {
            widget
                .ensure_uploaded(*timestep.now(), &gpu, &window, encoder)
                .ok();
        }
    }

    pub fn ensure_uploaded(
        &mut self,
        now: Instant,
        gpu: &Gpu,
        win: &Window,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<()> {
        // Draw into the paint context.
        self.paint_context.reset_for_frame();
        self.root
            .read()
            .upload(now, win, gpu, &mut self.paint_context)?;

        if !self.paint_context.widget_info_pool.is_empty() {
            ensure!(self.paint_context.widget_info_pool.len() <= Self::MAX_WIDGETS);
            gpu.upload_slice_to(
                "widget-info-upload",
                &self.paint_context.widget_info_pool,
                self.widget_info_buffer.clone(),
                encoder,
            );
        }

        if !self.paint_context.background_pool.is_empty() {
            ensure!(self.paint_context.background_pool.len() <= Self::MAX_BACKGROUND_VERTICES);
            gpu.upload_slice_to(
                "widget-bg-vertex-upload",
                &self.paint_context.background_pool,
                self.background_vertex_buffer.clone(),
                encoder,
            );
        }

        if !self.paint_context.image_pool.is_empty() {
            ensure!(self.paint_context.image_pool.len() <= Self::MAX_IMAGE_VERTICES);
            gpu.upload_slice_to(
                "widget-image-vertex-upload",
                &self.paint_context.image_pool,
                self.image_vertex_buffer.clone(),
                encoder,
            );
        }

        if !self.paint_context.text_pool.is_empty() {
            ensure!(self.paint_context.text_pool.len() <= Self::MAX_TEXT_VERTICES);
            gpu.upload_slice_to(
                "widget-text-vertex-upload",
                &self.paint_context.text_pool,
                self.text_vertex_buffer.clone(),
                encoder,
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
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &self.widget_info_buffer,
                            offset: 0,
                            size: None,
                        }),
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

    fn sys_handle_dump_texture(mut widgets: ResMut<WidgetBuffer<T>>, mut gpu: ResMut<Gpu>) {
        widgets
            .paint_context
            .handle_dump_texture(&mut gpu)
            .map_err(|e| {
                error!("Widgets::handle_dump_texture: {}", e);
                e
            })
            .ok();
    }

    fn sys_maintain_font_atlas(
        mut widgets: ResMut<WidgetBuffer<T>>,
        gpu: Res<Gpu>,
        maybe_encoder: ResMut<Option<wgpu::CommandEncoder>>,
    ) {
        if let Some(encoder) = maybe_encoder.into_inner() {
            widgets.paint_context.maintain_font_atlas(&gpu, encoder);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use input::DemoFocus;

    #[test]
    fn test_label_widget() -> Result<()> {
        let mut runtime = Gpu::for_test_unix()?;
        runtime
            .insert_resource(AppDirs::new(Some("nitrogen"), true).unwrap())
            .insert_resource(TimeStep::new_60fps())
            .load_extension::<WidgetBuffer<DemoFocus>>()?;

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
        runtime
            .resource_mut::<WidgetBuffer<DemoFocus>>()
            .root_container()
            .write()
            .add_child("label", label);

        runtime.run_frame_once();

        Ok(())
    }
}
