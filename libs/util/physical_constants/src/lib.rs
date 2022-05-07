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
mod atmosphere;

pub use atmosphere::StandardAtmosphere;

use absolute_unit::{meters, meters_per_second2, Acceleration, Length, Meters, Seconds};
use once_cell::sync::Lazy;

pub static STANDARD_GRAVITY: Lazy<Acceleration<Meters, Seconds>> =
    Lazy::new(|| meters_per_second2!(9.80665));

pub static EARTH_RADIUS: Lazy<Length<Meters>> = Lazy::new(|| meters!(6_356_766));

pub const FEET_TO_M: f32 = 0.304_8;
pub const FEET_TO_DAM: f32 = 0.030_48;
pub const FEET_TO_M_32: f32 = 0.304_800;
pub const FEET_TO_M_64: f64 = 0.304_800;
pub const FEET_TO_KM: f32 = 0.000_304_8;
pub const METERS_TO_FEET_32: f32 = 1f32 / FEET_TO_M_32;
pub const METERS_TO_FEET_64: f64 = 1f64 / FEET_TO_M_64;

pub const EARTH_RADIUS_KM: f64 = 6360.0;
pub const EARTH_RADIUS_KM_32: f32 = EARTH_RADIUS_KM as f32;
pub const EVEREST_HEIGHT_KM: f64 = 8.848_039_2;

pub const GRAVITY_M_S2_32: f32 = 9.80665;
pub const GRAVITY_M_S2_64: f64 = 9.80665;
