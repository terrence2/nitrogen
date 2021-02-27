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
    box_packing::{PositionH, PositionV},
    paint_context::PaintContext,
    widget::{UploadMetrics, Widget},
};
use failure::{err_msg, Fallible};
use gpu::GPU;
use input::GenericEvent;
use nitrous::Interpreter;
use parking_lot::RwLock;
use std::{collections::HashMap, sync::Arc};

// Pack boxes at an edge.
#[derive(Debug)]
pub struct FloatPacking {
    name: String,
    widget: Arc<RwLock<dyn Widget>>,
    float_h: PositionH,
    float_v: PositionV,
}

impl FloatPacking {
    pub fn new(name: &str, widget: Arc<RwLock<dyn Widget>>) -> Self {
        Self {
            name: name.to_owned(),
            widget,
            float_h: PositionH::Start,
            float_v: PositionV::Top,
        }
    }

    pub fn widget(&self) -> Arc<RwLock<dyn Widget>> {
        self.widget.clone()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn set_float(&mut self, float_h: PositionH, float_v: PositionV) {
        self.float_h = float_h;
        self.float_v = float_v;
    }
}

// Items packed from top to bottom.
#[derive(Debug)]
pub struct FloatBox {
    children: HashMap<String, FloatPacking>,
}

impl FloatBox {
    pub fn new() -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(Self {
            children: HashMap::new(),
        }))
    }

    pub fn add_child(&mut self, name: &str, child: Arc<RwLock<dyn Widget>>) -> &mut FloatPacking {
        self.children
            .insert(name.to_owned(), FloatPacking::new(name, child));
        self.packing_mut(name).unwrap()
    }

    pub fn packing(&self, name: &str) -> &FloatPacking {
        &self.children[name]
    }

    pub fn packing_mut(&mut self, name: &str) -> Fallible<&mut FloatPacking> {
        self.children
            .get_mut(name)
            .ok_or(err_msg("unknown widget in float"))
    }
}

impl Widget for FloatBox {
    // Webgpu: (-1, -1) maps to the bottom-left of the screen.
    // Widget: (0, 0) maps to the top-left of the widget.
    fn upload(&self, gpu: &GPU, context: &mut PaintContext) -> Fallible<UploadMetrics> {
        let mut widget_info_indexes = Vec::with_capacity(self.children.len());
        for pack in self.children.values() {
            let widget = pack.widget.read();
            let mut child_metrics = widget.upload(gpu, context)?;

            // Apply float to child.
            let x_offset = match pack.float_h {
                PositionH::Start => -1f32,
                PositionH::Center => -child_metrics.width / 2f32,
                PositionH::End => 1f32 - child_metrics.width,
            };
            let y_offset = match pack.float_v {
                PositionV::Top => 1f32,
                PositionV::Center => child_metrics.height / 2.0,
                PositionV::Bottom => -1f32 + child_metrics.height,
            };
            for &widget_info_index in &child_metrics.widget_info_indexes {
                context.widget_info_pool[widget_info_index as usize].position[0] += x_offset;
                context.widget_info_pool[widget_info_index as usize].position[1] += y_offset;
            }

            widget_info_indexes.append(&mut child_metrics.widget_info_indexes);
        }
        Ok(UploadMetrics {
            widget_info_indexes,
            width: 2f32,
            height: 2f32,
        })
    }

    fn handle_events(
        &mut self,
        events: &[GenericEvent],
        interpreter: Arc<RwLock<Interpreter>>,
    ) -> Fallible<()> {
        for child in self.children.values() {
            child
                .widget
                .write()
                .handle_events(events, interpreter.clone())?;
        }
        Ok(())
    }
}
