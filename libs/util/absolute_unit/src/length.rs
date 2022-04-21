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
use crate::{impl_unit_for_floats, impl_unit_for_integers, Area, Scalar};
use approx::AbsDiffEq;
use ordered_float::OrderedFloat;
use std::{
    fmt,
    fmt::Debug,
    marker::PhantomData,
    ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign},
};

pub trait LengthUnit: Copy + Debug + Eq + PartialEq + 'static {
    fn unit_name() -> &'static str;
    fn unit_short_name() -> &'static str;
    fn suffix() -> &'static str;
    fn nanometers_in_unit() -> f64;
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Length<Unit: LengthUnit> {
    v: OrderedFloat<f64>, // in Unit
    phantom: PhantomData<Unit>,
}

impl<Unit: LengthUnit> Length<Unit> {
    pub fn f64(self) -> f64 {
        f64::from(self)
    }

    pub fn f32(self) -> f32 {
        f32::from(self)
    }
}

impl<Unit> fmt::Display for Length<Unit>
where
    Unit: LengthUnit,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:0.4}{}", self.v, Unit::suffix())
    }
}

impl<'a, UnitA, UnitB> From<&'a Length<UnitA>> for Length<UnitB>
where
    UnitA: LengthUnit,
    UnitB: LengthUnit,
{
    fn from(v: &'a Length<UnitA>) -> Self {
        Self {
            v: v.v * UnitA::nanometers_in_unit() / UnitB::nanometers_in_unit(),
            phantom: PhantomData,
        }
    }
}

impl<UnitA, UnitB> Add<Length<UnitA>> for Length<UnitB>
where
    UnitA: LengthUnit,
    UnitB: LengthUnit,
{
    type Output = Length<UnitB>;

    fn add(self, other: Length<UnitA>) -> Self {
        Self {
            v: self.v + Length::<UnitB>::from(&other).v,
            phantom: PhantomData,
        }
    }
}

impl<UnitA, UnitB> AddAssign<Length<UnitA>> for Length<UnitB>
where
    UnitA: LengthUnit,
    UnitB: LengthUnit,
{
    fn add_assign(&mut self, other: Length<UnitA>) {
        self.v += Length::<UnitB>::from(&other).v;
    }
}

impl<UnitA, UnitB> Sub<Length<UnitA>> for Length<UnitB>
where
    UnitA: LengthUnit,
    UnitB: LengthUnit,
{
    type Output = Length<UnitB>;

    fn sub(self, other: Length<UnitA>) -> Self {
        Self {
            v: self.v - Length::<UnitB>::from(&other).v,
            phantom: PhantomData,
        }
    }
}

impl<UnitA, UnitB> SubAssign<Length<UnitA>> for Length<UnitB>
where
    UnitA: LengthUnit,
    UnitB: LengthUnit,
{
    fn sub_assign(&mut self, other: Length<UnitA>) {
        self.v -= Length::<UnitB>::from(&other).v;
    }
}

impl<UnitA, UnitB> Mul<Length<UnitA>> for Length<UnitB>
where
    UnitA: LengthUnit,
    UnitB: LengthUnit,
{
    type Output = Area<UnitB>;

    fn mul(self, other: Length<UnitA>) -> Self::Output {
        Area::<UnitB>::from(self.v.0 * Length::<UnitB>::from(&other).f64())
    }
}

impl<Unit> Mul<Scalar> for Length<Unit>
where
    Unit: LengthUnit,
{
    type Output = Length<Unit>;

    fn mul(self, s: Scalar) -> Self {
        Self {
            v: self.v * s.f64(),
            phantom: PhantomData,
        }
    }
}

impl<Unit> MulAssign<Scalar> for Length<Unit>
where
    Unit: LengthUnit,
{
    fn mul_assign(&mut self, s: Scalar) {
        self.v *= s.f64();
    }
}

impl<Unit> Div<Scalar> for Length<Unit>
where
    Unit: LengthUnit,
{
    type Output = Length<Unit>;

    fn div(self, s: Scalar) -> Self {
        Self {
            v: self.v / s.f64(),
            phantom: PhantomData,
        }
    }
}

impl<Unit> DivAssign<Scalar> for Length<Unit>
where
    Unit: LengthUnit,
{
    fn div_assign(&mut self, s: Scalar) {
        self.v /= s.f64();
    }
}

impl<Unit: LengthUnit> AbsDiffEq for Length<Unit> {
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
        impl<Unit> From<$Num> for Length<Unit>
        where
            Unit: LengthUnit,
        {
            fn from(v: $Num) -> Self {
                Self {
                    v: OrderedFloat(v as f64),
                    phantom: PhantomData,
                }
            }
        }

        impl<Unit> From<&$Num> for Length<Unit>
        where
            Unit: LengthUnit,
        {
            fn from(v: &$Num) -> Self {
                Self {
                    v: OrderedFloat(*v as f64),
                    phantom: PhantomData,
                }
            }
        }

        impl<Unit> From<Length<Unit>> for $Num
        where
            Unit: LengthUnit,
        {
            fn from(v: Length<Unit>) -> $Num {
                v.v.0 as $Num
            }
        }
    };
}
impl_unit_for_floats!(impl_length_unit_for_numeric_type);
impl_unit_for_integers!(impl_length_unit_for_numeric_type);

#[cfg(test)]
mod test {
    use crate::{feet, meters};
    use nalgebra::{Point3, Vector3};

    #[test]
    fn test_meters_to_feet() {
        let m = meters!(1);
        println!("m : {}", m);
        println!("ft: {}", feet!(m));
    }

    #[test]
    fn test_nalgebra_integration() {
        let pt = Point3::new(feet!(10), feet!(13), feet!(17));
        let v = Vector3::new(feet!(10), feet!(13), feet!(17));
        let rv = pt + v;
        assert_eq!(rv, Point3::new(feet!(20), feet!(26), feet!(34)));
    }
}
