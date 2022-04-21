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
use crate::time::TimeUnit;

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Seconds;
impl TimeUnit for Seconds {
    fn unit_name() -> &'static str {
        "seconds"
    }
    fn unit_short_name() -> &'static str {
        "s"
    }
    fn seconds_in_unit() -> f64 {
        1.
    }
}

#[macro_export]
macro_rules! seconds {
    ($num:expr) => {
        $crate::Time::<$crate::Seconds>::from(&$num)
    };
}
