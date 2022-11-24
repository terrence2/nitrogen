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
    supports_shift_ops, supports_value_type_conversion, Angle, AngleUnit, AngularAcceleration,
    DynamicUnits, Time, TimeUnit,
};
use ordered_float::OrderedFloat;
use std::{
    fmt,
    fmt::Debug,
    marker::PhantomData,
    ops::{Div, Mul},
};

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct AngularVelocity<UnitAngle: AngleUnit, UnitTime: TimeUnit> {
    v: OrderedFloat<f64>,
    phantom_1: PhantomData<UnitAngle>,
    phantom_2: PhantomData<UnitTime>,
}
supports_quantity_ops!(AngularVelocity<A, B>, AngleUnit, TimeUnit);
supports_shift_ops!(AngularVelocity<A1, B1>, AngularVelocity<A2, B2>, AngleUnit, TimeUnit);
supports_scalar_ops!(AngularVelocity<A, B>, AngleUnit, TimeUnit);
supports_absdiffeq!(AngularVelocity<A, B>, AngleUnit, TimeUnit);
supports_value_type_conversion!(AngularVelocity<A, B>, AngleUnit, TimeUnit, impl_value_type_conversions);

impl<L, T> fmt::Display for AngularVelocity<L, T>
where
    L: AngleUnit,
    T: TimeUnit,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.v.0, f)?;
        write!(f, "{}/{}", L::UNIT_SUFFIX, T::UNIT_SHORT_NAME)
    }
}

impl<'a, LA, TA, LB, TB> From<&'a AngularVelocity<LA, TA>> for AngularVelocity<LB, TB>
where
    LA: AngleUnit,
    TA: TimeUnit,
    LB: AngleUnit,
    TB: TimeUnit,
{
    fn from(v: &'a AngularVelocity<LA, TA>) -> Self {
        let angle_ratio = LA::RADIANS_IN_UNIT / LB::RADIANS_IN_UNIT;
        let time_ratio = TB::SECONDS_IN_UNIT / TA::SECONDS_IN_UNIT;
        Self {
            v: v.v * angle_ratio * time_ratio,
            phantom_1: PhantomData,
            phantom_2: PhantomData,
        }
    }
}

impl<L, T> From<DynamicUnits> for AngularVelocity<L, T>
where
    L: AngleUnit,
    T: TimeUnit,
{
    fn from(v: DynamicUnits) -> Self {
        let f = v.ordered_float();
        v.assert_units_equal(DynamicUnits::new1o1::<L, T>(0f64.into()));
        Self {
            v: f,
            phantom_1: PhantomData,
            phantom_2: PhantomData,
        }
    }
}

impl<LA, TA> AngularVelocity<LA, TA>
where
    LA: AngleUnit,
    TA: TimeUnit,
{
    pub fn as_dyn(&self) -> DynamicUnits {
        DynamicUnits::new1o1::<LA, TA>(self.v)
    }
}

impl<LA, TA, TB> Div<Time<TB>> for AngularVelocity<LA, TA>
where
    LA: AngleUnit,
    TA: TimeUnit,
    TB: TimeUnit,
{
    type Output = AngularAcceleration<LA, TA>;

    fn div(self, other: Time<TB>) -> Self::Output {
        AngularAcceleration::<LA, TA>::from(self.v.0 / Time::<TA>::from(&other).f64())
    }
}

impl<LA, TA, TB> Mul<Time<TB>> for AngularVelocity<LA, TA>
where
    LA: AngleUnit,
    TA: TimeUnit,
    TB: TimeUnit,
{
    type Output = Angle<LA>;

    fn mul(self, other: Time<TB>) -> Self::Output {
        Angle::<LA>::from(self.v.0 * Time::<TA>::from(&other).f64())
    }
}

// Angular velocity is strange in that radians is a unitless quantity: squaring a velocity
// results in acceleration directly.
impl<LA, TA, LB, TB> Mul<AngularVelocity<LB, TB>> for AngularVelocity<LA, TA>
where
    LA: AngleUnit,
    TA: TimeUnit,
    LB: AngleUnit,
    TB: TimeUnit,
{
    type Output = AngularAcceleration<LA, TA>;

    fn mul(self, other: AngularVelocity<LB, TB>) -> Self::Output {
        AngularAcceleration::<LA, TA>::from(
            self.v.0 * AngularVelocity::<LA, TA>::from(&other).f64(),
        )
    }
}

#[cfg(test)]
mod test {
    use crate::{degrees_per_second, radians_per_second};
    use approx::assert_abs_diff_eq;

    #[test]
    fn test_angular_velocity() {
        let r_p_s = radians_per_second!(100.);
        let d_p_s = degrees_per_second!(r_p_s);
        println!("r/s: {}", r_p_s);
        println!("d/s : {}", d_p_s);
        assert_abs_diff_eq!(r_p_s, radians_per_second!(d_p_s));
    }

    #[test]
    fn test_angular_velocity_shift() {
        let r_p_s = radians_per_second!(100) + degrees_per_second!(5_732);
        assert_abs_diff_eq!(r_p_s, radians_per_second!(200.042), epsilon = 0.001);
    }
}
