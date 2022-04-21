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
use approx::AbsDiffEq;
use ordered_float::OrderedFloat;
use std::{
    fmt,
    fmt::Debug,
    marker::PhantomData,
    ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign},
};

pub trait MassUnit: Copy + Debug + Eq + PartialEq + 'static {
    fn unit_name() -> &'static str;
    fn unit_short_name() -> &'static str;
    fn grams_in_unit() -> f64;
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Mass<Unit: MassUnit> {
    v: OrderedFloat<f64>, // in Unit
    phantom: PhantomData<Unit>,
}

impl<Unit: MassUnit> Mass<Unit> {
    pub fn f64(self) -> f64 {
        f64::from(self)
    }

    pub fn f32(self) -> f32 {
        f32::from(self)
    }
}

impl<Unit> fmt::Display for Mass<Unit>
where
    Unit: MassUnit,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:0.4}{}", self.v, Unit::unit_short_name())
    }
}

impl<'a, UnitA, UnitB> From<&'a Mass<UnitA>> for Mass<UnitB>
where
    UnitA: MassUnit,
    UnitB: MassUnit,
{
    fn from(v: &'a Mass<UnitA>) -> Self {
        Self {
            v: v.v * UnitA::grams_in_unit() / UnitB::grams_in_unit(),
            phantom: PhantomData,
        }
    }
}

impl<UnitA, UnitB> Add<Mass<UnitA>> for Mass<UnitB>
where
    UnitA: MassUnit,
    UnitB: MassUnit,
{
    type Output = Mass<UnitB>;

    fn add(self, other: Mass<UnitA>) -> Self {
        Self {
            v: self.v + Mass::<UnitB>::from(&other).v,
            phantom: PhantomData,
        }
    }
}

impl<UnitA, UnitB> AddAssign<Mass<UnitA>> for Mass<UnitB>
where
    UnitA: MassUnit,
    UnitB: MassUnit,
{
    fn add_assign(&mut self, other: Mass<UnitA>) {
        self.v += Mass::<UnitB>::from(&other).v;
    }
}

impl<UnitA, UnitB> Sub<Mass<UnitA>> for Mass<UnitB>
where
    UnitA: MassUnit,
    UnitB: MassUnit,
{
    type Output = Mass<UnitB>;

    fn sub(self, other: Mass<UnitA>) -> Self {
        Self {
            v: self.v - Mass::<UnitB>::from(&other).v,
            phantom: PhantomData,
        }
    }
}

impl<UnitA, UnitB> SubAssign<Mass<UnitA>> for Mass<UnitB>
where
    UnitA: MassUnit,
    UnitB: MassUnit,
{
    fn sub_assign(&mut self, other: Mass<UnitA>) {
        self.v -= Mass::<UnitB>::from(&other).v;
    }
}

impl<Unit> Mul<Scalar> for Mass<Unit>
where
    Unit: MassUnit,
{
    type Output = Mass<Unit>;

    fn mul(self, s: Scalar) -> Self {
        Self {
            v: self.v * s.f64(),
            phantom: PhantomData,
        }
    }
}

impl<Unit> MulAssign<Scalar> for Mass<Unit>
where
    Unit: MassUnit,
{
    fn mul_assign(&mut self, s: Scalar) {
        self.v *= s.f64();
    }
}

impl<Unit> Div<Scalar> for Mass<Unit>
where
    Unit: MassUnit,
{
    type Output = Mass<Unit>;

    fn div(self, s: Scalar) -> Self {
        Self {
            v: self.v / s.f64(),
            phantom: PhantomData,
        }
    }
}

impl<Unit> DivAssign<Scalar> for Mass<Unit>
where
    Unit: MassUnit,
{
    fn div_assign(&mut self, s: Scalar) {
        self.v /= s.f64();
    }
}

impl<Unit: MassUnit> AbsDiffEq for Mass<Unit> {
    type Epsilon = f64;

    fn default_epsilon() -> Self::Epsilon {
        f64::default_epsilon()
    }

    fn abs_diff_eq(&self, other: &Self, epsilon: Self::Epsilon) -> bool {
        self.v.0.abs_diff_eq(&other.v.0, epsilon)
    }
}

macro_rules! impl_length_unit_for_numeric_type {
    ($Num:ty) => {
        impl<Unit> From<$Num> for Mass<Unit>
        where
            Unit: MassUnit,
        {
            fn from(v: $Num) -> Self {
                Self {
                    v: OrderedFloat(v as f64),
                    phantom: PhantomData,
                }
            }
        }

        impl<Unit> From<&$Num> for Mass<Unit>
        where
            Unit: MassUnit,
        {
            fn from(v: &$Num) -> Self {
                Self {
                    v: OrderedFloat(*v as f64),
                    phantom: PhantomData,
                }
            }
        }

        impl<Unit> From<Mass<Unit>> for $Num
        where
            Unit: MassUnit,
        {
            fn from(v: Mass<Unit>) -> $Num {
                v.v.0 as $Num
            }
        }
    };
}
impl_unit_for_floats!(impl_length_unit_for_numeric_type);
impl_unit_for_integers!(impl_length_unit_for_numeric_type);

#[cfg(test)]
mod test {
    use crate::{kilograms, pounds};
    use approx::assert_abs_diff_eq;

    #[test]
    fn test_mass() {
        let lb = pounds!(2);
        println!("lb: {}", lb);
        println!("kg: {}", kilograms!(lb));
        assert_abs_diff_eq!(kilograms!(lb), kilograms!(0.907_184_74));
    }
}
