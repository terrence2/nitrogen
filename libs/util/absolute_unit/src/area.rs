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
use crate::{
    impl_absdiffeq_for_type, impl_scalar_math_for_type, impl_unit_for_value_types,
    impl_value_type_conversions, Length, LengthUnit,
};
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
    phantom_1: PhantomData<Unit>,
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
        let ratio = UnitA::meters_in_unit() / UnitB::meters_in_unit();
        Self {
            v: v.v * ratio * ratio,
            phantom_1: PhantomData,
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
            phantom_1: PhantomData,
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
            phantom_1: PhantomData,
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

impl_scalar_math_for_type!(Area<A>, LengthUnit);

impl_absdiffeq_for_type!(Area<A>, LengthUnit);

impl_unit_for_value_types!(Area<A>, LengthUnit, impl_value_type_conversions);

#[cfg(test)]
mod test {
    use crate::{feet, feet2, meters, meters2, scalar};
    use approx::assert_abs_diff_eq;

    #[test]
    fn test_meters_to_feet() {
        let ft = feet2!(1);
        println!("ft^2: {}", ft);
        println!("m^2 : {}", meters2!(ft));
        assert_abs_diff_eq!(meters2!(ft), meters2!(0.092_903), epsilon = 0.000_001);
    }

    #[test]
    fn test_scalar_area() {
        assert_abs_diff_eq!(meters2!(2) * scalar!(2), meters2!(4));
    }

    #[test]
    fn test_derived_area() {
        let ft2 = feet!(2) * meters!(1);
        println!("ft2: {}", ft2);
        assert_abs_diff_eq!(ft2, feet2!(6.561_679), epsilon = 0.000_001);
    }

    #[test]
    fn test_derived_length() {
        let m = meters2!(4) / feet!(10);
        println!("m: {}", m);
        assert_abs_diff_eq!(m, meters!(1.312_335), epsilon = 0.000_001);
    }
}
