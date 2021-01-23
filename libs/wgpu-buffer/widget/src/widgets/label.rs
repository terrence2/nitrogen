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
    font_context::FontId,
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

pub struct Label {
    line: TextRun,
    width: Option<f32>,
}

impl Label {
    pub fn new<S: AsRef<str> + Into<String>>(content: S) -> Self {
        Self {
            line: TextRun::from_text(content.as_ref()).with_hidden_selection(),
            width: None,
        }
    }

    pub fn with_size(mut self, size_pts: f32) -> Self {
        self.line.set_default_size_pts(size_pts);
        self
    }

    pub fn with_font(mut self, font_id: FontId) -> Self {
        self.line.set_default_font(font_id);
        // Note: this is a label; we don't allow selection, so no need to save and restore it.
        self.line.select_all();
        self.line.change_font(font_id);
        self.line.select_none();
        self
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.line.set_default_color(color);
        // Note: this is a label; we don't allow selection, so no need to save and restore it.
        self.line.select_all();
        self.line.change_color(color);
        self.line.select_none();
        self
    }

    pub fn with_pre_blended_text(mut self) -> Self {
        self.line = self.line.with_pre_blended_text();
        self
    }

    pub fn wrapped(self) -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(self))
    }

    pub fn set_text<S: AsRef<str> + Into<String>>(&mut self, content: S) {
        self.line.select_all();
        self.line.insert(content.as_ref());
    }
}

impl Widget for Label {
    fn upload(&self, gpu: &GPU, context: &mut PaintContext) -> Fallible<UploadMetrics> {
        let info = WidgetInfo::default(); //.with_foreground_color(self.default_color);
        let widget_info_index = context.push_widget(&info);

        let (line_metrics, _) = self.line.upload(0f32, widget_info_index, gpu, context)?;

        Ok(UploadMetrics {
            widget_info_indexes: line_metrics.widget_info_indexes,
            width: self.width.unwrap_or(line_metrics.width),
            height: line_metrics.height,
        })
    }

    fn handle_keyboard(&mut self, _events: &[(KeyboardInput, ModifiersState)]) -> Fallible<()> {
        Ok(())
    }
}
