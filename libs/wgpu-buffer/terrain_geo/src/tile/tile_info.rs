// This file is part of OpenFA.
//
// OpenFA is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// OpenFA is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with OpenFA.  If not, see <http://www.gnu.org/licenses/>.
use zerocopy::{AsBytes, FromBytes};

// We allocate a block of these GPU side to tell us what projection to use for each tile.
#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Default)]
pub struct TileInfo {
    base: [f32; 2],
    angular_extent: f32,
    _pad: u32,
}

impl TileInfo {
    pub fn new(base: [f32; 2], angular_extent: f32) -> Self {
        Self {
            base,
            angular_extent,
            _pad: 0,
        }
    }
}
