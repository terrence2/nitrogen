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
use crate::{
    font_context::FontContext,
    region::Position,
    text_run::{SpanSelection, TextSpan},
    widget_info::WidgetInfo,
    widget_vertex::WidgetVertex,
};
use anyhow::Result;
use font_common::Font;
use gpu::Gpu;
use nitrous::{inject_nitrous_resource, method, NitrousResource};
use std::{borrow::Borrow, ops::Range};
use window::{
    size::{AbsSize, RelSize},
    Window,
};

#[derive(Debug, NitrousResource)]
pub struct PaintContext {
    pub font_context: FontContext,
    pub widget_info_pool: Vec<WidgetInfo>,
    pub background_pool: Vec<WidgetVertex>,
    pub text_pool: Vec<WidgetVertex>,
    pub image_pool: Vec<WidgetVertex>,
}

#[inject_nitrous_resource]
impl PaintContext {
    // FIXME: use these
    pub const BACKGROUND_DEPTH: RelSize = RelSize::Gpu(0.75);
    pub const BORDER_DEPTH: RelSize = RelSize::Gpu(0.5);

    // Note: we adjust offset up by 0.2 so that selection regions can be drawn under the text
    pub const TEXT_DEPTH: RelSize = RelSize::Gpu(0.2);

    pub const BOX_DEPTH_SIZE: RelSize = RelSize::Gpu(1.);

    pub fn new(gpu: &Gpu) -> Self {
        Self {
            font_context: FontContext::new(gpu),
            widget_info_pool: Vec::new(),
            background_pool: Vec::new(),
            image_pool: Vec::new(),
            text_pool: Vec::new(),
        }
    }

    // Some data is frame-coherent, some is fresh for each frame. We mix them together in this
    // struct, inconveniently, so that we need to thread fewer random parameters through our
    // entire upload call stack.
    pub fn reset_for_frame(&mut self) {
        self.widget_info_pool.truncate(0);
        self.background_pool.truncate(0);
        self.image_pool.truncate(0);
        self.text_pool.truncate(0);
    }

    pub fn widget_mut(&mut self, offset: u32) -> &mut WidgetInfo {
        &mut self.widget_info_pool[offset as usize]
    }

    pub fn add_font<S: Borrow<str> + Into<String>>(&mut self, font_name: S, font: Font) {
        self.font_context.add_font(font_name, font);
    }

    #[method]
    pub fn dump_glyphs(&mut self) -> Result<()> {
        self.font_context.dump_glyphs()
    }

    pub fn push_widget(&mut self, info: &WidgetInfo) -> u32 {
        let offset = self.widget_info_pool.len();
        self.widget_info_pool.push(*info);
        offset as u32
    }

    pub fn layout_text(
        &mut self,
        span: &TextSpan,
        offset: Position<AbsSize>,
        widget_info_index: u32,
        selection_area: SpanSelection,
        win: &Window,
        gpu: &Gpu,
    ) -> Result<()> {
        self.font_context.layout_text(
            span,
            widget_info_index,
            offset.with_depth(offset.depth() + Self::TEXT_DEPTH),
            selection_area,
            win,
            gpu,
            &mut self.text_pool,
            &mut self.background_pool,
        )
    }

    pub fn maintain_font_atlas(&mut self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder) {
        self.font_context.maintain_font_atlas(gpu, encoder);
    }

    pub fn handle_dump_texture(&mut self, gpu: &mut Gpu) -> Result<()> {
        self.font_context.handle_dump_texture(gpu)
    }

    pub fn background_vertex_count(&self) -> usize {
        self.background_pool.len()
    }

    pub fn text_vertex_count(&self) -> usize {
        self.text_pool.len()
    }

    pub fn background_vertex_range(&self) -> Range<u32> {
        0u32..self.background_pool.len() as u32
    }

    pub fn text_vertex_range(&self) -> Range<u32> {
        0u32..self.text_pool.len() as u32
    }
}
