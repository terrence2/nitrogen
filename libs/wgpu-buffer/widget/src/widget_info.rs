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
use zerocopy::{AsBytes, FromBytes};

/// Stored on the GPU, one per widget. Widget vertices reference one of these slots so that
/// pipelines can get at the data they need.
#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Debug, Default)]
pub struct WidgetInfo {
    pub position: [f32; 4],
    flags: [u32; 4],
}

const GLASS_BACKGROUND: u32 = 0x0000_0001;
const PRE_BLEND_TEXT: u32 = 0x0000_0002;

impl WidgetInfo {
    pub fn set_glass_background(&mut self, status: bool) {
        if status {
            self.flags[0] |= GLASS_BACKGROUND;
        } else {
            self.flags[0] &= !GLASS_BACKGROUND;
        }
    }

    pub fn set_pre_blend_text(&mut self, status: bool) {
        if status {
            self.flags[0] |= PRE_BLEND_TEXT;
        } else {
            self.flags[0] &= !PRE_BLEND_TEXT;
        }
    }
}
