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
use crate::{PressureUnit, Unit};

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct PoundsSquareFoot;
impl Unit for PoundsSquareFoot {
    const UNIT_NAME: &'static str = "pounds per square foot";
    const UNIT_SHORT_NAME: &'static str = "lb/ft^2";
    const UNIT_SUFFIX: &'static str = "lb/ft^2";
}
impl PressureUnit for PoundsSquareFoot {
    const PASCALS_IN_UNIT: f64 = 47.880;
}

#[macro_export]
macro_rules! pounds_square_foot {
    ($num:expr) => {
        $crate::Pressure::<$crate::PoundsSquareFoot>::from(&$num)
    };
}

#[macro_export]
macro_rules! psf {
    ($num:expr) => {
        $crate::Pressure::<$crate::PoundsSquareFoot>::from(&$num)
    };
}
