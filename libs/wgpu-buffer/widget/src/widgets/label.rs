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
    region::{Extent, Position, Region},
    text_run::TextRun,
    widget::{Labeled, Widget, WidgetComponent},
    widget_info::WidgetInfo,
};
use anyhow::Result;
use bevy_ecs::prelude::*;
use gpu::Gpu;
use nitrous::{inject_nitrous_resource, method, HeapMut, NitrousResource, Value};
use parking_lot::RwLock;
use std::{sync::Arc, time::Instant};
use window::{
    size::{RelSize, Size},
    Window,
};

#[derive(Debug)]
pub struct Label {
    line: TextRun,

    metrics: TextSpanMetrics,
    allocated_position: Position<Size>,
    allocated_extent: Extent<Size>,
}

impl Label {
    pub fn new<S: AsRef<str> + Into<String>>(content: S) -> Self {
        Self {
            line: TextRun::from_text(content.as_ref()).with_hidden_selection(),

            metrics: TextSpanMetrics::default(),
            allocated_position: Position::origin(),
            allocated_extent: Extent::zero(),
        }
    }

    pub fn with_pre_blended_text(mut self) -> Self {
        self.line = self.line.with_pre_blended_text();
        self
    }

    pub fn wrapped(self, name: &str, mut heap: HeapMut) -> Result<Entity> {
        Ok(heap
            .spawn_named(name)?
            .insert_named(WidgetComponent::new(self))?
            .id())
    }

    fn show(&mut self, content: &str) {
        self.set_text(content);
    }

    fn set_font_by_id(&mut self, font_id: Value) -> Result<()> {
        self.set_font(FontId::from_value(font_id)?);
        Ok(())
    }

    fn set_font_size(&mut self, size: Value) -> Result<()> {
        self.set_size(Size::from_pts(size.to_float()? as f32));
        Ok(())
    }
}

impl Labeled for Label {
    fn set_text<S: AsRef<str> + Into<String>>(&mut self, content: S) {
        self.line.select_all();
        self.line.insert(content.as_ref());
    }

    fn set_size(&mut self, size: Size) {
        self.line.set_default_size(size);
        self.line.select_all();
        self.line.change_size(size);
        self.line.select_none();
    }

    fn set_color(&mut self, color: Color) {
        self.line.set_default_color(color);
        // Note: this is a label; we don't allow selection, so no need to save and restore it.
        self.line.select_all();
        self.line.change_color(color);
        self.line.select_none();
    }

    fn set_font(&mut self, font_id: FontId) {
        self.line.set_default_font(font_id);
        // Note: this is a label; we don't allow selection, so no need to save and restore it.
        self.line.select_all();
        self.line.change_font(font_id);
        self.line.select_none();
    }
}

impl Widget for Label {
    fn measure(&self, win: &Window, font_context: &FontContext) -> Result<Extent<Size>> {
        // FIXME: why were we caching metrics?
        let metrics = self.line.measure(win, font_context)?.to_owned();
        Ok(Extent::<Size>::new(
            metrics.width.into(),
            (metrics.height - metrics.descent).into(),
        ))
    }

    // fn layout(
    //     &mut self,
    //     _now: Instant,
    //     region: Region<RelSize>,
    //     win: &Window,
    //     _font_context: &mut FontContext,
    // ) -> Result<()> {
    //     let mut position = region.position().as_abs(win);
    //     *position.bottom_mut() = position.bottom() - self.metrics.descent;
    //     self.allocated_position = position.into();
    //     self.allocated_extent = *region.extent();
    //     Ok(())
    // }

    fn upload(
        &self,
        _now: Instant,
        win: &Window,
        gpu: &Gpu,
        context: &mut PaintContext,
    ) -> Result<()> {
        let widget_info_index = context.push_widget(&WidgetInfo::default());

        self.line.upload(
            self.allocated_position,
            widget_info_index,
            win,
            gpu,
            context,
        )?;

        Ok(())
    }
}
