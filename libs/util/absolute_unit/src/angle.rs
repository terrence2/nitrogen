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
    impl_value_type_conversions, radians, scalar, supports_absdiffeq, supports_quantity_ops,
    supports_scalar_ops, supports_shift_ops, supports_value_type_conversion, ArcSeconds, Degrees,
    Scalar, Unit,
};
use ordered_float::OrderedFloat;
use std::{fmt, fmt::Debug, marker::PhantomData, ops::Neg};

pub trait AngleUnit: Unit + Copy + Debug + Eq + PartialEq + 'static {
    const RADIANS_IN_UNIT: f64;
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Angle<Unit: AngleUnit> {
    v: OrderedFloat<f64>,
    phantom_1: PhantomData<Unit>,
}
supports_quantity_ops!(Angle<A>, AngleUnit);
supports_shift_ops!(Angle<A1>, Angle<A2>, AngleUnit);
supports_scalar_ops!(Angle<A>, AngleUnit);
supports_absdiffeq!(Angle<A>, AngleUnit);
supports_value_type_conversion!(Angle<A>, AngleUnit, impl_value_type_conversions);

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

    pub fn split_degrees_minutes_seconds(&self) -> (i32, i32, i32) {
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
        write!(f, "{:0.4}{}", self.v, Unit::UNIT_SUFFIX)
    }
}

impl<'a, UnitA, UnitB> From<&'a Angle<UnitA>> for Angle<UnitB>
where
    UnitA: AngleUnit,
    UnitB: AngleUnit,
{
    fn from(v: &'a Angle<UnitA>) -> Self {
        Self {
            v: v.v * UnitA::RADIANS_IN_UNIT / UnitB::RADIANS_IN_UNIT,
            phantom_1: PhantomData,
        }
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

#[cfg(test)]
mod test {
    use crate::{arcminutes, arcseconds, degrees, radians};
    use approx::assert_abs_diff_eq;
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
    fn test_basic_angle_math() {
        assert_abs_diff_eq!(degrees!(2) + degrees!(2), degrees!(4));
    }

    #[test]
    fn test_arcminute_arcsecond() {
        let a = degrees!(1);
        assert_abs_diff_eq!(arcminutes!(a).f32(), 60f32);
        assert_abs_diff_eq!(arcseconds!(a).f32(), 60f32 * 60f32);
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
        assert_abs_diff_eq!(
            degrees!(-179),
            degrees!(180 + 3_600 + 1).wrap(degrees!(-180), degrees!(180)),
            epsilon = 0.000_000_000_001
        );
        assert_abs_diff_eq!(
            degrees!(179),
            degrees!(-180 - 3_600 - 1).wrap(degrees!(-180), degrees!(180)),
            epsilon = 0.000_000_000_001
        );
    }
}
