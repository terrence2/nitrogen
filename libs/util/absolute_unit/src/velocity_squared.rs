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
    supports_shift_ops, supports_value_type_conversion, DynamicUnits, LengthUnit, TimeUnit,
    Velocity,
};
use ordered_float::OrderedFloat;
use std::{fmt, fmt::Debug, marker::PhantomData};

// While there is no "meaning" to this unit, traditionally, it shows up in _so many_ places
// that having a way to represent it as an intermediate is extremely useful to avoid dynamic
// analysis of unit types (and associated .as_dyn() line noise).
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct VelocitySquared<UnitLength: LengthUnit, UnitTime: TimeUnit> {
    v: OrderedFloat<f64>,
    phantom_1: PhantomData<UnitLength>,
    phantom_2: PhantomData<UnitTime>,
}
supports_quantity_ops!(VelocitySquared<A, B>, LengthUnit, TimeUnit);
supports_shift_ops!(VelocitySquared<A1, B1>, VelocitySquared<A2, B2>, LengthUnit, TimeUnit);
supports_scalar_ops!(VelocitySquared<A, B>, LengthUnit, TimeUnit);
supports_absdiffeq!(VelocitySquared<A, B>, LengthUnit, TimeUnit);
supports_value_type_conversion!(VelocitySquared<A, B>, LengthUnit, TimeUnit, impl_value_type_conversions);

impl<L, T> fmt::Display for VelocitySquared<L, T>
where
    L: LengthUnit,
    T: TimeUnit,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.v.0, f)?;
        write!(f, "{}^2/{}^2", L::UNIT_SHORT_NAME, T::UNIT_SHORT_NAME)
    }
}

impl<L, T> VelocitySquared<L, T>
where
    L: LengthUnit,
    T: TimeUnit,
{
    pub fn sqrt(&self) -> Velocity<L, T> {
        Velocity::<L, T>::from(self.v.sqrt())
    }

    pub fn as_dyn(&self) -> DynamicUnits {
        DynamicUnits::new2o2::<L, L, T, T>(self.v)
    }
}

impl<'a, LA, TA, LB, TB> From<&'a VelocitySquared<LA, TA>> for VelocitySquared<LB, TB>
where
    LA: LengthUnit,
    TA: TimeUnit,
    LB: LengthUnit,
    TB: TimeUnit,
{
    fn from(v: &'a VelocitySquared<LA, TA>) -> Self {
        let length_ratio = LA::METERS_IN_UNIT / LB::METERS_IN_UNIT;
        let time_ratio = TB::SECONDS_IN_UNIT / TA::SECONDS_IN_UNIT;
        Self {
            v: v.v * length_ratio * length_ratio * time_ratio * time_ratio,
            phantom_1: PhantomData,
            phantom_2: PhantomData,
        }
    }
}

// impl<LA, TA, TB> Mul<Time<TB>> for VelocitySquared<LA, TA>
// where
//     LA: LengthUnit,
//     TA: TimeUnit,
//     TB: TimeUnit,
// {
//     type Output = Velocity<LA, TA>;
//
//     fn mul(self, other: Time<TB>) -> Self::Output {
//         Velocity::<LA, TA>::from(self.v.0 * Time::<TA>::from(&other).f64())
//     }
// }

#[cfg(test)]
mod test {
    use crate::meters_per_second;
    use approx::assert_abs_diff_eq;

    #[test]
    fn test_velocity_squared() {
        let a = meters_per_second!(1f64);
        let b = meters_per_second!(1f64);
        assert_abs_diff_eq!(
            (a * a + b * b).sqrt(),
            meters_per_second!(1.414),
            epsilon = 0.001
        );
    }
}
