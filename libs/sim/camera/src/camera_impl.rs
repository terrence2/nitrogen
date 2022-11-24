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
    degrees, meters, radians, Angle, AngleUnit, Degrees, Kilometers, Length, LengthUnit, Meters,
    Radians,
};
use anyhow::Result;
use bevy_ecs::prelude::*;
use geodesy::{Cartesian, GeoCenter};
use geometry::Plane;
use measure::WorldSpaceFrame;
use nalgebra::{Isometry3, Matrix4, Perspective3, Point3, UnitQuaternion, Vector3};
use nitrous::{
    inject_nitrous_component, inject_nitrous_resource, method, NitrousComponent, NitrousResource,
};
use runtime::{Extension, Runtime};
use std::f64::consts::PI;
use window::{DisplayConfig, Window, WindowStep};

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum CameraStep {
    ApplyInput,
    HandleDisplayChange,
}

pub struct CameraSystem;
impl Extension for CameraSystem {
    fn init(runtime: &mut Runtime) -> Result<()> {
        runtime.insert_named_resource(
            "camera",
            ScreenCamera::new(
                radians!(PI / 2.0),
                runtime.resource::<Window>().render_aspect_ratio(),
                meters!(0.5),
            ),
        );
        // Input happens in EventMapper in Input stage. This queues up scripts to run
        // as part of the Script stage. This stage mutates InputState here and elsewhere.
        // We then process this input in the Simulation stage so that input gets shown
        // in the first frame after it happens. Anything that touches the WorldSpaceFrame
        // of the screen camera entity should be installed before CameraStep::ApplyInput.
        runtime.add_sim_system(ScreenCamera::sys_apply_input.label(CameraStep::ApplyInput));
        runtime.add_frame_system(
            ScreenCamera::sys_apply_display_changes
                .label(CameraStep::HandleDisplayChange)
                .after(WindowStep::HandleEvents),
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
struct InputState {
    fov_delta: Angle<Degrees>,
}

#[derive(Clone, Debug, Component, NitrousComponent)]
#[Name = "camera"]
pub struct HudCamera {
    _camera: ScreenCamera,
}

#[inject_nitrous_component]
impl HudCamera {
    // FIXME: passthrough, maybe? Implement Hud Camera support.
}

#[derive(Clone, Debug, Default, Component)]
pub struct ScreenCameraController;

#[derive(Clone, Debug, NitrousResource)]
pub struct ScreenCamera {
    // Camera parameters
    fov_y: Angle<Radians>,
    aspect_ratio: f64,
    z_near: Length<Meters>,

    #[property]
    exposure: f64,

    input: InputState,

    // Camera view state.
    position: Cartesian<GeoCenter, Meters>,
    forward: Vector3<f64>,
    up: Vector3<f64>,
    right: Vector3<f64>,
}

#[inject_nitrous_resource]
impl ScreenCamera {
    const INITIAL_EXPOSURE: f64 = 10e-5;

    // FIXME: aspect ratio is wrong. Should be 16:9 and not 9:16.
    // aspect ratio is rise over run: h / w
    pub fn new<AngUnit: AngleUnit>(
        fov_y: Angle<AngUnit>,
        aspect_ratio: f64,
        z_near: Length<Meters>,
    ) -> Self {
        Self {
            fov_y: radians!(fov_y),
            aspect_ratio,
            z_near,
            exposure: Self::INITIAL_EXPOSURE,

            input: InputState {
                fov_delta: degrees!(0),
            },

            position: Vector3::new(0f64, 0f64, 0f64).into(),
            forward: Vector3::new(0f64, 0f64, -1f64),
            up: Vector3::new(0f64, 1f64, 0f64),
            right: Vector3::new(1f64, 0f64, 0f64),
        }
    }

    pub fn on_display_config_updated(&mut self, config: &DisplayConfig) {
        self.set_aspect_ratio(config.render_aspect_ratio());
    }

    #[method]
    pub fn increase_fov(&mut self, pressed: bool) {
        self.input.fov_delta = degrees!(i32::from(pressed));
    }

    #[method]
    pub fn decrease_fov(&mut self, pressed: bool) {
        self.input.fov_delta = degrees!(-i32::from(pressed));
    }

    #[method]
    pub fn increase_exposure(&mut self) {
        self.exposure *= 1.1;
    }

    #[method]
    pub fn decrease_exposure(&mut self) {
        self.exposure /= 1.1;
    }

    pub fn exposure(&self) -> f32 {
        self.exposure as f32
    }

    pub fn fov_y(&self) -> Angle<Radians> {
        self.fov_y
    }

    pub fn set_fov_y<T: AngleUnit>(&mut self, fov: Angle<T>) {
        self.fov_y = radians!(fov);
    }

    pub fn z_near<Unit: LengthUnit>(&self) -> Length<Unit> {
        Length::<Unit>::from(&self.z_near)
    }

    pub fn aspect_ratio(&self) -> f64 {
        self.aspect_ratio
    }

    pub fn set_aspect_ratio(&mut self, aspect_ratio: f64) {
        self.aspect_ratio = aspect_ratio;
    }

    pub fn position<T: LengthUnit>(&self) -> Cartesian<GeoCenter, T> {
        Cartesian::<GeoCenter, T>::new(
            self.position.coords[0],
            self.position.coords[1],
            self.position.coords[2],
        )
    }

    pub fn forward(&self) -> &Vector3<f64> {
        &self.forward
    }

    pub fn up(&self) -> &Vector3<f64> {
        &self.up
    }

    pub fn right(&self) -> &Vector3<f64> {
        &self.right
    }

    pub fn perspective<T: LengthUnit>(&self) -> Perspective3<f64> {
        // Source: https://nlguillemot.wordpress.com/2016/12/07/reversed-z-in-opengl/
        // See also: https://outerra.blogspot.com/2012/11/maximizing-depth-buffer-range-and.html
        // Infinite depth perspective with flipped w so that we can use inverted depths.
        // float f = 1.0f / tan(fovY_radians / 2.0f);
        // return glm::mat4(
        //     f / WbyH, 0.0f,  0.0f,  0.0f,
        //     0.0f,        f,  0.0f,  0.0f,
        //     0.0f,     0.0f,  0.0f, -1.0f,
        //     0.0f,     0.0f, zNear,  0.0f);

        // TL;DR is that we set the Z in clip space to zNear instead of -1 (and write z
        // into the w coordinate, like always). When we do the perspective divide by w, this
        // inverts the z _and_ changes the scaling.

        // Note for inverting the transform on the GPU:
        // z = -1
        // w = z*zNear
        // z' = -1 / (z / zNear)
        // z = -1 / (z' / zNear)

        let mut matrix: Matrix4<f64> = num::Zero::zero();
        let f = 1.0 / (self.fov_y.f64() / 2.0).tan();
        let fp = f / self.aspect_ratio; // aspect is h/w, so invert
        matrix[(0, 0)] = f;
        matrix[(1, 1)] = fp;
        matrix[(3, 2)] = -1.0;
        matrix[(2, 3)] = Length::<T>::from(&self.z_near).into();
        Perspective3::from_matrix_unchecked(matrix)
    }

    pub fn view<T: LengthUnit>(&self) -> Isometry3<f64> {
        let eye = self.position::<T>().vec64();
        Isometry3::look_at_rh(
            &Point3::from(eye),
            &Point3::from(eye + self.forward()),
            &-self.up(),
        )
    }

    pub fn look_at_rh<T: LengthUnit>(&self) -> UnitQuaternion<f64> {
        UnitQuaternion::look_at_rh(self.forward(), &-self.up())
    }

    pub fn world_space_frustum<T: LengthUnit>(&self) -> [Plane; 5] {
        // Taken from this paper:
        //   https://www.gamedevs.org/uploads/fast-extraction-viewing-frustum-planes-from-world-view-projection-matrix.pdf

        // FIXME: must be kilometers?
        let eye = Cartesian::<GeoCenter, Kilometers>::new(
            self.position.coords[0],
            self.position.coords[1],
            self.position.coords[2],
        )
        .vec64();
        let view = Isometry3::look_at_rh(
            &Point3::from(eye),
            &Point3::from(eye + self.forward),
            // FIXME: is this upside down?
            &self.up,
        );

        let m = self.perspective::<T>().as_matrix() * view.to_homogeneous();

        let lp = (m.row(3) + m.row(0)).transpose();
        let lm = lp.xyz().magnitude();
        let left = Plane::from_normal_and_distance(lp.xyz() / lm, -lp[3] / lm);

        let rp = (m.row(3) - m.row(0)).transpose();
        let rm = rp.xyz().magnitude();
        let right = Plane::from_normal_and_distance(rp.xyz() / rm, -rp[3] / rm);

        let bp = (m.row(3) + m.row(1)).transpose();
        let bm = bp.xyz().magnitude();
        let bottom = Plane::from_normal_and_distance(bp.xyz() / bm, -bp[3] / bm);

        let tp = (m.row(3) - m.row(1)).transpose();
        let tm = tp.xyz().magnitude();
        let top = Plane::from_normal_and_distance(tp.xyz() / tm, -tp[3] / tm);

        let np = (m.row(3) + m.row(2)).transpose();
        let nm = np.xyz().magnitude();
        let near = Plane::from_normal_and_distance(np.xyz() / nm, -np[3] / nm);

        [left, right, bottom, top, near]
    }

    fn apply_input_state(&mut self) {
        let mut fov = degrees!(self.fov_y);
        fov += self.input.fov_delta;
        fov = fov.min(degrees!(90)).max(degrees!(1));
        self.fov_y = radians!(fov);
    }

    pub fn update_frame(&mut self, frame: &WorldSpaceFrame) {
        self.position = *frame.position();
        let basis = frame.basis();
        self.forward = basis.forward;
        self.right = basis.right;
        self.up = -basis.up;
    }

    // Apply interpreted inputs from prior stage; apply new world position.
    fn sys_apply_input(
        mut camera: ResMut<ScreenCamera>,
        query: Query<(&WorldSpaceFrame, &ScreenCameraController)>,
    ) {
        let (frame, _) = query.single();
        camera.apply_input_state();
        camera.update_frame(frame);
    }

    // Apply updated system config, e.g. aspect
    fn sys_apply_display_changes(
        mut camera: ResMut<ScreenCamera>,
        updated_config: Res<Option<DisplayConfig>>,
    ) {
        if let Some(config) = updated_config.as_ref() {
            camera.on_display_config_updated(config);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::arc_ball_camera::ArcBallController;
    use absolute_unit::{degrees, meters};
    use anyhow::Result;
    use approx::assert_relative_eq;
    use geodesy::{GeoSurface, Graticule, Target};
    use nalgebra::Vector4;

    #[test]
    fn test_perspective() {
        let camera = ScreenCamera::new(degrees!(90), 9.0 / 11.0, meters!(0.3));
        let p = camera.perspective::<Meters>().to_homogeneous();
        let wrld = Vector4::new(0000.0, 0.0, -10000.0, 1.0);
        let eye = camera.view::<Meters>().to_homogeneous() * wrld;
        let clip = p * eye;
        let ndc = (clip / clip[3]).xyz();
        let w = camera.z_near::<Meters>().f64() / ndc.z;
        let eyep = Vector3::new(ndc.x * w / camera.aspect_ratio(), ndc.y * w, -w);
        let wrldp = camera.view::<Meters>().inverse().to_homogeneous() * eyep.to_homogeneous();

        println!(
            "wrld: {}eye: {}clip: {}ndc: {}, w: {}\neyep: {}, wrldp: {}",
            wrld, eye, clip, ndc, w, eyep, wrldp
        );

        assert_relative_eq!(wrld.x, wrldp.x, epsilon = 0.000000001);
        assert_relative_eq!(wrld.y, wrldp.y, epsilon = 0.000000001);
        assert_relative_eq!(wrld.z, wrldp.z, epsilon = 0.000000001);
    }

    #[test]
    fn test_depth_restore() -> Result<()> {
        let aspect_ratio = 0.9488875526157546;
        let mut camera = ScreenCamera::new(degrees!(90), aspect_ratio, meters!(0.5));
        let mut arcball = ArcBallController::default();
        arcball.set_target(Graticule::<GeoSurface>::new(
            degrees!(0),
            degrees!(0),
            meters!(2),
        ));
        arcball.set_eye(Graticule::<Target>::new(
            degrees!(89),
            degrees!(0),
            meters!(4_000_000),
            // meters!(1_400_000),
        ))?;
        let frame = arcball.world_space_frame();
        camera.update_frame(&frame);

        let camera_position_km = camera.position::<Kilometers>().vec64();
        let camera_inverse_perspective_km: Matrix4<f64> =
            camera.perspective::<Kilometers>().inverse();
        let camera_inverse_view_km = camera.view::<Kilometers>().inverse().to_homogeneous();

        // Given corner positions in ndc of -1,-1 and 1,1... what does a mostly forward vector
        // in ndc map to, given the above camera?
        let corner = Vector4::new(0.1, 0.1, 0.0, 1.0);

        let eye = (camera_inverse_perspective_km * corner).normalize();
        let wrld = (camera_inverse_view_km * eye).normalize();
        println!("pos: {}", camera_position_km);
        println!("eye : {}", eye);
        println!("wrld: {}", wrld);
        println!("pos: {}", camera_position_km.xyz() + wrld.xyz() * 8000.0);

        Ok(())
    }
}
