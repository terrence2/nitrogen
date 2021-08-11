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
    font_context::{FontContext, TextSpanMetrics},
    region::Position,
    text_run::{SpanSelection, TextSpan},
    widget_info::WidgetInfo,
    widget_vertex::WidgetVertex,
};
use anyhow::Result;
use font_common::FontInterface;
use gpu::{
    size::{AbsSize, LeftBound, RelSize},
    Gpu, UploadTracker,
};
use parking_lot::RwLock;
use std::{borrow::Borrow, sync::Arc};
use tokio::runtime::Runtime;

#[derive(Debug)]
pub struct PaintContext {
    pub current_depth: RelSize,
    pub font_context: FontContext,
    pub widget_info_pool: Vec<WidgetInfo>,
    pub background_pool: Vec<WidgetVertex>,
    pub text_pool: Vec<WidgetVertex>,
    pub image_pool: Vec<WidgetVertex>,
}

impl PaintContext {
    pub const BACKGROUND_DEPTH: RelSize = RelSize::from_percent(0.75);
    pub const BORDER_DEPTH: RelSize = RelSize::from_percent(0.5);
    pub const TEXT_DEPTH: RelSize = RelSize::from_percent(0.25);
    pub const BOX_DEPTH_SIZE: RelSize = RelSize::from_percent(1.);

    pub fn new(gpu: &Gpu) -> Result<Self> {
        Ok(Self {
            current_depth: RelSize::zero(),
            font_context: FontContext::new(gpu)?,
            widget_info_pool: Vec::new(),
            background_pool: Vec::new(),
            image_pool: Vec::new(),
            text_pool: Vec::new(),
        })
    }

    // Some data is frame-coherent, some is fresh for each frame. We mix them together in this
    // struct, inconveniently, so that we need to thread fewer random parameters through our
    // entire upload call stack.
    pub fn reset_for_frame(&mut self) {
        self.current_depth = RelSize::zero();
        self.widget_info_pool.truncate(0);
        self.background_pool.truncate(0);
        self.image_pool.truncate(0);
        self.text_pool.truncate(0);
    }

    pub fn widget_mut(&mut self, offset: u32) -> &mut WidgetInfo {
        &mut self.widget_info_pool[offset as usize]
    }

    pub fn add_font<S: Borrow<str> + Into<String>>(
        &mut self,
        font_name: S,
        font: Arc<RwLock<dyn FontInterface>>,
    ) {
        self.font_context.add_font(font_name, font);
    }

    pub fn dump_glyphs(&mut self) {
        self.font_context.dump_glyphs();
    }

    pub fn enter_box(&mut self) {
        self.current_depth += Self::BOX_DEPTH_SIZE;
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
        gpu: &Gpu,
    ) -> Result<TextSpanMetrics> {
        self.font_context.layout_text(
            span,
            widget_info_index,
            offset.with_depth(self.current_depth + Self::TEXT_DEPTH),
            selection_area,
            gpu,
            &mut self.text_pool,
            &mut self.background_pool,
        )
    }

    pub fn make_upload_buffer(
        &mut self,
        gpu: &mut Gpu,
        async_rt: &Runtime,
        tracker: &mut UploadTracker,
    ) -> Result<()> {
        self.font_context.make_upload_buffer(gpu, async_rt, tracker)
    }

    pub fn maintain_font_atlas<'a>(
        &'a self,
        cpass: wgpu::ComputePass<'a>,
    ) -> Result<wgpu::ComputePass<'a>> {
        self.font_context.maintain_font_atlas(cpass)
    }
}
