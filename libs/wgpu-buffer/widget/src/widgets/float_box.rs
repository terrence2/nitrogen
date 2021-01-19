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
use failure::Fallible;
use gpu::GPU;
use parking_lot::RwLock;
use std::sync::Arc;
use winit::event::{KeyboardInput, ModifiersState};

// Pack boxes at an edge.
pub struct FloatPacking {
    widget: Arc<RwLock<dyn Widget>>,
    offset: usize,
    float_h: PositionH,
    float_v: PositionV,
}

impl FloatPacking {
    pub fn new(widget: Arc<RwLock<dyn Widget>>, offset: usize) -> Self {
        Self {
            widget,
            offset,
            float_h: PositionH::Start,
            float_v: PositionV::Top,
        }
    }

    pub fn widget(&self) -> Arc<RwLock<dyn Widget>> {
        self.widget.clone()
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn set_float(&mut self, float_h: PositionH, float_v: PositionV) {
        self.float_h = float_h;
        self.float_v = float_v;
    }
}

// Items packed from top to bottom.
pub struct FloatBox {
    children: Vec<FloatPacking>,
}

impl FloatBox {
    pub fn new() -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(Self {
            children: Vec::new(),
        }))
    }

    pub fn add_child(&mut self, child: Arc<RwLock<dyn Widget>>) -> &mut FloatPacking {
        let offset = self.children.len();
        self.children.push(FloatPacking::new(child, offset));
        self.packing_mut(offset)
    }

    pub fn packing(&self, offset: usize) -> &FloatPacking {
        &self.children[offset]
    }

    pub fn packing_mut(&mut self, offset: usize) -> &mut FloatPacking {
        &mut self.children[offset]
    }
}

impl Widget for FloatBox {
    // Webgpu: (-1, -1) maps to the bottom-left of the screen.
    // FIXME: use widget info for depth instead of vertices; save some upload bandwidth.
    fn upload(&self, gpu: &GPU, context: &mut PaintContext) -> Fallible<UploadMetrics> {
        let mut widget_info_indexes = Vec::with_capacity(self.children.len());
        for pack in &self.children {
            let widget = pack.widget.read();
            let mut child_metrics = widget.upload(gpu, context)?;

            // Apply float to child.
            let x_offset = match pack.float_h {
                PositionH::Start => -1f32,
                PositionH::Center => -child_metrics.width / 2f32,
                PositionH::End => 1f32 - child_metrics.width,
            };
            let y_offset = match pack.float_v {
                PositionV::Top => 1f32 - child_metrics.height,
                PositionV::Center => child_metrics.height / 2.0,
                PositionV::Bottom => -1f32 + child_metrics.baseline_height,
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
            baseline_height: 2f32,
        })
    }

    fn handle_keyboard(&mut self, events: &[(KeyboardInput, ModifiersState)]) -> Fallible<()> {
        for child in &self.children {
            child.widget.write().handle_keyboard(events)?;
        }
        Ok(())
    }
}
