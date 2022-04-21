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
use crate::{impl_unit_for_floats, impl_unit_for_integers, Scalar};
use ordered_float::OrderedFloat;
use std::{
    fmt,
    marker::PhantomData,
    ops::{Div, DivAssign, Mul, MulAssign},
};

pub trait TemperatureUnit: Copy {
    fn unit_name() -> &'static str;
    fn suffix() -> &'static str;
    fn convert_to_kelvin(degrees_in: f64) -> f64;
    fn convert_from_kelvin(degrees_k: f64) -> f64;
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Temperature<Unit: TemperatureUnit> {
    kelvin: OrderedFloat<f64>, // in kelvin
    phantom: PhantomData<Unit>,
}

impl<Unit: TemperatureUnit> Temperature<Unit> {
    pub fn f64(self) -> f64 {
        f64::from(self)
    }

    pub fn f32(self) -> f32 {
        f32::from(self)
    }
}

impl<Unit> fmt::Display for Temperature<Unit>
where
    Unit: TemperatureUnit,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{:0.4}{}",
            Unit::convert_from_kelvin(self.kelvin.0),
            Unit::suffix()
        )
    }
}

impl<'a, UnitA, UnitB> From<&'a Temperature<UnitA>> for Temperature<UnitB>
where
    UnitA: TemperatureUnit,
    UnitB: TemperatureUnit,
{
    fn from(v: &'a Temperature<UnitA>) -> Self {
        Self {
            kelvin: v.kelvin,
            phantom: PhantomData,
        }
    }
}

/// Only makes sense on rankine and kelvin. It will still "work" in that it will scale by the
/// absolute temperature concept, since we work in kelvin, but the numbers will not make a huge
/// amount of sense for "multiply" in context if used with a non-origin unit system like C or F.
impl<Unit> Mul<Scalar> for Temperature<Unit>
where
    Unit: TemperatureUnit,
{
    type Output = Temperature<Unit>;

    fn mul(self, other: Scalar) -> Self {
        Self {
            kelvin: self.kelvin * other.f64(),
            phantom: PhantomData,
        }
    }
}

impl<Unit> MulAssign<Scalar> for Temperature<Unit>
where
    Unit: TemperatureUnit,
{
    fn mul_assign(&mut self, other: Scalar) {
        self.kelvin *= other.f64();
    }
}

impl<Unit> Div<Scalar> for Temperature<Unit>
where
    Unit: TemperatureUnit,
{
    type Output = Temperature<Unit>;

    fn div(self, other: Scalar) -> Self {
        Self {
            kelvin: self.kelvin / other.f64(),
            phantom: PhantomData,
        }
    }
}

impl<Unit> DivAssign<Scalar> for Temperature<Unit>
where
    Unit: TemperatureUnit,
{
    fn div_assign(&mut self, other: Scalar) {
        self.kelvin /= other.f64();
    }
}

macro_rules! impl_length_unit_for_numeric_type {
    ($Num:ty) => {
        impl<Unit> From<$Num> for Temperature<Unit>
        where
            Unit: TemperatureUnit,
        {
            fn from(v: $Num) -> Self {
                Self {
                    kelvin: OrderedFloat(Unit::convert_to_kelvin(v as f64)),
                    phantom: PhantomData,
                }
            }
        }

        impl<Unit> From<&$Num> for Temperature<Unit>
        where
            Unit: TemperatureUnit,
        {
            fn from(v: &$Num) -> Self {
                Self {
                    kelvin: OrderedFloat(Unit::convert_to_kelvin(*v as f64)),
                    phantom: PhantomData,
                }
            }
        }

        impl<Unit> From<Temperature<Unit>> for $Num
        where
            Unit: TemperatureUnit,
        {
            fn from(v: Temperature<Unit>) -> $Num {
                Unit::convert_from_kelvin(v.kelvin.0) as $Num
            }
        }
    };
}
impl_unit_for_floats!(impl_length_unit_for_numeric_type);
impl_unit_for_integers!(impl_length_unit_for_numeric_type);

#[cfg(test)]
mod test {
    use crate::{celsius, fahrenheit, kelvin, rankine};

    #[test]
    fn test_meters_to_feet() {
        let f = fahrenheit!(100);
        println!("f: {}", f);
        println!("c: {}", celsius!(f));
        println!("r: {}", rankine!(f));
        println!("k: {}", kelvin!(f));
    }
}
