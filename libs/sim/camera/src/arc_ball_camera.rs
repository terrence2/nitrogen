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
use crate::camera_impl::CameraStep;
use absolute_unit::{
    degrees, meters, radians, scalar, Degrees, Length, LengthUnit, Meters, Radians,
};
use anyhow::{bail, ensure, Result};
use bevy_ecs::prelude::*;
use geodesy::{Cartesian, GeoCenter, GeoSurface, Graticule, Target};
use measure::WorldSpaceFrame;
use nalgebra::{Unit as NUnit, UnitQuaternion, Vector3};
use nitrous::{inject_nitrous_component, method, NitrousComponent};
use runtime::{Extension, Runtime};
use std::{f64::consts::PI, fmt::Write};

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum ArcBallStep {
    ApplyInput,
}

/// The ArcBall system will, if the "player" entity has an ArcBallController
/// will translate device events into relevant arcball updates on that player
/// and apply those updates on each frame.
pub struct ArcBallSystem;
impl Extension for ArcBallSystem {
    fn init(runtime: &mut Runtime) -> Result<()> {
        runtime.add_sim_system(
            ArcBallController::sys_apply_input
                .label(ArcBallStep::ApplyInput)
                .before(CameraStep::ApplyInput),
        );
        Ok(())
    }
}

#[derive(Debug)]
struct InputState {
    in_rotate: bool,
    in_move: bool,
    target_height_delta: Length<Meters>,
}

#[derive(Debug, Component, NitrousComponent)]
#[Name = "arcball"]
pub struct ArcBallController {
    input: InputState,

    target: Graticule<GeoSurface>,
    eye: Graticule<Target>,
}

impl Default for ArcBallController {
    fn default() -> Self {
        Self {
            input: InputState {
                target_height_delta: meters!(0),
                in_rotate: false,
                in_move: false,
            },
            target: Graticule::<GeoSurface>::new(radians!(0), radians!(0), meters!(10.)),
            eye: Graticule::<Target>::new(
                radians!(degrees!(10.)),
                radians!(degrees!(25.)),
                meters!(10.),
            ),
        }
    }
}

#[inject_nitrous_component]
impl ArcBallController {
    pub fn world_space_frame(&self) -> WorldSpaceFrame {
        let target = self.cartesian_target_position::<Meters>();
        let eye = self.cartesian_eye_position::<Meters>();
        let forward = (target - eye).vec64();
        WorldSpaceFrame::new(eye, forward)
    }

    #[method]
    pub fn notable_location(&self, name: &str) -> Result<Graticule<GeoSurface>> {
        Ok(match name {
            "ISS" => Graticule::<GeoSurface>::new(
                degrees!(27.9880704),
                degrees!(-86.9245623),
                meters!(408_000.),
            ),
            "Everest" => Graticule::<GeoSurface>::new(
                degrees!(27.9880704),
                degrees!(-86.9245623),
                meters!(8000.),
            ),
            "London" => Graticule::<GeoSurface>::new(degrees!(51.5), degrees!(-0.1), meters!(100.)),
            _ => bail!("unknown notable location: {}", name),
        })
    }

    #[method]
    pub fn eye_for(&self, kind: &str) -> Graticule<Target> {
        match kind {
            "ISS" => Graticule::<Target>::new(degrees!(58), degrees!(308.0), meters!(1_308.)),
            "Everest" => Graticule::<Target>::new(degrees!(9), degrees!(130), meters!(12_000.)),
            _ => Graticule::<Target>::new(degrees!(11.5), degrees!(149.5), meters!(67_668.)),
        }
    }

    #[method]
    pub fn target(&self) -> Graticule<GeoSurface> {
        self.target
    }

    #[method]
    pub fn set_target(&mut self, target: Graticule<GeoSurface>) {
        assert!(target.latitude.is_finite());
        self.target = target;
    }

    #[method]
    pub fn eye(&self) -> Graticule<Target> {
        self.eye
    }

    #[method]
    pub fn set_eye(&mut self, eye: Graticule<Target>) -> Result<()> {
        ensure!(
            eye.latitude < radians!(degrees!(90)),
            "eye coordinate past limits"
        );
        self.eye = eye;
        Ok(())
    }

    pub fn distance(&self) -> Length<Meters> {
        self.eye.distance
    }

    pub fn set_distance<Unit: LengthUnit>(&mut self, distance: Length<Unit>) {
        self.eye.distance = meters!(distance);
    }

    #[method]
    pub fn show_parameters(&self) -> Result<String> {
        let mut out = String::new();
        writeln!(
            out,
            "arcball.set_target_latitude_degrees({});",
            self.target_latitude_degrees()
        )?;
        writeln!(
            out,
            "arcball.set_target_longitude_degrees({});",
            self.target_longitude_degrees()
        )?;
        writeln!(
            out,
            "arcball.set_target_height_meters({});",
            self.target_height_meters()
        )?;
        writeln!(
            out,
            "arcball.set_eye_latitude_degrees({});",
            self.eye_latitude_degrees()
        )?;
        writeln!(
            out,
            "arcball.set_eye_longitude_degrees({});",
            self.eye_longitude_degrees()
        )?;
        writeln!(
            out,
            "arcball.set_eye_distance_meters({});",
            self.eye_distance_meters()
        )?;
        println!("{}", out);
        Ok(out)
    }

    #[method]
    pub fn target_latitude_degrees(&self) -> f64 {
        self.target.lat::<Degrees>().f64()
    }

    #[method]
    pub fn target_longitude_degrees(&self) -> f64 {
        self.target.lon::<Degrees>().f64()
    }

    #[method]
    pub fn target_latitude_radians(&self) -> f64 {
        self.target.lat::<Radians>().f64()
    }

    #[method]
    pub fn target_longitude_radians(&self) -> f64 {
        self.target.lon::<Radians>().f64()
    }

    #[method]
    pub fn target_height_meters(&self) -> f64 {
        meters!(self.target.distance).f64()
    }

    #[method]
    pub fn set_target_latitude_degrees(&mut self, v: f64) {
        self.target.latitude = radians!(degrees!(v));
    }

    #[method]
    pub fn set_target_longitude_degrees(&mut self, v: f64) {
        self.target.longitude = radians!(degrees!(v));
    }

    #[method]
    pub fn set_target_latitude_radians(&mut self, v: f64) {
        self.target.latitude = radians!(v);
    }

    #[method]
    pub fn set_target_longitude_radians(&mut self, v: f64) {
        self.target.longitude = radians!(v);
    }

    #[method]
    pub fn set_target_height_meters(&mut self, v: f64) {
        self.target.distance = meters!(v);
    }

    #[method]
    pub fn eye_latitude_degrees(&self) -> f64 {
        self.eye.lat::<Degrees>().f64()
    }

    #[method]
    pub fn eye_longitude_degrees(&self) -> f64 {
        self.eye.lon::<Degrees>().f64()
    }

    #[method]
    pub fn eye_latitude_radians(&self) -> f64 {
        self.eye.lat::<Radians>().f64()
    }

    #[method]
    pub fn eye_longitude_radians(&self) -> f64 {
        self.eye.lon::<Radians>().f64()
    }

    #[method]
    pub fn eye_distance_meters(&self) -> f64 {
        meters!(self.eye.distance).f64()
    }

    #[method]
    pub fn set_eye_latitude_degrees(&mut self, v: f64) {
        self.eye.latitude = radians!(degrees!(v));
    }

    #[method]
    pub fn set_eye_longitude_degrees(&mut self, v: f64) {
        self.eye.longitude = radians!(degrees!(v));
    }

    #[method]
    pub fn set_eye_latitude_radians(&mut self, v: f64) {
        self.eye.latitude = radians!(v);
    }

    #[method]
    pub fn set_eye_longitude_radians(&mut self, v: f64) {
        self.eye.longitude = radians!(v);
    }

    #[method]
    pub fn set_eye_distance_meters(&mut self, v: f64) {
        self.eye.distance = meters!(v);
    }

    fn cartesian_target_position<Unit: LengthUnit>(&self) -> Cartesian<GeoCenter, Unit> {
        Cartesian::<GeoCenter, Unit>::from(Graticule::<GeoCenter>::from(self.target))
    }

    fn cartesian_eye_position<Unit: LengthUnit>(&self) -> Cartesian<GeoCenter, Unit> {
        let r_lon = UnitQuaternion::from_axis_angle(
            &NUnit::new_unchecked(Vector3::new(0f64, 1f64, 0f64)),
            -f64::from(self.target.longitude),
        );
        let r_lat = UnitQuaternion::from_axis_angle(
            &NUnit::new_normalize(r_lon * Vector3::new(1f64, 0f64, 0f64)),
            PI / 2.0 - f64::from(self.target.latitude),
        );
        let cart_target = self.cartesian_target_position::<Unit>();
        let cart_eye_rel_target_flat = Cartesian::<Target, Unit>::from(self.eye);
        let cart_eye_rel_target_framed =
            Cartesian::<Target, Unit>::from(r_lat * r_lon * cart_eye_rel_target_flat.vec64());
        cart_target + cart_eye_rel_target_framed
    }

    #[method]
    pub fn pan_view(&mut self, pressed: bool) {
        self.input.in_rotate = pressed;
    }

    #[method]
    pub fn move_view(&mut self, pressed: bool) {
        self.input.in_move = pressed;
    }

    #[method]
    pub fn handle_mousemotion(&mut self, x: f64, y: f64) {
        if self.input.in_rotate {
            self.eye.longitude -= degrees!(x * 0.5);

            self.eye.latitude += degrees!(y * 0.5f64);
            self.eye.latitude = self
                .eye
                .latitude
                .clamp(radians!(-PI / 2.0 + 0.001), radians!(PI / 2.0 - 0.001));
        }

        if self.input.in_move {
            let sensitivity: f64 = f64::from(self.distance()) / 60_000_000.0;

            let dir = self.eye.longitude;
            let lat = f64::from(degrees!(self.target.latitude))
                + (dir.cos() * scalar!(y * sensitivity)).into_inner();
            let lon = f64::from(degrees!(self.target.longitude))
                + (-dir.sin() * scalar!(y * sensitivity)).into_inner();
            self.target.latitude = radians!(degrees!(lat));
            self.target.longitude = radians!(degrees!(lon));

            let dir = self.eye.longitude + degrees!(PI / 2.0);
            let lat = f64::from(degrees!(self.target.latitude))
                + (-dir.sin() * scalar!(x * sensitivity)).into_inner();
            let lon = f64::from(degrees!(self.target.longitude))
                + (-dir.cos() * scalar!(x * sensitivity)).into_inner();
            self.target.latitude = radians!(degrees!(lat));
            self.target.longitude = radians!(degrees!(lon));
        }
    }

    #[method]
    pub fn handle_mousewheel(&mut self, vertical: f64) {
        // up/down is y
        //   Up is negative
        //   Down is positive
        //   Works in steps of 15 for my mouse.
        self.eye.distance *= scalar!(if vertical > 0f64 { 1.1f64 } else { 0.9f64 });
        self.eye.distance = self.eye.distance.max(meters!(0.01));
    }

    #[method]
    pub fn target_up(&mut self, pressed: bool) {
        if pressed {
            self.input.target_height_delta = meters!(1);
        } else {
            self.input.target_height_delta = meters!(0);
        }
    }

    #[method]
    pub fn target_down(&mut self, pressed: bool) {
        if pressed {
            self.input.target_height_delta = meters!(-1);
        } else {
            self.input.target_height_delta = meters!(0);
        }
    }

    #[method]
    pub fn target_up_fast(&mut self, pressed: bool) {
        if pressed {
            self.input.target_height_delta = meters!(100);
        } else {
            self.input.target_height_delta = meters!(0);
        }
    }

    #[method]
    pub fn target_down_fast(&mut self, pressed: bool) {
        if pressed {
            self.input.target_height_delta = meters!(-100);
        } else {
            self.input.target_height_delta = meters!(0);
        }
    }

    // Take the inputs applied via interpreting key presses in the prior stage and apply it.
    fn sys_apply_input(mut query: Query<(&mut ArcBallController, &mut WorldSpaceFrame)>) {
        for (mut arcball, mut frame) in query.iter_mut() {
            arcball.apply_input_state();
            *frame = arcball.world_space_frame();
        }
    }

    fn apply_input_state(&mut self) {
        self.target.distance += self.input.target_height_delta;
        if self.target.distance < meters!(0f64) {
            self.target.distance = meters!(0f64);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use absolute_unit::{kilometers, Kilometers};
    use approx::assert_abs_diff_eq;
    use physical_constants::EARTH_RADIUS;

    #[test]
    fn it_can_compute_eye_positions_at_origin() -> Result<()> {
        let mut c = ArcBallController::default();
        c.set_eye(Graticule::new(radians!(0), radians!(0), meters!(0)))?;
        c.set_target(Graticule::new(radians!(0), radians!(0), meters!(0)));

        // Verify base target position.
        let t = c.cartesian_target_position::<Kilometers>();
        assert_abs_diff_eq!(t.coords[0], kilometers!(0));
        assert_abs_diff_eq!(t.coords[1], kilometers!(0));
        assert_abs_diff_eq!(t.coords[2], kilometers!(*EARTH_RADIUS));

        // Target: 0/0; at latitude of 0:
        {
            // Longitude 0 maps to south, latitude 90 to up,
            // when rotated into the surface frame.
            c.set_eye(Graticule::<Target>::new(
                degrees!(0),
                degrees!(0),
                meters!(1),
            ))?;
            let e = c.cartesian_eye_position::<Kilometers>();
            assert_abs_diff_eq!(e.coords[0], kilometers!(0));
            assert_abs_diff_eq!(e.coords[1], kilometers!(-0.001));
            assert_abs_diff_eq!(e.coords[2], kilometers!(*EARTH_RADIUS));

            c.set_eye(Graticule::<Target>::new(
                degrees!(0),
                degrees!(90),
                meters!(1),
            ))?;
            let e = c.cartesian_eye_position::<Kilometers>();
            assert_abs_diff_eq!(e.coords[0], kilometers!(-0.001));
            assert_abs_diff_eq!(e.coords[1], kilometers!(0));
            assert_abs_diff_eq!(e.coords[2], kilometers!(*EARTH_RADIUS));

            c.set_eye(Graticule::<Target>::new(
                degrees!(0),
                degrees!(-90),
                meters!(1),
            ))?;
            let e = c.cartesian_eye_position::<Kilometers>();
            assert_abs_diff_eq!(e.coords[0], kilometers!(0.001));
            assert_abs_diff_eq!(e.coords[1], kilometers!(0));
            assert_abs_diff_eq!(e.coords[2], kilometers!(*EARTH_RADIUS));

            c.set_eye(Graticule::<Target>::new(
                degrees!(0),
                degrees!(-180),
                meters!(1),
            ))?;
            let e = c.cartesian_eye_position::<Kilometers>();
            assert_abs_diff_eq!(e.coords[0], kilometers!(0));
            assert_abs_diff_eq!(e.coords[1], kilometers!(0.001));
            assert_abs_diff_eq!(e.coords[2], kilometers!(*EARTH_RADIUS));
        }

        Ok(())
    }

    #[test]
    fn it_can_compute_eye_positions_with_offset_latitude() -> Result<()> {
        let mut c = ArcBallController::default();
        c.set_eye(Graticule::new(radians!(0), radians!(0), meters!(0)))?;
        c.set_target(Graticule::new(radians!(0), radians!(0), meters!(0)));

        // Verify base target position.
        let t = c.cartesian_target_position::<Kilometers>();
        assert_abs_diff_eq!(t.coords[0], kilometers!(0));
        assert_abs_diff_eq!(t.coords[1], kilometers!(0));
        assert_abs_diff_eq!(t.coords[2], kilometers!(*EARTH_RADIUS));

        // Target: 0/0; at latitude of 45
        {
            c.set_eye(Graticule::<Target>::new(
                degrees!(45),
                degrees!(0),
                meters!(1),
            ))?;
            let e = c.cartesian_eye_position::<Kilometers>();
            assert_abs_diff_eq!(e.coords[0], kilometers!(0));
            assert_abs_diff_eq!(
                e.coords[1],
                kilometers!(-0.000_707_106_781),
                epsilon = 0.000_000_000_001
            );
            assert_abs_diff_eq!(
                e.coords[2],
                kilometers!(*EARTH_RADIUS) + kilometers!(0.000_707_106_781)
            );

            c.set_eye(Graticule::<Target>::new(
                degrees!(45),
                degrees!(90),
                meters!(1),
            ))?;
            let e = c.cartesian_eye_position::<Kilometers>();
            assert_abs_diff_eq!(
                e.coords[0],
                kilometers!(-0.000_707_106_781),
                epsilon = 0.000_000_000_001
            );
            assert_abs_diff_eq!(e.coords[1], kilometers!(0));
            assert_abs_diff_eq!(
                e.coords[2],
                kilometers!(*EARTH_RADIUS) + kilometers!(0.000_707_106_781),
                epsilon = 0.000_000_000_001
            );
        }

        Ok(())
    }

    #[test]
    fn it_can_compute_eye_positions_with_offset_longitude() -> Result<()> {
        let mut c = ArcBallController::default();
        c.set_eye(Graticule::new(radians!(0), radians!(0), meters!(0)))?;
        c.set_target(Graticule::new(radians!(0), radians!(0), meters!(0)));

        // Verify base target position.
        let t = c.cartesian_target_position::<Kilometers>();
        assert_abs_diff_eq!(t.coords[0], kilometers!(0));
        assert_abs_diff_eq!(t.coords[1], kilometers!(0));
        assert_abs_diff_eq!(t.coords[2], kilometers!(*EARTH_RADIUS));
        // Target: 0/90; at eye latitude of 0
        {
            c.set_target(Graticule::<GeoSurface>::new(
                degrees!(0),
                degrees!(90),
                meters!(0),
            ));

            c.set_eye(Graticule::<Target>::new(
                degrees!(0),
                degrees!(0),
                kilometers!(1),
            ))?;
            let e = c.cartesian_eye_position::<Kilometers>();
            assert_abs_diff_eq!(e.coords[0], kilometers!(-*EARTH_RADIUS));
            assert_abs_diff_eq!(e.coords[1], kilometers!(-1));
            assert_abs_diff_eq!(e.coords[2], kilometers!(0), epsilon = 0.000_000_000_001);

            c.set_eye(Graticule::<Target>::new(
                degrees!(0),
                degrees!(90),
                kilometers!(1),
            ))?;
            let e = c.cartesian_eye_position::<Kilometers>();
            assert_abs_diff_eq!(
                e.coords[0],
                kilometers!(-*EARTH_RADIUS),
                epsilon = 0.000_000_000_001
            );
            assert_abs_diff_eq!(e.coords[1], kilometers!(0), epsilon = 0.000_000_000_001);
            assert_abs_diff_eq!(e.coords[2], kilometers!(-1), epsilon = 0.000_000_000_001);
        }

        Ok(())
    }
}
