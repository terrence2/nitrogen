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
#[derive(AsBytes, FromBytes, Copy, Clone, Default, Debug)]
pub struct TileInfo {
    tile_base_as: [f32; 2],
    tile_angular_extent_as: f32,
    atlas_slot: f32,
}

impl TileInfo {
    pub fn new(tile_base_as: [f32; 2], tile_angular_extent_as: f32, slot: usize) -> Self {
        Self {
            tile_base_as,
            tile_angular_extent_as,
            atlas_slot: slot as f32,
        }
    }
}
