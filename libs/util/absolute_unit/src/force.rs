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
    impl_value_type_conversions, supports_absdiffeq, supports_scalar_ops, supports_shift_ops,
    supports_value_type_conversion,
};
use ordered_float::OrderedFloat;
use std::{fmt, fmt::Debug, marker::PhantomData};

pub trait ForceUnit: Copy + Debug + Eq + PartialEq + 'static {
    fn unit_name() -> &'static str;
    fn unit_short_name() -> &'static str;
    fn newtons_in_unit() -> f64;
}

/// mass * length / time / time
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Force<Unit: ForceUnit> {
    v: OrderedFloat<f64>, // in Unit
    phantom_1: PhantomData<Unit>,
}
supports_shift_ops!(Force<A1>, Force<A2>, ForceUnit);
supports_scalar_ops!(Force<A>, ForceUnit);
supports_absdiffeq!(Force<A>, ForceUnit);
supports_value_type_conversion!(Force<A>, ForceUnit, impl_value_type_conversions);

impl<Unit> fmt::Display for Force<Unit>
where
    Unit: ForceUnit,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:0.4}{}", self.v, Unit::unit_short_name())
    }
}

impl<'a, UnitA, UnitB> From<&'a Force<UnitA>> for Force<UnitB>
where
    UnitA: ForceUnit,
    UnitB: ForceUnit,
{
    fn from(v: &'a Force<UnitA>) -> Self {
        Self {
            v: v.v * UnitA::newtons_in_unit() / UnitB::newtons_in_unit(),
            phantom_1: PhantomData,
        }
    }
}

#[cfg(test)]
mod test {
    use crate::{newtons, pounds_of_force, scalar};
    use approx::assert_abs_diff_eq;

    #[test]
    fn test_force() {
        let lbf = pounds_of_force!(2);
        println!("lbf: {}", lbf);
        println!("N  : {}", newtons!(lbf));
        assert_abs_diff_eq!(newtons!(lbf), newtons!(8.896_443_2));
    }

    #[test]
    fn test_force_scalar() {
        assert_abs_diff_eq!(newtons!(2) * scalar!(2), newtons!(4));
    }
}
