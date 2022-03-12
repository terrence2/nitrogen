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
use absolute_unit::{LengthUnit, Meters};
use bevy_ecs::prelude::*;
use geodesy::{Cartesian, GeoCenter, GeoSurface, Graticule, Target};
use nalgebra::{convert, Unit as NUnit, UnitQuaternion, Vector3};
use std::f64::consts::PI;

pub struct BasisVectors<T> {
    pub forward: Vector3<T>,
    pub right: Vector3<T>,
    pub up: Vector3<T>,
}

#[derive(Component, Debug, Default)]
pub struct WorldSpaceFrame {
    position: Cartesian<GeoCenter, Meters>,
    facing: UnitQuaternion<f64>,
}

impl WorldSpaceFrame {
    fn cartesian_position<Unit: LengthUnit>(
        position: Graticule<GeoSurface>,
    ) -> Cartesian<GeoCenter, Unit> {
        Cartesian::<GeoCenter, Unit>::from(Graticule::<GeoCenter>::from(position))
    }

    fn cartesian_forward<Unit: LengthUnit>(
        position: Graticule<GeoSurface>,
        forward: Graticule<Target>,
    ) -> Cartesian<Target, Unit> {
        let r_lon = UnitQuaternion::from_axis_angle(
            &NUnit::new_unchecked(Vector3::new(0f64, 1f64, 0f64)),
            -f64::from(position.longitude),
        );
        let r_lat = UnitQuaternion::from_axis_angle(
            &NUnit::new_normalize(r_lon * Vector3::new(1f64, 0f64, 0f64)),
            PI / 2.0 - f64::from(position.latitude),
        );
        let cart_eye_rel_target_flat = Cartesian::<Target, Unit>::from(forward);
        Cartesian::<Target, Unit>::from(r_lat * r_lon * cart_eye_rel_target_flat.vec64())
    }

    pub fn from_graticule(position: Graticule<GeoSurface>, forward: Graticule<Target>) -> Self {
        let cart_position = Self::cartesian_position::<Meters>(position);
        let cart_forward = Self::cartesian_forward::<Meters>(position, forward);
        Self::new(cart_position, cart_forward.vec64())
    }

    pub fn new(position: Cartesian<GeoCenter, Meters>, forward: Vector3<f64>) -> Self {
        let up_like = position.vec64().normalize();
        let facing = UnitQuaternion::face_towards(&forward, &up_like);
        Self { position, facing }
    }

    pub fn position(&self) -> &Cartesian<GeoCenter, Meters> {
        &self.position
    }

    pub fn basis(&self) -> BasisVectors<f64> {
        BasisVectors {
            forward: (self.facing * Vector3::z_axis()).into_inner(),
            right: (self.facing * Vector3::x_axis()).into_inner(),
            up: (self.facing * Vector3::y_axis()).into_inner(),
        }
    }

    pub fn facing(&self) -> &UnitQuaternion<f64> {
        &self.facing
    }

    pub fn facing32(&self) -> UnitQuaternion<f32> {
        convert(self.facing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use absolute_unit::meters;

    #[test]
    fn it_works() {
        WorldSpaceFrame::new(
            Cartesian::new(meters!(0), meters!(100), meters!(100)),
            Vector3::x_axis().into_inner(),
        );
    }
}
