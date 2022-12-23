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
use crate::DynamicUnits;
use ordered_float::OrderedFloat;
use std::fmt::Formatter;
use std::{
    fmt::Display,
    ops::{Div, Mul, Neg},
};

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Scalar(pub(crate) OrderedFloat<f64>);

impl Scalar {
    pub fn f64(self) -> f64 {
        self.into_inner()
    }

    pub fn ln(self) -> Scalar {
        Scalar(OrderedFloat(self.0.ln()))
    }

    pub fn abs(self) -> Scalar {
        Scalar(OrderedFloat(self.0.abs()))
    }

    pub fn into_inner(self) -> f64 {
        self.0.into_inner()
    }

    pub fn as_dyn(&self) -> DynamicUnits {
        DynamicUnits::new0o0(self.0)
    }
}

impl From<DynamicUnits> for Scalar {
    fn from(v: DynamicUnits) -> Self {
        let f = v.ordered_float();
        v.assert_units_empty();
        Self(f)
    }
}

impl Display for Scalar {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Neg for Scalar {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self(OrderedFloat(-self.into_inner()))
    }
}

impl Mul<Scalar> for Scalar {
    type Output = Scalar;

    fn mul(self, rhs: Scalar) -> Self::Output {
        Self(OrderedFloat(self.into_inner() * rhs.into_inner()))
    }
}

impl Div<Scalar> for Scalar {
    type Output = Scalar;

    fn div(self, rhs: Scalar) -> Self::Output {
        Self(OrderedFloat(self.into_inner() / rhs.into_inner()))
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
