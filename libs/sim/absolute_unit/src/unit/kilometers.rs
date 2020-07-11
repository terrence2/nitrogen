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
use crate::{length::LengthUnit, unit::meters::Meters};

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub struct Kilometers;
impl LengthUnit for Kilometers {
    fn unit_name() -> &'static str {
        "kilometers"
    }
    fn suffix() -> &'static str {
        "km"
    }
    fn nanometers_in_unit() -> i64 {
        Meters::nanometers_in_unit() * 1_000
    }
}

#[macro_export]
macro_rules! kilometers {
    ($num:expr) => {
        $crate::Length::<$crate::Kilometers>::from(&$num)
    };
}
