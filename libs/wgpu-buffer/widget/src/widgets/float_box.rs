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
    widget::Widget,
};
use gpu::GPU;
use parking_lot::RwLock;
use std::sync::Arc;

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
    fn upload(&self, gpu: &GPU, context: &mut PaintContext) {
        for pack in &self.children {
            pack.widget().read().upload(gpu, context);
        }
    }
}
