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
use crate::{GeoCenter, Graticule, Target};
use absolute_unit::{Length, LengthUnit, Radians};
use nalgebra::{Point3, Vector3};
use std::{
    fmt,
    marker::PhantomData,
    ops::{Add, Sub},
};

pub trait CartesianOrigin {
    fn origin_name() -> &'static str;
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Cartesian<Origin, Unit>
where
    Origin: CartesianOrigin,
    Unit: LengthUnit,
{
    pub coords: [Length<Unit>; 3],
    phantom: PhantomData<Origin>,
}

impl<Origin, Unit> Cartesian<Origin, Unit>
where
    Origin: CartesianOrigin,
    Unit: LengthUnit,
{
    pub fn new<UnitB: LengthUnit>(x: Length<UnitB>, y: Length<UnitB>, z: Length<UnitB>) -> Self {
        Self {
            coords: [(&x).into(), (&y).into(), (&z).into()],
            phantom: PhantomData,
        }
    }

    pub fn convert<UnitB: LengthUnit>(&self) -> Cartesian<Origin, UnitB> {
        Cartesian::new(self.coords[0], self.coords[1], self.coords[2])
    }

    pub fn vec64(&self) -> Vector3<f64> {
        Vector3::new(
            f64::from(self.coords[0]),
            f64::from(self.coords[1]),
            f64::from(self.coords[2]),
        )
    }

    pub fn point64(&self) -> Point3<f64> {
        Point3::new(
            f64::from(self.coords[0]),
            f64::from(self.coords[1]),
            f64::from(self.coords[2]),
        )
    }
}

impl<Origin, Unit> fmt::Display for Cartesian<Origin, Unit>
where
    Origin: CartesianOrigin,
    Unit: LengthUnit,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[")?;
        fmt::Display::fmt(&self.coords[0], f)?;
        write!(f, ", ")?;
        fmt::Display::fmt(&self.coords[1], f)?;
        write!(f, ", ")?;
        fmt::Display::fmt(&self.coords[2], f)?;
        write!(f, "]")?;
        write!(f, "{}", Origin::origin_name())
    }
}

impl<Origin, Unit> From<Vector3<f64>> for Cartesian<Origin, Unit>
where
    Origin: CartesianOrigin,
    Unit: LengthUnit,
{
    #[inline]
    fn from(v: Vector3<f64>) -> Self {
        Self {
            coords: [
                Length::<Unit>::from(v[0]),
                Length::<Unit>::from(v[1]),
                Length::<Unit>::from(v[2]),
            ],
            phantom: PhantomData,
        }
    }
}

impl<Origin, Unit> From<Point3<Length<Unit>>> for Cartesian<Origin, Unit>
where
    Origin: CartesianOrigin,
    Unit: LengthUnit,
{
    #[inline]
    fn from(v: Point3<Length<Unit>>) -> Self {
        Self {
            coords: [v.coords[0], v.coords[1], v.coords[2]],
            phantom: PhantomData,
        }
    }
}

impl<Origin, Unit> From<Point3<f64>> for Cartesian<Origin, Unit>
where
    Origin: CartesianOrigin,
    Unit: LengthUnit,
{
    #[inline]
    fn from(v: Point3<f64>) -> Self {
        Self {
            coords: [
                Length::<Unit>::from(v[0]),
                Length::<Unit>::from(v[1]),
                Length::<Unit>::from(v[2]),
            ],
            phantom: PhantomData,
        }
    }
}

impl<Unit> From<Graticule<GeoCenter>> for Cartesian<GeoCenter, Unit>
where
    Unit: LengthUnit,
{
    #[inline]
    fn from(graticule: Graticule<GeoCenter>) -> Self {
        let lat = graticule.lat::<Radians>();
        let lon = graticule.lon::<Radians>();
        let base = graticule.distance;
        Self {
            coords: [
                (&(base * -lon.sin() * lat.cos())).into(),
                (&(base * lat.sin())).into(),
                (&(base * lon.cos() * lat.cos())).into(),
            ],
            phantom: PhantomData,
        }
    }
}

impl<Unit> From<Graticule<Target>> for Cartesian<Target, Unit>
where
    Unit: LengthUnit,
{
    #[inline]
    fn from(graticule: Graticule<Target>) -> Self {
        let lat = graticule.latitude;
        let lon = graticule.longitude;
        Self {
            coords: [
                (&(graticule.distance * -lon.sin() * lat.cos())).into(),
                (&(graticule.distance * lat.sin())).into(),
                (&(graticule.distance * lon.cos() * lat.cos())).into(),
            ],
            phantom: PhantomData,
        }
    }
}

impl<UnitLHS, UnitRHS> Add<Cartesian<Target, UnitRHS>> for Cartesian<GeoCenter, UnitLHS>
where
    UnitLHS: LengthUnit,
    UnitRHS: LengthUnit,
{
    type Output = Cartesian<GeoCenter, UnitLHS>;

    fn add(self, other: Cartesian<Target, UnitRHS>) -> Self {
        Self {
            coords: [
                self.coords[0] + other.coords[0],
                self.coords[1] + other.coords[1],
                self.coords[2] + other.coords[2],
            ],
            phantom: PhantomData,
        }
    }
}

impl<UnitLHS, UnitRHS> Sub<Cartesian<GeoCenter, UnitRHS>> for Cartesian<GeoCenter, UnitLHS>
where
    UnitLHS: LengthUnit,
    UnitRHS: LengthUnit,
{
    type Output = Cartesian<Target, UnitLHS>;

    fn sub(self, other: Cartesian<GeoCenter, UnitRHS>) -> Self::Output {
        Self::Output {
            coords: [
                self.coords[0] - other.coords[0],
                self.coords[1] - other.coords[1],
                self.coords[2] - other.coords[2],
            ],
            phantom: PhantomData,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{GeoCenter, GeoSurface};
    use absolute_unit::{degrees, kilometers, meters, Kilometers};
    use approx::assert_abs_diff_eq;
    use physical_constants::EARTH_RADIUS;

    // Normalized Device Coordinates
    // X to the right
    // Y to the top
    // Z to the front
    // Note: right-handed
    #[test]
    fn test_longitude() {
        // Locked at latitude 0, does longitude vary correctly?
        // Longitude 0 -> point away from screen
        let g = Graticule::<GeoSurface>::new(degrees!(0), degrees!(0), meters!(0));
        let c = Cartesian::<GeoCenter, Kilometers>::from(Graticule::<GeoCenter>::from(g));
        assert_abs_diff_eq!(c.coords[0], kilometers!(0));
        assert_abs_diff_eq!(c.coords[1], kilometers!(0));
        assert_abs_diff_eq!(c.coords[2], kilometers!(*EARTH_RADIUS));

        // Longitude +90 (east); since up is north and forward is 0, we expect +90
        // to map to a negative x position.
        let g = Graticule::<GeoSurface>::new(degrees!(0), degrees!(90), meters!(0));
        let c = Cartesian::<GeoCenter, Kilometers>::from(Graticule::<GeoCenter>::from(g));
        assert_abs_diff_eq!(c.coords[0], kilometers!(-*EARTH_RADIUS));
        assert_abs_diff_eq!(c.coords[1], kilometers!(0));
        assert_abs_diff_eq!(c.coords[2], kilometers!(0), epsilon = 0.000_000_000_001);

        // Longitude +45 (east); since up is north and forward is 0, we expect +45
        // to map to a negative x position and positive z.
        let g = Graticule::<GeoSurface>::new(degrees!(0), degrees!(45), meters!(0));
        let c = Cartesian::<GeoCenter, Kilometers>::from(Graticule::<GeoCenter>::from(g));
        assert!(c.coords[0] < kilometers!(0) && c.coords[0] > kilometers!(-*EARTH_RADIUS));
        assert_abs_diff_eq!(c.coords[1], kilometers!(0));
        assert!(c.coords[2] > kilometers!(0) && c.coords[2] < kilometers!(*EARTH_RADIUS));

        // Longitude +135 (east); since up is north and forward is 0, we expect +45
        // to map to a negative x position and positive z.
        let g = Graticule::<GeoSurface>::new(degrees!(0), degrees!(135), meters!(0));
        let c = Cartesian::<GeoCenter, Kilometers>::from(Graticule::<GeoCenter>::from(g));
        assert!(c.coords[0] < kilometers!(0) && c.coords[0] > kilometers!(-*EARTH_RADIUS));
        assert_abs_diff_eq!(c.coords[1], kilometers!(0));
        assert!(c.coords[2] < kilometers!(0) && c.coords[2] > kilometers!(-*EARTH_RADIUS));

        // Longitude -90 (west); since up is north and forward is 0, we expect -90
        // to map to a positive x position.
        let g = Graticule::<GeoSurface>::new(degrees!(0), degrees!(-90), meters!(0));
        let c = Cartesian::<GeoCenter, Kilometers>::from(Graticule::<GeoCenter>::from(g));
        assert_abs_diff_eq!(c.coords[0], kilometers!(*EARTH_RADIUS));
        assert_abs_diff_eq!(c.coords[1], kilometers!(0));
        assert_abs_diff_eq!(c.coords[2], kilometers!(0), epsilon = 0.000_000_000_001);

        // Longitude -45 (west); since up is north and forward is 0, we expect -45
        // to map to a positive x position and positive z.
        let g = Graticule::<GeoSurface>::new(degrees!(0), degrees!(-45), meters!(0));
        let c = Cartesian::<GeoCenter, Kilometers>::from(Graticule::<GeoCenter>::from(g));
        assert!(c.coords[0] > kilometers!(0) && c.coords[0] < kilometers!(*EARTH_RADIUS));
        assert_abs_diff_eq!(c.coords[1], kilometers!(0));
        assert!(c.coords[2] > kilometers!(0) && c.coords[2] < kilometers!(*EARTH_RADIUS));

        // Longitude -135 (west); since up is north and forward is 0, we expect -45
        // to map to a positive x position and positive z.
        let g = Graticule::<GeoSurface>::new(degrees!(0), degrees!(-135), meters!(0));
        let c = Cartesian::<GeoCenter, Kilometers>::from(Graticule::<GeoCenter>::from(g));
        assert!(c.coords[0] > kilometers!(0) && c.coords[0] < kilometers!(*EARTH_RADIUS));
        assert_abs_diff_eq!(c.coords[1], kilometers!(0));
        assert!(c.coords[2] < kilometers!(0) && c.coords[2] > kilometers!(-*EARTH_RADIUS));

        // Longitude -180 (west): opposite of 0
        let g = Graticule::<GeoSurface>::new(degrees!(0), degrees!(-180), meters!(0));
        let c = Cartesian::<GeoCenter, Kilometers>::from(Graticule::<GeoCenter>::from(g));
        assert_abs_diff_eq!(c.coords[0], kilometers!(0), epsilon = 0.000_000_000_001);
        assert_abs_diff_eq!(c.coords[1], kilometers!(0));
        assert_abs_diff_eq!(c.coords[2], kilometers!(-*EARTH_RADIUS));

        // Longitude +180 (east): same as -180
        let g = Graticule::<GeoSurface>::new(degrees!(0), degrees!(-180), meters!(0));
        let c = Cartesian::<GeoCenter, Kilometers>::from(Graticule::<GeoCenter>::from(g));
        assert_abs_diff_eq!(c.coords[0], kilometers!(0), epsilon = 0.000_000_000_001);
        assert_abs_diff_eq!(c.coords[1], kilometers!(0));
        assert_abs_diff_eq!(c.coords[2], kilometers!(-*EARTH_RADIUS));
    }

    #[test]
    fn test_latitude() {
        // +90 should be straight up
        let g = Graticule::<GeoSurface>::new(degrees!(90), degrees!(0), meters!(0));
        let c = Cartesian::<GeoCenter, Kilometers>::from(Graticule::<GeoCenter>::from(g));
        assert_abs_diff_eq!(c.coords[0], kilometers!(0));
        assert_abs_diff_eq!(c.coords[1], kilometers!(*EARTH_RADIUS));
        assert_abs_diff_eq!(c.coords[2], kilometers!(0), epsilon = 0.000_000_000_001);

        let g = Graticule::<GeoSurface>::new(degrees!(90), degrees!(90), meters!(0));
        let c = Cartesian::<GeoCenter, Kilometers>::from(Graticule::<GeoCenter>::from(g));
        assert_abs_diff_eq!(c.coords[0], kilometers!(0), epsilon = 0.000_000_000_001);
        assert_abs_diff_eq!(c.coords[1], kilometers!(*EARTH_RADIUS));
        assert_abs_diff_eq!(c.coords[2], kilometers!(0));

        let g = Graticule::<GeoSurface>::new(degrees!(90), degrees!(-90), meters!(0));
        let c = Cartesian::<GeoCenter, Kilometers>::from(Graticule::<GeoCenter>::from(g));
        assert_abs_diff_eq!(c.coords[0], kilometers!(0), epsilon = 0.000_000_000_001);
        assert_abs_diff_eq!(c.coords[1], kilometers!(*EARTH_RADIUS));
        assert_abs_diff_eq!(c.coords[2], kilometers!(0));

        let g = Graticule::<GeoSurface>::new(degrees!(90), degrees!(-180), meters!(0));
        let c = Cartesian::<GeoCenter, Kilometers>::from(Graticule::<GeoCenter>::from(g));
        assert_abs_diff_eq!(c.coords[0], kilometers!(0));
        assert_abs_diff_eq!(c.coords[1], kilometers!(*EARTH_RADIUS));
        assert_abs_diff_eq!(c.coords[2], kilometers!(0), epsilon = 0.000_000_000_001);

        // -90 should be straight down
        let g = Graticule::<GeoSurface>::new(degrees!(-90), degrees!(0), meters!(0));
        let c = Cartesian::<GeoCenter, Kilometers>::from(Graticule::<GeoCenter>::from(g));
        assert_abs_diff_eq!(c.coords[0], kilometers!(0));
        assert_abs_diff_eq!(c.coords[1], kilometers!(-*EARTH_RADIUS));
        assert_abs_diff_eq!(c.coords[2], kilometers!(0), epsilon = 0.000_000_000_001);

        let g = Graticule::<GeoSurface>::new(degrees!(-90), degrees!(90), meters!(0));
        let c = Cartesian::<GeoCenter, Kilometers>::from(Graticule::<GeoCenter>::from(g));
        assert_abs_diff_eq!(c.coords[0], kilometers!(0), epsilon = 0.000_000_000_001);
        assert_abs_diff_eq!(c.coords[1], kilometers!(-*EARTH_RADIUS));
        assert_abs_diff_eq!(c.coords[2], kilometers!(0), epsilon = 0.000_000_000_001);

        let g = Graticule::<GeoSurface>::new(degrees!(-90), degrees!(-90), meters!(0));
        let c = Cartesian::<GeoCenter, Kilometers>::from(Graticule::<GeoCenter>::from(g));
        assert_abs_diff_eq!(c.coords[0], kilometers!(0), epsilon = 0.000_000_000_001);
        assert_abs_diff_eq!(c.coords[1], kilometers!(-*EARTH_RADIUS));
        assert_abs_diff_eq!(c.coords[2], kilometers!(0), epsilon = 0.000_000_000_001);

        let g = Graticule::<GeoSurface>::new(degrees!(-90), degrees!(-180), meters!(0));
        let c = Cartesian::<GeoCenter, Kilometers>::from(Graticule::<GeoCenter>::from(g));
        assert_abs_diff_eq!(c.coords[0], kilometers!(0));
        assert_abs_diff_eq!(c.coords[1], kilometers!(-*EARTH_RADIUS));
        assert_abs_diff_eq!(c.coords[2], kilometers!(0), epsilon = 0.000_000_000_001);
    }
}
