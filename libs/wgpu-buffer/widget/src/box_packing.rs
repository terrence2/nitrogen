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
use crate::widgets::Widget;
use parking_lot::RwLock;
use std::sync::Arc;

#[derive(Copy, Clone, Debug)]
pub enum PositionH {
    Start,
    Center,
    End,
}

#[derive(Copy, Clone, Debug)]
pub enum PositionV {
    Top,
    Center,
    Bottom,
    Baseline, // widget defined visual bottom
}

// Determine how the given widget should be packed into its box.
pub struct BoxPacking {
    widget: Arc<RwLock<dyn Widget>>,
    offset: usize,
    // position_v: PositionV,
    // position_h: PositionH,
}

impl BoxPacking {
    pub fn new(widget: Arc<RwLock<dyn Widget>>, offset: usize) -> Self {
        Self {
            widget,
            offset,
            // position_v: PositionV::Top,
            // position_h: PositionH::Start,
        }
    }

    pub fn widget(&self) -> Arc<RwLock<dyn Widget>> {
        self.widget.clone()
    }

    pub fn offset(&self) -> usize {
        self.offset
    }
}
