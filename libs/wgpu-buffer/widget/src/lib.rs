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
mod layout;
mod packing;
mod widget_vertex;
mod widgets;

pub use crate::{
    widget_vertex::WidgetVertex,
    widgets::{label::Label, vertical_box::VerticalBox, PaintContext, Widget, WidgetInfo},
};

use commandable::{commandable, Commandable};
use failure::Fallible;
use font_common::FontInterface;
use font_ttf::TtfFont;
use gpu::{UploadTracker, GPU};
use log::trace;
use parking_lot::RwLock;
use std::{mem, num::NonZeroU64, ops::Range, sync::Arc};

// Drawing UI efficiently:
//
// We have one pipeline for each of the following.
// 1) Draw all widget backgrounds / borders in one pipeline, with depth.
// 2) Draw all text
// 3) Draw all images
//
// Widget upload recurses through the tree of widgets. Each layer gets a 1.0 wide depth slot to
// render into. They may upload vertices to 3 vertex pools, one for each of the above concerns.
// Rendering is done from leaf up, making use of the depth test to avoid overpaint. Vertices
// contain x, y, and z coordinates in screen space, s and t texture coordinates, and an index
// into the widget info buffer. There is one slot in the info buffer per widget where the majority
// of the widget data lives, so save space in vertices.

// Fallback for when we have no libs loaded.
// https://fonts.google.com/specimen/Quantico?selection.family=Quantico
pub const SANS_FONT_NAME: &str = "sans";
pub const MONO_FONT_NAME: &str = "mono";
const FIRA_SANS_REGULAR_TTF_DATA: &[u8] =
    include_bytes!("../../../../assets/font/FiraSans-Regular.ttf");
const FIRA_MONO_REGULAR_TTF_DATA: &[u8] =
    include_bytes!("../../../../assets/font/FiraMono-Regular.ttf");

// Context required for rendering a specific text span (as opposed to the layout in general).
// e.g. the vertex and index buffers.
struct LayoutTextRenderContext {
    render_width: f32,
    vertex_buffer: Arc<Box<wgpu::Buffer>>,
    index_buffer: Arc<Box<wgpu::Buffer>>,
    index_count: u32,
}

pub type FontName = String;

#[derive(Commandable)]
pub struct WidgetBuffer {
    // Widget state.
    root: Arc<RwLock<VerticalBox>>,
    paint_context: PaintContext,

    // The four key buffers.
    widget_info_buffer_size: wgpu::BufferAddress,
    widget_info_buffer: Arc<Box<wgpu::Buffer>>,

    // background_vertex_buffer_size: wgpu::BufferAddress,
    // background_vertex_buffer: Arc<Box<wgpu::Buffer>>,
    //
    // image_vertex_buffer_size: wgpu::BufferAddress,
    // image_vertex_buffer: Arc<Box<wgpu::Buffer>>,
    text_vertex_buffer_size: wgpu::BufferAddress,
    text_vertex_buffer: Arc<Box<wgpu::Buffer>>,

    // The accumulated bind group for all widget rendering, encompasing everything we uploaded above.
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: Option<wgpu::BindGroup>,
}

#[commandable]
impl WidgetBuffer {
    const MAX_WIDGETS: usize = 512;
    const MAX_TEXT_VERTICES: usize = Self::MAX_WIDGETS * 128 * 6;

    pub fn new(gpu: &mut GPU) -> Fallible<Self> {
        trace!("WidgetBuffer::new");

        let mut paint_context = PaintContext::new(gpu.device());
        paint_context.add_font("mono", TtfFont::from_bytes(&FIRA_MONO_REGULAR_TTF_DATA)?);
        paint_context.add_font("sans", TtfFont::from_bytes(&FIRA_SANS_REGULAR_TTF_DATA)?);

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
            root: Arc::new(RwLock::new(VerticalBox::new())),
            paint_context,

            widget_info_buffer_size,
            widget_info_buffer,

            text_vertex_buffer_size,
            text_vertex_buffer,

            bind_group_layout,
            bind_group: None,
        })
    }

    pub fn root(&self) -> Arc<RwLock<VerticalBox>> {
        self.root.clone()
    }

    pub fn add_font<S: Into<String>>(
        &mut self,
        font_name: S,
        font: Arc<RwLock<dyn FontInterface>>,
    ) {
        self.paint_context.add_font(font_name.into(), font);
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    /// Must only be called after first upload
    pub fn bind_group(&self) -> &wgpu::BindGroup {
        self.bind_group.as_ref().unwrap()
    }

    pub fn text_vertex_buffer(&self) -> wgpu::BufferSlice {
        self.text_vertex_buffer.slice(
            0u64..(mem::size_of::<WidgetVertex>() * self.paint_context.text_pool.len()) as u64,
        )
    }

    pub fn text_vertex_range(&self) -> Range<u32> {
        0u32..self.paint_context.text_pool.len() as u32
    }

    pub fn create_label<S: Into<String>>(&self, markup: S) -> Arc<RwLock<Label>> {
        Arc::new(RwLock::new(Label::new(markup)))
    }

    pub fn make_upload_buffer(&mut self, gpu: &GPU, tracker: &mut UploadTracker) -> Fallible<()> {
        self.paint_context.reset_for_frame();
        self.root.read().upload(gpu, &mut self.paint_context);

        self.paint_context.font_context.upload(gpu, tracker);

        let widget_info_upload = gpu.push_slice(
            "widget-info-upload",
            &self.paint_context.widget_info_pool,
            wgpu::BufferUsage::COPY_SRC,
        );
        let widget_info_size = self.widget_info_buffer_size.min(
            (mem::size_of::<WidgetInfo>() * self.paint_context.widget_info_pool.len())
                as wgpu::BufferAddress,
        );
        tracker.upload_ba(
            widget_info_upload,
            self.widget_info_buffer.clone(),
            widget_info_size,
        );

        let text_vertex_upload = gpu.push_slice(
            "widget-text-vertex-upload",
            &self.paint_context.text_pool,
            wgpu::BufferUsage::COPY_SRC,
        );
        let text_vertex_upload_size = self.text_vertex_buffer_size.min(
            (mem::size_of::<WidgetVertex>() * self.paint_context.text_pool.len())
                as wgpu::BufferAddress,
        );
        tracker.upload_ba(
            text_vertex_upload,
            self.text_vertex_buffer.clone(),
            text_vertex_upload_size,
        );

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
        let label = widgets.create_label("ABCDEFGHIJKLMNOPQRSTUVWXYZ");
        widgets.root().write().add_child(label);

        let mut tracker = Default::default();
        widgets.make_upload_buffer(&gpu, &mut tracker)?;

        Ok(())
    }

    /*
    #[cfg(unix)]
    #[test]
    fn it_can_manage_text_layouts() -> Fallible<()> {
        use winit::platform::unix::EventLoopExtUnix;
        let event_loop = EventLoop::<()>::new_any_thread();
        let window = Window::new(&event_loop)?;
        let mut gpu = GPU::new(&window, Default::default())?;

        let mut layout_buffer = TextLayoutBuffer::new(&mut gpu)?;

        layout_buffer
            .add_screen_text("quantico", "Top Left (r)", &gpu)?
            .with_color(&[1f32, 0f32, 0f32, 1f32])
            .with_horizontal_position(TextPositionH::Left)
            .with_horizontal_anchor(TextAnchorH::Left)
            .with_vertical_position(TextPositionV::Top)
            .with_vertical_anchor(TextAnchorV::Top);

        layout_buffer
            .add_screen_text("quantico", "Top Right (b)", &gpu)?
            .with_color(&[0f32, 0f32, 1f32, 1f32])
            .with_horizontal_position(TextPositionH::Right)
            .with_horizontal_anchor(TextAnchorH::Right)
            .with_vertical_position(TextPositionV::Top)
            .with_vertical_anchor(TextAnchorV::Top);

        layout_buffer
            .add_screen_text("quantico", "Bottom Left (w)", &gpu)?
            .with_color(&[1f32, 1f32, 1f32, 1f32])
            .with_horizontal_position(TextPositionH::Left)
            .with_horizontal_anchor(TextAnchorH::Left)
            .with_vertical_position(TextPositionV::Bottom)
            .with_vertical_anchor(TextAnchorV::Bottom);

        layout_buffer
            .add_screen_text("quantico", "Bottom Right (m)", &gpu)?
            .with_color(&[1f32, 0f32, 1f32, 1f32])
            .with_horizontal_position(TextPositionH::Right)
            .with_horizontal_anchor(TextAnchorH::Right)
            .with_vertical_position(TextPositionV::Bottom)
            .with_vertical_anchor(TextAnchorV::Bottom);

        let handle_clr = layout_buffer
            .add_screen_text("quantico", "", &gpu)?
            .with_span("THR: AFT  1.0G   2462   LCOS   740 M61")
            .with_color(&[1f32, 0f32, 0f32, 1f32])
            .with_horizontal_position(TextPositionH::Center)
            .with_horizontal_anchor(TextAnchorH::Center)
            .with_vertical_position(TextPositionV::Bottom)
            .with_vertical_anchor(TextAnchorV::Bottom)
            .handle();

        let handle_fin = layout_buffer
            .add_screen_text("quantico", "DONE: 0%", &gpu)?
            .with_color(&[0f32, 1f32, 0f32, 1f32])
            .with_horizontal_position(TextPositionH::Center)
            .with_horizontal_anchor(TextAnchorH::Center)
            .with_vertical_position(TextPositionV::Center)
            .with_vertical_anchor(TextAnchorV::Center)
            .handle();

        for i in 0..32 {
            if i < 16 {
                handle_clr
                    .grab(&mut layout_buffer)
                    .set_color(&[0f32, i as f32 / 16f32, 0f32, 1f32])
            } else {
                handle_clr.grab(&mut layout_buffer).set_color(&[
                    (i as f32 - 16f32) / 16f32,
                    1f32,
                    (i as f32 - 16f32) / 16f32,
                    1f32,
                ])
            };
            let msg = format!("DONE: {}%", ((i as f32 / 32f32) * 100f32) as u32);
            handle_fin.grab(&mut layout_buffer).set_span(&msg);
        }
        Ok(())
    }
     */
}
