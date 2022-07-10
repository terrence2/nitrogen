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
use absolute_unit::{
    meters, meters_per_second, meters_per_second2, radians_per_second, Acceleration,
    AngularVelocity, Length, LengthUnit, Meters, Radians, Seconds, TimeUnit, Velocity,
};
use bevy_ecs::prelude::*;
use geodesy::{Cartesian, GeoCenter, GeoSurface, Graticule, Target};
use nalgebra::{convert, Point3, Unit as NUnit, UnitQuaternion, Vector3};
use nitrous::{inject_nitrous_component, method, NitrousComponent};
use physical_constants::EARTH_RADIUS;
use std::f64::consts::PI;

pub struct BasisVectors<T> {
    pub forward: Vector3<T>,
    pub right: Vector3<T>,
    pub up: Vector3<T>,
}

/// World space is OpenGL-style, right-handed coordinates with the
/// prime meridian (London) on the positive z-axis, the pacific on
/// the positive x axis and the north pole on the positive y axis.
/// For convenience, `facing` translates into and out of the same
/// style of local coordinate system.
#[derive(Component, NitrousComponent, Debug, Default, Clone)]
#[Name = "frame"]
pub struct WorldSpaceFrame {
    position: Cartesian<GeoCenter, Meters>,
    facing: UnitQuaternion<f64>,
}

#[inject_nitrous_component]
impl WorldSpaceFrame {
    #[method]
    fn x(&self) -> f64 {
        self.position.coords[0].f64()
    }

    #[method]
    fn y(&self) -> f64 {
        self.position.coords[1].f64()
    }

    #[method]
    fn z(&self) -> f64 {
        self.position.coords[2].f64()
    }

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

    pub fn from_quaternion(
        position: Cartesian<GeoCenter, Meters>,
        facing: UnitQuaternion<f64>,
    ) -> Self {
        Self { position, facing }
    }

    pub fn new(position: Cartesian<GeoCenter, Meters>, forward: Vector3<f64>) -> Self {
        let up_like = position.vec64().normalize();
        let facing = UnitQuaternion::face_towards(&forward, &up_like);
        Self { position, facing }
    }

    pub fn position(&self) -> &Cartesian<GeoCenter, Meters> {
        &self.position
    }

    pub fn altitude_asl(&self) -> Length<Meters> {
        meters!(self.position().vec64().magnitude()) - *EARTH_RADIUS
    }

    pub fn position_pt3(&self) -> Point3<Length<Meters>> {
        Point3::new(
            self.position.coords[0],
            self.position.coords[1],
            self.position.coords[2],
        )
    }

    pub fn position_graticule(&self) -> Graticule<GeoSurface> {
        Graticule::<GeoSurface>::from(Graticule::<GeoCenter>::from(self.position))
    }

    pub fn set_position_graticule(&mut self, grat: Graticule<GeoSurface>) {
        self.position = grat.cartesian::<Meters>();
    }

    pub fn position_mut(&mut self) -> &mut Cartesian<GeoCenter, Meters> {
        &mut self.position
    }

    pub fn set_position(&mut self, point: Point3<Length<Meters>>) {
        self.position = Cartesian::<GeoCenter, Meters>::from(point);
    }

    pub fn basis(&self) -> BasisVectors<f64> {
        // OpenGL-styled right-handed coordinate system.
        BasisVectors {
            forward: (self.facing * Vector3::z_axis()).into_inner(),
            right: (self.facing * Vector3::x_axis()).into_inner(),
            up: (self.facing * Vector3::y_axis()).into_inner(),
        }
    }

    pub fn forward(&self) -> Vector3<f64> {
        (self.facing * Vector3::z_axis()).into_inner()
    }

    pub fn facing(&self) -> &UnitQuaternion<f64> {
        &self.facing
    }

    pub fn facing_mut(&mut self) -> &mut UnitQuaternion<f64> {
        &mut self.facing
    }

    pub fn facing32(&self) -> UnitQuaternion<f32> {
        convert(self.facing)
    }
}

/// BodyMotion is body-relative motion, not frame relative. That said
/// it is easy to decompose the body-relative motion into frame-relative
/// motion, given a WorldSpaceFrame.
///
/// Note: this structure uses the same OpenGL styled coordinates as
/// the world space frame. There are accessors for the coordinate system
/// used in Allerton's "Principles of Flight Simulation", the standard
/// body coordinate system for planes.
#[derive(Component, NitrousComponent, Copy, Clone, Debug, Default)]
pub struct BodyMotion {
    acceleration_m_s2: Vector3<Acceleration<Meters, Seconds>>,
    linear_velocity: Vector3<Velocity<Meters, Seconds>>,
    angular_velocity: Vector3<AngularVelocity<Radians, Seconds>>,
}

#[inject_nitrous_component]
impl BodyMotion {
    pub fn new_forward<UnitLength: LengthUnit, UnitTime: TimeUnit>(
        vehicle_forward_velocity: Velocity<UnitLength, UnitTime>,
    ) -> Self {
        Self {
            acceleration_m_s2: Vector3::new(
                meters_per_second2!(0f64),
                meters_per_second2!(0f64),
                meters_per_second2!(0f64),
            ),
            linear_velocity: Vector3::new(
                meters_per_second!(0f64),
                meters_per_second!(0f64),
                -meters_per_second!(vehicle_forward_velocity),
            ),
            angular_velocity: Vector3::new(
                radians_per_second!(0f64),
                radians_per_second!(0f64),
                radians_per_second!(0f64),
            ),
        }
    }

    pub fn velocity(&self) -> &Vector3<Velocity<Meters, Seconds>> {
        &self.linear_velocity
    }

    pub fn cg_velocity(&self) -> Velocity<Meters, Seconds> {
        meters_per_second!(self.linear_velocity.map(|v| v.f64()).magnitude())
    }

    pub fn acceleration_m_s2(&self) -> &Vector3<Acceleration<Meters, Seconds>> {
        &self.acceleration_m_s2
    }

    pub fn freeze(&mut self) {
        self.acceleration_m_s2 = Vector3::new(
            meters_per_second2!(0_f64),
            meters_per_second2!(0_f64),
            meters_per_second2!(0_f64),
        );
        self.linear_velocity = Vector3::new(
            meters_per_second!(0_f64),
            meters_per_second!(0_f64),
            meters_per_second!(0_f64),
        );
        self.angular_velocity = Vector3::new(
            radians_per_second!(0_f64),
            radians_per_second!(0_f64),
            radians_per_second!(0_f64),
        );
    }

    // vehicle fwd axis: X -> u, L -> p
    // This maps to -z
    pub fn vehicle_forward_acceleration(&self) -> Acceleration<Meters, Seconds> {
        -self.acceleration_m_s2.z
    }

    pub fn set_vehicle_forward_acceleration(&mut self, u_dot: Acceleration<Meters, Seconds>) {
        self.acceleration_m_s2.z = -u_dot;
    }

    pub fn vehicle_forward_velocity(&self) -> Velocity<Meters, Seconds> {
        -self.linear_velocity.z
    }

    pub fn set_vehicle_forward_velocity(&mut self, u: Velocity<Meters, Seconds>) {
        self.linear_velocity.z = -u;
    }

    pub fn vehicle_roll_velocity(&self) -> AngularVelocity<Radians, Seconds> {
        self.angular_velocity.z
    }

    // vehicle right axis: Y -> v, M -> q
    // this maps to +x
    pub fn vehicle_sideways_acceleration(&self) -> Acceleration<Meters, Seconds> {
        self.acceleration_m_s2.x
    }

    pub fn set_vehicle_sideways_acceleration(&mut self, v_dot: Acceleration<Meters, Seconds>) {
        self.acceleration_m_s2.x = v_dot;
    }

    pub fn vehicle_sideways_velocity(&self) -> Velocity<Meters, Seconds> {
        self.linear_velocity.x
    }

    pub fn set_vehicle_sideways_velocity(&mut self, v: Velocity<Meters, Seconds>) {
        self.linear_velocity.x = v;
    }

    pub fn vehicle_pitch_velocity(&self) -> AngularVelocity<Radians, Seconds> {
        self.angular_velocity.x
    }

    pub fn set_vehicle_pitch_velocity(&mut self, q: AngularVelocity<Radians, Seconds>) {
        self.angular_velocity.x = q;
    }

    // down  axis: Z, w, N, r
    // this maps to -y
    pub fn vehicle_vertical_acceleration(&self) -> Acceleration<Meters, Seconds> {
        -self.acceleration_m_s2.y
    }

    pub fn set_vehicle_vertical_acceleration(&mut self, w_dot: Acceleration<Meters, Seconds>) {
        self.acceleration_m_s2.y = -w_dot;
    }

    pub fn vehicle_vertical_velocity(&self) -> Velocity<Meters, Seconds> {
        -self.linear_velocity.y
    }

    pub fn set_vehicle_vertical_velocity(&mut self, w: Velocity<Meters, Seconds>) {
        self.linear_velocity.y = -w;
    }

    pub fn vehicle_yaw_velocity(&self) -> AngularVelocity<Radians, Seconds> {
        -self.angular_velocity.y
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
