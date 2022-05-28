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
mod font_context;
mod layout;
mod paint_context;
mod region;
mod text_run;
mod widget;
mod widget_info;
mod widget_vertex;
mod widgets;

pub use crate::{
    font_context::FontId,
    layout::{Expand, LayoutMeasurements, LayoutNode, LayoutPacking, PositionH, PositionV},
    paint_context::PaintContext,
    region::{Border, Extent, Position, Region},
    widget::{Labeled, Widget, WidgetComponent, WidgetFocus},
    widget_info::WidgetInfo,
    widget_vertex::WidgetVertex,
    widgets::{label::Label, terminal::Terminal},
};

use animate::TimeStep;
use anyhow::{ensure, Result};
use bevy_ecs::prelude::*;
use event_mapper::EventMapperStep;
use font_common::FontAdvance;
use font_ttf::TtfFont;
use gpu::{Gpu, GpuStep};
use input::{InputEvent, InputEventVec, InputTarget};
use log::trace;
use nitrous::{inject_nitrous_resource, NitrousResource};
use runtime::{report, Extension, Runtime, ScriptHerder};
use std::{mem, num::NonZeroU64, sync::Arc, time::Instant};
use window::{
    size::{AbsSize, Size},
    Window, WindowStep,
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

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum WidgetRenderStep {
    // Pre-encoder
    PrepareForFrame,
    LayoutWidgets,

    // Encoder
    EnsureUploaded,
    MaintainFontAtlas,

    // Post-frame
    DumpAtlas,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum WidgetSimStep {
    HandleTerminal,
    ToggleTerminal,
    HandleEvents,
    ReportScriptCompletions,
}

#[derive(Debug, NitrousResource)]
pub struct WidgetBuffer {
    // Widget state.
    root: LayoutNode,
    cursor_position: Position<AbsSize>,

    // The four key buffers.
    widget_info_buffer: Arc<wgpu::Buffer>,
    background_vertex_buffer: Arc<wgpu::Buffer>,
    image_vertex_buffer: Arc<wgpu::Buffer>,
    text_vertex_buffer: Arc<wgpu::Buffer>,

    // The accumulated bind group for all widget rendering, encompassing everything we uploaded above.
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: Option<wgpu::BindGroup>,
}

impl Extension for WidgetBuffer {
    fn init(runtime: &mut Runtime) -> Result<()> {
        let mut paint_context = PaintContext::new(runtime.resource::<Gpu>());
        let fira_mono = TtfFont::from_bytes(FIRA_MONO_REGULAR_TTF_DATA, FontAdvance::Mono)?;
        let fira_sans = TtfFont::from_bytes(FIRA_SANS_REGULAR_TTF_DATA, FontAdvance::Sans)?;
        let dejavu_mono = TtfFont::from_bytes(DEJAVU_MONO_REGULAR_TTF_DATA, FontAdvance::Mono)?;
        let dejavu_sans = TtfFont::from_bytes(DEJAVU_SANS_REGULAR_TTF_DATA, FontAdvance::Sans)?;
        paint_context.add_font("sans", dejavu_sans.clone());
        paint_context.add_font("mono", fira_mono.clone());
        paint_context.add_font("dejavu-sans", dejavu_sans);
        paint_context.add_font("dejavu-mono", dejavu_mono);
        paint_context.add_font("fira-sans", fira_sans);
        paint_context.add_font("fira-mono", fira_mono);
        runtime.insert_named_resource("paint", paint_context);

        let widget = WidgetBuffer::new(
            LayoutNode::new_float("root", runtime.heap_mut())?,
            runtime.resource::<Gpu>(),
            runtime.resource::<PaintContext>(),
        )?;
        runtime.insert_named_resource("widget", widget);

        runtime.add_input_system(
            Self::sys_handle_input_events
                .label(WidgetSimStep::HandleEvents)
                .after(EventMapperStep::HandleEvents),
        );

        runtime.add_frame_system(
            Self::sys_prepare_for_frame
                .label(WidgetRenderStep::PrepareForFrame)
                .after(WindowStep::HandleEvents),
        );
        runtime.add_frame_system(
            Self::sys_layout_widgets
                .label(WidgetRenderStep::LayoutWidgets)
                .after(WindowStep::HandleEvents),
        );
        runtime.add_frame_system(
            Self::sys_maintain_font_atlas
                .label(WidgetRenderStep::MaintainFontAtlas)
                .after(WidgetRenderStep::PrepareForFrame)
                .after(WidgetRenderStep::LayoutWidgets)
                .after(GpuStep::CreateCommandEncoder)
                .before(GpuStep::SubmitCommands),
        );
        runtime.add_frame_system(
            Self::sys_ensure_uploaded
                .label(WidgetRenderStep::EnsureUploaded)
                .after(WidgetRenderStep::MaintainFontAtlas)
                .after(GpuStep::CreateCommandEncoder)
                .before(GpuStep::SubmitCommands),
        );
        runtime.add_frame_system(
            Self::sys_handle_dump_texture
                .label(WidgetRenderStep::DumpAtlas)
                .after(GpuStep::PresentTargetSurface),
        );
        Ok(())
    }
}

#[inject_nitrous_resource]
impl WidgetBuffer {
    const MAX_WIDGETS: usize = 512;
    const MAX_TEXT_VERTICES: usize = Self::MAX_WIDGETS * 128 * 6;
    const MAX_BACKGROUND_VERTICES: usize = Self::MAX_WIDGETS * 128 * 6; // note: rounded corners
    const MAX_IMAGE_VERTICES: usize = Self::MAX_WIDGETS * 4 * 6;

    pub fn new(root: LayoutNode, gpu: &Gpu, paint_context: &PaintContext) -> Result<Self> {
        trace!("WidgetBuffer::new");

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

        // let root = FloatBox::new();
        // let terminal = Terminal::new(&mut paint_context.font_context, state_dir, gpu)?
        //     .with_visible(false)
        //     .wrapped();
        // root.write().add_child("terminal", terminal.clone());

        Ok(Self {
            root,
            // paint_context,
            cursor_position: Position::origin(),

            widget_info_buffer,
            background_vertex_buffer,
            text_vertex_buffer,
            image_vertex_buffer,

            bind_group_layout,
            bind_group: None,
        })
    }

    pub fn background_vertex_buffer(&self, paint: &PaintContext) -> wgpu::BufferSlice {
        self.background_vertex_buffer
            .slice(0u64..(mem::size_of::<WidgetVertex>() * paint.background_vertex_count()) as u64)
    }

    pub fn text_vertex_buffer(&self, paint: &PaintContext) -> wgpu::BufferSlice {
        self.text_vertex_buffer
            .slice(0u64..(mem::size_of::<WidgetVertex>() * paint.text_vertex_count()) as u64)
    }

    pub fn root_mut(&mut self) -> &mut LayoutNode {
        &mut self.root
    }

    pub fn root(&self) -> &LayoutNode {
        &self.root
    }

    // pub fn add_font<S: Borrow<str> + Into<String>>(&mut self, font_name: S, font: Font) {
    //     self.paint_context.add_font(font_name, font);
    // }

    // pub fn font_context(&self) -> &FontContext {
    //     &self.paint_context.font_context
    // }

    // #[method]
    // pub fn font_id_for_name(&self, font_name: &str) -> Value {
    //     self.paint_context
    //         .font_context
    //         .font_id_for_name(font_name)
    //         .as_value()
    // }

    // #[method]
    // pub fn dump_glyphs(&mut self) -> Result<()> {
    //     self.paint_context.dump_glyphs()
    // }

    // #[method]
    // pub fn set_terminal_font_size(&mut self, size: i64) {
    //     self.terminal
    //         .write()
    //         .set_font_size(AbsSize::Pts(size as f32))
    // }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    /// Must only be called after first upload
    pub fn bind_group(&self) -> &wgpu::BindGroup {
        self.bind_group.as_ref().unwrap()
    }

    pub fn sys_handle_input_events(
        events: Res<InputEventVec>,
        term_focus: Res<InputTarget>,
        window: Res<Window>,
        mut herder: ResMut<ScriptHerder>,
        mut widgets: ResMut<WidgetBuffer>,
    ) {
        report!(widgets.handle_events(&events, &term_focus, &mut herder, &window));
    }

    fn handle_events(
        &mut self,
        events: &[InputEvent],
        _term_focus: &InputTarget,
        _herder: &mut ScriptHerder,
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
            // TODO: we will probably need to handle at least press events
            // self.root_container().write().handle_event(
            //     event,
            //     if focus.is_terminal_focused() {
            //         WidgetFocus::Terminal
            //     } else {
            //         WidgetFocus::Game
            //     },
            //     self.cursor_position,
            //     herder,
            // )?;
        }
        Ok(())
    }

    fn sys_prepare_for_frame(mut context: ResMut<PaintContext>) {
        context.reset_for_frame();
    }

    fn sys_layout_widgets(
        packings: Query<&LayoutPacking>,
        mut measures: Query<&mut LayoutMeasurements>,
        mut widgets: ResMut<WidgetBuffer>,
    ) {
        report!(widgets.root_mut().measure_layout(&packings, &mut measures));
        report!(widgets
            .root_mut()
            .perform_layout(Region::full(), 100., &packings, &mut measures));
    }

    #[allow(clippy::too_many_arguments)]
    fn sys_ensure_uploaded(
        mut widget: ResMut<WidgetBuffer>,
        mut paint_context: ResMut<PaintContext>,
        packings: Query<&LayoutPacking>,
        measures: Query<&LayoutMeasurements>,
        timestep: Res<TimeStep>,
        gpu: Res<Gpu>,
        window: Res<Window>,
        maybe_encoder: ResMut<Option<wgpu::CommandEncoder>>,
    ) {
        if let Some(encoder) = maybe_encoder.into_inner() {
            widget
                .ensure_uploaded(
                    packings,
                    measures,
                    &mut paint_context,
                    *timestep.now(),
                    &gpu,
                    &window,
                    encoder,
                )
                .ok();
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn ensure_uploaded(
        &mut self,
        packings: Query<&LayoutPacking>,
        measures: Query<&LayoutMeasurements>,
        paint_context: &mut PaintContext,
        now: Instant,
        gpu: &Gpu,
        win: &Window,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<()> {
        // Draw into the paint context.
        self.root_mut()
            .draw_non_client(now, &packings, &measures, win, gpu, paint_context)?;

        if paint_context.widget_info_pool.is_empty() {
            paint_context.widget_info_pool.push(WidgetInfo::default());
        }
        ensure!(paint_context.widget_info_pool.len() <= Self::MAX_WIDGETS);
        gpu.upload_slice_to(
            "widget-info-upload",
            &paint_context.widget_info_pool,
            self.widget_info_buffer.clone(),
            encoder,
        );

        if paint_context.background_pool.is_empty() {
            for _ in 0..6 {
                paint_context.background_pool.push(WidgetVertex::default());
            }
        }
        ensure!(paint_context.background_pool.len() <= Self::MAX_BACKGROUND_VERTICES);
        gpu.upload_slice_to(
            "widget-bg-vertex-upload",
            &paint_context.background_pool,
            self.background_vertex_buffer.clone(),
            encoder,
        );

        if paint_context.image_pool.is_empty() {
            for _ in 0..6 {
                paint_context.image_pool.push(WidgetVertex::default());
            }
        }
        ensure!(paint_context.image_pool.len() <= Self::MAX_IMAGE_VERTICES);
        gpu.upload_slice_to(
            "widget-image-vertex-upload",
            &paint_context.image_pool,
            self.image_vertex_buffer.clone(),
            encoder,
        );

        if paint_context.text_pool.is_empty() {
            for _ in 0..6 {
                paint_context.text_pool.push(WidgetVertex::default());
            }
        }
        ensure!(paint_context.text_pool.len() <= Self::MAX_TEXT_VERTICES);
        gpu.upload_slice_to(
            "widget-text-vertex-upload",
            &paint_context.text_pool,
            self.text_vertex_buffer.clone(),
            encoder,
        );

        // FIXME: We should only need a new bind group if the underlying texture
        // FIXME: atlas grew and we have a new texture reference, not every frame.
        let sheet = paint_context.font_context.glyph_sheet();
        let atlas_texture = sheet.texture_binding(1);
        let atlas_sampler = sheet.sampler_binding(2);
        self.bind_group = Some(gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
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
                atlas_texture,
                // glyph_sheet_sampler: Sampler2d
                atlas_sampler,
            ],
        }));

        Ok(())
    }

    fn sys_handle_dump_texture(mut context: ResMut<PaintContext>, mut gpu: ResMut<Gpu>) {
        report!(context.handle_dump_texture(&mut gpu));
    }

    fn sys_maintain_font_atlas(
        mut paint_context: ResMut<PaintContext>,
        gpu: Res<Gpu>,
        maybe_encoder: ResMut<Option<wgpu::CommandEncoder>>,
    ) {
        if let Some(encoder) = maybe_encoder.into_inner() {
            paint_context.maintain_font_atlas(&gpu, encoder);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use input::InputTarget;
    use platform_dirs::AppDirs;

    #[test]
    fn test_label_widget() -> Result<()> {
        let mut runtime = Gpu::for_test_unix()?;
        runtime
            .insert_resource(AppDirs::new(Some("nitrogen"), true).unwrap())
            .insert_resource(TimeStep::new_60fps())
            .load_extension::<InputTarget>()?
            .load_extension::<WidgetBuffer>()?;

        // let label = Label::new(
        //     "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789\
        //     สิบสองกษัตริย์ก่อนหน้าแลถัดไป       สององค์ไซร้โง่เขลาเบาปัญญา\
        //     Зарегистрируйтесь сейчас на Десятую Международную Конференцию по\
        //     გთხოვთ ახლავე გაიაროთ რეგისტრაცია Unicode-ის მეათე საერთაშორისო\
        //     ∮ E⋅da = Q,  n → ∞, ∑ f(i) = ∏ g(i), ∀x∈ℝ: ⌈x⌉ = −⌊−x⌋, α ∧ ¬β = ¬(¬α ∨ β)\
        //     Οὐχὶ ταὐτὰ παρίσταταί μοι γιγνώσκειν, ὦ ἄνδρες ᾿Αθηναῖοι,\
        //     ði ıntəˈnæʃənəl fəˈnɛtık əsoʊsiˈeıʃn\
        //     Y [ˈʏpsilɔn], Yen [jɛn], Yoga [ˈjoːgɑ]",
        // )
        // .wrapped();
        // runtime
        //     .resource_mut::<WidgetBuffer>()
        //     .root_container()
        //     .write()
        //     .add_child("label", label);

        runtime.run_frame_once();

        Ok(())
    }
}
