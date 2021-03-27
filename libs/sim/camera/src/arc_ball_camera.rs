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
use crate::Camera;
use absolute_unit::{
    degrees, meters, radians, Angle, Degrees, Kilometers, Length, LengthUnit, Meters,
};
use anyhow::{ensure, Result};
use geodesy::{Cartesian, GeoCenter, GeoSurface, Graticule, Target};
use gpu::{Gpu, ResizeHint};
use nalgebra::{Unit as NUnit, UnitQuaternion, Vector3};
use nitrous::{Interpreter, Value};
use nitrous_injector::{inject_nitrous_module, method, NitrousModule};
use parking_lot::RwLock;
use std::{f64::consts::PI, sync::Arc};

#[derive(Debug, NitrousModule)]
pub struct ArcBallCamera {
    camera: Camera,

    in_rotate: bool,
    in_move: bool,
    fov_delta: Angle<Degrees>,
    target_height_delta: Length<Meters>,
    target: Graticule<GeoSurface>,
    eye: Graticule<Target>,
}

#[inject_nitrous_module]
impl ArcBallCamera {
    pub fn new(
        z_near: Length<Meters>,
        gpu: &mut Gpu,
        interpreter: &mut Interpreter,
    ) -> Arc<RwLock<Self>> {
        let arcball = Arc::new(RwLock::new(Self::detached(gpu.aspect_ratio(), z_near)));
        gpu.add_resize_observer(arcball.clone());
        interpreter.put_global("camera", Value::Module(arcball.clone()));
        arcball
    }

    pub fn detached(aspect_ratio: f64, z_near: Length<Meters>) -> Self {
        let fov_y = radians!(PI / 2f64);
        Self {
            camera: Camera::from_parameters(fov_y, aspect_ratio, z_near),
            target: Graticule::<GeoSurface>::new(radians!(0), radians!(0), meters!(0)),
            target_height_delta: meters!(0),
            eye: Graticule::<Target>::new(
                radians!(PI / 2.0),
                radians!(3f64 * PI / 4.0),
                meters!(1),
            ),
            fov_delta: degrees!(0),
            in_rotate: false,
            in_move: false,
        }
    }

    pub fn add_default_bindings(&mut self, interpreter: &mut Interpreter) -> Result<()> {
        interpreter.interpret_once(
            r#"
                let bindings := mapper.create_bindings("arc_ball_camera");
                bindings.bind("mouse1", "camera.pan_view(pressed)");
                bindings.bind("mouse3", "camera.move_view(pressed)");
                bindings.bind("mouseMotion", "camera.handle_mousemotion(dx, dy)");
                bindings.bind("mouseWheel", "camera.handle_mousewheel(vertical_delta)");
                bindings.bind("PageUp", "camera.increase_fov(pressed)");
                bindings.bind("PageDown", "camera.decrease_fov(pressed)");
                bindings.bind("Shift+Up", "camera.target_up_fast(pressed)");
                bindings.bind("Shift+Down", "camera.target_down_fast(pressed)");
                bindings.bind("Up", "camera.target_up(pressed)");
                bindings.bind("Down", "camera.target_down(pressed)");
                bindings.bind("Shift+LBracket", "camera.decrease_exposure(pressed)");
                bindings.bind("Shift+RBracket", "camera.increase_exposure(pressed)");
            "#,
        )?;
        Ok(())
    }

    pub fn camera(&self) -> &Camera {
        &self.camera
    }

    pub fn camera_mut(&mut self) -> &mut Camera {
        &mut self.camera
    }

    pub fn get_target(&self) -> Graticule<GeoSurface> {
        self.target
    }

    pub fn set_target(&mut self, target: Graticule<GeoSurface>) {
        self.target = target;
    }

    pub fn get_eye_relative(&self) -> Graticule<Target> {
        self.eye
    }

    pub fn set_eye_relative(&mut self, eye: Graticule<Target>) -> Result<()> {
        ensure!(
            eye.latitude < radians!(degrees!(90)),
            "eye coordinate past limits"
        );
        self.eye = eye;
        Ok(())
    }

    pub fn get_distance(&self) -> Length<Meters> {
        self.eye.distance
    }

    pub fn set_distance<Unit: LengthUnit>(&mut self, distance: Length<Unit>) {
        self.eye.distance = meters!(distance);
    }

    #[method]
    pub fn show_parameters(&self) -> String {
        let mut out = String::new();
        out += &format!("tgt lat: {}\n", self.target.latitude.f64());
        out += &format!("tgt lon: {}\n", self.target.longitude.f64());
        out += &format!("tgt dst: {}\n", self.target.distance.f64());
        out += &format!("eye lat: {}\n", self.eye.latitude.f64());
        out += &format!("eye lon: {}\n", self.eye.longitude.f64());
        out += &format!("eye dst: {}\n", self.eye.distance.f64());
        println!("{}", out);
        out
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
        self.in_rotate = pressed;
    }

    #[method]
    pub fn move_view(&mut self, pressed: bool) {
        self.in_move = pressed;
    }

    #[method]
    pub fn increase_fov(&mut self, pressed: bool) {
        self.fov_delta = degrees!(if pressed { 1 } else { 0 });
    }

    #[method]
    pub fn decrease_fov(&mut self, pressed: bool) {
        self.fov_delta = degrees!(if pressed { -1 } else { 0 });
    }

    #[method]
    pub fn increase_exposure(&mut self, pressed: bool) {
        if pressed {
            self.camera.increase_exposure();
        }
    }

    #[method]
    pub fn decrease_exposure(&mut self, pressed: bool) {
        if pressed {
            self.camera.decrease_exposure();
        }
    }

    #[method]
    pub fn handle_mousemotion(&mut self, x: f64, y: f64) {
        if self.in_rotate {
            self.eye.longitude -= degrees!(x * 0.5);

            self.eye.latitude += degrees!(y * 0.5f64);
            self.eye.latitude = self
                .eye
                .latitude
                .min(radians!(PI / 2.0 - 0.001))
                .max(radians!(-PI / 2.0 + 0.001));
        }

        if self.in_move {
            let sensitivity: f64 = f64::from(self.get_distance()) / 60_000_000.0;

            let dir = self.eye.longitude;
            let lat = f64::from(degrees!(self.target.latitude)) + dir.cos() * y * sensitivity;
            let lon = f64::from(degrees!(self.target.longitude)) + -dir.sin() * y * sensitivity;
            self.target.latitude = radians!(degrees!(lat));
            self.target.longitude = radians!(degrees!(lon));

            let dir = self.eye.longitude + degrees!(PI / 2.0);
            let lat = f64::from(degrees!(self.target.latitude)) + -dir.sin() * x * sensitivity;
            let lon = f64::from(degrees!(self.target.longitude)) + -dir.cos() * x * sensitivity;
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
        self.eye.distance *= if vertical > 0f64 { 1.1f64 } else { 0.9f64 };
        self.eye.distance = self.eye.distance.max(meters!(0.01));
    }

    #[method]
    pub fn target_up(&mut self, pressed: bool) {
        if pressed {
            self.target_height_delta = meters!(1);
        } else {
            self.target_height_delta = meters!(0);
        }
    }

    #[method]
    pub fn target_down(&mut self, pressed: bool) {
        if pressed {
            self.target_height_delta = meters!(-1);
        } else {
            self.target_height_delta = meters!(0);
        }
    }

    #[method]
    pub fn target_up_fast(&mut self, pressed: bool) {
        if pressed {
            self.target_height_delta = meters!(100);
        } else {
            self.target_height_delta = meters!(0);
        }
    }

    #[method]
    pub fn target_down_fast(&mut self, pressed: bool) {
        if pressed {
            self.target_height_delta = meters!(-100);
        } else {
            self.target_height_delta = meters!(0);
        }
    }

    pub fn think(&mut self) {
        let mut fov = degrees!(self.camera.fov_y());
        fov += self.fov_delta;
        fov = fov.min(degrees!(90)).max(degrees!(1));
        self.camera.set_fov_y(fov);

        self.target.distance += self.target_height_delta;
        if self.target.distance < meters!(0f64) {
            self.target.distance = meters!(0f64);
        }

        let target = self.cartesian_target_position::<Kilometers>();
        let eye = self.cartesian_eye_position::<Kilometers>();
        let forward = (target - eye).vec64();
        let right = eye.vec64().cross(&forward);
        let up = right.cross(&forward);
        self.camera.push_frame_parameters(
            Cartesian::new(
                meters!(eye.coords[0]),
                meters!(eye.coords[1]),
                meters!(eye.coords[2]),
            ),
            forward.normalize(),
            up.normalize(),
            right.normalize(),
        );
    }
}

impl ResizeHint for ArcBallCamera {
    fn note_resize(&mut self, gpu: &Gpu) -> Result<()> {
        self.camera.set_aspect_ratio(gpu.aspect_ratio());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use absolute_unit::{kilometers, Kilometers};
    use approx::assert_abs_diff_eq;
    use physical_constants::EARTH_RADIUS_KM;

    #[test]
    fn it_can_compute_eye_positions_at_origin() -> Result<()> {
        let mut c = ArcBallCamera::detached(1f64, meters!(0.1f64));

        // Verify base target position.
        let t = c.cartesian_target_position::<Kilometers>();
        assert_abs_diff_eq!(t.coords[0], kilometers!(0));
        assert_abs_diff_eq!(t.coords[1], kilometers!(0));
        assert_abs_diff_eq!(t.coords[2], kilometers!(EARTH_RADIUS_KM));

        // Target: 0/0; at latitude of 0:
        {
            // Longitude 0 maps to south, latitude 90 to up,
            // when rotated into the surface frame.
            c.set_eye_relative(Graticule::<Target>::new(
                degrees!(0),
                degrees!(0),
                meters!(1),
            ))?;
            let e = c.cartesian_eye_position::<Kilometers>();
            assert_abs_diff_eq!(e.coords[0], kilometers!(0));
            assert_abs_diff_eq!(e.coords[1], kilometers!(-0.001));
            assert_abs_diff_eq!(e.coords[2], kilometers!(EARTH_RADIUS_KM));

            c.set_eye_relative(Graticule::<Target>::new(
                degrees!(0),
                degrees!(90),
                meters!(1),
            ))?;
            let e = c.cartesian_eye_position::<Kilometers>();
            assert_abs_diff_eq!(e.coords[0], kilometers!(-0.001));
            assert_abs_diff_eq!(e.coords[1], kilometers!(0));
            assert_abs_diff_eq!(e.coords[2], kilometers!(EARTH_RADIUS_KM));

            c.set_eye_relative(Graticule::<Target>::new(
                degrees!(0),
                degrees!(-90),
                meters!(1),
            ))?;
            let e = c.cartesian_eye_position::<Kilometers>();
            assert_abs_diff_eq!(e.coords[0], kilometers!(0.001));
            assert_abs_diff_eq!(e.coords[1], kilometers!(0));
            assert_abs_diff_eq!(e.coords[2], kilometers!(EARTH_RADIUS_KM));

            c.set_eye_relative(Graticule::<Target>::new(
                degrees!(0),
                degrees!(-180),
                meters!(1),
            ))?;
            let e = c.cartesian_eye_position::<Kilometers>();
            assert_abs_diff_eq!(e.coords[0], kilometers!(0));
            assert_abs_diff_eq!(e.coords[1], kilometers!(0.001));
            assert_abs_diff_eq!(e.coords[2], kilometers!(EARTH_RADIUS_KM));
        }

        Ok(())
    }

    #[test]
    fn it_can_compute_eye_positions_with_offset_latitude() -> Result<()> {
        let mut c = ArcBallCamera::detached(1f64, meters!(0.1f64));

        // Verify base target position.
        let t = c.cartesian_target_position::<Kilometers>();
        assert_abs_diff_eq!(t.coords[0], kilometers!(0));
        assert_abs_diff_eq!(t.coords[1], kilometers!(0));
        assert_abs_diff_eq!(t.coords[2], kilometers!(EARTH_RADIUS_KM));

        // Target: 0/0; at latitude of 45
        {
            c.set_eye_relative(Graticule::<Target>::new(
                degrees!(45),
                degrees!(0),
                meters!(1),
            ))?;
            let e = c.cartesian_eye_position::<Kilometers>();
            assert_abs_diff_eq!(e.coords[0], kilometers!(0));
            assert_abs_diff_eq!(e.coords[1], kilometers!(-0.000_707_106_781));
            assert_abs_diff_eq!(
                e.coords[2],
                kilometers!(EARTH_RADIUS_KM + 0.000_707_106_781)
            );

            c.set_eye_relative(Graticule::<Target>::new(
                degrees!(45),
                degrees!(90),
                meters!(1),
            ))?;
            let e = c.cartesian_eye_position::<Kilometers>();
            assert_abs_diff_eq!(e.coords[0], kilometers!(-0.000_707_106_781));
            assert_abs_diff_eq!(e.coords[1], kilometers!(0));
            assert_abs_diff_eq!(
                e.coords[2],
                kilometers!(EARTH_RADIUS_KM + 0.000_707_106_781)
            );
        }

        Ok(())
    }

    #[test]
    fn it_can_compute_eye_positions_with_offset_longitude() -> Result<()> {
        let mut c = ArcBallCamera::detached(1f64, meters!(0.1f64));

        // Verify base target position.
        let t = c.cartesian_target_position::<Kilometers>();
        assert_abs_diff_eq!(t.coords[0], kilometers!(0));
        assert_abs_diff_eq!(t.coords[1], kilometers!(0));
        assert_abs_diff_eq!(t.coords[2], kilometers!(EARTH_RADIUS_KM));
        // Target: 0/90; at eye latitude of 0
        {
            c.set_target(Graticule::<GeoSurface>::new(
                degrees!(0),
                degrees!(90),
                meters!(0),
            ));

            c.set_eye_relative(Graticule::<Target>::new(
                degrees!(0),
                degrees!(0),
                kilometers!(1),
            ))?;
            let e = c.cartesian_eye_position::<Kilometers>();
            assert_abs_diff_eq!(e.coords[0], kilometers!(-EARTH_RADIUS_KM));
            assert_abs_diff_eq!(e.coords[1], kilometers!(-1));
            assert_abs_diff_eq!(e.coords[2], kilometers!(0));

            c.set_eye_relative(Graticule::<Target>::new(
                degrees!(0),
                degrees!(90),
                kilometers!(1),
            ))?;
            let e = c.cartesian_eye_position::<Kilometers>();
            assert_abs_diff_eq!(e.coords[0], kilometers!(-EARTH_RADIUS_KM));
            assert_abs_diff_eq!(e.coords[1], kilometers!(0));
            assert_abs_diff_eq!(e.coords[2], kilometers!(-1));
        }

        Ok(())
    }
}
