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
    supports_shift_ops, supports_value_type_conversion, Acceleration, Area, Force, ForceUnit,
    LengthUnit, Newtons, RotationalInertia, TimeUnit, Unit,
};
use ordered_float::OrderedFloat;
use std::{fmt, fmt::Debug, marker::PhantomData, ops::Mul};

pub trait MassUnit: Unit + Copy + Debug + Eq + PartialEq + 'static {
    const GRAMS_IN_UNIT: f64;
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Mass<Unit: MassUnit> {
    v: OrderedFloat<f64>, // in Unit
    phantom_1: PhantomData<Unit>,
}
supports_quantity_ops!(Mass<A>, MassUnit);
supports_shift_ops!(Mass<A1>, Mass<A2>, MassUnit);
supports_scalar_ops!(Mass<A>, MassUnit);
supports_absdiffeq!(Mass<A>, MassUnit);
supports_value_type_conversion!(Mass<A>, MassUnit, impl_value_type_conversions);

impl<Unit> fmt::Display for Mass<Unit>
where
    Unit: MassUnit,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.v.0, f)?;
        write!(f, "{}", Unit::UNIT_SHORT_NAME)
    }
}

impl<'a, UnitA, UnitB> From<&'a Mass<UnitA>> for Mass<UnitB>
where
    UnitA: MassUnit,
    UnitB: MassUnit,
{
    fn from(v: &'a Mass<UnitA>) -> Self {
        Self {
            v: v.v * UnitA::GRAMS_IN_UNIT / UnitB::GRAMS_IN_UNIT,
            phantom_1: PhantomData,
        }
    }
}

impl<MA, LB, TB> Mul<Acceleration<LB, TB>> for Mass<MA>
where
    MA: MassUnit,
    LB: LengthUnit,
    TB: TimeUnit,
{
    type Output = Force<Newtons>;

    fn mul(self, rhs: Acceleration<LB, TB>) -> Self::Output {
        let acc = Acceleration::<
            <Newtons as ForceUnit>::UnitLength,
            <Newtons as ForceUnit>::UnitTime,
        >::from(&rhs);
        let mass = Mass::<<Newtons as ForceUnit>::UnitMass>::from(self.v.0);
        Self::Output::from(mass.f64() * acc.f64())
    }
}

impl<MA, LB> Mul<Area<LB>> for Mass<MA>
where
    MA: MassUnit,
    LB: LengthUnit,
{
    type Output = RotationalInertia<MA, LB>;

    fn mul(self, rhs: Area<LB>) -> Self::Output {
        Self::Output::from(self.f64() * rhs.f64())
    }
}

#[cfg(test)]
mod test {
    use crate::{kilograms, pounds_mass, scalar};
    use approx::assert_abs_diff_eq;

    #[test]
    fn test_mass() {
        let lb = pounds_mass!(60_000_f64);
        let kg = kilograms!(27_215.5_f64);
        assert_abs_diff_eq!(kg, kilograms!(lb), epsilon = 0.1);
        assert_abs_diff_eq!(lb, pounds_mass!(kg), epsilon = 0.1);
    }

    #[test]
    fn test_mass_scalar() {
        assert_abs_diff_eq!(pounds_mass!(2) * scalar!(2), pounds_mass!(4));
    }
}
