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
use crate::impl_unit_for_numerics;
use std::{
    fmt,
    marker::PhantomData,
    ops::{Add, AddAssign, Div, DivAssign, Mul, Neg, Sub, SubAssign},
};

pub trait AngleUnit: Copy {
    fn unit_name() -> &'static str;
    fn suffix() -> &'static str;
    fn femto_radians_in_unit() -> i64;
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub struct Angle<Unit: AngleUnit> {
    femto_rad: i64, // femto = 10**-15
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
        if self.femto_rad < min.femto_rad {
            min
        } else if self.femto_rad > max.femto_rad {
            max
        } else {
            self
        }
    }

    pub fn wrap(self, min: Self, max: Self) -> Self {
        // This clever approach is from: https://stackoverflow.com/a/707426/11820706
        debug_assert!(max.femto_rad > min.femto_rad);
        let range_size = max.femto_rad - min.femto_rad;
        let mut out = self;
        if out.femto_rad < min.femto_rad {
            out.femto_rad += range_size * ((min.femto_rad - out.femto_rad) / range_size + 1);
        }
        out.femto_rad = min.femto_rad + (out.femto_rad - min.femto_rad) % range_size;
        out
    }

    pub fn sign(&self) -> i8 {
        self.femto_rad.signum() as i8
    }

    pub fn cos(self) -> f64 {
        f64::from(self).cos()
    }

    pub fn sin(self) -> f64 {
        f64::from(self).sin()
    }

    pub fn tan(self) -> f64 {
        f64::from(self).tan()
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
        let v = self.femto_rad as f64 / Unit::femto_radians_in_unit() as f64;
        write!(f, "{:0.4}{}", v, Unit::suffix())
    }
}

impl<'a, UnitA, UnitB> From<&'a Angle<UnitA>> for Angle<UnitB>
where
    UnitA: AngleUnit,
    UnitB: AngleUnit,
{
    fn from(v: &'a Angle<UnitA>) -> Self {
        Self {
            femto_rad: v.femto_rad,
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
            femto_rad: self.femto_rad + other.femto_rad,
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
        self.femto_rad += other.femto_rad;
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
            femto_rad: self.femto_rad - other.femto_rad,
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
        self.femto_rad -= other.femto_rad;
    }
}

impl<Unit> Neg for Angle<Unit>
where
    Unit: AngleUnit,
{
    type Output = Self;

    fn neg(mut self) -> Self::Output {
        self.femto_rad = -self.femto_rad;
        self
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
                    femto_rad: (v as f64 * Unit::femto_radians_in_unit() as f64) as i64,
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
                    femto_rad: (*v as f64 * Unit::femto_radians_in_unit() as f64) as i64,
                    phantom: PhantomData,
                }
            }
        }

        impl<Unit> From<Angle<Unit>> for $Num
        where
            Unit: AngleUnit,
        {
            fn from(v: Angle<Unit>) -> $Num {
                (v.femto_rad as f64 / Unit::femto_radians_in_unit() as f64) as $Num
            }
        }

        impl<Unit> Mul<$Num> for Angle<Unit>
        where
            Unit: AngleUnit,
        {
            type Output = Self;

            fn mul(self, rhs: $Num) -> Self {
                Self {
                    femto_rad: (self.femto_rad as f64 * rhs as f64).round() as i64,
                    phantom: PhantomData,
                }
            }
        }

        impl<Unit> Div<$Num> for Angle<Unit>
        where
            Unit: AngleUnit,
        {
            type Output = Self;

            fn div(self, rhs: $Num) -> Self {
                Self {
                    femto_rad: (self.femto_rad as f64 / rhs as f64).round() as i64,
                    phantom: PhantomData,
                }
            }
        }

        impl<Unit> DivAssign<$Num> for Angle<Unit>
        where
            Unit: AngleUnit,
        {
            fn div_assign(&mut self, rhs: $Num) {
                self.femto_rad = (self.femto_rad as f64 / rhs as f64).round() as i64;
            }
        }
    };
}
impl_unit_for_numerics!(impl_angle_unit_for_numeric_type);

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
