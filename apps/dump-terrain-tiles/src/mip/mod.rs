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
mod index;
mod tile;

pub use index::{Index as MipIndex, IndexDataSet as MipIndexDataSet};
pub use tile::Tile as MipTile;

use absolute_unit::{Angle, ArcSeconds};
use geodesy::{GeoCenter, Graticule};

#[derive(Copy, Clone, Debug)]
pub struct Region {
    pub base: Graticule<GeoCenter>,
    pub extent: Angle<ArcSeconds>,
}
