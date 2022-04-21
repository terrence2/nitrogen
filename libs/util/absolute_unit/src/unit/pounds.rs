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
use crate::mass::MassUnit;

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Pounds;
impl MassUnit for Pounds {
    fn unit_name() -> &'static str {
        "pounds"
    }
    fn unit_short_name() -> &'static str {
        "lb"
    }
    fn grams_in_unit() -> f64 {
        453.592_37
    }
}

#[macro_export]
macro_rules! pounds {
    ($num:expr) => {
        $crate::Mass::<$crate::Pounds>::from(&$num)
    };
}
