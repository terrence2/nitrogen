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
use absolute_unit::{Kilometers, Meters};
use anyhow::Result;
use camera::Camera;
use core::num::NonZeroU64;
use gpu::{Gpu, UploadTracker};
use nalgebra::{convert, Matrix3, Matrix4, Point3, Vector3, Vector4};
use nitrous::{Interpreter, Value};
use nitrous_injector::{inject_nitrous_module, method, NitrousModule};
use parking_lot::RwLock;
use std::{mem, sync::Arc};
use zerocopy::{AsBytes, FromBytes};

pub fn m2v(m: &Matrix4<f32>) -> [[f32; 4]; 4] {
    let mut v = [[0f32; 4]; 4];
    for i in 0..16 {
        v[i / 4][i % 4] = m[i];
    }
    v
}

pub fn m33_to_v(m: &Matrix3<f64>) -> [[f32; 4]; 4] {
    m2v(&convert(m.to_homogeneous()))
}

pub fn m44_to_v(m: &Matrix4<f64>) -> [[f32; 4]; 4] {
    m2v(&convert::<Matrix4<f64>, Matrix4<f32>>(*m))
}

pub fn p2v(p: &Point3<f32>) -> [f32; 4] {
    [p.x, p.y, p.z, 0f32]
}

pub fn v2v(v: &Vector4<f32>) -> [f32; 4] {
    [v[0], v[1], v[2], v[3]]
}

pub fn v3_to_v(v: &Vector3<f64>) -> [f32; 4] {
    v2v(&convert(v.to_homogeneous()))
}

#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Debug, Default)]
struct Globals {
    // Overlay screen info
    screen_letterbox_projection: [[f32; 4]; 4],

    // Screen info
    screen_physical_width: f32,
    screen_physical_height: f32,
    screen_logical_width: f32,
    screen_logical_height: f32,

    // Camera properties
    camera_fov_y: f32,
    camera_aspect_ratio: f32,
    camera_z_near_m: f32,
    camera_z_near_km: f32,
    camera_forward: [f32; 4],
    camera_up: [f32; 4],
    camera_right: [f32; 4],
    camera_position_m: [f32; 4],
    camera_position_km: [f32; 4],
    camera_perspective_m: [[f32; 4]; 4],
    camera_perspective_km: [[f32; 4]; 4],
    camera_inverse_perspective_m: [[f32; 4]; 4],
    camera_inverse_perspective_km: [[f32; 4]; 4],
    camera_view_m: [[f32; 4]; 4],
    camera_view_km: [[f32; 4]; 4],
    camera_inverse_view_m: [[f32; 4]; 4],
    camera_inverse_view_km: [[f32; 4]; 4],
    camera_look_at_rhs_m: [[f32; 4]; 4],
    camera_exposure: f32,

    // Tone mapping
    tone_gamma: f32,

    // Pad out to [f32;4] alignment
    pad1: [f32; 2],
}

impl Globals {
    // Scale from 1:1 being full screen width to 1:1 being a letterbox, either with top-bottom
    // cutouts or left-right cutouts, depending on the aspect. This lets our screen drawing
    // routines (e.g. for text) assume that everything is undistorted, even if coordinates at
    // the edges go outside the +/- 1 range.
    pub fn with_screen_overlay_projection(mut self, gpu: &Gpu) -> Self {
        let dim = gpu.physical_size();
        let aspect = gpu.aspect_ratio_f32() * 4f32 / 3f32;
        let (w, h) = if dim.width > dim.height {
            (aspect, -1f32)
        } else {
            (1f32, -1f32 / aspect)
        };
        self.screen_letterbox_projection =
            m2v(&Matrix4::new_nonuniform_scaling(&Vector3::new(w, h, 1f32)));

        let physical = gpu.physical_size();
        let logical = gpu.logical_size();
        self.screen_physical_width = physical.width as f32;
        self.screen_physical_height = physical.width as f32;
        self.screen_logical_width = logical.width as f32;
        self.screen_logical_height = logical.width as f32;

        self
    }

    pub fn with_camera(mut self, camera: &Camera) -> Self {
        self.camera_fov_y = camera.fov_y().f32();
        self.camera_aspect_ratio = camera.aspect_ratio() as f32;
        self.camera_z_near_m = camera.z_near::<Meters>().f32();
        self.camera_z_near_km = camera.z_near::<Kilometers>().f32();
        self.camera_forward = v3_to_v(camera.forward());
        self.camera_up = v3_to_v(camera.up());
        self.camera_right = v3_to_v(camera.right());
        self.camera_position_m = v3_to_v(&camera.position::<Meters>().vec64());
        self.camera_position_km = v3_to_v(&camera.position::<Kilometers>().vec64());
        self.camera_perspective_m = m44_to_v(&camera.perspective::<Meters>().to_homogeneous());
        self.camera_perspective_km = m44_to_v(&camera.perspective::<Kilometers>().to_homogeneous());
        self.camera_inverse_perspective_m = m44_to_v(&camera.perspective::<Meters>().inverse());
        self.camera_inverse_perspective_km =
            m44_to_v(&camera.perspective::<Kilometers>().inverse());
        self.camera_view_m = m44_to_v(&camera.view::<Meters>().to_homogeneous());
        self.camera_view_km = m44_to_v(&camera.view::<Kilometers>().to_homogeneous());
        self.camera_inverse_view_m = m44_to_v(&camera.view::<Meters>().inverse().to_homogeneous());
        self.camera_inverse_view_km =
            m44_to_v(&camera.view::<Kilometers>().inverse().to_homogeneous());
        self.camera_look_at_rhs_m = m44_to_v(
            &camera
                .look_at_rh::<Meters>()
                .to_rotation_matrix()
                .to_homogeneous(),
        );
        self.camera_exposure = camera.exposure();
        self
    }

    pub fn with_tone(mut self, tone_gamma: f32) -> Self {
        self.tone_gamma = tone_gamma;
        self
    }
}

#[derive(Debug, NitrousModule)]
pub struct GlobalParametersBuffer {
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    buffer_size: wgpu::BufferAddress,
    parameters_buffer: Arc<wgpu::Buffer>,
    tone_gamma: f32,
}

#[inject_nitrous_module]
impl GlobalParametersBuffer {
    const INITIAL_GAMMA: f32 = 2.2f32;

    pub fn new(device: &wgpu::Device, interpreter: &mut Interpreter) -> Arc<RwLock<Self>> {
        let buffer_size = mem::size_of::<Globals>() as wgpu::BufferAddress;
        let parameters_buffer = Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals-buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsage::STORAGE | wgpu::BufferUsage::COPY_DST,
            mapped_at_creation: false,
        }));

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals-bind-group-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStage::all(),
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(buffer_size),
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals-bind-group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer {
                    buffer: &parameters_buffer,
                    offset: 0,
                    size: None,
                },
            }],
        });

        let globals = Arc::new(RwLock::new(Self {
            bind_group_layout,
            bind_group,
            buffer_size,
            parameters_buffer,
            tone_gamma: Self::INITIAL_GAMMA,
        }));

        interpreter.put_global("globals", Value::Module(globals.clone()));

        globals
    }

    pub fn add_default_bindings(&mut self, interpreter: &mut Interpreter) -> Result<()> {
        interpreter.interpret_once(
            r#"
                let bindings := mapper.create_bindings("globals");
                bindings.bind("LBracket", "globals.decrease_gamma(pressed)");
                bindings.bind("RBracket", "globals.increase_gamma(pressed)");
            "#,
        )?;
        Ok(())
    }

    #[method]
    pub fn increase_gamma(&mut self, pressed: bool) {
        println!("GAMMA INCREASE");
        if pressed {
            self.tone_gamma *= 1.1;
        }
    }

    #[method]
    pub fn decrease_gamma(&mut self, pressed: bool) {
        println!("GAMMA DECREASE");
        if pressed {
            self.tone_gamma /= 1.1;
        }
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn make_upload_buffer(
        &self,
        camera: &Camera,
        gpu: &Gpu,
        tracker: &mut UploadTracker,
    ) -> Result<()> {
        let globals: Globals = Default::default();
        let globals = globals
            .with_screen_overlay_projection(gpu)
            .with_camera(camera)
            .with_tone(self.tone_gamma);
        let buffer = gpu.push_data(
            "global-upload-buffer",
            &globals,
            wgpu::BufferUsage::MAP_READ | wgpu::BufferUsage::COPY_SRC,
        );
        tracker.upload_ba(buffer, self.parameters_buffer.clone(), self.buffer_size);
        Ok(())
    }

    /*
    fn arcball_camera_to_buffer(
        tile_width_ft: f32,
        tile_height_ft: f32,
        tile_origin_lat_deg: f32,
        tile_origin_lon_deg: f32,
        camera: &ArcBallCamera,
        gpu: &GPU,
    ) -> Globals {
        fn deg2rad(deg: f64) -> f64 {
            deg * PI / 180.0
        }
        fn ft2hm(ft: f64) -> f64 {
            ft * FEET_TO_HM_64
        }

        let tile_width_hm = ft2hm(tile_width_ft as f64);
        let tile_height_hm = ft2hm(tile_height_ft as f64);

        let lat = deg2rad(tile_origin_lat_deg as f64);
        let lon = deg2rad(tile_origin_lon_deg as f64);

        /*
        fn rad2deg(rad: f32) -> f32 {
            rad * 180f32 / PI
        }
        let ft_per_degree = lat.cos() * 69.172f32 * 5_280f32;
        let angular_height = tile_height_ft as f32 / ft_per_degree;
        println!(
            "\"{}\": TL coord: {}, {}",
            terrain.name(),
            rad2deg(lat + deg2rad(angular_height)),
            rad2deg(lon)
        );
        */

        // Lat/Lon to XYZ in KM.
        // x = (N + h) * cos(lat) * cos(lon)
        // y = (N + h) * cos(lat) * sin(lon)
        // z = (( b^2 / a^2 ) * N + h) * sin(lat)
        let base = Point3::new(lat.cos() * lon.sin(), -lat.sin(), lat.cos() * lon.cos());
        let base_in_km = base * 6360f64;

        let r_lon = UnitQuaternion::from_axis_angle(
            &Unit::new_unchecked(Vector3::new(0f64, -1f64, 0f64)),
            -lon,
        );
        let r_lat = UnitQuaternion::from_axis_angle(
            &Unit::new_unchecked(r_lon * Vector3::new(1f64, 0f64, 0f64)),
            -(PI / 2.0 - lat),
        );

        let tile_ul_eye = camera.eye();
        let tile_ul_tgt = camera.get_target();
        let ul_to_c = Vector3::new(tile_width_hm / 2f64, 0f64, tile_height_hm / 2f64);
        let tile_c_eye = tile_ul_eye - ul_to_c;
        let tile_c_tgt = tile_ul_tgt - ul_to_c;
        let tile_up = camera.up;

        // Create a matrix to translate between tile and earth coordinates.
        let rot_m = Matrix4::from((r_lat * r_lon).to_rotation_matrix());
        let trans_m = Matrix4::new_translation(&Vector3::new(
            base_in_km.coords[0],
            base_in_km.coords[1],
            base_in_km.coords[2],
        ));
        let scale_m = Matrix4::new_scaling(HM_TO_KM);
        let tile_to_earth = trans_m * scale_m * rot_m;

        let tile_center_offset = Vector3::new(
            tile_width_ft * FEET_TO_HM_32 / 2.0,
            0f32,
            tile_height_ft * FEET_TO_HM_32 / 2.0,
        );

        let earth_eye = tile_to_earth * tile_c_eye.to_homogeneous();
        let earth_tgt = tile_to_earth * tile_c_tgt.to_homogeneous();
        let earth_up = (tile_to_earth * tile_up.to_homogeneous()).normalize();

        let earth_view = Isometry3::look_at_rh(
            &Point3::from(earth_eye.xyz()),
            &Point3::from(earth_tgt.xyz()),
            &earth_up.xyz(),
        );

        let earth_inv_view: Matrix4<f32> = convert(earth_view.inverse().to_homogeneous());
        let earth_inv_proj: Matrix4<f32> = convert(camera.projection().inverse());

        let dim = gpu.physical_size();
        let aspect = gpu.aspect_ratio_f32() * 4f32 / 3f32;
        let (w, h) = if dim.width > dim.height {
            (aspect, 1f32)
        } else {
            (1f32, 1f32 / aspect)
        };
        Globals {
            screen_projection: m2v(&Matrix4::new_nonuniform_scaling(&Vector3::new(w, h, 1f32))),
            view: m2v(&camera.view_matrix()),
            proj: m2v(&camera.projection_matrix()),
            inv_view: m2v(&earth_inv_view),
            inv_proj: m2v(&earth_inv_proj),
            tile_to_earth: m2v(&convert(tile_to_earth)),
            tile_to_earth_rotation: m2v(&convert(rot_m)),
            tile_to_earth_scale: m2v(&convert(scale_m)),
            tile_to_earth_translation: v2v(&convert(base_in_km.coords.to_homogeneous())),
            tile_center_offset: v2v(&tile_center_offset.to_homogeneous()),
        }
    }
    */
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpu::Gpu;
    use winit::{event_loop::EventLoop, window::Window};

    #[cfg(unix)]
    #[test]
    fn it_can_create_a_buffer() -> Result<()> {
        use winit::platform::unix::EventLoopExtUnix;
        let event_loop = EventLoop::<()>::new_any_thread();
        let window = Window::new(&event_loop)?;
        let mut interpreter = Interpreter::default();
        let gpu = Gpu::new(window, Default::default(), &mut interpreter)?;
        let _globals_buffer = GlobalParametersBuffer::new(gpu.read().device(), &mut interpreter);
        Ok(())
    }
}
