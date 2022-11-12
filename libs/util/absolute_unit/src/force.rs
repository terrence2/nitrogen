// This file is part of Nitrogen.
// // Nitrogen is free software: you can redistribute it and/or modify
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
    impl_value_type_conversions, supports_absdiffeq, supports_cancellation, supports_quantity_ops,
    supports_scalar_ops, supports_shift_ops, supports_value_type_conversion, Acceleration,
    DynamicUnits, Length, LengthUnit, Mass, MassUnit, Scalar, TimeUnit, Torque, Unit,
};
use ordered_float::OrderedFloat;
use std::{
    fmt,
    fmt::Debug,
    marker::PhantomData,
    ops::{Div, Mul},
};

pub trait ForceUnit: Unit + Copy + Debug + Eq + PartialEq + 'static {
    const NEWTONS_IN_UNIT: f64;

    type UnitMass: MassUnit;
    type UnitLength: LengthUnit;
    type UnitTime: TimeUnit;
}

/// mass * length / time / time
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Force<Unit: ForceUnit> {
    v: OrderedFloat<f64>, // in Unit
    phantom_1: PhantomData<Unit>,
}
supports_absdiffeq!(Force<A>, ForceUnit);
supports_cancellation!(Force<A1>, Force<A2>, ForceUnit);
supports_quantity_ops!(Force<A>, ForceUnit);
supports_scalar_ops!(Force<A>, ForceUnit);
supports_shift_ops!(Force<A1>, Force<A2>, ForceUnit);
supports_value_type_conversion!(Force<A>, ForceUnit, impl_value_type_conversions);

impl<Unit> fmt::Display for Force<Unit>
where
    Unit: ForceUnit,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.v.0, f)?;
        write!(f, "{}", Unit::UNIT_SHORT_NAME)
    }
}

impl<'a, UnitA, UnitB> From<&'a Force<UnitA>> for Force<UnitB>
where
    UnitA: ForceUnit,
    UnitB: ForceUnit,
{
    fn from(v: &'a Force<UnitA>) -> Self {
        Self {
            v: v.v * UnitA::NEWTONS_IN_UNIT / UnitB::NEWTONS_IN_UNIT,
            phantom_1: PhantomData,
        }
    }
}

impl<F> From<DynamicUnits> for Force<F>
where
    F: ForceUnit,
{
    fn from(v: DynamicUnits) -> Self {
        let f = v.ordered_float();
        v.assert_units_equal(DynamicUnits::new2o2::<
            F::UnitMass,
            F::UnitLength,
            F::UnitTime,
            F::UnitTime,
        >(0f64.into()));
        Self {
            v: f,
            phantom_1: PhantomData,
        }
    }
}

impl<F> Force<F>
where
    F: ForceUnit,
{
    pub fn as_dyn(&self) -> DynamicUnits {
        DynamicUnits::new2o2::<F::UnitMass, F::UnitLength, F::UnitTime, F::UnitTime>(self.v)
    }
}

impl<F, M> Div<Mass<M>> for Force<F>
where
    F: ForceUnit, // kg*m/s^2
    M: MassUnit,
{
    type Output = Acceleration<F::UnitLength, F::UnitTime>;

    fn div(self, rhs: Mass<M>) -> Self::Output {
        let mass = Mass::<F::UnitMass>::from(&rhs);
        Self::Output::from(self.v.0 / mass.f64())
    }
}

impl<F, L, T> Div<Acceleration<L, T>> for Force<F>
where
    F: ForceUnit,
    L: LengthUnit,
    T: TimeUnit,
{
    type Output = Mass<F::UnitMass>;

    fn div(self, rhs: Acceleration<L, T>) -> Self::Output {
        let acc = Acceleration::<F::UnitLength, F::UnitTime>::from(&rhs);
        Self::Output::from(self.v.0 / acc.f64())
    }
}

impl<F, L> Mul<Length<L>> for Force<F>
where
    F: ForceUnit, // kg*m/s^2
    L: LengthUnit,
{
    type Output = Torque<F, L>;

    fn mul(self, rhs: Length<L>) -> Self::Output {
        Self::Output::from(self.v.0 * rhs.f64())
    }
}

#[cfg(test)]
mod test {
    use crate::{kilograms, newtons, pounds_force, pounds_mass, scalar};
    use approx::assert_abs_diff_eq;

    #[test]
    fn test_force() {
        let lbf = pounds_force!(35_000_f64);
        let n = newtons!(155_687.7_f64);
        assert_abs_diff_eq!(newtons!(lbf), n, epsilon = 0.1);
        let lbf = pounds_force!(n);
        assert_abs_diff_eq!(lbf, pounds_force!(35_000_f64), epsilon = 0.1);
    }

    #[test]
    fn test_force_scalar() {
        assert_abs_diff_eq!(newtons!(2) * scalar!(2), newtons!(4));
    }

    #[test]
    fn test_force_to_acceleration() {
        let lbf = pounds_force!(70_000_f64);
        let lb = pounds_mass!(55_000_f64);
        let imperial_units_should_be_a_capital_offense = (lbf / lb).f64() / 32.1742;
        assert!(imperial_units_should_be_a_capital_offense < 1_f64);
        assert!((newtons!(lbf) / kilograms!(lb)).f64() / 9.80665 > 1_f64);
    }
}
