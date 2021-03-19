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
use anyhow::Result;
use nalgebra::{
    Isometry3, Matrix4, Perspective3, Point3, Similarity3, Translation3, Unit, UnitQuaternion,
    Vector3,
};
use nitrous::{Interpreter, Value};
use nitrous_injector::{inject_nitrous_module, method, NitrousModule};
use parking_lot::RwLock;
use std::{f64::consts::PI, sync::Arc};

#[derive(Debug, NitrousModule)]
pub struct UfoCamera {
    position: Translation3<f64>,
    rotation: UnitQuaternion<f64>,
    fov_y: f64,
    aspect_ratio: f64,
    projection: Perspective3<f64>,
    z_near: f64,
    z_far: f64,

    pub speed: f64,
    pub sensitivity: f64,
    move_vector: Vector3<f64>,
    rot_vector: Vector3<f64>,
}

#[inject_nitrous_module]
impl UfoCamera {
    pub fn new(aspect_ratio: f64, z_near: f64, z_far: f64) -> Self {
        Self {
            position: Translation3::new(0f64, 0f64, 0f64),
            rotation: UnitQuaternion::from_axis_angle(
                &Unit::new_normalize(Vector3::new(0f64, -1f64, 0f64)),
                0f64,
            ),
            fov_y: PI / 2f64,
            aspect_ratio,
            projection: Perspective3::new(1f64 / aspect_ratio, PI / 2f64, z_near, z_far),
            z_near,
            z_far,
            speed: 1.0,
            sensitivity: 0.2,
            move_vector: nalgebra::zero(),
            rot_vector: nalgebra::zero(),
        }
    }

    pub fn init(self, interpreter: Arc<RwLock<Interpreter>>) -> Result<Arc<RwLock<Self>>> {
        let ufo = Arc::new(RwLock::new(self));
        interpreter
            .write()
            .put_global("camera", Value::Module(ufo.clone()));
        Ok(ufo)
    }

    pub fn with_default_bindings(self, interpreter: Arc<RwLock<Interpreter>>) -> Result<Self> {
        interpreter.write().interpret_once(
            r#"
                let bindings := mapper.create_bindings("arc_ball_camera");
                bindings.bind("mouseMotion", "camera.on_mousemove(dx, dy)");
                bindings.bind("mouseWheel", "camera.on_mousewheel(delta_vertical)");
                bindings.bind("Equals", "camera.zoom_in(pressed)");
                bindings.bind("Subtract", "camera.zoom_out(pressed)");
                bindings.bind("c", "camera.rotate_right(pressed)");
                bindings.bind("z", "camera.rotate_left(pressed)");
                bindings.bind("a", "camera.move_left(pressed)");
                bindings.bind("d", "camera.move_right(pressed)");
                bindings.bind("w", "camera.move_forward(pressed)");
                bindings.bind("s", "camera.move_backward(pressed)");
                bindings.bind("space", "camera.move_up(pressed)");
                bindings.bind("Control", "camera.move_down(pressed)");
            "#,
        )?;
        Ok(self)
    }

    pub fn set_position(&mut self, x: f64, y: f64, z: f64) {
        self.position = Translation3::new(x, y, z);
    }

    pub fn eye(&self) -> Point3<f64> {
        self.position.transform_point(&Point3::new(0.0, 0.0, 0.0))
    }

    pub fn up(&self) -> Vector3<f64> {
        self.rotation * Vector3::new(0.0, -1.0, 0.0)
    }

    pub fn set_rotation(&mut self, v: &Vector3<f64>, ang: f64) {
        self.rotation = UnitQuaternion::from_axis_angle(&Unit::new_normalize(*v), ang);
    }

    pub fn apply_rotation(&mut self, v: &Vector3<f64>, ang: f64) {
        let quat = UnitQuaternion::from_axis_angle(&Unit::new_normalize(*v), ang);
        self.rotation *= quat;
    }

    pub fn set_aspect_ratio(&mut self, aspect_ratio: f64) {
        self.aspect_ratio = aspect_ratio;
        self.projection =
            Perspective3::new(1f64 / aspect_ratio, self.fov_y, self.z_near, self.z_far)
    }

    pub fn target(&self) -> Point3<f64> {
        let forward = self.rotation * Vector3::new(0.0, 0.0, 1.0);
        self.position.transform_point(&Point3::from(forward))
    }

    #[method]
    pub fn zoom_in(&mut self, pressed: bool) {
        if pressed {
            self.fov_y -= 5.0 * PI / 180.0;
            self.fov_y = self.fov_y.min(10.0 * PI / 180.0);
            self.projection = Perspective3::new(
                1f64 / self.aspect_ratio,
                self.fov_y,
                self.z_near,
                self.z_far,
            )
        }
    }

    #[method]
    pub fn zoom_out(&mut self, pressed: bool) {
        if pressed {
            self.fov_y += 5.0 * PI / 180.0;
            self.fov_y = self.fov_y.max(90.0 * PI / 180.0);
            self.projection = Perspective3::new(
                1f64 / self.aspect_ratio,
                self.fov_y,
                self.z_near,
                self.z_far,
            )
        }
    }

    pub fn think(&mut self) {
        let forward = self.rotation * Vector3::new(0.0, 0.0, 1.0);
        let right = self.rotation * Vector3::new(1.0, 0.0, 0.0);
        let up = self.rotation * Vector3::new(0.0, -1.0, 0.0);

        let pitch_rot = UnitQuaternion::from_axis_angle(
            &Unit::new_unchecked(right),
            self.rot_vector.y * self.sensitivity * PI / 180.0,
        );
        let yaw_rot = UnitQuaternion::from_axis_angle(
            &Unit::new_unchecked(up),
            self.rot_vector.x * self.sensitivity * PI / 180.0,
        );
        let roll_rot = UnitQuaternion::from_axis_angle(
            &Unit::new_unchecked(forward),
            self.rot_vector.z / 50.0,
        );
        self.rot_vector.x = 0.0;
        self.rot_vector.y = 0.0;

        self.rotation = yaw_rot * self.rotation;
        self.rotation = pitch_rot * self.rotation;
        self.rotation = roll_rot * self.rotation;

        if self.move_vector.norm_squared() > 0.0 {
            let mv = (self.rotation * self.move_vector.normalize()) * self.speed;
            self.position.x += mv.x;
            self.position.y += mv.y;
            self.position.z += mv.z;
        }
    }

    #[method]
    pub fn on_mousemove(&mut self, x: f64, y: f64) {
        self.rot_vector.x = x;
        self.rot_vector.y = y;
    }

    #[method]
    pub fn on_mousewheel(&mut self, delta_vertical: f64) {
        if delta_vertical > 0.0 {
            self.speed *= 0.8;
        } else {
            self.speed *= 1.2;
        }
    }

    #[method]
    pub fn rotate_right(&mut self, pressed: bool) {
        if pressed {
            self.rot_vector.z = 1.0;
        } else {
            self.rot_vector.z = 0.0;
        }
    }

    #[method]
    pub fn rotate_left(&mut self, pressed: bool) {
        if pressed {
            self.rot_vector.z = -1.0;
        } else {
            self.rot_vector.z = 0.0;
        }
    }

    #[method]
    pub fn move_up(&mut self, pressed: bool) {
        if pressed {
            self.move_vector.y = -1f64;
        } else {
            self.move_vector.y = 0f64;
        }
    }

    #[method]
    pub fn move_down(&mut self, pressed: bool) {
        if pressed {
            self.move_vector.y = 1f64;
        } else {
            self.move_vector.y = 0f64;
        }
    }

    #[method]
    pub fn move_right(&mut self, pressed: bool) {
        if pressed {
            self.move_vector.x = 1f64;
        } else {
            self.move_vector.x = 0f64;
        }
    }

    #[method]
    pub fn move_left(&mut self, pressed: bool) {
        if pressed {
            self.move_vector.x = -1f64;
        } else {
            self.move_vector.x = 0f64;
        }
    }

    #[method]
    pub fn move_forward(&mut self, pressed: bool) {
        // n.b. -1 points forward
        if pressed {
            self.move_vector.z = -1f64;
        } else {
            self.move_vector.z = 0f64;
        }
    }

    #[method]
    pub fn move_backward(&mut self, pressed: bool) {
        if pressed {
            self.move_vector.z = 1f64;
        } else {
            self.move_vector.z = 0f64;
        }
    }

    pub fn view(&self) -> Isometry3<f32> {
        // FIXME:
        Isometry3::identity()
    }

    pub fn projection(&self) -> Perspective3<f64> {
        self.projection
    }

    pub fn view_matrix(&self) -> Matrix4<f32> {
        let simi = Similarity3::from_parts(self.position, self.rotation, 1.0);
        nalgebra::convert(simi.inverse().to_homogeneous())
    }

    pub fn projection_matrix(&self) -> Matrix4<f32> {
        nalgebra::convert(*self.projection.as_matrix())
    }

    pub fn inverted_projection_matrix(&self) -> Matrix4<f32> {
        nalgebra::convert(self.projection.inverse())
    }

    pub fn inverted_view_matrix(&self) -> Matrix4<f32> {
        let simi = Similarity3::from_parts(self.position, self.rotation, 1.0);
        nalgebra::convert(simi.to_homogeneous())
    }

    pub fn position(&self) -> Point3<f32> {
        let down: Translation3<f32> = nalgebra::convert(self.position);
        Point3::new(down.vector[0], down.vector[1], down.vector[2])
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_move() {
        let mut camera = UfoCamera::new(1.0, 1.0, 10.0);
        camera.move_right(true);
        camera.think();
        assert_relative_eq!(camera.position.x, camera.speed);
        camera.move_right(false);
        camera.move_left(true);
        camera.think();
        camera.think();
        assert_relative_eq!(camera.position.x, -camera.speed);
        assert_relative_eq!(camera.position.y, 0.0);
        assert_relative_eq!(camera.position.z, 0.0);
    }
}
