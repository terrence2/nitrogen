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
use crate::color::Color;
use zerocopy::{AsBytes, FromBytes};

/// Stored on the GPU, one per widget. Widget vertices reference one of these slots so that
/// pipelines can get at the data they need.
#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Debug, Default)]
pub struct WidgetInfo {
    foreground_color: [f32; 4],
    background_color: [f32; 4],
    border_color: [f32; 4],
    pub position: [f32; 4],
}

impl WidgetInfo {
    pub fn background_color(&self) -> &[f32; 4] {
        &self.background_color
    }

    pub fn with_foreground_color(mut self, color: Color) -> Self {
        self.foreground_color = color.to_f32_array();
        self
    }

    pub fn with_background_color(mut self, color: Color) -> Self {
        self.background_color = color.to_f32_array();
        self
    }
}
