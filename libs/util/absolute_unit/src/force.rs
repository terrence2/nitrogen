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
use crate::{impl_unit_for_floats, impl_unit_for_integers, Scalar};
use approx::AbsDiffEq;
use ordered_float::OrderedFloat;
use std::{
    fmt,
    fmt::Debug,
    marker::PhantomData,
    ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign},
};

pub trait ForceUnit: Copy + Debug + Eq + PartialEq + 'static {
    fn unit_name() -> &'static str;
    fn unit_short_name() -> &'static str;
    fn newtons_in_unit() -> f64;
}

/// mass * length / time / time
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Force<Unit: ForceUnit> {
    v: OrderedFloat<f64>, // in Unit
    phantom: PhantomData<Unit>,
}

impl<Unit: ForceUnit> Force<Unit> {
    pub fn f64(self) -> f64 {
        f64::from(self)
    }

    pub fn f32(self) -> f32 {
        f32::from(self)
    }
}

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
            phantom: PhantomData,
        }
    }
}

impl<UnitA, UnitB> Add<Force<UnitA>> for Force<UnitB>
where
    UnitA: ForceUnit,
    UnitB: ForceUnit,
{
    type Output = Force<UnitB>;

    fn add(self, other: Force<UnitA>) -> Self {
        Self {
            v: self.v + Force::<UnitB>::from(&other).v,
            phantom: PhantomData,
        }
    }
}

impl<UnitA, UnitB> AddAssign<Force<UnitA>> for Force<UnitB>
where
    UnitA: ForceUnit,
    UnitB: ForceUnit,
{
    fn add_assign(&mut self, other: Force<UnitA>) {
        self.v += Force::<UnitB>::from(&other).v;
    }
}

impl<UnitA, UnitB> Sub<Force<UnitA>> for Force<UnitB>
where
    UnitA: ForceUnit,
    UnitB: ForceUnit,
{
    type Output = Force<UnitB>;

    fn sub(self, other: Force<UnitA>) -> Self {
        Self {
            v: self.v - Force::<UnitB>::from(&other).v,
            phantom: PhantomData,
        }
    }
}

impl<UnitA, UnitB> SubAssign<Force<UnitA>> for Force<UnitB>
where
    UnitA: ForceUnit,
    UnitB: ForceUnit,
{
    fn sub_assign(&mut self, other: Force<UnitA>) {
        self.v -= Force::<UnitB>::from(&other).v;
    }
}

impl<Unit> Mul<Scalar> for Force<Unit>
where
    Unit: ForceUnit,
{
    type Output = Force<Unit>;

    fn mul(self, s: Scalar) -> Self {
        Self {
            v: self.v * s.f64(),
            phantom: PhantomData,
        }
    }
}

impl<Unit> MulAssign<Scalar> for Force<Unit>
where
    Unit: ForceUnit,
{
    fn mul_assign(&mut self, s: Scalar) {
        self.v *= s.f64();
    }
}

impl<Unit> Div<Scalar> for Force<Unit>
where
    Unit: ForceUnit,
{
    type Output = Force<Unit>;

    fn div(self, s: Scalar) -> Self {
        Self {
            v: self.v / s.f64(),
            phantom: PhantomData,
        }
    }
}

impl<Unit> DivAssign<Scalar> for Force<Unit>
where
    Unit: ForceUnit,
{
    fn div_assign(&mut self, s: Scalar) {
        self.v /= s.f64();
    }
}

impl<Unit: ForceUnit> AbsDiffEq for Force<Unit> {
    type Epsilon = f64;

    fn default_epsilon() -> Self::Epsilon {
        f64::default_epsilon()
    }

    fn abs_diff_eq(&self, other: &Self, epsilon: Self::Epsilon) -> bool {
        self.v.0.abs_diff_eq(&other.v.0, epsilon)
    }
}

macro_rules! impl_length_unit_for_numeric_type {
    ($Num:ty) => {
        impl<Unit> From<$Num> for Force<Unit>
        where
            Unit: ForceUnit,
        {
            fn from(v: $Num) -> Self {
                Self {
                    v: OrderedFloat(v as f64),
                    phantom: PhantomData,
                }
            }
        }

        impl<Unit> From<&$Num> for Force<Unit>
        where
            Unit: ForceUnit,
        {
            fn from(v: &$Num) -> Self {
                Self {
                    v: OrderedFloat(*v as f64),
                    phantom: PhantomData,
                }
            }
        }

        impl<Unit> From<Force<Unit>> for $Num
        where
            Unit: ForceUnit,
        {
            fn from(v: Force<Unit>) -> $Num {
                v.v.0 as $Num
            }
        }
    };
}
impl_unit_for_floats!(impl_length_unit_for_numeric_type);
impl_unit_for_integers!(impl_length_unit_for_numeric_type);

#[cfg(test)]
mod test {
    use crate::{newtons, pounds_of_force};
    use approx::assert_abs_diff_eq;

    #[test]
    fn test_force() {
        let lbf = pounds_of_force!(2);
        println!("lbf: {}", lbf);
        println!("N  : {}", newtons!(lbf));
        assert_abs_diff_eq!(newtons!(lbf), newtons!(8.896_443_2));
    }
}
