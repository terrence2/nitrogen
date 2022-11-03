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
pub struct PoundsMass;
impl Unit for PoundsMass {
    const UNIT_NAME: &'static str = "pounds";
    const UNIT_SHORT_NAME: &'static str = "lb";
    const UNIT_SUFFIX: &'static str = "lb";
}
impl MassUnit for PoundsMass {
    const GRAMS_IN_UNIT: f64 = 453.592_37;
}

#[macro_export]
macro_rules! pounds_mass {
    ($num:expr) => {
        $crate::Mass::<$crate::PoundsMass>::from(&$num)
    };
}

#[macro_export]
macro_rules! pounds_mass_per_second {
    ($num:expr) => {
        $crate::MassRate::<$crate::PoundsMass, $crate::Seconds>::from(&$num)
    };
}

#[macro_export]
macro_rules! pounds_per_feet3 {
    ($num:expr) => {
        $crate::Density::<$crate::PoundsMass, $crate::Feet>::from(&$num)
    };
}
