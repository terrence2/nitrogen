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
use ordered_float::OrderedFloat;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, Neg, Sub, SubAssign};

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Scalar(pub(crate) OrderedFloat<f64>);

impl Scalar {
    pub(crate) fn f64(self) -> f64 {
        self.into_inner()
    }

    pub fn into_inner(self) -> f64 {
        self.0.into_inner()
    }
}

impl Neg for Scalar {
    type Output = Self;

    fn neg(mut self) -> Self::Output {
        Self(OrderedFloat(-self.into_inner()))
    }
}

impl Mul<Scalar> for Scalar {
    type Output = Scalar;

    fn mul(self, rhs: Scalar) -> Self::Output {
        Self(OrderedFloat(self.into_inner() * rhs.into_inner()))
    }
}

impl From<f64> for Scalar {
    fn from(v: f64) -> Self {
        Scalar(OrderedFloat(v))
    }
}

#[macro_export]
macro_rules! scalar {
    ($num:expr) => {
        $crate::Scalar::from($num as f64)
    };
}
