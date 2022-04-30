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
pub struct Slugs;
impl Unit for Slugs {
    const UNIT_NAME: &'static str = "slugs";
    const UNIT_SHORT_NAME: &'static str = "slug";
    const UNIT_SUFFIX: &'static str = "slug";
}
impl MassUnit for Slugs {
    const GRAMS_IN_UNIT: f64 = 14_593.90;
}

#[macro_export]
macro_rules! slugs {
    ($num:expr) => {
        $crate::Mass::<$crate::Slugs>::from(&$num)
    };
}

#[macro_export]
macro_rules! slugs_per_foot3 {
    ($num:expr) => {
        $crate::Density::<$crate::Slugs, $crate::Feet>::from(&$num)
    };
}
