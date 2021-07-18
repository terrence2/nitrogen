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
    paint_context::PaintContext,
    widget::{Padding, Size, UploadMetrics, Widget},
    widget_info::WidgetInfo,
    widgets::label::Label,
};
use anyhow::Result;
use gpu::Gpu;
use input::GenericEvent;
use nitrous::Interpreter;
use parking_lot::RwLock;
use std::sync::Arc;

#[derive(Debug)]
pub struct Button {
    label: Arc<RwLock<Label>>,
    padding: Padding,
}

impl Button {
    pub fn new_with_text<S: AsRef<str> + Into<String>>(s: S) -> Self {
        Button {
            label: Label::new(s).wrapped(),
            padding: Padding::new_uniform(Size::Px(3.)),
        }
    }

    pub fn wrapped(self) -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(self))
    }
}

impl Widget for Button {
    fn upload(&self, gpu: &Gpu, context: &mut PaintContext) -> Result<UploadMetrics> {
        let info = WidgetInfo::default();
        let widget_info_index = context.push_widget(&info);

        let mut label_metrics = self.label.read().upload(gpu, context)?;
        label_metrics.adjust_height(self.padding.top(), gpu, context);

        let mut widget_info_indexes = vec![widget_info_index];
        widget_info_indexes.append(&mut label_metrics.widget_info_indexes);

        Ok(UploadMetrics {
            widget_info_indexes,
            width: self.padding.left_gpu(gpu) + label_metrics.width + self.padding.right_gpu(gpu),
            height: self.padding.top_gpu(gpu) + label_metrics.height + self.padding.bottom_gpu(gpu),
        })
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
