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
    impl_value_type_conversions, supports_absdiffeq, supports_scalar_ops, supports_shift_ops,
    supports_value_type_conversion,
};
use ordered_float::OrderedFloat;
use std::{fmt, fmt::Debug, marker::PhantomData};

pub trait TimeUnit: Copy + Debug + Eq + PartialEq + 'static {
    const UNIT_NAME: &'static str;
    const UNIT_SHORT_NAME: &'static str;
    const SECONDS_IN_UNIT: f64;
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Time<Unit: TimeUnit> {
    v: OrderedFloat<f64>, // in Unit
    phantom_1: PhantomData<Unit>,
}
supports_shift_ops!(Time<A1>, Time<A2>, TimeUnit);
supports_scalar_ops!(Time<A>, TimeUnit);
supports_absdiffeq!(Time<A>, TimeUnit);
supports_value_type_conversion!(Time<A>, TimeUnit, impl_value_type_conversions);

impl<Unit> fmt::Display for Time<Unit>
where
    Unit: TimeUnit,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:0.4}{}", self.v, Unit::UNIT_SHORT_NAME)
    }
}

impl<'a, UnitA, UnitB> From<&'a Time<UnitA>> for Time<UnitB>
where
    UnitA: TimeUnit,
    UnitB: TimeUnit,
{
    fn from(v: &'a Time<UnitA>) -> Self {
        Self {
            v: v.v * UnitA::SECONDS_IN_UNIT / UnitB::SECONDS_IN_UNIT,
            phantom_1: PhantomData,
        }
    }
}

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
