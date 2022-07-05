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
#[cfg(debug_assertions)]
use crate::Radians;
use crate::Unit;
#[cfg(debug_assertions)]
use hashbag::HashBag;
use ordered_float::OrderedFloat;
#[cfg(debug_assertions)]
use std::any::TypeId;
use std::ops::{Add, Div, Mul, Sub};

#[derive(Default, Debug, Clone)]
pub struct DynamicUnits {
    #[cfg(debug_assertions)]
    numerator: HashBag<TypeId>,
    #[cfg(debug_assertions)]
    denominator: HashBag<TypeId>,
    v: OrderedFloat<f64>,
}

impl DynamicUnits {
    pub fn ordered_float(&self) -> OrderedFloat<f64> {
        self.v
    }

    pub fn f64(&self) -> f64 {
        self.v.0
    }

    #[allow(unused_mut)]
    pub fn assert_units_equal(mut self, _other: &DynamicUnits) {
        #[cfg(debug_assertions)]
        {
            // Cancel out the target units, checking that there are units to cancel.
            for n in _other.numerator.iter() {
                assert!(self.numerator.remove(n) > 0);
            }
            for d in _other.denominator.iter() {
                assert!(self.denominator.remove(d) > 0);
            }
            // Remove any radians, from either side, as that is technically a unitless quantity.
            self.numerator.take_all(&TypeId::of::<Radians>());
            self.denominator.take_all(&TypeId::of::<Radians>());

            // Any remainder must _also_ would cancel out, exactly.
            assert_eq!(self.numerator, self.denominator);
        }
    }

    pub fn new0o0(v: OrderedFloat<f64>) -> Self {
        Self {
            v,
            #[cfg(debug_assertions)]
            numerator: HashBag::default(),
            #[cfg(debug_assertions)]
            denominator: HashBag::default(),
        }
    }

    pub fn new1o0<N0>(v: OrderedFloat<f64>) -> Self
    where
        N0: Unit + 'static,
    {
        DynamicUnits {
            v,
            #[cfg(debug_assertions)]
            numerator: HashBag::from_iter([TypeId::of::<N0>()]),
            #[cfg(debug_assertions)]
            denominator: HashBag::default(),
        }
    }

    pub fn new1o1<N0, D0>(v: OrderedFloat<f64>) -> Self
    where
        N0: Unit + 'static,
        D0: Unit + 'static,
    {
        DynamicUnits {
            v,
            #[cfg(debug_assertions)]
            numerator: HashBag::from_iter([TypeId::of::<N0>()]),
            #[cfg(debug_assertions)]
            denominator: HashBag::from_iter([TypeId::of::<D0>()]),
        }
    }

    pub fn new1o2<N0, D0, D1>(v: OrderedFloat<f64>) -> Self
    where
        N0: Unit + 'static,
        D0: Unit + 'static,
        D1: Unit + 'static,
    {
        DynamicUnits {
            v,
            #[cfg(debug_assertions)]
            numerator: HashBag::from_iter([TypeId::of::<N0>()]),
            #[cfg(debug_assertions)]
            denominator: HashBag::from_iter([TypeId::of::<D0>(), TypeId::of::<D1>()]),
        }
    }

    pub fn new1o3<N0, D0, D1, D2>(v: OrderedFloat<f64>) -> Self
    where
        N0: Unit + 'static,
        D0: Unit + 'static,
        D1: Unit + 'static,
        D2: Unit + 'static,
    {
        DynamicUnits {
            v,
            #[cfg(debug_assertions)]
            numerator: HashBag::from_iter([TypeId::of::<N0>()]),
            #[cfg(debug_assertions)]
            denominator: HashBag::from_iter([
                TypeId::of::<D0>(),
                TypeId::of::<D1>(),
                TypeId::of::<D2>(),
            ]),
        }
    }

    pub fn new2o0<N0, N1>(v: OrderedFloat<f64>) -> Self
    where
        N0: Unit + 'static,
        N1: Unit + 'static,
    {
        DynamicUnits {
            v,
            #[cfg(debug_assertions)]
            numerator: HashBag::from_iter([TypeId::of::<N0>(), TypeId::of::<N1>()]),
            #[cfg(debug_assertions)]
            denominator: HashBag::default(),
        }
    }

    pub fn new2o2<N0, N1, D0, D1>(v: OrderedFloat<f64>) -> Self
    where
        N0: Unit + 'static,
        N1: Unit + 'static,
        D0: Unit + 'static,
        D1: Unit + 'static,
    {
        DynamicUnits {
            v,
            #[cfg(debug_assertions)]
            numerator: HashBag::from_iter([TypeId::of::<N0>(), TypeId::of::<N1>()]),
            #[cfg(debug_assertions)]
            denominator: HashBag::from_iter([TypeId::of::<D0>(), TypeId::of::<D1>()]),
        }
    }

    pub fn new3o0<N0, N1, N2>(v: OrderedFloat<f64>) -> Self
    where
        N0: Unit + 'static,
        N1: Unit + 'static,
        N2: Unit + 'static,
    {
        DynamicUnits {
            v,
            #[cfg(debug_assertions)]
            numerator: HashBag::from_iter([
                TypeId::of::<N0>(),
                TypeId::of::<N1>(),
                TypeId::of::<N2>(),
            ]),
            #[cfg(debug_assertions)]
            denominator: HashBag::default(),
        }
    }

    pub fn new3o2<N0, N1, N2, D0, D1>(v: OrderedFloat<f64>) -> Self
    where
        N0: Unit + 'static,
        N1: Unit + 'static,
        N2: Unit + 'static,
        D0: Unit + 'static,
        D1: Unit + 'static,
    {
        DynamicUnits {
            v,
            #[cfg(debug_assertions)]
            numerator: HashBag::from_iter([
                TypeId::of::<N0>(),
                TypeId::of::<N1>(),
                TypeId::of::<N2>(),
            ]),
            #[cfg(debug_assertions)]
            denominator: HashBag::from_iter([TypeId::of::<D0>(), TypeId::of::<D1>()]),
        }
    }
}

impl Add<DynamicUnits> for DynamicUnits {
    type Output = DynamicUnits;

    fn add(mut self, rhs: DynamicUnits) -> Self::Output {
        #[cfg(debug_assertions)]
        {
            debug_assert_eq!(self.numerator, rhs.numerator, "numerator");
            debug_assert_eq!(self.denominator, rhs.denominator, "denominator");
        }
        self.v += rhs.v;
        self
    }
}

impl Sub<DynamicUnits> for DynamicUnits {
    type Output = DynamicUnits;

    fn sub(mut self, rhs: DynamicUnits) -> Self::Output {
        #[cfg(debug_assertions)]
        {
            debug_assert_eq!(self.numerator, rhs.numerator, "numerator");
            debug_assert_eq!(self.denominator, rhs.denominator, "denominator");
        }
        self.v -= rhs.v;
        self
    }
}

impl Mul<DynamicUnits> for DynamicUnits {
    type Output = DynamicUnits;

    fn mul(mut self, rhs: DynamicUnits) -> Self::Output {
        #[cfg(debug_assertions)]
        {
            self.numerator.extend(rhs.numerator.iter());
            self.denominator.extend(rhs.denominator.iter());
        }
        self.v *= rhs.v;
        self
    }
}

impl Div<DynamicUnits> for DynamicUnits {
    type Output = DynamicUnits;

    fn div(mut self, rhs: DynamicUnits) -> Self::Output {
        #[cfg(debug_assertions)]
        {
            self.numerator.extend(rhs.denominator.iter());
            self.denominator.extend(rhs.numerator.iter());
        }
        self.v /= rhs.v;
        self
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        kilograms_per_meter3, meters2, meters_per_second, scalar, Force, Meters, Newtons, Seconds,
    };
    use approx::assert_abs_diff_eq;

    #[test]
    fn test_dyn_mul() {
        let v = meters_per_second!(3.);
        let v2 = v.as_dyn() * v.as_dyn();
        assert_abs_diff_eq!(v2.f64(), 9.);
        v2.assert_units_equal(&DynamicUnits::new2o2::<Meters, Meters, Seconds, Seconds>(
            0.0.into(),
        ));
    }

    #[test]
    fn test_dyn_cancellation() {
        let coef = scalar!(0.5f64).as_dyn();
        let coef_d = scalar!(0.01f64).as_dyn();
        let p = kilograms_per_meter3!(0.1f64).as_dyn();
        let v = meters_per_second!(3f64).as_dyn();
        let a = meters2!(1f64).as_dyn();
        let _drag_lbf: Force<Newtons> = (coef * coef_d * p * v.clone() * v * a).into();
    }
}
