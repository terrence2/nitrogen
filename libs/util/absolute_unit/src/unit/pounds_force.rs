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
use crate::{Feet, ForceUnit, PoundsMass, Seconds, Unit};

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct PoundsForce;
impl Unit for PoundsForce {
    const UNIT_NAME: &'static str = "pounds(force)";
    const UNIT_SHORT_NAME: &'static str = "lbf";
    const UNIT_SUFFIX: &'static str = "lbf";
}
impl ForceUnit for PoundsForce {
    const NEWTONS_IN_UNIT: f64 = 1. / 0.224_809;

    type UnitMass = PoundsMass;
    type UnitLength = Feet;
    type UnitTime = Seconds;
}

#[macro_export]
macro_rules! pounds_force {
    ($num:expr) => {
        $crate::Force::<$crate::PoundsForce>::from(&$num)
    };
}

#[macro_export]
macro_rules! pdl {
    ($num:expr) => {
        $crate::Force::<$crate::PoundsForce>::from(&$num)
    };
}
