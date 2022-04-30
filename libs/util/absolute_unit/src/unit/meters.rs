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
pub struct Meters;
impl Unit for Meters {
    const UNIT_NAME: &'static str = "meters";
    const UNIT_SHORT_NAME: &'static str = "m";
    const UNIT_SUFFIX: &'static str = "m";
}
impl LengthUnit for Meters {
    const METERS_IN_UNIT: f64 = 1.0;
}

#[macro_export]
macro_rules! meters {
    ($num:expr) => {
        $crate::Length::<$crate::Meters>::from(&$num)
    };
}

#[macro_export]
macro_rules! meters2 {
    ($num:expr) => {
        $crate::Area::<$crate::Meters>::from(&$num)
    };
}

#[macro_export]
macro_rules! meters_per_second {
    ($num:expr) => {
        $crate::Velocity::<$crate::Meters, $crate::Seconds>::from(&$num)
    };
}

#[macro_export]
macro_rules! meters_per_second2 {
    ($num:expr) => {
        $crate::Acceleration::<$crate::Meters, $crate::Seconds>::from(&$num)
    };
}
