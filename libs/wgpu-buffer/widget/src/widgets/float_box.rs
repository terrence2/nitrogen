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
use crate::widgets::{PaintContext, Widget};
use failure::Fallible;
use parking_lot::RwLock;
use std::sync::Arc;

pub struct FloatPackInfo {
    child: Arc<RwLock<dyn Widget>>,
    position: [f32; 2],
}

pub struct FloatBox {
    children: Vec<FloatPackInfo>,
}

impl FloatBox {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
        }
    }

    pub fn pin_child(&mut self, child: Arc<RwLock<dyn Widget>>, x: f32, y: f32) {
        self.children.push(FloatPackInfo {
            child,
            position: [x, y],
        });
    }
}

impl Widget for FloatBox {
    fn upload(&self, context: &mut PaintContext) {
        for pack in &self.children {
            pack.child.read().upload(context);
        }
    }
    // fn draw<'a>(&self, rpass: wgpu::RenderPass<'a>) -> Fallible<wgpu::RenderPass<'a>> {
    //     let mut rpass = rpass;
    //     for child in &self.children {
    //         rpass = child.draw(rpass)?;
    //     }
    //     Ok(rpass)
    // }
}
