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
use approx::AbsDiffEq;
use ordered_float::OrderedFloat;
use std::{
    fmt,
    fmt::Debug,
    marker::PhantomData,
    ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign},
};

pub trait TimeUnit: Copy + Debug + Eq + PartialEq + 'static {
    fn unit_name() -> &'static str;
    fn unit_short_name() -> &'static str;
    fn seconds_in_unit() -> f64;
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Time<Unit: TimeUnit> {
    v: OrderedFloat<f64>, // in Unit
    phantom_1: PhantomData<Unit>,
}

impl<Unit> fmt::Display for Time<Unit>
where
    Unit: TimeUnit,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:0.4}{}", self.v, Unit::unit_short_name())
    }
}

impl<'a, UnitA, UnitB> From<&'a Time<UnitA>> for Time<UnitB>
where
    UnitA: TimeUnit,
    UnitB: TimeUnit,
{
    fn from(v: &'a Time<UnitA>) -> Self {
        Self {
            v: v.v * UnitA::seconds_in_unit() / UnitB::seconds_in_unit(),
            phantom_1: PhantomData,
        }
    }
}

impl<UnitA, UnitB> Add<Time<UnitA>> for Time<UnitB>
where
    UnitA: TimeUnit,
    UnitB: TimeUnit,
{
    type Output = Time<UnitB>;

    fn add(self, other: Time<UnitA>) -> Self {
        Self {
            v: self.v + Time::<UnitB>::from(&other).v,
            phantom_1: PhantomData,
        }
    }
}

impl<UnitA, UnitB> AddAssign<Time<UnitA>> for Time<UnitB>
where
    UnitA: TimeUnit,
    UnitB: TimeUnit,
{
    fn add_assign(&mut self, other: Time<UnitA>) {
        self.v += Time::<UnitB>::from(&other).v;
    }
}

impl<UnitA, UnitB> Sub<Time<UnitA>> for Time<UnitB>
where
    UnitA: TimeUnit,
    UnitB: TimeUnit,
{
    type Output = Time<UnitB>;

    fn sub(self, other: Time<UnitA>) -> Self {
        Self {
            v: self.v - Time::<UnitB>::from(&other).v,
            phantom_1: PhantomData,
        }
    }
}

impl<UnitA, UnitB> SubAssign<Time<UnitA>> for Time<UnitB>
where
    UnitA: TimeUnit,
    UnitB: TimeUnit,
{
    fn sub_assign(&mut self, other: Time<UnitA>) {
        self.v -= Time::<UnitB>::from(&other).v;
    }
}

impl_scalar_math_for_type!(Time<A>, TimeUnit);

impl_absdiffeq_for_type!(Time<A>, TimeUnit);

impl_unit_for_value_types!(Time<A>, TimeUnit, impl_value_type_conversions);

#[cfg(test)]
mod test {
    use crate::{hours, scalar, seconds};
    use approx::assert_abs_diff_eq;

    #[test]
    fn test_time() {
        let h = hours!(1);
        println!("h: {}", h);
        println!("s: {}", seconds!(h));
        assert_abs_diff_eq!(seconds!(h), seconds!(3_600));
    }

    #[test]
    fn test_time_scalar() {
        assert_abs_diff_eq!(seconds!(2) * scalar!(2), seconds!(4));
    }
}
