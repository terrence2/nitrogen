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
    color::Color,
    paint_context::PaintContext,
    widget::{UploadMetrics, Widget},
    widget_info::WidgetInfo,
    SANS_FONT_NAME,
};
use gpu::GPU;
use parking_lot::RwLock;
use std::sync::Arc;

pub struct Span {
    span: String,
    size_pts: f32,
    font_id: u32,
    color: Color,
}

pub struct Line {
    spans: Vec<Span>,
}

impl Line {
    pub fn from_markup<S: Into<String>>(markup: S) -> Self {
        Self {
            spans: vec![Span {
                span: markup.into(),
                size_pts: 12.,
                font_id: 0,
                color: Default::default(),
            }],
        }
    }
}

pub struct TextEdit {
    // FIXME: font cache
    lines: Vec<Line>,
    width: f32,
    default_color: Color,
    default_font: String,
    default_size_pts: f32,
}

impl TextEdit {
    pub fn new(markup: &str) -> Self {
        let mut obj = Self {
            lines: vec![],
            width: 1.,
            default_color: Color::Black,
            default_font: SANS_FONT_NAME.to_owned(),
            default_size_pts: 12.,
        };
        obj.replace_content(markup);
        obj
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.default_color = color;
        self
    }

    pub fn with_font(mut self, font: &str) -> Self {
        self.default_font = font.to_owned();
        self
    }

    pub fn with_width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn wrapped(self) -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(self))
    }

    pub fn replace_content(&mut self, markup: &str) {
        let lines = markup
            .split('\n')
            .map(|s| Line::from_markup(s))
            .collect::<Vec<Line>>();
        self.lines = lines;
    }

    pub fn append_line<S: Into<String>>(&mut self, markup: S) {
        self.lines.push(Line::from_markup(markup));
    }
}

impl Widget for TextEdit {
    fn upload(&self, gpu: &GPU, context: &mut PaintContext) -> UploadMetrics {
        let info = WidgetInfo::default().with_foreground_color(self.default_color);
        let widget_info_index = context.push_widget(&info);

        let mut height_offset = 0f32;
        for line in &self.lines {
            let line_gap = context
                .font_context
                .get_font(&self.default_font)
                .read()
                .line_gap(self.default_size_pts);
            let mut max_height = 0f32;
            for span in &line.spans {
                // FIXME: one info per span so that we can set the color
                let span_metrics = context.layout_text(
                    &span.span,
                    &self.default_font,
                    span.size_pts,
                    [0., -height_offset],
                    widget_info_index,
                    gpu,
                );
                // FIXME: need to be able to offset height by line.
                max_height = max_height.max(span_metrics.height + line_gap);
            }
            height_offset += max_height;
        }

        UploadMetrics {
            widget_info_indexes: vec![widget_info_index],
            width: self.width,
            baseline_height: height_offset,
            height: height_offset,
        }
    }
}
