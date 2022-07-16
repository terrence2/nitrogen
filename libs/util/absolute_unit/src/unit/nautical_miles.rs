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
use crate::{LengthUnit, Unit};

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct NauticalMiles;
impl Unit for NauticalMiles {
    const UNIT_NAME: &'static str = "nautical miles";
    const UNIT_SHORT_NAME: &'static str = "nm";
    const UNIT_SUFFIX: &'static str = "nm";
}
impl LengthUnit for NauticalMiles {
    const METERS_IN_UNIT: f64 = 1_852.;
}

#[macro_export]
macro_rules! nautical_miles {
    ($num:expr) => {
        $crate::Length::<$crate::NauticalMiles>::from(&$num)
    };
}

#[macro_export]
macro_rules! nm {
    ($num:expr) => {
        $crate::Length::<$crate::NauticalMiles>::from(&$num)
    };
}

#[macro_export]
macro_rules! nautical_miles_per_hour {
    ($num:expr) => {
        $crate::Velocity::<$crate::NauticalMiles, $crate::Hours>::from(&$num)
    };
}

#[macro_export]
macro_rules! knots {
    ($num:expr) => {
        $crate::Velocity::<$crate::NauticalMiles, $crate::Hours>::from(&$num)
    };
}
