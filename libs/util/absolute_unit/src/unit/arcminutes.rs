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
use crate::{AngleUnit, Unit};
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct ArcMinutes;
impl Unit for ArcMinutes {
    const UNIT_NAME: &'static str = "arcminutes";
    const UNIT_SHORT_NAME: &'static str = "arcmin";
    const UNIT_SUFFIX: &'static str = "'";
}
impl AngleUnit for ArcMinutes {
    const RADIANS_IN_UNIT: f64 = PI / 180f64 / 60f64;
}

#[macro_export]
macro_rules! arcminutes {
    ($num:expr) => {
        $crate::Angle::<$crate::ArcMinutes>::from(&$num)
    };
}
