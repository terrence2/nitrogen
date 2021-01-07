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
    font_context::{FontId, SANS_FONT_ID},
    paint_context::PaintContext,
    text_run::TextRun,
    widget::{UploadMetrics, Widget},
    widget_info::WidgetInfo,
};
use failure::Fallible;
use gpu::GPU;
use parking_lot::RwLock;
use std::sync::Arc;
use winit::event::{KeyboardInput, ModifiersState};

pub struct TextEdit {
    lines: Vec<TextRun>,
    width: f32,
    read_only: bool,
    default_color: Color,
    default_font: FontId,
    default_size_pts: f32,
}

impl TextEdit {
    pub fn new(markup: &str) -> Self {
        let mut obj = Self {
            lines: vec![],
            width: 1.,
            read_only: true, // NOTE: writable text edits not supported yet.
            default_color: Color::Black,
            default_font: SANS_FONT_ID,
            default_size_pts: 12.,
        };
        obj.replace_content(markup);
        obj
    }

    pub fn with_default_color(mut self, color: Color) -> Self {
        self.default_color = color;
        self
    }

    pub fn with_default_font(mut self, font_id: FontId) -> Self {
        self.default_font = font_id;
        self
    }

    pub fn with_default_size(mut self, size_pts: f32) -> Self {
        self.default_size_pts = size_pts;
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
            .map(TextRun::from_text)
            .collect::<Vec<TextRun>>();
        self.lines = lines;
    }

    pub fn append_line(&mut self, markup: &str) {
        self.lines.push(TextRun::from_text(markup));
    }
}

impl Widget for TextEdit {
    fn upload(&self, gpu: &GPU, context: &mut PaintContext) -> UploadMetrics {
        let info = WidgetInfo::default().with_foreground_color(self.default_color);
        let widget_info_index = context.push_widget(&info);

        let mut height_offset = 0f32;
        for line in &self.lines {
            let run_metrics = line.upload(height_offset, widget_info_index, gpu, context);
            height_offset += run_metrics.height;
        }

        UploadMetrics {
            widget_info_indexes: vec![widget_info_index],
            width: self.width,
            baseline_height: height_offset,
            height: height_offset,
        }
    }

    fn handle_keyboard(&mut self, _events: &[(KeyboardInput, ModifiersState)]) -> Fallible<()> {
        assert!(self.read_only);
        Ok(())
    }
}
