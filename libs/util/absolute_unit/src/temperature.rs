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
use crate::{impl_unit_for_floats, impl_unit_for_integers};
use approx::AbsDiffEq;
use std::{
    fmt,
    marker::PhantomData,
    ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign},
};

pub trait TemperatureUnit: Copy {
    fn unit_name() -> &'static str;
    fn suffix() -> &'static str;
    fn nanokelvin_in_unit() -> i64;
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Temperature<Unit: TemperatureUnit> {
    nm: i64, // in nanometers
    phantom: PhantomData<Unit>,
}
