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
    supports_shift_ops, supports_value_type_conversion, AngleUnit, AngularVelocity, Time, TimeUnit,
};
use ordered_float::OrderedFloat;
use std::{fmt, fmt::Debug, marker::PhantomData, ops::Mul};

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct AngularAcceleration<UnitAngle: AngleUnit, UnitTime: TimeUnit> {
    v: OrderedFloat<f64>,
    phantom_1: PhantomData<UnitAngle>,
    phantom_2: PhantomData<UnitTime>,
}
supports_quantity_ops!(AngularAcceleration<A, B>, AngleUnit, TimeUnit);
supports_shift_ops!(AngularAcceleration<A1, B1>, AngularAcceleration<A2, B2>, AngleUnit, TimeUnit);
supports_scalar_ops!(AngularAcceleration<A, B>, AngleUnit, TimeUnit);
supports_absdiffeq!(AngularAcceleration<A, B>, AngleUnit, TimeUnit);
supports_value_type_conversion!(AngularAcceleration<A, B>, AngleUnit, TimeUnit, impl_value_type_conversions);

impl<L, T> fmt::Display for AngularAcceleration<L, T>
where
    L: AngleUnit,
    T: TimeUnit,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.v.0, f)?;
        write!(f, "{}/{}^2", L::UNIT_SHORT_NAME, T::UNIT_SHORT_NAME)
    }
}

impl<'a, LA, TA, LB, TB> From<&'a AngularAcceleration<LA, TA>> for AngularAcceleration<LB, TB>
where
    LA: AngleUnit,
    TA: TimeUnit,
    LB: AngleUnit,
    TB: TimeUnit,
{
    fn from(v: &'a AngularAcceleration<LA, TA>) -> Self {
        let angle_ratio = LA::RADIANS_IN_UNIT / LB::RADIANS_IN_UNIT;
        let time_ratio = TB::SECONDS_IN_UNIT / TA::SECONDS_IN_UNIT;
        Self {
            v: v.v * angle_ratio * time_ratio * time_ratio,
            phantom_1: PhantomData,
            phantom_2: PhantomData,
        }
    }
}

impl<LA, TA, TB> Mul<Time<TB>> for AngularAcceleration<LA, TA>
where
    LA: AngleUnit,
    TA: TimeUnit,
    TB: TimeUnit,
{
    type Output = AngularVelocity<LA, TA>;

    fn mul(self, other: Time<TB>) -> Self::Output {
        AngularVelocity::<LA, TA>::from(self.v.0 * Time::<TA>::from(&other).f64())
    }
}

#[cfg(test)]
mod test {
    use crate::{degrees_per_second2, radians_per_second, radians_per_second2, seconds};
    use approx::assert_abs_diff_eq;

    #[test]
    fn test_angular_acceleration() {
        let r_p_s2 = radians_per_second2!(100.);
        let d_p_s2 = degrees_per_second2!(r_p_s2);
        println!("{}", r_p_s2);
        println!("{}", d_p_s2);
        assert_abs_diff_eq!(r_p_s2, radians_per_second2!(d_p_s2));
    }

    #[test]
    fn test_angular_accel_shift() {
        let r_p_s2 = radians_per_second2!(100) + degrees_per_second2!(5_732);
        assert_abs_diff_eq!(r_p_s2, radians_per_second2!(200.042), epsilon = 0.001);
    }

    #[test]
    fn test_angular_accel_convert_velocity() {
        let rps2 = radians_per_second2!(100f32);
        assert_abs_diff_eq!(rps2 * seconds!(10f32), radians_per_second!(1000f32));
    }
}
