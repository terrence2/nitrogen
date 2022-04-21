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
use crate::{impl_unit_for_floats, impl_unit_for_integers, radians, scalar, Scalar};
use ordered_float::OrderedFloat;
use std::{
    fmt,
    marker::PhantomData,
    ops::{Add, AddAssign, Div, DivAssign, Mul, Neg, Sub, SubAssign},
};

pub trait AngleUnit: Copy {
    fn unit_name() -> &'static str;
    fn suffix() -> &'static str;
    fn femto_radians_in_unit() -> f64;
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Angle<Unit: AngleUnit> {
    v: OrderedFloat<f64>,
    phantom: PhantomData<Unit>,
}

impl<Unit: AngleUnit> Angle<Unit> {
    pub fn floor(self) -> f64 {
        f64::from(self).floor()
    }

    pub fn ceil(self) -> f64 {
        f64::from(self).ceil()
    }

    pub fn round(self) -> f64 {
        f64::from(self).round()
    }

    pub fn clamp(self, min: Self, max: Self) -> Self {
        if self.v < min.v {
            min
        } else if self.v > max.v {
            max
        } else {
            self
        }
    }

    // In integer units min<x<=max
    pub fn wrap(self, min: Self, max: Self) -> Self {
        debug_assert!(max.v > min.v);
        let range_size = max.v - min.v;
        let mut out = self;
        while out.v <= min.v {
            out.v += range_size;
        }
        while out.v > max.v {
            out.v -= range_size;
        }
        out
    }

    pub fn sign(&self) -> i8 {
        self.v.0.signum() as i8
    }

    pub fn cos(self) -> Scalar {
        scalar!(f64::from(radians!(self)).cos())
    }

    pub fn sin(self) -> Scalar {
        scalar!(f64::from(radians!(self)).sin())
    }

    pub fn tan(self) -> Scalar {
        scalar!(f64::from(radians!(self)).tan())
    }

    pub fn f32(self) -> f32 {
        f32::from(self)
    }

    pub fn f64(self) -> f64 {
        f64::from(self)
    }

    pub fn split_degrees_minutes_seconds(&self) -> (i32, i32, i32) {
        use crate::unit::{arcseconds::ArcSeconds, degrees::Degrees};

        let mut arcsecs = Angle::<ArcSeconds>::from(self).f64() as i64;
        let degrees = Angle::<Degrees>::from(self).f64() as i64;
        arcsecs -= degrees * 3_600;
        let minutes = arcsecs / 60;
        arcsecs -= minutes * 60;
        (degrees as i32, minutes as i32, arcsecs as i32)
    }

    pub fn format_latitude(&self) -> String {
        let mut lat = *self;
        let lat_hemi = if lat.f64() >= 0.0 {
            "N"
        } else {
            lat = -lat;
            "S"
        };
        let (lat_d, lat_m, lat_s) = lat.split_degrees_minutes_seconds();
        format!("{}{:03}d{:02}m{:02}s", lat_hemi, lat_d, lat_m, lat_s)
    }

    pub fn format_longitude(&self) -> String {
        let mut lon = *self;
        let lon_hemi = if lon.f64() >= 0.0 {
            "E"
        } else {
            lon = -lon;
            "W"
        };
        let (lon_d, lon_m, lon_s) = lon.split_degrees_minutes_seconds();
        format!("{}{:03}d{:02}m{:02}s", lon_hemi, lon_d, lon_m, lon_s)
    }
}

impl<Unit> fmt::Display for Angle<Unit>
where
    Unit: AngleUnit,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:0.4}{}", self.v, Unit::suffix())
    }
}

impl<'a, UnitA, UnitB> From<&'a Angle<UnitA>> for Angle<UnitB>
where
    UnitA: AngleUnit,
    UnitB: AngleUnit,
{
    fn from(v: &'a Angle<UnitA>) -> Self {
        Self {
            v: v.v * UnitA::femto_radians_in_unit() / UnitB::femto_radians_in_unit(),
            phantom: PhantomData,
        }
    }
}

impl<UnitA, UnitB> Add<Angle<UnitA>> for Angle<UnitB>
where
    UnitA: AngleUnit,
    UnitB: AngleUnit,
{
    type Output = Angle<UnitB>;

    fn add(self, other: Angle<UnitA>) -> Self {
        Self {
            v: self.v + Angle::<UnitB>::from(&other).v,
            phantom: PhantomData,
        }
    }
}

impl<UnitA, UnitB> AddAssign<Angle<UnitA>> for Angle<UnitB>
where
    UnitA: AngleUnit,
    UnitB: AngleUnit,
{
    fn add_assign(&mut self, other: Angle<UnitA>) {
        self.v += Angle::<UnitB>::from(&other).v;
    }
}

impl<UnitA, UnitB> Sub<Angle<UnitA>> for Angle<UnitB>
where
    UnitA: AngleUnit,
    UnitB: AngleUnit,
{
    type Output = Angle<UnitB>;

    fn sub(self, other: Angle<UnitA>) -> Self {
        Self {
            v: self.v - Angle::<UnitB>::from(&other).v,
            phantom: PhantomData,
        }
    }
}

impl<UnitA, UnitB> SubAssign<Angle<UnitA>> for Angle<UnitB>
where
    UnitA: AngleUnit,
    UnitB: AngleUnit,
{
    fn sub_assign(&mut self, other: Angle<UnitA>) {
        self.v -= Angle::<UnitB>::from(&other).v;
    }
}

impl<Unit> Neg for Angle<Unit>
where
    Unit: AngleUnit,
{
    type Output = Self;

    fn neg(mut self) -> Self::Output {
        self.v = -self.v;
        self
    }
}

impl<Unit> Mul<Scalar> for Angle<Unit>
where
    Unit: AngleUnit,
{
    type Output = Angle<Unit>;

    fn mul(self, other: Scalar) -> Self {
        Self {
            v: self.v * other.f64(),
            phantom: PhantomData,
        }
    }
}

impl<Unit> Div<Scalar> for Angle<Unit>
where
    Unit: AngleUnit,
{
    type Output = Self;

    fn div(self, rhs: Scalar) -> Self {
        Self {
            v: self.v / rhs.f64(),
            phantom: PhantomData,
        }
    }
}

impl<Unit> DivAssign<Scalar> for Angle<Unit>
where
    Unit: AngleUnit,
{
    fn div_assign(&mut self, rhs: Scalar) {
        self.v /= rhs.f64();
    }
}

macro_rules! impl_angle_unit_for_numeric_type {
    ($Num:ty) => {
        impl<Unit> From<$Num> for Angle<Unit>
        where
            Unit: AngleUnit,
        {
            fn from(v: $Num) -> Self {
                Self {
                    v: OrderedFloat(v as f64),
                    phantom: PhantomData,
                }
            }
        }

        impl<Unit> From<&$Num> for Angle<Unit>
        where
            Unit: AngleUnit,
        {
            fn from(v: &$Num) -> Self {
                Self {
                    v: OrderedFloat(*v as f64),
                    phantom: PhantomData,
                }
            }
        }

        impl<Unit> From<Angle<Unit>> for $Num
        where
            Unit: AngleUnit,
        {
            fn from(v: Angle<Unit>) -> $Num {
                v.v.0 as $Num
            }
        }
    };
}
impl_unit_for_floats!(impl_angle_unit_for_numeric_type);
impl_unit_for_integers!(impl_angle_unit_for_numeric_type);

#[cfg(test)]
mod test {
    use crate::{arcminutes, arcseconds, degrees, radians};
    use approx::assert_relative_eq;
    use std::f64::consts::PI;

    #[test]
    fn test_rad_to_deg() {
        let r = radians!(-PI);
        println!("r    : {}", r);
        println!("r raw: {:?}", r);
        println!("r i64: {}", i64::from(r));
        println!("r i32: {}", i32::from(r));
        println!("r i16: {}", i16::from(r));
        println!("r i8 : {}", i8::from(r));
        println!("r f64: {}", f64::from(r));
        println!("r f32: {}", f32::from(r));

        println!("d    : {}", degrees!(r));
        println!("d    : {}", f64::from(degrees!(r)));
    }

    #[test]
    fn test_arcminute_arcsecond() {
        let a = degrees!(1);
        assert_relative_eq!(arcminutes!(a).f32(), 60f32);
        assert_relative_eq!(arcseconds!(a).f32(), 60f32 * 60f32);
    }

    #[test]
    fn test_wrapping() {
        assert_eq!(
            degrees!(179),
            degrees!(-181).wrap(degrees!(-180), degrees!(180))
        );
        assert_eq!(
            degrees!(-179),
            degrees!(181).wrap(degrees!(-180), degrees!(180))
        );
        assert_relative_eq!(
            degrees!(-179).f64(),
            degrees!(180 + 3_600 + 1)
                .wrap(degrees!(-180), degrees!(180))
                .f64(),
            epsilon = 0.000_000_000_001
        );
        assert_relative_eq!(
            degrees!(179).f64(),
            degrees!(-180 - 3_600 - 1)
                .wrap(degrees!(-180), degrees!(180))
                .f64(),
            epsilon = 0.000_000_000_001
        );
    }
}
