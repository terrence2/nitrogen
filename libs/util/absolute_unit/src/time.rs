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

pub trait TimeUnit: Copy + Debug + Eq + PartialEq + 'static {
    fn unit_name() -> &'static str;
    fn unit_short_name() -> &'static str;
    fn seconds_in_unit() -> f64;
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Time<Unit: TimeUnit> {
    v: OrderedFloat<f64>, // in Unit
    phantom: PhantomData<Unit>,
}

impl<Unit: TimeUnit> Time<Unit> {
    pub fn f64(self) -> f64 {
        f64::from(self)
    }

    pub fn f32(self) -> f32 {
        f32::from(self)
    }
}

impl<Unit> fmt::Display for Time<Unit>
where
    Unit: TimeUnit,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:0.4}{}", self.v, Unit::unit_short_name())
    }
}

impl<'a, UnitA, UnitB> From<&'a Time<UnitA>> for Time<UnitB>
where
    UnitA: TimeUnit,
    UnitB: TimeUnit,
{
    fn from(v: &'a Time<UnitA>) -> Self {
        Self {
            v: v.v * UnitA::seconds_in_unit() / UnitB::seconds_in_unit(),
            phantom: PhantomData,
        }
    }
}

impl<UnitA, UnitB> Add<Time<UnitA>> for Time<UnitB>
where
    UnitA: TimeUnit,
    UnitB: TimeUnit,
{
    type Output = Time<UnitB>;

    fn add(self, other: Time<UnitA>) -> Self {
        Self {
            v: self.v + Time::<UnitB>::from(&other).v,
            phantom: PhantomData,
        }
    }
}

impl<UnitA, UnitB> AddAssign<Time<UnitA>> for Time<UnitB>
where
    UnitA: TimeUnit,
    UnitB: TimeUnit,
{
    fn add_assign(&mut self, other: Time<UnitA>) {
        self.v += Time::<UnitB>::from(&other).v;
    }
}

impl<UnitA, UnitB> Sub<Time<UnitA>> for Time<UnitB>
where
    UnitA: TimeUnit,
    UnitB: TimeUnit,
{
    type Output = Time<UnitB>;

    fn sub(self, other: Time<UnitA>) -> Self {
        Self {
            v: self.v - Time::<UnitB>::from(&other).v,
            phantom: PhantomData,
        }
    }
}

impl<UnitA, UnitB> SubAssign<Time<UnitA>> for Time<UnitB>
where
    UnitA: TimeUnit,
    UnitB: TimeUnit,
{
    fn sub_assign(&mut self, other: Time<UnitA>) {
        self.v -= Time::<UnitB>::from(&other).v;
    }
}

impl<Unit> Mul<Scalar> for Time<Unit>
where
    Unit: TimeUnit,
{
    type Output = Time<Unit>;

    fn mul(self, s: Scalar) -> Self {
        Self {
            v: self.v * s.f64(),
            phantom: PhantomData,
        }
    }
}

impl<Unit> MulAssign<Scalar> for Time<Unit>
where
    Unit: TimeUnit,
{
    fn mul_assign(&mut self, s: Scalar) {
        self.v *= s.f64();
    }
}

impl<Unit> Div<Scalar> for Time<Unit>
where
    Unit: TimeUnit,
{
    type Output = Time<Unit>;

    fn div(self, s: Scalar) -> Self {
        Self {
            v: self.v / s.f64(),
            phantom: PhantomData,
        }
    }
}

impl<Unit> DivAssign<Scalar> for Time<Unit>
where
    Unit: TimeUnit,
{
    fn div_assign(&mut self, s: Scalar) {
        self.v /= s.f64();
    }
}

impl<Unit: TimeUnit> AbsDiffEq for Time<Unit> {
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
        impl<Unit> From<$Num> for Time<Unit>
        where
            Unit: TimeUnit,
        {
            fn from(v: $Num) -> Self {
                Self {
                    v: OrderedFloat(v as f64),
                    phantom: PhantomData,
                }
            }
        }

        impl<Unit> From<&$Num> for Time<Unit>
        where
            Unit: TimeUnit,
        {
            fn from(v: &$Num) -> Self {
                Self {
                    v: OrderedFloat(*v as f64),
                    phantom: PhantomData,
                }
            }
        }

        impl<Unit> From<Time<Unit>> for $Num
        where
            Unit: TimeUnit,
        {
            fn from(v: Time<Unit>) -> $Num {
                v.v.0 as $Num
            }
        }
    };
}
impl_unit_for_floats!(impl_length_unit_for_numeric_type);
impl_unit_for_integers!(impl_length_unit_for_numeric_type);

#[cfg(test)]
mod test {
    use crate::{hours, seconds};
    use approx::assert_abs_diff_eq;

    #[test]
    fn test_time() {
        let h = hours!(1);
        println!("h: {}", h);
        println!("s: {}", seconds!(h));
        assert_abs_diff_eq!(seconds!(h), seconds!(3_600));
    }
}
