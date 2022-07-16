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
pub struct Pascals;
impl Unit for Pascals {
    const UNIT_NAME: &'static str = "pascals";
    const UNIT_SHORT_NAME: &'static str = "Pa";
    const UNIT_SUFFIX: &'static str = "Pa";
}
impl PressureUnit for Pascals {
    const PASCALS_IN_UNIT: f64 = 1.0;
}

#[macro_export]
macro_rules! pascals {
    ($num:expr) => {
        $crate::Pressure::<$crate::Pascals>::from(&$num)
    };
}