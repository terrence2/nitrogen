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
mod glyph_cache;
mod layout;
mod layout_vertex;
mod widget_vertex;
mod widgets;

use crate::{glyph_cache::GlyphCache, layout::Layout};
pub use crate::{
    layout_vertex::LayoutVertex,
    widgets::{float_box::FloatBox, label::Label, PaintContext, Widget},
};

use atlas::{AtlasPacker, Frame};
use commandable::{commandable, Commandable};
use failure::Fallible;
use font_common::FontInterface;
use font_ttf::TtfFont;
use gpu::{UploadTracker, GPU};
use image::Rgba;
use log::trace;
use parking_lot::RwLock;
use std::{collections::HashMap, sync::Arc};

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
pub const FALLBACK_FONT_NAME: &str = "quantico";
const QUANTICO_TTF_DATA: &[u8] = include_bytes!("../../../../assets/font/quantico.ttf");

#[derive(Copy, Clone, Debug)]
pub enum TextAnchorH {
    Center,
    Left,
    Right,
}

#[derive(Copy, Clone, Debug)]
pub enum TextAnchorV {
    Center,
    Top,
    Bottom,
    // TODO: look for empty space under '1' or 'a' or similar.
    // Baseline,
}

#[derive(Copy, Clone, Debug)]
pub enum TextPositionH {
    // In vulkan screen space: -1.0 -> 1.0
    Vulkan(f32),

    // In FA screen space: 0 -> 640
    FA(u32),

    // Labeled positions
    Center,
    Left,
    Right,
}

impl TextPositionH {
    fn to_vulkan(self) -> f32 {
        const SCALE: f32 = 640f32;
        match self {
            TextPositionH::Center => 0f32,
            TextPositionH::Left => -1f32,
            TextPositionH::Right => 1f32,
            TextPositionH::Vulkan(v) => v,
            TextPositionH::FA(i) => (i as f32) / SCALE * 2f32 - 1f32,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum TextPositionV {
    // In vulkan screen space: -1.0 -> 1.0
    Vulkan(f32),

    // In FA screen space: 0 -> 640 or 0 -> 480 depending on axis
    FA(u32),

    // Labeled positions
    Center,
    Top,
    Bottom,
}

impl TextPositionV {
    fn to_vulkan(self) -> f32 {
        const SCALE: f32 = 480f32;
        match self {
            TextPositionV::Center => 0f32,
            TextPositionV::Top => -1f32,
            TextPositionV::Bottom => 1f32,
            TextPositionV::Vulkan(v) => v,
            TextPositionV::FA(i) => (i as f32) / SCALE * 2f32 - 1f32,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct LayoutHandle(usize);

impl LayoutHandle {
    pub fn grab<'a>(&self, buffer: &'a mut TextLayoutBuffer) -> &'a mut Layout {
        buffer.layout_mut(*self)
    }
}

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
pub struct TextLayoutBuffer {
    // Map from fonts to the glyph cache needed to create and render layouts.
    glyph_cache: HashMap<FontName, Arc<RwLock<GlyphCache>>>,

    // FIXME: How do we want to manage our draw state? Seems like pre-mature optimization at this point.
    layout_map: HashMap<FontName, Vec<LayoutHandle>>,

    // Individual spans, rather than a label that may contain marked-up text.
    layouts: Vec<Layout>,

    root: Arc<RwLock<FloatBox>>,
    paint_context: PaintContext,

    glyph_bind_group_layout: wgpu::BindGroupLayout,
    layout_bind_group_layout: wgpu::BindGroupLayout,
}

#[commandable]
impl TextLayoutBuffer {
    pub fn new(gpu: &mut GPU) -> Fallible<Self> {
        trace!("LayoutBuffer::new");

        let glyph_bind_group_layout = GlyphCache::create_bind_group_layout(gpu.device());
        let mut glyph_cache = HashMap::new();

        let layout_bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("text-layout-bind-group-layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStage::VERTEX,
                        ty: wgpu::BindingType::StorageBuffer {
                            dynamic: false,
                            readonly: true,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                });

        // Add fallback font.
        glyph_cache.insert(
            FALLBACK_FONT_NAME.to_owned(),
            Arc::new(RwLock::new(GlyphCache::new(
                TtfFont::from_bytes("quantico", &QUANTICO_TTF_DATA, gpu)?,
                &glyph_bind_group_layout,
                gpu,
            ))),
        );

        Ok(Self {
            root: Arc::new(RwLock::new(FloatBox::new())),
            paint_context: PaintContext::new(),
            glyph_cache,
            layout_map: HashMap::new(),
            layouts: Vec::new(),
            glyph_bind_group_layout,
            layout_bind_group_layout,
        })
    }

    pub fn root(&self) -> Arc<RwLock<FloatBox>> {
        self.root.clone()
    }

    pub fn add_font(&mut self, font_name: FontName, font: Box<dyn FontInterface>, gpu: &GPU) {
        self.paint_context.add_font(font_name, font);
    }

    pub fn glyph_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.glyph_bind_group_layout
    }

    pub fn layout_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout_bind_group_layout
    }

    pub fn layouts(&self) -> &Vec<Layout> {
        &self.layouts
    }

    pub fn layouts_by_font(&self) -> &HashMap<FontName, Vec<LayoutHandle>> {
        &self.layout_map
    }

    pub fn layout(&self, handle: LayoutHandle) -> &Layout {
        &self.layouts[handle.0]
    }

    pub fn layout_mut(&mut self, handle: LayoutHandle) -> &mut Layout {
        &mut self.layouts[handle.0]
    }

    pub fn glyph_cache(&self, font_name: &str) -> Arc<RwLock<GlyphCache>> {
        if let Some(cache) = self.glyph_cache.get(font_name) {
            return cache.to_owned();
        }
        self.glyph_cache[FALLBACK_FONT_NAME].to_owned()
    }

    // pub fn layout_mut(&mut self, handle: LayoutHandle) -> &mut Layout {}

    pub fn add_screen_text(
        &mut self,
        font_name: &str,
        text: &str,
        gpu: &GPU,
    ) -> Fallible<&mut Layout> {
        let glyph_cache = self.glyph_cache(font_name);
        let handle = LayoutHandle(self.layouts.len());
        let layout = Layout::new(
            handle,
            text,
            glyph_cache,
            &self.layout_bind_group_layout,
            gpu,
        )?;
        self.layouts.push(layout);
        self.layout_map
            .entry(font_name.to_owned())
            .and_modify(|e| e.push(handle))
            .or_insert_with(|| vec![handle]);
        Ok(self.layout_mut(handle))
    }

    pub fn create_label<S: Into<String>>(&self, markup: S, _size_em: f32) -> Arc<RwLock<Label>> {
        Arc::new(RwLock::new(Label::new(markup)))
    }

    pub fn make_upload_buffer(&mut self, gpu: &GPU, tracker: &mut UploadTracker) -> Fallible<()> {
        self.root.read().upload(&mut self.paint_context);

        // for layout in self.layouts.iter_mut() {
        //     layout.make_upload_buffer(&layout.glyph_cache().read(), gpu, tracker)?;
        // }

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

        let mut widgets = TextLayoutBuffer::new(&mut gpu)?;
        let mut label = widgets.create_label("hello", 2.0);
        widgets.root().write().pin_child(label, 0.0, 0.0);

        Ok(())
    }

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
}
