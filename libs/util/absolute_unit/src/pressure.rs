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
    supports_shift_ops, supports_value_type_conversion, Unit,
};
use ordered_float::OrderedFloat;
use std::{fmt, fmt::Debug, marker::PhantomData};

pub trait PressureUnit: Unit + Copy + Debug + Eq + PartialEq + 'static {
    const PASCALS_IN_UNIT: f64;
}

// Force / Area
// Mass * Length / (Time * Time * Length * Length)
// Mass / (Time * Time * Length)
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Pressure<UnitPressure: PressureUnit> {
    v: OrderedFloat<f64>,
    phantom_1: PhantomData<UnitPressure>,
}
supports_quantity_ops!(Pressure<A>, PressureUnit);
supports_shift_ops!(Pressure<A1>, Pressure<A2>, PressureUnit);
supports_scalar_ops!(Pressure<A>, PressureUnit);
supports_absdiffeq!(Pressure<A>, PressureUnit);
supports_value_type_conversion!(Pressure<A>, PressureUnit, impl_value_type_conversions);

impl<P> fmt::Display for Pressure<P>
where
    P: PressureUnit,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:0.4}{}", self.v, P::UNIT_SHORT_NAME,)
    }
}

impl<'a, PA, PB> From<&'a Pressure<PB>> for Pressure<PA>
where
    PA: PressureUnit,
    PB: PressureUnit,
{
    fn from(v: &'a Pressure<PB>) -> Self {
        let pressure_ratio = PB::PASCALS_IN_UNIT / PA::PASCALS_IN_UNIT;
        Self {
            v: v.v * pressure_ratio,
            phantom_1: PhantomData,
        }
    }
}

// impl<LA, TA, TB> Div<Time<TB>> for Pressure<LA, TA>
//     where
//         LA: LengthUnit,
//         TA: TimeUnit,
//         TB: TimeUnit,
// {
//     type Output = Acceleration<LA, TA>;
//
//     fn div(self, other: Time<TB>) -> Self::Output {
//         Acceleration::<LA, TA>::from(self.v.0 / Time::<TA>::from(&other).f64())
//     }
// }

#[cfg(test)]
mod test {
    use crate::{pascals, psf};
    use approx::assert_abs_diff_eq;

    #[test]
    fn test_pressure() {
        let psf = psf!(2_116.22f32);
        let pas = pascals!(101_325f32);
        println!("{}", psf);
        println!("{}", pas);
        assert_abs_diff_eq!(psf, psf!(pas), epsilon = 0.01);
    }
}
