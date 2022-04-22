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
    impl_value_type_conversions, LengthUnit, TimeUnit,
};
use ordered_float::OrderedFloat;
use std::{
    fmt,
    fmt::Debug,
    marker::PhantomData,
    ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign},
};

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Velocity<UnitLength: LengthUnit, UnitTime: TimeUnit> {
    v: OrderedFloat<f64>,
    phantom_1: PhantomData<UnitLength>,
    phantom_2: PhantomData<UnitTime>,
}

impl<L, T> fmt::Display for Velocity<L, T>
where
    L: LengthUnit,
    T: TimeUnit,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{:0.4}{}/{}",
            self.v,
            L::unit_short_name(),
            T::unit_short_name()
        )
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
        let length_ratio = LA::meters_in_unit() / LB::meters_in_unit();
        let time_ratio = TB::seconds_in_unit() / TA::seconds_in_unit();
        Self {
            v: v.v * length_ratio * length_ratio * time_ratio * time_ratio,
            phantom_1: PhantomData,
            phantom_2: PhantomData,
        }
    }
}

impl<LA, TA, LB, TB> Add<Velocity<LA, TA>> for Velocity<LB, TB>
where
    LA: LengthUnit,
    TA: TimeUnit,
    LB: LengthUnit,
    TB: TimeUnit,
{
    type Output = Velocity<LB, TB>;

    fn add(self, other: Velocity<LA, TA>) -> Self {
        Self {
            v: self.v + Velocity::<LB, TB>::from(&other).v,
            phantom_1: PhantomData,
            phantom_2: PhantomData,
        }
    }
}

impl<LA, TA, LB, TB> AddAssign<Velocity<LA, TA>> for Velocity<LB, TB>
where
    LA: LengthUnit,
    TA: TimeUnit,
    LB: LengthUnit,
    TB: TimeUnit,
{
    fn add_assign(&mut self, other: Velocity<LA, TA>) {
        self.v += Velocity::<LB, TB>::from(&other).v;
    }
}

impl<LA, TA, LB, TB> Sub<Velocity<LA, TA>> for Velocity<LB, TB>
where
    LA: LengthUnit,
    TA: TimeUnit,
    LB: LengthUnit,
    TB: TimeUnit,
{
    type Output = Velocity<LB, TB>;

    fn sub(self, other: Velocity<LA, TA>) -> Self {
        Self {
            v: self.v - Velocity::<LB, TB>::from(&other).v,
            phantom_1: PhantomData,
            phantom_2: PhantomData,
        }
    }
}

impl<LA, TA, LB, TB> SubAssign<Velocity<LA, TA>> for Velocity<LB, TB>
where
    LA: LengthUnit,
    TA: TimeUnit,
    LB: LengthUnit,
    TB: TimeUnit,
{
    fn sub_assign(&mut self, other: Velocity<LA, TA>) {
        self.v -= Velocity::<LB, TB>::from(&other).v;
    }
}

// impl<LA, TA, LB, TB> Div<Length<LA, TA>> for Velocity<LB, TB>
// where
//     LA: LengthUnit,
//     TA: TimeUnit,
//     LB: LengthUnit,
//     TB: TimeUnit,
// {
//     type Output = Length<LB, TB>;
//
//     fn div(self, other: Length<LA, TA>) -> Self::Output {
//         Length::<LB, TB>::from(self.v.0 / Length::<LB, TB>::from(&other).f64())
//     }
// }

impl_scalar_math_for_type!(Velocity<A, B>, LengthUnit, TimeUnit);

impl_absdiffeq_for_type!(Velocity<A, B>, LengthUnit, TimeUnit);

impl_unit_for_value_types!(Velocity<A, B>, LengthUnit, TimeUnit, impl_value_type_conversions);

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

    /*
    #[test]
    fn test_derived_area() {
        let ft2 = feet!(2) * meters!(1);
        println!("ft2: {}", ft2);
        assert_abs_diff_eq!(ft2, feet2!(6.561_679_790_026_247));
    }

    #[test]
    fn test_derived_length() {
        let m = meters2!(4) / feet!(10);
        println!("m: {}", m);
        assert_abs_diff_eq!(m, meters!(1.312_335_958_005_249_4));
    }
     */
}
