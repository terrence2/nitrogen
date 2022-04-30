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
    impl_value_type_conversions, supports_absdiffeq, supports_quantity_ops, supports_scalar_ops,
    supports_shift_ops, supports_value_type_conversion, Mass, MassUnit, PoundsMass, PoundsWeight,
    Unit,
};
use ordered_float::OrderedFloat;
use std::{fmt, fmt::Debug, marker::PhantomData};

pub trait WeightUnit: Unit + Copy + Debug + Eq + PartialEq + 'static {
    const POUNDS_IN_UNIT: f64;
}

/// Weight is a force (lb*ft/s^2), where ft/s^2 is always G.
/// weight = mass * gravity
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Weight<Unit: WeightUnit> {
    v: OrderedFloat<f64>, // in Unit
    phantom_1: PhantomData<Unit>,
}
supports_quantity_ops!(Weight<A>, WeightUnit);
supports_shift_ops!(Weight<A1>, Weight<A2>, WeightUnit);
supports_scalar_ops!(Weight<A>, WeightUnit);
supports_absdiffeq!(Weight<A>, WeightUnit);
supports_value_type_conversion!(Weight<A>, WeightUnit, impl_value_type_conversions);

impl<Unit> fmt::Display for Weight<Unit>
where
    Unit: WeightUnit,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:0.4}{}", self.v, Unit::UNIT_SHORT_NAME)
    }
}

impl<'a, UnitA, UnitB> From<&'a Weight<UnitA>> for Weight<UnitB>
where
    UnitA: WeightUnit,
    UnitB: WeightUnit,
{
    fn from(v: &'a Weight<UnitA>) -> Self {
        Self {
            v: v.v * UnitA::POUNDS_IN_UNIT / UnitB::POUNDS_IN_UNIT,
            phantom_1: PhantomData,
        }
    }
}

impl<Unit> Weight<Unit>
where
    Unit: WeightUnit,
{
    pub fn mass<UnitB: MassUnit>(&self) -> Mass<UnitB> {
        let lb_weight = Weight::<PoundsWeight>::from(self).f64();
        Mass::<UnitB>::from(&Mass::<PoundsMass>::from(lb_weight / 32.174_1))
    }
}

#[cfg(test)]
mod test {
    use crate::{pounds_weight, scalar};
    use approx::assert_abs_diff_eq;

    #[test]
    fn test_weight_scalar() {
        assert_abs_diff_eq!(pounds_weight!(2) * scalar!(2), pounds_weight!(4));
    }
}
