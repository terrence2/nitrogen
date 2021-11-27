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
use orrery::Orrery;
use parking_lot::RwLock;
use std::{mem, sync::Arc};
use window::WindowHandle;
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

    // Orrery
    orrery_sun_direction: [f32; 4],

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
    pub fn set_screen_overlay_projection(&mut self, win: &WindowHandle) {
        let physical = win.physical_size();
        let aspect = win.aspect_ratio_f32() * 4f32 / 3f32;
        let (w, h) = if physical.width > physical.height {
            (aspect, -1f32)
        } else {
            (1f32, -1f32 / aspect)
        };
        self.screen_letterbox_projection =
            m2v(&Matrix4::new_nonuniform_scaling(&Vector3::new(w, h, 1f32)));

        let physical = win.physical_size();
        let logical = win.logical_size();
        self.screen_physical_width = physical.width as f32;
        self.screen_physical_height = physical.width as f32;
        self.screen_logical_width = logical.width as f32;
        self.screen_logical_height = logical.width as f32;
    }

    pub fn set_camera(&mut self, camera: &Camera) {
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
    }

    pub fn set_orrery(&mut self, orrery: &Orrery) {
        self.orrery_sun_direction = v3_to_v(&orrery.sun_direction());
    }

    pub fn set_tone(&mut self, tone_gamma: f32) {
        self.tone_gamma = tone_gamma;
    }
}

#[derive(Debug, NitrousModule)]
pub struct GlobalParametersBuffer {
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    buffer_size: wgpu::BufferAddress,
    parameters_buffer: Arc<wgpu::Buffer>,
    globals: Globals,
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
            globals: Default::default(),
            tone_gamma: Self::INITIAL_GAMMA,
        }));

        interpreter.put_global("globals", Value::Module(globals.clone()));

        globals
    }

    pub fn add_debug_bindings(&mut self, interpreter: &mut Interpreter) -> Result<()> {
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
        if pressed {
            self.tone_gamma *= 1.1;
        }
    }

    #[method]
    pub fn decrease_gamma(&mut self, pressed: bool) {
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

    pub fn track_state_changes(&mut self, camera: &Camera, orrery: &Orrery, win: &WindowHandle) {
        self.globals.set_camera(camera);
        self.globals.set_orrery(orrery);
        self.globals.set_tone(self.tone_gamma);
        self.globals.set_screen_overlay_projection(win);
    }

    pub fn ensure_uploaded(&mut self, gpu: &Gpu, tracker: &mut UploadTracker) -> Result<()> {
        let buffer = gpu.push_data(
            "global-upload-buffer",
            &self.globals,
            wgpu::BufferUsage::MAP_READ | wgpu::BufferUsage::COPY_SRC,
        );
        tracker.upload_ba(buffer, self.parameters_buffer.clone(), self.buffer_size);
        Ok(())
    }
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
