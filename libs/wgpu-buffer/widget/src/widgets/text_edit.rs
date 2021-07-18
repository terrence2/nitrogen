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
    widget::{Size, UploadMetrics, Widget},
    widget_info::WidgetInfo,
};
use anyhow::Result;
use gpu::Gpu;
use input::GenericEvent;
use nitrous::Interpreter;
use parking_lot::RwLock;
use std::sync::Arc;

#[derive(Debug)]
pub struct TextEdit {
    lines: Vec<TextRun>,
    width: f32,
    read_only: bool,
    default_color: Color,
    default_font: FontId,
    default_size: Size,
}

impl TextEdit {
    pub fn new(markup: &str) -> Self {
        let mut obj = Self {
            lines: vec![],
            width: 1.,
            read_only: true, // NOTE: writable text edits not supported yet.
            default_color: Color::Black,
            default_font: SANS_FONT_ID,
            default_size: Size::Pts(12.),
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

    pub fn with_default_size(mut self, size: Size) -> Self {
        self.default_size = size;
        self
    }

    pub fn with_width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn with_text(mut self, text: &str) -> Self {
        self.replace_content(text);
        self
    }

    pub fn wrapped(self) -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(self))
    }

    pub fn replace_content(&mut self, markup: &str) {
        let lines = markup
            .split('\n')
            .map(|markup| self.make_run(markup))
            .collect::<Vec<TextRun>>();
        self.lines = lines;
    }

    pub fn append_line(&mut self, markup: &str) {
        self.lines.push(self.make_run(markup));
    }

    fn make_run(&self, text: &str) -> TextRun {
        TextRun::empty()
            .with_hidden_selection()
            .with_default_size(self.default_size)
            .with_default_color(self.default_color)
            .with_default_font(self.default_font)
            .with_text(text)
    }
}

impl Widget for TextEdit {
    fn upload(&self, gpu: &Gpu, context: &mut PaintContext) -> Result<UploadMetrics> {
        let info = WidgetInfo::default();
        let widget_info_index = context.push_widget(&info);

        let mut height_offset = 0f32;
        for (i, line) in self.lines.iter().enumerate() {
            let (run_metrics, span_metrics) =
                line.upload(height_offset, widget_info_index, gpu, context)?;

            if i != self.lines.len() - 1 {
                height_offset += span_metrics.line_gap;
            }
            height_offset += run_metrics.height;
        }

        Ok(UploadMetrics {
            widget_info_indexes: vec![widget_info_index],
            width: self.width,
            height: height_offset,
        })
    }

    fn handle_event(
        &mut self,
        _event: &GenericEvent,
        _focus: &str,
        _interpreter: Arc<RwLock<Interpreter>>,
    ) -> Result<()> {
        assert!(self.read_only);
        Ok(())
    }
}
