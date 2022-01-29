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
    font_context::{FontContext, FontId, SANS_FONT_ID},
    paint_context::PaintContext,
    region::{Extent, Position, Region},
    text_run::TextRun,
    widget::{Widget, WidgetFocus},
    widget_info::WidgetInfo,
};
use anyhow::Result;
use gpu::Gpu;
use input::InputEvent;
use parking_lot::RwLock;
use runtime::ScriptHerder;
use std::{sync::Arc, time::Instant};
use window::{
    size::{AbsSize, LeftBound, Size},
    Window,
};

#[derive(Debug)]
pub struct TextEdit {
    lines: Vec<TextRun>,
    read_only: bool,
    default_color: Color,
    default_font: FontId,
    default_size: Size,

    measured_extent: Extent<AbsSize>,
    layout_position: Position<Size>,
    layout_extent: Extent<Size>,
}

impl TextEdit {
    pub fn new(markup: &str) -> Self {
        let mut obj = Self {
            lines: vec![],
            read_only: true, // NOTE: writable text edits not supported yet.
            default_color: Color::Black,
            default_font: SANS_FONT_ID,
            default_size: Size::from_pts(12.),

            measured_extent: Extent::zero(),
            layout_position: Position::origin(),
            layout_extent: Extent::zero(),
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
    fn measure(&mut self, win: &Window, font_context: &mut FontContext) -> Result<Extent<Size>> {
        let mut width = AbsSize::zero();
        let mut height_offset = AbsSize::zero();
        for (i, line) in self.lines.iter().enumerate() {
            let span_metrics = line.measure(win, font_context)?;
            if i != self.lines.len() - 1 {
                height_offset += span_metrics.line_gap;
            }
            height_offset += span_metrics.height;
            width = width.max(&span_metrics.width);
        }
        self.measured_extent = Extent::new(width, height_offset);
        Ok(self.measured_extent.into())
    }

    fn layout(
        &mut self,
        _now: Instant,
        region: Region<Size>,
        _win: &Window,
        _font_context: &mut FontContext,
    ) -> Result<()> {
        self.layout_position = *region.position();
        self.layout_extent = *region.extent();
        Ok(())
    }

    fn upload(
        &self,
        _now: Instant,
        win: &Window,
        gpu: &Gpu,
        context: &mut PaintContext,
    ) -> Result<()> {
        let info = WidgetInfo::default();
        let widget_info_index = context.push_widget(&info);

        let mut pos = self.layout_position.as_abs(win);
        *pos.bottom_mut() += self.measured_extent.height();
        for (i, line) in self.lines.iter().enumerate() {
            let span_metrics = line.measure(win, &mut context.font_context)?;
            *pos.bottom_mut() -= span_metrics.height;
            let span_metrics = line.upload(pos.into(), widget_info_index, win, gpu, context)?;
            if i != self.lines.len() - 1 {
                *pos.bottom_mut() -= span_metrics.line_gap;
            }
        }

        Ok(())
    }

    fn handle_event(
        &mut self,
        _event: &InputEvent,
        _focus: WidgetFocus,
        _cursor_position: Position<AbsSize>,
        _herder: &mut ScriptHerder,
    ) -> Result<()> {
        assert!(self.read_only);
        Ok(())
    }
}
