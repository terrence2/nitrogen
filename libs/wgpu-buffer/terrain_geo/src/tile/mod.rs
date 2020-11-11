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
mod index_paint_vertex;
mod layer_pack;
mod quad_tree;
mod tile_info;
mod tile_manager;
mod tile_set;

pub(crate) use layer_pack::LayerPack;
pub use layer_pack::{LayerPackBuilder, LayerPackHeader, LayerPackIndexItem};
pub(crate) use tile_manager::TileManager;

use absolute_unit::{arcseconds, meters, Angle, ArcSeconds};
use failure::{bail, Fallible};
use geodesy::{GeoCenter, Graticule};
use lazy_static::lazy_static;
use std::ops::Range;

// The physical number of pixels in the tile.
pub const TILE_PHYSICAL_SIZE: usize = 512;

// Number of samples that the tile is wide and tall. This leaves a one pixel strip at each side
// for linear filtering to pull from when we use this source as a texture.
pub const TILE_SAMPLES: i64 = 510;

// Width and height of the tile coverage. Multiply with the tile scale to get width or height
// in arcseconds.
pub const TILE_EXTENT: i64 = TILE_SAMPLES - 1;

#[repr(u16)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TileCompression {
    None = 0,
    Bz2 = 1,
}

impl TileCompression {
    pub fn from_u16(i: u16) -> Self {
        match i {
            0 => TileCompression::None,
            1 => TileCompression::Bz2,
            _ => panic!("not a valid tile-compression"),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
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

    /// The raw sample kind stored in the atlas texture.
    pub fn texture_format(&self) -> wgpu::TextureFormat {
        match self {
            Self::Color => wgpu::TextureFormat::Rgba8Unorm,
            Self::Normal => wgpu::TextureFormat::Rg16Sint,
            Self::Height => wgpu::TextureFormat::R16Sint,
        }
    }
}

const ARCSEC_LEVEL: usize = 12;
lazy_static! {
    static ref LEVEL_SCALES: [Angle<ArcSeconds>; ARCSEC_LEVEL + 1] = {
        let mut out = [arcseconds!(0); ARCSEC_LEVEL + 1];
        let mut scale = arcseconds!(1);
        for level in (0..=ARCSEC_LEVEL).rev() {
            out[level] = scale;
            scale = scale * 2.0;
        }
        out
    };
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct TerrainLevel(usize);

impl TerrainLevel {
    // Given a resolution of 1", we require 12 doublings to cover the full planet.
    // This gives us a base scale of 4096" per sample per 509 sample tile to get
    // everything, with a ridiculous overlap.
    pub fn base_scale() -> Angle<ArcSeconds> {
        LEVEL_SCALES[0]
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

    pub fn arcsecond_level() -> usize {
        ARCSEC_LEVEL
    }

    // We need to cover 360 degrees worth of tiles longitude and 180 degrees of latitude.
    //   >>> (360 * 60 * 60) / 509
    //   2546.1689587426326
    // Response is in lat/lon, which maps to y, x.
    pub fn index_resolution() -> (u32, u32) {
        (1280, 2560)
    }

    pub fn index_base() -> Graticule<GeoCenter> {
        Graticule::<GeoCenter>::new(
            arcseconds!(-TILE_EXTENT as f64 * (Self::index_resolution().0 as f64 / 2.0)),
            arcseconds!(-TILE_EXTENT as f64 * (Self::index_resolution().1 as f64 / 2.0)),
            meters!(0),
        )
    }

    pub fn index_extent() -> (Angle<ArcSeconds>, Angle<ArcSeconds>) {
        let (pix_lat, pix_lon) = Self::index_resolution();
        (
            arcseconds!(pix_lat as i64 * TILE_EXTENT),
            arcseconds!(pix_lon as i64 * TILE_EXTENT),
        )
    }

    pub fn as_scale(&self) -> Angle<ArcSeconds> {
        LEVEL_SCALES[self.0]
    }

    pub fn angular_extent(&self) -> Angle<ArcSeconds> {
        self.as_scale() * TILE_EXTENT
    }

    pub fn new(level: usize) -> Self {
        Self(level)
    }

    pub fn offset(&self) -> usize {
        self.0
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
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

    pub fn from_index(index: usize) -> Self {
        match index {
            0 => Self::SouthWest, // 00
            1 => Self::SouthEast, // 01
            2 => Self::NorthWest, // 10
            3 => Self::NorthEast, // 11
            _ => panic!("Not a valid index"),
        }
    }

    pub fn all_indices() -> Range<usize> {
        0..4
    }

    pub fn all() -> [ChildIndex; 4] {
        [
            Self::SouthWest,
            Self::SouthEast,
            Self::NorthWest,
            Self::NorthEast,
        ]
    }
}
