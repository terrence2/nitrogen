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
mod manager;
mod quad_tree;

pub(crate) use manager::TileManager;

use absolute_unit::{arcseconds, meters, Angle, ArcSeconds};
use failure::{bail, Fallible};
use geodesy::{GeoCenter, Graticule};

// The physical number of pixels in the tile.
pub const TILE_PHYSICAL_SIZE: usize = 512;

// Number of samples that the tile is wide and tall. This leaves a one pixel strip at each side
// for linear filtering to pull from when we use this source as a texture.
pub const TILE_SAMPLES: i64 = 510;

// Width and height of the tile coverage. Multiply with the tile scale to get width or height
// in arcseconds.
pub const TILE_EXTENT: i64 = TILE_SAMPLES - 1;

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum DataSetCoordinates {
    Spherical,
    CartesianPolar,
}

impl DataSetCoordinates {
    pub fn name(&self) -> String {
        match self {
            Self::Spherical => "spherical",
            Self::CartesianPolar => "cartesian_polar",
        }
        .to_owned()
    }

    pub fn from_name(name: &str) -> Fallible<Self> {
        Ok(match name {
            "spherical" => Self::Spherical,
            "cartesian_polar" => Self::CartesianPolar,
            _ => bail!("not a valid data set kind"),
        })
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum DataSetDataKind {
    Color,
    Normal,
    Height,
}

impl DataSetDataKind {
    pub fn name(&self) -> String {
        match self {
            Self::Color => "color",
            Self::Normal => "normal",
            Self::Height => "height",
        }
        .to_owned()
    }

    pub fn from_name(name: &str) -> Fallible<Self> {
        Ok(match name {
            "color" => Self::Color,
            "normal" => Self::Normal,
            "height" => Self::Height,
            _ => bail!("not a valid data set kind"),
        })
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct TerrainLevel(usize);

impl TerrainLevel {
    // The arcsecond sample resolution required to cover a full earth at TILE_EXTENT samples.
    //
    // At a tile resolution of arcseconds, we need:
    // 360 * 60 * 60 / 509 => 2546.1689587426326
    //
    // tiles to cover the full width of the globe. Alternatively, we can view this as needing a
    // scale of that much per tile to cover the globe with one tile.
    //
    // Thus, we standardize on a baseline of 2560 tiles and accept a few tiles of wrapping on each
    // side. We try to be symmetrical, tile wise across 0x0, which means that the wrapping happens
    // halfway through our arcsecond resolution tiles.
    //
    // The underlying type for Angle is femtoradians, so we can represent lengths down to
    // micrometer scale, even at earth radius. It also means we likely need to round a bit when
    // reading out at arcsecond resolution.
    pub fn base_scale() -> Angle<ArcSeconds> {
        arcseconds!(2560)
    }

    pub fn base_angular_extent() -> Angle<ArcSeconds> {
        Self::base_scale() * TILE_EXTENT
    }

    pub fn base() -> Graticule<GeoCenter> {
        Graticule::<GeoCenter>::new(
            arcseconds!(-TILE_EXTENT as f64 * (Self::base_scale().f64() / 2.0)),
            arcseconds!(-TILE_EXTENT as f64 * (Self::base_scale().f64() / 2.0)),
            meters!(0),
        )
    }

    pub fn as_scale(&self) -> Angle<ArcSeconds> {
        let mut s = Self::base_scale();
        for _ in 0..self.0 {
            s /= 2.0;
        }
        s
    }

    pub fn new(level: usize) -> Self {
        Self(level)
    }

    pub fn offset(&self) -> usize {
        self.0
    }
}

pub enum ChildIndex {
    SouthWest,
    SouthEast,
    NorthWest,
    NorthEast,
}

impl ChildIndex {
    pub fn to_index(&self) -> usize {
        match self {
            Self::SouthWest => 0, // 00
            Self::SouthEast => 1, // 01
            Self::NorthWest => 2, // 10
            Self::NorthEast => 3, // 11
        }
    }
}
