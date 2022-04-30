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
    supports_shift_ops, supports_value_type_conversion, DynamicUnits, LengthUnit, MassUnit,
};
use ordered_float::OrderedFloat;
use std::{fmt, fmt::Debug, marker::PhantomData};

// mass / length^3
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Density<UnitMass: MassUnit, UnitLength: LengthUnit> {
    v: OrderedFloat<f64>,
    phantom_1: PhantomData<UnitMass>,
    phantom_2: PhantomData<UnitLength>,
}
supports_quantity_ops!(Density<A, B>, MassUnit, LengthUnit);
supports_shift_ops!(Density<A1, B1>, Density<A2, B2>, MassUnit, LengthUnit);
supports_scalar_ops!(Density<A, B>, MassUnit, LengthUnit);
supports_absdiffeq!(Density<A, B>, MassUnit, LengthUnit);
supports_value_type_conversion!(Density<A, B>, MassUnit, LengthUnit, impl_value_type_conversions);

impl<M, L> fmt::Display for Density<M, L>
where
    M: MassUnit,
    L: LengthUnit,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{:0.4}{}/{}^3",
            self.v,
            L::UNIT_SHORT_NAME,
            M::UNIT_SHORT_NAME
        )
    }
}

impl<'a, MA, LA, MB, LB> From<&'a Density<MB, LB>> for Density<MA, LA>
where
    MA: MassUnit,
    LA: LengthUnit,
    MB: MassUnit,
    LB: LengthUnit,
{
    fn from(v: &'a Density<MB, LB>) -> Self {
        let mass_ratio = MB::GRAMS_IN_UNIT / MA::GRAMS_IN_UNIT;
        let length_ratio = LA::METERS_IN_UNIT / LB::METERS_IN_UNIT;
        Self {
            v: v.v * mass_ratio * length_ratio * length_ratio * length_ratio,
            phantom_1: PhantomData,
            phantom_2: PhantomData,
        }
    }
}

impl<MA, LA> Density<MA, LA>
where
    MA: MassUnit,
    LA: LengthUnit,
{
    pub fn as_dyn(&self) -> DynamicUnits {
        DynamicUnits::new1o3::<MA, LA, LA, LA>(self.v)
    }
}

#[cfg(test)]
mod test {
    use crate::{kilograms_per_meter3, slugs_per_foot3};
    use approx::assert_abs_diff_eq;

    #[test]
    fn test_density() {
        let s_p_f3 = slugs_per_foot3!(100f64);
        let kg_p_m3 = kilograms_per_meter3!(s_p_f3);
        println!("{}", s_p_f3);
        println!("{}", kg_p_m3);
        assert_abs_diff_eq!(s_p_f3, slugs_per_foot3!(kg_p_m3), epsilon = 0.000_000_1);
    }

    #[test]
    fn test_density_shift() {
        // let m_p_s = meters_per_second!(100) + miles_per_hour!(100);
        // assert_abs_diff_eq!(m_p_s, meters_per_second!(144.704), epsilon = 0.001);
    }
}
