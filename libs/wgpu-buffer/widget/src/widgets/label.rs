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
    widget_vertex::WidgetVertex,
    widgets::{PaintContext, Widget, WidgetInfo},
    FontName,
};
use failure::Fallible;
use shader_shared::Group;

pub struct Label {
    content: String,
    size_em: f32,
    font_name: FontName,
    info: WidgetInfo,
}

impl Label {
    pub fn new<S: Into<String>>(markup: S) -> Self {
        Self {
            content: markup.into(),
            size_em: 1.0,
            font_name: crate::FALLBACK_FONT_NAME.to_owned(),
            info: WidgetInfo {
                border_color: [0f32; 4],
                background_color: [0f32; 4],
                foreground_color: [1f32, 0f32, 0f32, 1f32],
            },
        }
    }
}

impl Widget for Label {
    fn upload(&self, context: &mut PaintContext) {
        let widget_id = context.push_widget(self.info);
        LayoutEngine::span_to_triangles(
            &self.content,
            &mut context.font_context,
            &self.font_name,
            self.size_em,
            context.current_depth + PaintContext::TEXT_DEPTH,
            widget_id,
            &mut context.text_pool,
        );
    }
}
