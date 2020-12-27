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
use crate::{box_packing::BoxPacking, paint_context::PaintContext, widget::Widget};
use gpu::GPU;
use parking_lot::RwLock;
use std::sync::Arc;

// Items packed from top to bottom.
pub struct VerticalBox {
    children: Vec<BoxPacking>,
}

impl VerticalBox {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
        }
    }

    pub fn add_child(&mut self, child: Arc<RwLock<dyn Widget>>) -> &mut BoxPacking {
        let offset = self.children.len();
        self.children.push(BoxPacking::new(child, offset));
        self.packing_mut(offset)
    }

    pub fn packing(&self, offset: usize) -> &BoxPacking {
        &self.children[offset]
    }

    pub fn packing_mut(&mut self, offset: usize) -> &mut BoxPacking {
        &mut self.children[offset]
    }
}

impl Widget for VerticalBox {
    fn upload(&self, gpu: &GPU, context: &mut PaintContext) {
        for pack in &self.children {
            pack.widget().read().upload(gpu, context);
        }
    }
}
