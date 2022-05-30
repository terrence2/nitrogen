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
    font_context::FontId,
    layout::{LayoutMeasurements, LayoutPacking},
    paint_context::PaintContext,
    region::Extent,
    text_run::TextRun,
    widget::Labeled,
    widget_info::WidgetInfo,
    WidgetRenderStep,
};
use anyhow::Result;
use bevy_ecs::prelude::*;
use csscolorparser::Color;
use gpu::Gpu;
use nitrous::{inject_nitrous_component, method, HeapMut, NitrousComponent, Value};
use runtime::{report, Extension, Runtime};
use window::{
    size::{RelSize, ScreenDir, Size},
    Window,
};

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum LabelRenderStep {
    Measure,
    Upload,
}

#[derive(Component, NitrousComponent, Debug)]
#[Name = "label"]
pub struct Label {
    line: TextRun,
}

impl Extension for Label {
    fn init(runtime: &mut Runtime) -> Result<()> {
        runtime.add_frame_system(
            Label::sys_measure
                .label(LabelRenderStep::Measure)
                .before(WidgetRenderStep::LayoutWidgets),
        );
        runtime.add_frame_system(
            Label::sys_upload
                .label(LabelRenderStep::Upload)
                .after(WidgetRenderStep::PrepareForFrame)
                .after(WidgetRenderStep::LayoutWidgets)
                .before(WidgetRenderStep::EnsureUploaded),
        );
        Ok(())
    }
}

#[inject_nitrous_component]
impl Label {
    fn sys_measure(
        mut labels: Query<(&Label, &LayoutPacking, &mut LayoutMeasurements)>,
        win: Res<Window>,
        paint_context: Res<PaintContext>,
    ) {
        for (label, packing, mut measure) in labels.iter_mut() {
            let metrics = report!(label.line.measure(&win, &paint_context.font_context));
            let extent = Extent::<RelSize>::new(
                metrics.width.as_rel(&win, ScreenDir::Horizontal),
                metrics.height.as_rel(&win, ScreenDir::Vertical),
            );
            measure.set_child_extent(extent, packing);
            measure.set_metrics(metrics);
        }
    }

    fn sys_upload(
        labels: Query<(&Label, &LayoutMeasurements)>,
        win: Res<Window>,
        gpu: Res<Gpu>,
        mut paint_context: ResMut<PaintContext>,
    ) {
        for (label, measure) in labels.iter() {
            let widget_info_index = paint_context.push_widget(&WidgetInfo::default());

            // Account for descender
            let mut pos = *measure.child_allocation().position();
            *pos.bottom_mut() -= measure.metrics().descent.as_rel(&win, ScreenDir::Vertical);
            report!(label.line.upload(
                pos.into(),
                widget_info_index,
                &win,
                &gpu,
                &mut paint_context,
            ));
        }
    }

    pub fn new<S: AsRef<str> + Into<String>>(content: S) -> Self {
        Self {
            line: TextRun::empty()
                .with_hidden_selection()
                .with_text(content.as_ref()),
        }
    }

    pub fn with_pre_blended_text(mut self) -> Self {
        self.line = self.line.with_pre_blended_text();
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

    #[method]
    fn show(&mut self, content: &str) {
        self.set_text(content);
    }

    #[method]
    fn set_font_by_id(&mut self, font_id: Value) -> Result<()> {
        self.set_font(FontId::from_value(font_id)?);
        Ok(())
    }

    #[method]
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
