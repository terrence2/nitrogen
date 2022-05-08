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
    supports_shift_ops, supports_value_type_conversion, Area, Unit,
};
use ordered_float::OrderedFloat;
use std::{fmt, fmt::Debug, marker::PhantomData, ops::Mul};

pub trait LengthUnit: Unit + Copy + Debug + Eq + PartialEq + 'static {
    const METERS_IN_UNIT: f64;
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Length<Unit: LengthUnit> {
    v: OrderedFloat<f64>, // in Unit
    phantom_1: PhantomData<Unit>,
}
supports_quantity_ops!(Length<A>, LengthUnit);
supports_shift_ops!(Length<A1>, Length<A2>, LengthUnit);
supports_scalar_ops!(Length<A>, LengthUnit);
supports_absdiffeq!(Length<A>, LengthUnit);
supports_value_type_conversion!(Length<A>, LengthUnit, impl_value_type_conversions);

impl<Unit> fmt::Display for Length<Unit>
where
    Unit: LengthUnit,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.v.0, f)?;
        write!(f, "{}", Unit::UNIT_SUFFIX)
    }
}

impl<'a, UnitA, UnitB> From<&'a Length<UnitA>> for Length<UnitB>
where
    UnitA: LengthUnit,
    UnitB: LengthUnit,
{
    fn from(v: &'a Length<UnitA>) -> Self {
        Self {
            v: v.v * UnitA::METERS_IN_UNIT / UnitB::METERS_IN_UNIT,
            phantom_1: PhantomData,
        }
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

#[cfg(test)]
mod test {
    use crate::{feet, kilometers, meters, scalar};
    use approx::assert_abs_diff_eq;
    use nalgebra::{Point3, Vector3};

    #[test]
    fn test_meters_to_feet() {
        let m = meters!(1);
        println!("m : {}", m);
        println!("ft: {}", feet!(m));
        assert_abs_diff_eq!(kilometers!(m), kilometers!(0.001));
    }

    #[test]
    fn test_scalar_length() {
        assert_abs_diff_eq!(meters!(2) * scalar!(2), meters!(4));
    }

    #[test]
    fn test_nalgebra_integration() {
        let pt = Point3::new(feet!(10), feet!(13), feet!(17));
        let v = Vector3::new(feet!(10), feet!(13), feet!(17));
        let rv = pt + v;
        assert_eq!(rv, Point3::new(feet!(20), feet!(26), feet!(34)));
    }
}
