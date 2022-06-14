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
    supports_shift_ops, supports_value_type_conversion, Acceleration, DynamicUnits, Length,
    LengthUnit, Time, TimeUnit, VelocitySquared,
};
use ordered_float::OrderedFloat;
use std::{
    fmt,
    fmt::Debug,
    marker::PhantomData,
    ops::{Div, Mul},
};

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Velocity<UnitLength: LengthUnit, UnitTime: TimeUnit> {
    v: OrderedFloat<f64>,
    phantom_1: PhantomData<UnitLength>,
    phantom_2: PhantomData<UnitTime>,
}
supports_quantity_ops!(Velocity<A, B>, LengthUnit, TimeUnit);
supports_shift_ops!(Velocity<A1, B1>, Velocity<A2, B2>, LengthUnit, TimeUnit);
supports_scalar_ops!(Velocity<A, B>, LengthUnit, TimeUnit);
supports_absdiffeq!(Velocity<A, B>, LengthUnit, TimeUnit);
supports_value_type_conversion!(Velocity<A, B>, LengthUnit, TimeUnit, impl_value_type_conversions);

impl<L, T> fmt::Display for Velocity<L, T>
where
    L: LengthUnit,
    T: TimeUnit,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.v.0, f)?;
        write!(f, "{}/{}", L::UNIT_SHORT_NAME, T::UNIT_SHORT_NAME)
    }
}

impl<'a, LA, TA, LB, TB> From<&'a Velocity<LA, TA>> for Velocity<LB, TB>
where
    LA: LengthUnit,
    TA: TimeUnit,
    LB: LengthUnit,
    TB: TimeUnit,
{
    fn from(v: &'a Velocity<LA, TA>) -> Self {
        let length_ratio = LA::METERS_IN_UNIT / LB::METERS_IN_UNIT;
        let time_ratio = TB::SECONDS_IN_UNIT / TA::SECONDS_IN_UNIT;
        Self {
            v: v.v * length_ratio * time_ratio,
            phantom_1: PhantomData,
            phantom_2: PhantomData,
        }
    }
}

impl<LA, TA> Velocity<LA, TA>
where
    LA: LengthUnit,
    TA: TimeUnit,
{
    pub fn as_dyn(&self) -> DynamicUnits {
        DynamicUnits::new1o1::<LA, TA>(self.v)
    }
}

impl<LA, TA, TB> Div<Time<TB>> for Velocity<LA, TA>
where
    LA: LengthUnit,
    TA: TimeUnit,
    TB: TimeUnit,
{
    type Output = Acceleration<LA, TA>;

    fn div(self, other: Time<TB>) -> Self::Output {
        Acceleration::<LA, TA>::from(self.v.0 / Time::<TA>::from(&other).f64())
    }
}

impl<LA, TA, TB> Mul<Time<TB>> for Velocity<LA, TA>
where
    LA: LengthUnit,
    TA: TimeUnit,
    TB: TimeUnit,
{
    type Output = Length<LA>;

    fn mul(self, other: Time<TB>) -> Self::Output {
        Length::<LA>::from(self.v.0 * Time::<TA>::from(&other).f64())
    }
}

impl<LA, TA, LB, TB> Mul<Velocity<LB, TB>> for Velocity<LA, TA>
where
    LA: LengthUnit,
    TA: TimeUnit,
    LB: LengthUnit,
    TB: TimeUnit,
{
    type Output = VelocitySquared<LA, TA>;

    fn mul(self, other: Velocity<LB, TB>) -> Self::Output {
        VelocitySquared::<LA, TA>::from(self.v.0 * Velocity::<LA, TA>::from(&other).f64())
    }
}

impl<LA, TA, LB, TB> Div<Velocity<LB, TB>> for Velocity<LA, TA>
where
    LA: LengthUnit,
    TA: TimeUnit,
    LB: LengthUnit,
    TB: TimeUnit,
{
    type Output = f64;

    fn div(self, other: Velocity<LB, TB>) -> Self::Output {
        self.v.0 / Velocity::<LA, TA>::from(&other).f64()
    }
}

#[cfg(test)]
mod test {
    use crate::{meters_per_second, miles_per_hour};
    use approx::assert_abs_diff_eq;

    #[test]
    fn test_velocity() {
        let m_p_s = meters_per_second!(100.);
        let mph = miles_per_hour!(m_p_s);
        println!("m/s: {}", m_p_s);
        println!("mph : {}", mph);
        assert_abs_diff_eq!(m_p_s, meters_per_second!(mph));
    }

    #[test]
    fn test_velocity_shift() {
        let m_p_s = meters_per_second!(100) + miles_per_hour!(100);
        assert_abs_diff_eq!(m_p_s, meters_per_second!(144.704), epsilon = 0.001);
    }

    #[test]
    fn test_velocity_cancel() {
        assert_abs_diff_eq!(2., meters_per_second!(6f64) / meters_per_second!(3f64))
    }
}
