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
    font_context::{FontContext, FontId, TextSpanMetrics},
    paint_context::PaintContext,
    size::{Extent, Position, ScreenDir, Size},
    text_run::TextRun,
    widget::Widget,
    widget_info::WidgetInfo,
};
use anyhow::Result;
use gpu::Gpu;
use input::GenericEvent;
use nitrous::Interpreter;
use parking_lot::RwLock;
use std::sync::Arc;

#[derive(Debug)]
pub struct Label {
    position: Position<Size>,
    metrics: TextSpanMetrics,
    line: TextRun,
}

impl Label {
    pub fn new<S: AsRef<str> + Into<String>>(content: S) -> Self {
        Self {
            position: Position::origin(),
            metrics: TextSpanMetrics::default(),
            line: TextRun::from_text(content.as_ref()).with_hidden_selection(),
        }
    }

    pub fn with_size(mut self, size: Size) -> Self {
        self.line.set_default_size(size);
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
    fn measure(&mut self, gpu: &Gpu, font_context: &mut FontContext) -> Result<Extent<Size>> {
        self.metrics = self.line.measure(gpu, font_context)?;
        Ok(Extent::<Size>::new(
            self.metrics.width.into(),
            (self.metrics.height - self.metrics.descent).into(),
        ))
    }

    fn layout(
        &mut self,
        gpu: &Gpu,
        mut position: Position<Size>,
        _extent: Extent<Size>,
        _font_context: &mut FontContext,
    ) -> Result<()> {
        *position.top_mut() =
            position
                .top()
                .sub(&self.metrics.descent.into(), gpu, ScreenDir::Vertical);
        self.position = position;
        Ok(())
    }

    fn upload(&self, gpu: &Gpu, context: &mut PaintContext) -> Result<()> {
        let widget_info_index = context.push_widget(&WidgetInfo::default());

        self.line
            .upload(self.position, widget_info_index, gpu, context)?;

        Ok(())
    }

    fn handle_event(
        &mut self,
        _event: &GenericEvent,
        _focus: &str,
        _interpreter: Arc<RwLock<Interpreter>>,
    ) -> Result<()> {
        Ok(())
    }
}
