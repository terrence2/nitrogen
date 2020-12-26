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
    layout::LayoutEngine,
    widgets::{PaintContext, Widget, WidgetInfo},
    SANS_FONT_NAME,
};
use gpu::GPU;

pub struct Label {
    content: String,
    size_pts: f32,
    font_name: String,
    info: WidgetInfo,
}

impl Label {
    pub fn new<S: Into<String>>(markup: S) -> Self {
        Self {
            content: markup.into(),
            size_pts: 14.0,
            font_name: SANS_FONT_NAME.to_owned(),
            info: WidgetInfo {
                border_color: [0f32; 4],
                background_color: [0f32; 4],
                foreground_color: [1f32, 0f32, 0f32, 1f32],
            },
        }
    }
}

impl Widget for Label {
    fn upload(&self, gpu: &GPU, context: &mut PaintContext) {
        let widget_id = context.push_widget(self.info);
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
        if std::env::var("DUMP") == Ok("1".to_owned()) {
            context
                .font_context
                .glyph_sheet
                .buffer()
                .save("./__dump__/atlas.png")
                .unwrap();
        }
    }
}
