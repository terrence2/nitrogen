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
pub struct Hours;
impl TimeUnit for Hours {
    const UNIT_NAME: &'static str = "hours";
    const UNIT_SHORT_NAME: &'static str = "h";
    const SECONDS_IN_UNIT: f64 = 3_600.;
}

#[macro_export]
macro_rules! hours {
    ($num:expr) => {
        $crate::Time::<$crate::Hours>::from(&$num)
    };
}
