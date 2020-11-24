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
pub use tile::{NeighborIndex, Tile as MipTile};

use absolute_unit::{Angle, ArcSeconds};
use geodesy::{GeoCenter, GeoSurface, Graticule};
use image::Rgb;
use std::ops::Range;
use terrain_geo::tile::TerrainLevel;

#[derive(Copy, Clone, Debug)]
pub struct Region {
    pub base: Graticule<GeoCenter>,
    pub extent: Angle<ArcSeconds>,
}

pub trait DataSource: Send + Sync {
    // Return true if the dataset has interesting data in the given region. Mip tiles fill a grid
    // substantially larger than the typical planet, so it is expected that a significant fraction
    // will be outside the coordinate system. We still give datasets the option in case they need
    // to handle data seams in some way.
    fn contains_region(&self, region: &Region) -> bool;

    // The deepest level for which this dataset has useful samples.
    fn root_level(&self) -> TerrainLevel;

    // Number of mip tiles we expect to intersect with this dataset. We use this as an optimization,
    // since it is predictable and we are limited in what we can track without incurring costs.
    fn expect_intersecting_tiles(&self, layer: usize) -> usize;

    // Range of mip tiles that we expect to be present in this dataset after filtering out
    // empty tiles.
    fn expect_present_tiles(&self, layer: usize) -> Range<usize>;

    // Perform the relevant sampling operation. Datasets not supporting a particular operation
    // are expected to panic! if an inappropriate method is called on that dataset.
    fn sample_nearest_height(&self, grat: &Graticule<GeoSurface>) -> i16;
    fn compute_local_normal(&self, grat: &Graticule<GeoSurface>) -> [i16; 2];
    fn sample_color(&self, grat: &Graticule<GeoSurface>) -> Rgb<u8>;
}
