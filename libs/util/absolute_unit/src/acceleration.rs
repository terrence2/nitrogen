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
    supports_shift_ops, supports_value_type_conversion, LengthUnit, Time, TimeUnit, Velocity,
};
use ordered_float::OrderedFloat;
use std::{fmt, fmt::Debug, marker::PhantomData, ops::Mul};

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Acceleration<UnitLength: LengthUnit, UnitTime: TimeUnit> {
    v: OrderedFloat<f64>,
    phantom_1: PhantomData<UnitLength>,
    phantom_2: PhantomData<UnitTime>,
}
supports_quantity_ops!(Acceleration<A, B>, LengthUnit, TimeUnit);
supports_shift_ops!(Acceleration<A1, B1>, Acceleration<A2, B2>, LengthUnit, TimeUnit);
supports_scalar_ops!(Acceleration<A, B>, LengthUnit, TimeUnit);
supports_absdiffeq!(Acceleration<A, B>, LengthUnit, TimeUnit);
supports_value_type_conversion!(Acceleration<A, B>, LengthUnit, TimeUnit, impl_value_type_conversions);

impl<L, T> fmt::Display for Acceleration<L, T>
where
    L: LengthUnit,
    T: TimeUnit,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.v.0, f)?;
        write!(f, "{}/{}^2", L::UNIT_SHORT_NAME, T::UNIT_SHORT_NAME)
    }
}

impl<'a, LA, TA, LB, TB> From<&'a Acceleration<LA, TA>> for Acceleration<LB, TB>
where
    LA: LengthUnit,
    TA: TimeUnit,
    LB: LengthUnit,
    TB: TimeUnit,
{
    fn from(v: &'a Acceleration<LA, TA>) -> Self {
        let length_ratio = LA::METERS_IN_UNIT / LB::METERS_IN_UNIT;
        let time_ratio = TB::SECONDS_IN_UNIT / TA::SECONDS_IN_UNIT;
        Self {
            v: v.v * length_ratio * time_ratio * time_ratio,
            phantom_1: PhantomData,
            phantom_2: PhantomData,
        }
    }
}

impl<LA, TA, TB> Mul<Time<TB>> for Acceleration<LA, TA>
where
    LA: LengthUnit,
    TA: TimeUnit,
    TB: TimeUnit,
{
    type Output = Velocity<LA, TA>;

    fn mul(self, other: Time<TB>) -> Self::Output {
        Velocity::<LA, TA>::from(self.v.0 * Time::<TA>::from(&other).f64())
    }
}

#[cfg(test)]
mod test {
    use crate::{
        feet_per_second2, hours, meters_per_second, meters_per_second2, miles_per_hour, seconds,
    };
    use approx::assert_abs_diff_eq;

    #[test]
    fn test_acceleration() {
        let m_p_s2 = meters_per_second2!(100.);
        let ft_p_s2 = feet_per_second2!(m_p_s2);
        println!("{}", m_p_s2);
        println!("{}", ft_p_s2);
        assert_abs_diff_eq!(m_p_s2, meters_per_second2!(ft_p_s2));
    }

    #[test]
    fn test_accel_shift() {
        let m_p_s2 = meters_per_second2!(100) + feet_per_second2!(100);
        assert_abs_diff_eq!(m_p_s2, meters_per_second2!(130.48), epsilon = 0.001);
    }

    #[test]
    fn test_accel_convert_time() {
        let mph2 = miles_per_hour!(100) / hours!(1);
        assert_abs_diff_eq!(
            meters_per_second2!(mph2),
            meters_per_second2!(0.0124177),
            epsilon = 0.000_001
        );
    }

    #[test]
    fn test_accel_convert_velocity() {
        let mps2 = meters_per_second2!(100f32);
        assert_abs_diff_eq!(mps2 * seconds!(10f32), meters_per_second!(1000f32));
    }
}
