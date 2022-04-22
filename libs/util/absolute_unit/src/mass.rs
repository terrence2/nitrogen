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
    impl_value_type_conversions, Scalar,
};
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
    phantom_1: PhantomData<Unit>,
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
            phantom_1: PhantomData,
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
            phantom_1: PhantomData,
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
            phantom_1: PhantomData,
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

impl_scalar_math_for_type!(Mass<A>, MassUnit);

impl_absdiffeq_for_type!(Mass<A>, MassUnit);

impl_unit_for_value_types!(Mass<A>, MassUnit, impl_value_type_conversions);

#[cfg(test)]
mod test {
    use crate::{kilograms, pounds, scalar};
    use approx::assert_abs_diff_eq;

    #[test]
    fn test_mass() {
        let lb = pounds!(2);
        println!("lb: {}", lb);
        println!("kg: {}", kilograms!(lb));
        assert_abs_diff_eq!(kilograms!(lb), kilograms!(0.907_184_74));
    }

    #[test]
    fn test_mass_scalar() {
        assert_abs_diff_eq!(pounds!(2) * scalar!(2), pounds!(4));
    }
}
