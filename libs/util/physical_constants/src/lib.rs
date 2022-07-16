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
pub static EVEREST_HEIGHT: Lazy<Length<Meters>> = Lazy::new(|| meters!(8_848.039_2));
