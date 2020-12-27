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
    color::Color, layout::LayoutEngine, paint_context::PaintContext, widget::Widget,
    widget_info::WidgetInfo, SANS_FONT_NAME,
};
use gpu::GPU;
use parking_lot::RwLock;
use std::sync::Arc;

pub struct Label {
    content: String,
    size_pts: f32,
    font_name: String,
    color: Color,
}

impl Label {
    pub fn new<S: Into<String>>(markup: S) -> Self {
        Self {
            content: markup.into(),
            size_pts: 12.0,
            font_name: SANS_FONT_NAME.to_owned(),
            color: Color::Black,
        }
    }

    pub fn with_size(mut self, size_pts: f32) -> Self {
        self.size_pts = size_pts.into();
        self
    }

    pub fn with_font<S: Into<String>>(mut self, font_name: S) -> Self {
        self.font_name = font_name.into();
        self
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn wrapped(self) -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(self))
    }

    pub fn set_markup<S: Into<String>>(&mut self, markup: S) {
        self.content = markup.into();
    }
}

impl Widget for Label {
    fn upload(&self, gpu: &GPU, context: &mut PaintContext) {
        let info = WidgetInfo::default().with_foreground_color(self.color);
        let widget_id = context.push_widget(&info);
        LayoutEngine::span_to_triangles(
            gpu,
            &self.content,
            &mut context.font_context,
            &self.font_name,
            self.size_pts,
            context.current_depth + PaintContext::TEXT_DEPTH,
            widget_id,
            &mut context.text_pool,
        );
    }
}
