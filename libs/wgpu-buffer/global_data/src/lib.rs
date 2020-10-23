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
use absolute_unit::{Kilometers, LengthUnit, Meters};
use camera::Camera;
use commandable::{commandable, Commandable};
use core::num::NonZeroU64;
use failure::Fallible;
use geodesy::{Cartesian, GeoCenter};
use gpu::{UploadTracker, GPU};
use nalgebra::{convert, Isometry3, Matrix4, Point3, Vector3, Vector4};
use std::{mem, sync::Arc};
use zerocopy::{AsBytes, FromBytes};

pub fn m2v(m: &Matrix4<f32>) -> [[f32; 4]; 4] {
    let mut v = [[0f32; 4]; 4];
    for i in 0..16 {
        v[i / 4][i % 4] = m[i];
    }
    v
}

pub fn p2v(p: &Point3<f32>) -> [f32; 4] {
    [p.x, p.y, p.z, 0f32]
}

pub fn v2v(v: &Vector4<f32>) -> [f32; 4] {
    [v[0], v[1], v[2], v[3]]
}

#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Debug, Default)]
struct Globals {
    // Overlay screen info
    screen_projection: [[f32; 4]; 4],

    // Camera parameters in tile space XYZ, 1hm per unit.
    camera_graticule_radians_meters: [f32; 4],
    view: [[f32; 4]; 4],
    proj: [[f32; 4]; 4],

    // Camera parameters in geocenter km (mostly for debugging).
    debug_geocenter_km_view: [[f32; 4]; 4],
    globals_m4_projection_meters: [[f32; 4]; 4],
    globals_m4_inv_projection_meters: [[f32; 4]; 4],

    // Inverted camera parameters in ecliptic XYZ, 1km per unit.
    geocenter_km_inverse_view: [[f32; 4]; 4],
    geocenter_km_inverse_proj: [[f32; 4]; 4],

    tile_to_earth: [[f32; 4]; 4],
    tile_to_earth_rotation: [[f32; 4]; 4],
    tile_to_earth_scale: [[f32; 4]; 4],
    tile_to_earth_translation: [f32; 4],
    tile_center_offset: [f32; 4],

    // Camera position in each of the above.
    camera_position_tile: [f32; 4],
    geocenter_km_camera_position: [f32; 4],
}

fn geocenter_cart_to_v<Unit: LengthUnit>(geocart: Cartesian<GeoCenter, Unit>) -> [f32; 4] {
    [
        f32::from(geocart.coords[0]),
        f32::from(geocart.coords[1]),
        f32::from(geocart.coords[2]),
        1f32,
    ]
}

impl Globals {
    // Scale from 1:1 being full screen width to 1:1 being a letterbox, either with top-bottom
    // cutouts or left-right cutouts, depending on the aspect. This lets our screen drawing
    // routines (e.g. for text) assume that everything is undistorted, even if coordinates at
    // the edges go outside the +/- 1 range.
    pub fn with_screen_overlay_projection(mut self, gpu: &GPU) -> Self {
        let dim = gpu.physical_size();
        let aspect = gpu.aspect_ratio_f32() * 4f32 / 3f32;
        let (w, h) = if dim.width > dim.height {
            (aspect, -1f32)
        } else {
            (1f32, -1f32 / aspect)
        };
        self.screen_projection = m2v(&Matrix4::new_nonuniform_scaling(&Vector3::new(w, h, 1f32)));
        self
    }

    // Raymarching the skybox uses the following inputs:
    //   geocenter_km_inverse_view
    //   geocenter_km_inverse_proj
    //   geocenter_km_camera_position
    //   sun direction vector (origin does not matter terribly much at 8 light minutes distance).
    //
    // It takes a [-1,1] fullscreen quad and turns it into worldspace vectors starting at the
    // the camera position and extending to the fullscreen quad corners, in world space.
    // Interpolation between these vectors automatically fills in one ray for every screen pixel.
    pub fn with_geocenter_km_raymarching(mut self, camera: &Camera) -> Self {
        let eye = camera.position::<Kilometers>().vec64();
        let view = Isometry3::look_at_rh(
            &Point3::from(eye),
            &Point3::from(eye + camera.forward()),
            &-camera.up(),
        );
        self.geocenter_km_inverse_view = m2v(&convert(view.inverse().to_homogeneous()));
        self.geocenter_km_inverse_proj = m2v(&convert(camera.projection::<Kilometers>().inverse()));
        self.geocenter_km_camera_position = geocenter_cart_to_v(camera.position::<Kilometers>());
        self
    }

    /*
    pub fn with_camera_info(mut self, camera: &Camera) -> Self {
        // FIXME: we're using the target right now so we can see tessellation in action.
        self.camera_graticule_radians_meters = [
            f32::from(camera.get_target().latitude),
            f32::from(camera.get_target().longitude),
            f32::from(camera.get_target().distance),
            1f32,
        ];
        self
    }
     */

    // Provide geocenter projections for use when we have nothing else to grab onto.
    pub fn with_debug_geocenter_helpers(mut self, camera: &Camera) -> Self {
        self.debug_geocenter_km_view = m2v(&convert(camera.view::<Kilometers>().to_homogeneous()));
        self
    }

    pub fn with_meter_projection(mut self, camera: &Camera) -> Self {
        self.globals_m4_projection_meters =
            m2v(&convert(camera.projection::<Meters>().to_homogeneous()));
        self.globals_m4_inv_projection_meters =
            m2v(&convert(camera.projection::<Meters>().inverse()));
        self
    }
}

#[derive(Commandable)]
pub struct GlobalParametersBuffer {
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    buffer_size: wgpu::BufferAddress,
    parameters_buffer: Arc<Box<wgpu::Buffer>>,

    pub tile_to_earth: Matrix4<f32>,
}

#[commandable]
impl GlobalParametersBuffer {
    pub fn new(device: &wgpu::Device) -> Fallible<Self> {
        let buffer_size = mem::size_of::<Globals>() as wgpu::BufferAddress;
        let parameters_buffer = Arc::new(Box::new(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals-buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsage::STORAGE | wgpu::BufferUsage::COPY_DST,
            mapped_at_creation: false,
        })));

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals-bind-group-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStage::all(),
                ty: wgpu::BindingType::StorageBuffer {
                    min_binding_size: NonZeroU64::new(buffer_size),
                    dynamic: false,
                    readonly: true,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals-bind-group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(parameters_buffer.slice(0..buffer_size)),
            }],
        });

        Ok(Self {
            bind_group_layout,
            bind_group,
            buffer_size,
            parameters_buffer,
            tile_to_earth: Matrix4::identity(),
        })
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
        gpu: &GPU,
        tracker: &mut UploadTracker,
    ) -> Fallible<()> {
        let globals: Globals = Default::default();
        let globals = globals
            .with_screen_overlay_projection(gpu)
            .with_meter_projection(camera)
            .with_geocenter_km_raymarching(camera)
            .with_debug_geocenter_helpers(camera);
        let buffer = gpu.push_data(
            "global-upload-buffer",
            &globals,
            wgpu::BufferUsage::MAP_READ | wgpu::BufferUsage::COPY_SRC,
        );
        tracker.upload_ba(buffer, self.parameters_buffer.clone(), self.buffer_size);
        Ok(())
    }

    pub fn make_upload_buffer_for_arcball_on_globe(
        &self,
        _camera: &Camera,
        _gpu: &GPU,
        _tracker: &mut UploadTracker,
    ) -> Fallible<()> {
        /*
        let globals = Self::arcball_camera_to_buffer(100f32, 100f32, 0f32, 0f32, camera, gpu);
        upload_buffers.push(self.make_gpu_buffer(globals, gpu));
        */
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
            camera_position_tile: p2v(&convert(camera.eye())),
            camera_position_earth_km: v2v(&convert(earth_eye)),
        }
    }
    */
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpu::GPU;
    use input::InputSystem;

    #[test]
    fn it_can_create_a_buffer() -> Fallible<()> {
        let input = InputSystem::new(vec![])?;
        let gpu = GPU::new(&input, Default::default())?;
        let _globals_buffer = GlobalParametersBuffer::new(gpu.device())?;
        Ok(())
    }
}
