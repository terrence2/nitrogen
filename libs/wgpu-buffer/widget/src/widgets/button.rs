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
    font_context::{FontContext, FontId},
    layout::{LayoutMeasurements, LayoutPacking},
    paint_context::PaintContext,
    region::Extent,
    text_run::TextRun,
    widget::Labeled,
    WidgetRenderStep,
};
use anyhow::Result;
use bevy_ecs::prelude::*;
use csscolorparser::Color;
use gpu::Gpu;
use nitrous::{inject_nitrous_component, HeapMut, NitrousComponent};
use runtime::{Extension, Runtime};
use std::{sync::Arc, time::Instant};
use window::{size::Size, Window};

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum ButtonRenderStep {
    Measure,
    Upload,
}

#[derive(Component, NitrousComponent, Debug)]
pub struct Button {
    line: TextRun,
    action: String,
}

impl Extension for Button {
    fn init(runtime: &mut Runtime) -> Result<()> {
        runtime.add_frame_system(
            Button::sys_measure
                .label(ButtonRenderStep::Measure)
                .before(WidgetRenderStep::LayoutWidgets),
        );
        runtime.add_frame_system(Button::sys_upload.label(ButtonRenderStep::Upload));
        Ok(())
    }
}

#[inject_nitrous_component]
impl Button {
    fn sys_measure(
        mut buttons: Query<(&Button, &mut LayoutMeasurements)>,
        win: Res<Window>,
        paint_context: Res<PaintContext>,
    ) {
        for (button, mut measurements) in buttons.iter_mut() {}
    }

    fn sys_upload(labels: Query<(&Button, &mut LayoutMeasurements)>) {}

    pub fn new_with_text<S: AsRef<str> + Into<String>>(s: S) -> Self {
        Button {
            line: TextRun::empty()
                .with_hidden_selection()
                .with_text(s.as_ref()),
            action: String::new(),
        }
    }

    pub fn with_action<S: Into<String>>(mut self, action: S) -> Self {
        self.action = action.into();
        self
    }

    pub fn wrapped(self, name: &str, mut heap: HeapMut) -> Result<Entity> {
        Ok(heap
            .spawn_named(name)?
            .insert_named(self)?
            .insert_named(LayoutPacking::default())?
            .insert(LayoutMeasurements::default())
            .id())
    }
}

impl Labeled for Button {
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

    fn set_color(&mut self, color: &Color) {
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

/*
impl Widget for Button {
    fn measure(&self, win: &Window, font_context: &FontContext) -> Result<Extent<Size>> {
        self.label.measure(win, font_context)
    }

    // fn layout(
    //     &mut self,
    //     now: Instant,
    //     region: Region<RelSize>,
    //     win: &Window,
    //     font_context: &mut FontContext,
    // ) -> Result<()> {
    //     self.label.write().layout(now, region, win, font_context)
    // }

    fn upload(
        &self,
        now: Instant,
        win: &Window,
        gpu: &Gpu,
        context: &mut PaintContext,
    ) -> Result<()> {
        self.label.upload(now, win, gpu, context)
    }
}

#[derive(Component, NitrousComponent)]
pub struct ButtonComponent {
    inner: Arc<Button>,
}

#[inject_nitrous_component]
impl ButtonComponent {
    pub fn new(inner: Arc<Button>) -> Self {
        Self { inner }
    }
}
 */
