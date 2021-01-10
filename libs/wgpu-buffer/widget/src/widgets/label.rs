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
            line: TextRun::from_text(content.as_ref()),
            width: None,
        }
    }

    pub fn with_size(mut self, size_pts: f32) -> Self {
        self.line.set_all_size_pts(size_pts);
        self
    }

    pub fn with_font(mut self, font_id: FontId) -> Self {
        self.line.set_all_font(font_id);
        self
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.line.set_all_color(color);
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
    fn upload(&self, gpu: &GPU, context: &mut PaintContext) -> UploadMetrics {
        let info = WidgetInfo::default(); //.with_foreground_color(self.default_color);
        let widget_info_index = context.push_widget(&info);

        let line_metrics = self.line.upload(0f32, widget_info_index, gpu, context);
        UploadMetrics {
            widget_info_indexes: vec![widget_info_index],
            width: self.width.unwrap_or(line_metrics.width),
            baseline_height: line_metrics.baseline_height,
            height: line_metrics.height,
        }
    }

    fn handle_keyboard(&mut self, _events: &[(KeyboardInput, ModifiersState)]) -> Fallible<()> {
        Ok(())
    }
}
