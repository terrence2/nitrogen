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
    font_context::FontContext, font_context::TextSpanMetrics, widget_info::WidgetInfo,
    widget_vertex::WidgetVertex,
};
use font_common::FontInterface;
use gpu::GPU;
use parking_lot::RwLock;
use std::sync::Arc;

pub struct PaintContext {
    pub current_depth: f32,
    pub font_context: FontContext,
    pub widget_info_pool: Vec<WidgetInfo>,
    pub background_pool: Vec<WidgetVertex>,
    pub text_pool: Vec<WidgetVertex>,
    pub image_pool: Vec<WidgetVertex>,
}

impl PaintContext {
    pub const TEXT_DEPTH: f32 = 0.75f32;
    pub const BOX_DEPTH_SIZE: f32 = 1f32;

    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            current_depth: 0f32,
            font_context: FontContext::new(device),
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
        self.current_depth = 0f32;
        self.widget_info_pool.truncate(0);
        self.background_pool.truncate(0);
        self.image_pool.truncate(0);
        self.text_pool.truncate(0);
    }

    pub fn add_font<S: Into<String>>(
        &mut self,
        font_name: S,
        font: Arc<RwLock<dyn FontInterface>>,
    ) {
        self.font_context.add_font(font_name.into(), font);
    }

    pub fn enter_box(&mut self) {
        self.current_depth += Self::BOX_DEPTH_SIZE;
    }

    pub fn push_widget(&mut self, info: &WidgetInfo) -> u32 {
        let offset = self.widget_info_pool.len();
        self.widget_info_pool.push(*info);
        offset as u32
    }

    /// Returns [width, baseline_height, total_height]
    pub fn layout_text(
        &mut self,
        span: &str,
        font_name: &str,
        size_pts: f32,
        widget_info_index: u32,
        gpu: &GPU,
    ) -> TextSpanMetrics {
        self.font_context.layout_text(
            span,
            font_name,
            size_pts,
            widget_info_index,
            self.current_depth + Self::TEXT_DEPTH,
            gpu,
            &mut self.text_pool,
        )
    }
}
