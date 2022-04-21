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
use crate::{impl_unit_for_floats, impl_unit_for_integers, Length, LengthUnit, Scalar};
use approx::AbsDiffEq;
use ordered_float::OrderedFloat;
use std::{
    fmt,
    fmt::Debug,
    marker::PhantomData,
    ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign},
};

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Area<Unit: LengthUnit> {
    v: OrderedFloat<f64>, // in Unit^2
    phantom: PhantomData<Unit>,
}

impl<Unit: LengthUnit> Area<Unit> {
    pub fn f64(self) -> f64 {
        f64::from(self)
    }

    pub fn f32(self) -> f32 {
        f32::from(self)
    }
}

impl<Unit> fmt::Display for Area<Unit>
where
    Unit: LengthUnit,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:0.4}{}^2", self.v, Unit::unit_short_name())
    }
}

impl<'a, UnitA, UnitB> From<&'a Area<UnitA>> for Area<UnitB>
where
    UnitA: LengthUnit,
    UnitB: LengthUnit,
{
    fn from(v: &'a Area<UnitA>) -> Self {
        let ratio = UnitA::nanometers_in_unit() / UnitB::nanometers_in_unit();
        Self {
            v: v.v * ratio * ratio,
            phantom: PhantomData,
        }
    }
}

impl<UnitA, UnitB> Add<Area<UnitA>> for Area<UnitB>
where
    UnitA: LengthUnit,
    UnitB: LengthUnit,
{
    type Output = Area<UnitB>;

    fn add(self, other: Area<UnitA>) -> Self {
        Self {
            v: self.v + Area::<UnitB>::from(&other).v,
            phantom: PhantomData,
        }
    }
}

impl<UnitA, UnitB> AddAssign<Area<UnitA>> for Area<UnitB>
where
    UnitA: LengthUnit,
    UnitB: LengthUnit,
{
    fn add_assign(&mut self, other: Area<UnitA>) {
        self.v += Area::<UnitB>::from(&other).v;
    }
}

impl<UnitA, UnitB> Sub<Area<UnitA>> for Area<UnitB>
where
    UnitA: LengthUnit,
    UnitB: LengthUnit,
{
    type Output = Area<UnitB>;

    fn sub(self, other: Area<UnitA>) -> Self {
        Self {
            v: self.v - Area::<UnitB>::from(&other).v,
            phantom: PhantomData,
        }
    }
}

impl<UnitA, UnitB> SubAssign<Area<UnitA>> for Area<UnitB>
where
    UnitA: LengthUnit,
    UnitB: LengthUnit,
{
    fn sub_assign(&mut self, other: Area<UnitA>) {
        self.v -= Area::<UnitB>::from(&other).v;
    }
}

impl<UnitA, UnitB> Div<Length<UnitA>> for Area<UnitB>
where
    UnitA: LengthUnit,
    UnitB: LengthUnit,
{
    type Output = Length<UnitB>;

    fn div(self, other: Length<UnitA>) -> Self::Output {
        Length::<UnitB>::from(self.v.0 / Length::<UnitB>::from(&other).f64())
    }
}

impl<Unit> Mul<Scalar> for Area<Unit>
where
    Unit: LengthUnit,
{
    type Output = Area<Unit>;

    fn mul(self, s: Scalar) -> Self {
        Self {
            v: self.v * s.f64(),
            phantom: PhantomData,
        }
    }
}

impl<Unit> MulAssign<Scalar> for Area<Unit>
where
    Unit: LengthUnit,
{
    fn mul_assign(&mut self, s: Scalar) {
        self.v *= s.f64();
    }
}

impl<Unit> Div<Scalar> for Area<Unit>
where
    Unit: LengthUnit,
{
    type Output = Area<Unit>;

    fn div(self, s: Scalar) -> Self {
        Self {
            v: self.v / s.f64(),
            phantom: PhantomData,
        }
    }
}

impl<Unit> DivAssign<Scalar> for Area<Unit>
where
    Unit: LengthUnit,
{
    fn div_assign(&mut self, s: Scalar) {
        self.v /= s.f64();
    }
}

impl<Unit: LengthUnit> AbsDiffEq for Area<Unit> {
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
        impl<Unit> From<$Num> for Area<Unit>
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

        impl<Unit> From<&$Num> for Area<Unit>
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

        impl<Unit> From<Area<Unit>> for $Num
        where
            Unit: LengthUnit,
        {
            fn from(v: Area<Unit>) -> $Num {
                v.v.0 as $Num
            }
        }
    };
}
impl_unit_for_floats!(impl_length_unit_for_numeric_type);
impl_unit_for_integers!(impl_length_unit_for_numeric_type);

#[cfg(test)]
mod test {
    use crate::{feet, feet2, meters, meters2};
    use approx::assert_abs_diff_eq;

    #[test]
    fn test_meters_to_feet() {
        let ft = feet2!(1);
        println!("ft^2: {}", ft);
        println!("m^2 : {}", meters2!(ft));
        assert_abs_diff_eq!(meters2!(ft), meters2!(0.092_903_04));
    }

    #[test]
    fn test_derived_area() {
        let ft2 = feet!(2) * meters!(1);
        println!("ft2: {}", ft2);
        assert_abs_diff_eq!(ft2, feet2!(6.561_679_790_026_247));
    }

    #[test]
    fn test_derived_length() {
        let m = meters2!(4) / feet!(10);
        println!("m: {}", m);
        assert_abs_diff_eq!(m, meters!(1.312_335_958_005_249_4));
    }
}
