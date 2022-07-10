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
    impl_value_type_conversions, supports_absdiffeq, supports_quantity_ops, supports_scalar_ops,
    supports_shift_ops, supports_value_type_conversion, Area, DynamicUnits, Length, LengthUnit,
};
use ordered_float::OrderedFloat;
use std::{fmt, fmt::Debug, marker::PhantomData, ops::Div};

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Volume<Unit: LengthUnit> {
    v: OrderedFloat<f64>, // in Unit^2
    phantom_1: PhantomData<Unit>,
}
supports_quantity_ops!(Volume<A>, LengthUnit);
supports_shift_ops!(Volume<A1>, Volume<A2>, LengthUnit);
supports_scalar_ops!(Volume<A>, LengthUnit);
supports_absdiffeq!(Volume<A>, LengthUnit);
supports_value_type_conversion!(Volume<A>, LengthUnit, impl_value_type_conversions);

impl<Unit> Volume<Unit>
where
    Unit: LengthUnit,
{
    pub fn as_dyn(&self) -> DynamicUnits {
        DynamicUnits::new3o0::<Unit, Unit, Unit>(self.v)
    }
}

impl<Unit> fmt::Display for Volume<Unit>
where
    Unit: LengthUnit,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.v.0, f)?;
        write!(f, "{}^3", Unit::UNIT_SHORT_NAME)
    }
}

impl<'a, UnitA, UnitB> From<&'a Volume<UnitA>> for Volume<UnitB>
where
    UnitA: LengthUnit,
    UnitB: LengthUnit,
{
    fn from(v: &'a Volume<UnitA>) -> Self {
        let ratio = UnitA::METERS_IN_UNIT / UnitB::METERS_IN_UNIT;
        Self {
            v: v.v * ratio * ratio,
            phantom_1: PhantomData,
        }
    }
}

impl<UnitA, UnitB> Div<Length<UnitA>> for Volume<UnitB>
where
    UnitA: LengthUnit,
    UnitB: LengthUnit,
{
    type Output = Area<UnitB>;

    fn div(self, other: Length<UnitA>) -> Self::Output {
        Area::<UnitB>::from(self.v.0 / Length::<UnitB>::from(&other).f64())
    }
}

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
