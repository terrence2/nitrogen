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
use bevy_ecs::prelude::*;
use camera::{CameraStep, HudCamera, ScreenCamera};
use core::num::NonZeroU64;
use gpu::{Gpu, GpuStep};
use nalgebra::{convert, Matrix3, Matrix4, Point3, Vector3, Vector4};
use nitrous::{inject_nitrous_resource, method, NitrousResource};
use orrery::Orrery;
use runtime::{Extension, FrameStage, Runtime};
use std::{mem, sync::Arc};
use window::{Window, WindowStep};
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
    // Screen info
    screen_physical_width: f32,
    screen_physical_height: f32,
    screen_render_width: f32,
    screen_render_height: f32,

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
    pub fn set_window_info(&mut self, win: &Window) {
        let physical = win.physical_size();
        let render = win.render_extent();
        self.screen_physical_width = physical.width as f32;
        self.screen_physical_height = physical.width as f32;
        self.screen_render_width = render.width as f32;
        self.screen_render_height = render.width as f32;
    }

    pub fn set_camera(&mut self, camera: &ScreenCamera) {
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
        self.camera_exposure = camera.exposure() as f32;
    }

    pub fn set_orrery(&mut self, orrery: &Orrery) {
        self.orrery_sun_direction = v3_to_v(&orrery.sun_direction());
    }

    pub fn set_tone(&mut self, tone_gamma: f32) {
        self.tone_gamma = tone_gamma;
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum GlobalsStep {
    TrackStateChanges,
    EnsureUpdated,
}

#[derive(Debug, NitrousResource)]
pub struct GlobalParametersBuffer {
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    buffer_size: wgpu::BufferAddress,
    parameters_buffer: Arc<wgpu::Buffer>,
    globals: Globals,
    tone_gamma: f32,
}

impl Extension for GlobalParametersBuffer {
    fn init(runtime: &mut Runtime) -> Result<()> {
        let globals = GlobalParametersBuffer::new(runtime.resource::<Gpu>().device());

        // TODO:  move to configuration, once that's a thing
        runtime.run_string(
            r#"
                bindings.bind("LBracket", "globals.decrease_gamma()");
                bindings.bind("RBracket", "globals.increase_gamma()");
            "#,
        )?;

        runtime.insert_named_resource("globals", globals);
        runtime.frame_stage_mut(FrameStage::Main).add_system(
            Self::sys_track_state_changes
                .label(GlobalsStep::TrackStateChanges)
                .after(WindowStep::HandleEvents)
                .after(CameraStep::HandleDisplayChange),
        );
        runtime.frame_stage_mut(FrameStage::Main).add_system(
            Self::sys_ensure_globals_updated
                .label(GlobalsStep::EnsureUpdated)
                .after(GlobalsStep::TrackStateChanges)
                .after(GpuStep::CreateCommandEncoder)
                .before(GpuStep::SubmitCommands),
        );

        Ok(())
    }
}

#[inject_nitrous_resource]
impl GlobalParametersBuffer {
    const INITIAL_GAMMA: f32 = 2.2f32;

    pub fn new(device: &wgpu::Device) -> Self {
        let buffer_size = mem::size_of::<Globals>() as wgpu::BufferAddress;
        let parameters_buffer = Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals-buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals-bind-group-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::all(),
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
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &parameters_buffer,
                    offset: 0,
                    size: None,
                }),
            }],
        });

        Self {
            bind_group_layout,
            bind_group,
            buffer_size,
            parameters_buffer,
            globals: Default::default(),
            tone_gamma: Self::INITIAL_GAMMA,
        }
    }

    #[method]
    pub fn increase_gamma(&mut self) {
        self.tone_gamma *= 1.1;
    }

    #[method]
    pub fn decrease_gamma(&mut self) {
        self.tone_gamma /= 1.1;
    }

    #[method]
    pub fn tone_gamma(&self) -> f64 {
        self.tone_gamma as f64
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    fn sys_track_state_changes(
        camera: Res<ScreenCamera>,
        query: Query<&HudCamera>,
        orrery: Res<Orrery>,
        window: Res<Window>,
        mut globals: ResMut<GlobalParametersBuffer>,
    ) {
        globals.track_state_changes(&camera, &orrery, &window);
        for _hud_camera in query.iter() {
            // FIXME: multiple camera support
        }
    }

    pub fn track_state_changes(&mut self, camera: &ScreenCamera, orrery: &Orrery, win: &Window) {
        self.globals.set_camera(camera);
        self.globals.set_orrery(orrery);
        self.globals.set_tone(self.tone_gamma);
        self.globals.set_window_info(win);
    }

    fn sys_ensure_globals_updated(
        globals: Res<GlobalParametersBuffer>,
        gpu: Res<Gpu>,
        maybe_encoder: ResMut<Option<wgpu::CommandEncoder>>,
    ) {
        if let Some(encoder) = maybe_encoder.into_inner() {
            let buffer = gpu.push_data(
                "global-upload-buffer",
                &globals.globals,
                wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_SRC,
            );
            // Note: we _could_ also recreate any bindings that refer to this buffer,
            //       but that's _lots_ of bindings in many systems, so we copy instead.
            encoder.copy_buffer_to_buffer(
                &buffer,
                0,
                &globals.parameters_buffer,
                0,
                globals.buffer_size,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpu::Gpu;

    #[cfg(unix)]
    #[test]
    fn it_can_create_a_buffer() -> Result<()> {
        let mut runtime = Gpu::for_test_unix()?;
        runtime.load_extension::<GlobalParametersBuffer>()?;
        assert!(runtime.resource::<GlobalParametersBuffer>().tone_gamma() > 0.0);
        Ok(())
    }
}
