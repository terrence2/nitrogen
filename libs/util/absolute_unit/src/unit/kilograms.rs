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
use crate::{MassUnit, Unit};

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Kilograms;
impl Unit for Kilograms {
    const UNIT_NAME: &'static str = "kilograms";
    const UNIT_SHORT_NAME: &'static str = "kg";
    const UNIT_SUFFIX: &'static str = "kg";
}
impl MassUnit for Kilograms {
    const GRAMS_IN_UNIT: f64 = 1_000.0;
}

#[macro_export]
macro_rules! kilograms {
    ($num:expr) => {
        $crate::Mass::<$crate::Kilograms>::from(&$num)
    };
}

#[macro_export]
macro_rules! kilograms_per_meter3 {
    ($num:expr) => {
        $crate::Density::<$crate::Kilograms, $crate::Meters>::from(&$num)
    };
}

#[macro_export]
macro_rules! kilograms_meter2 {
    ($num:expr) => {
        $crate::RotationalInertia::<$crate::Kilograms, $crate::Meters>::from(&$num)
    };
}

#[macro_export]
macro_rules! kilograms_per_second {
    ($num:expr) => {
        $crate::MassRate::<$crate::Kilograms, $crate::Seconds>::from(&$num)
    };
}
