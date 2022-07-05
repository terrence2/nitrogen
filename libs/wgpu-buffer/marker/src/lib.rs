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
mod vertex;

use crate::vertex::MarkerVertex;
use absolute_unit::{Length, Meters};
use anyhow::Result;
use bevy_ecs::prelude::*;
use camera::ScreenCamera;
use composite::CompositeRenderStep;
use csscolorparser::Color;
use geometry::{Aabb3, Cylinder, RenderPrimitive, Sphere};
use global_data::GlobalParametersBuffer;
use gpu::{Gpu, GpuStep};
use measure::WorldSpaceFrame;
use nalgebra::{Matrix4, Point3, UnitQuaternion, Vector3};
use nitrous::{
    inject_nitrous_component, inject_nitrous_resource, NitrousComponent, NitrousResource,
};
use runtime::{Extension, Runtime};
use shader_shared::Group;
use std::{collections::HashMap, f64::consts::PI, mem, ops::Range, sync::Arc};
use world::{WorldRenderPass, WorldStep};

/// Display points and vectors in the world for debugging purposes.

#[derive(Debug)]
struct MarkerPoint {
    position: Point3<Length<Meters>>,
    radius: Length<Meters>,
    color: Color,
}

#[derive(Debug)]
struct MarkerArrow {
    origin: Point3<Length<Meters>>,
    vector: Vector3<Length<Meters>>,
    radius: Length<Meters>,
    color: Color,
}

#[derive(Debug)]
struct MarkerBox {
    aabb: Aabb3,
    color: Color,
}

#[derive(Debug)]
struct MarkerCylinder {
    origin: Point3<Length<Meters>>,
    vector: Vector3<Length<Meters>>,
    radius: Length<Meters>,
    color: Color,
}

/// Put on an entity with a WorldSpaceFrame component and add points and arrows.
#[derive(Component, NitrousComponent, Debug, Default)]
pub struct EntityMarkers {
    points: HashMap<String, MarkerPoint>,
    boxes: HashMap<String, MarkerBox>,
    arrows: HashMap<String, MarkerArrow>,
    cylinders: HashMap<String, MarkerCylinder>,
}

#[inject_nitrous_component]
impl EntityMarkers {
    pub fn add_point(
        &mut self,
        name: &str,
        position: Point3<Length<Meters>>,
        radius: Length<Meters>,
        color: Color,
    ) {
        self.points.insert(
            name.to_owned(),
            MarkerPoint {
                position,
                radius,
                color,
            },
        );
    }

    pub fn add_box(
        &mut self,
        name: &str,
        lo: Point3<Length<Meters>>,
        hi: Point3<Length<Meters>>,
        color: Color,
    ) {
        self.boxes.insert(
            name.to_owned(),
            MarkerBox {
                aabb: Aabb3::from_bounds(hi, lo),
                color,
            },
        );
    }

    pub fn add_arrow(
        &mut self,
        name: &str,
        origin: Point3<Length<Meters>>,
        vector: Vector3<Length<Meters>>,
        radius: Length<Meters>,
        color: Color,
    ) {
        self.arrows.insert(
            name.to_owned(),
            MarkerArrow {
                origin,
                vector,
                radius,
                color,
            },
        );
    }

    pub fn add_cylinder(
        &mut self,
        name: &str,
        origin: Point3<Length<Meters>>,
        vector: Vector3<Length<Meters>>,
        radius: Length<Meters>,
        color: Color,
    ) {
        self.cylinders.insert(
            name.to_owned(),
            MarkerCylinder {
                origin,
                vector,
                radius,
                color,
            },
        );
    }

    pub fn update_arrow_vector(&mut self, name: &str, vector: Vector3<Length<Meters>>) {
        if let Some(mut arrow) = self.arrows.get_mut(name) {
            arrow.vector = vector;
        }
    }

    pub fn remove_arrow(&mut self, name: &str) {
        self.arrows.remove(name);
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum MarkersStep {
    // ResetUploadCursor,
    // AnimateDrawState,
    // ApplyTransforms,
    // ApplyFlags,
    // ApplyXforms,
    // PushToBlock,
    // UploadChunks,
    // UploadBlocks,
    UploadGeometry,
    Render,
    // CleanupOpenChunks,
}

#[derive(NitrousResource)]
pub struct Markers {
    pipeline: wgpu::RenderPipeline,
    vertices: Arc<wgpu::Buffer>,
    vertex_count: u32,
    indices: Arc<wgpu::Buffer>,
    index_count: u32,
}

impl Extension for Markers {
    fn init(runtime: &mut Runtime) -> Result<()> {
        runtime.insert_named_resource(
            "markers",
            Markers::new(
                runtime.resource::<GlobalParametersBuffer>(),
                runtime.resource::<Gpu>(),
            ),
        );
        runtime.add_frame_system(
            Self::sys_upload_geometry
                .label(MarkersStep::UploadGeometry)
                .after(GpuStep::CreateCommandEncoder)
                .before(GpuStep::SubmitCommands),
        );
        runtime.add_frame_system(
            Self::sys_draw_markers
                .label(MarkersStep::Render)
                .after(MarkersStep::UploadGeometry)
                // .after(ShapeStep::UploadBlocks)
                .after(WorldStep::Render)
                .before(CompositeRenderStep::Render),
        );

        Ok(())
    }
}

#[inject_nitrous_resource]
impl Markers {
    const MAX_VERTICIES: usize = 4096;
    const MAX_INDICES: usize = 8192;

    fn new(globals: &GlobalParametersBuffer, gpu: &Gpu) -> Self {
        let vert_shader =
            gpu.create_shader_module("marker.vert", include_bytes!("../target/marker.vert.spirv"));
        let frag_shader =
            gpu.create_shader_module("marker.frag", include_bytes!("../target/marker.frag.spirv"));

        let pipeline_layout =
            gpu.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("shape-render-pipeline-layout"),
                    push_constant_ranges: &[],
                    bind_group_layouts: &[globals.bind_group_layout()],
                });

        let pipeline = gpu
            .device()
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("shape-render-pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &vert_shader,
                    entry_point: "main",
                    buffers: &[MarkerVertex::descriptor()],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &frag_shader,
                    entry_point: "main",
                    targets: &[wgpu::ColorTargetState {
                        format: Gpu::SCREEN_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Cw,
                    cull_mode: Some(wgpu::Face::Back),
                    unclipped_depth: true,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: Gpu::DEPTH_FORMAT,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::Always,
                    stencil: wgpu::StencilState {
                        front: wgpu::StencilFaceState::IGNORE,
                        back: wgpu::StencilFaceState::IGNORE,
                        read_mask: 0,
                        write_mask: 0,
                    },
                    bias: wgpu::DepthBiasState {
                        constant: 0,
                        slope_scale: 0.0,
                        clamp: 0.0,
                    },
                }),
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview: None,
            });

        Self {
            pipeline,
            vertices: Arc::new(gpu.device().create_buffer(&wgpu::BufferDescriptor {
                label: "markers-vertex-buffer".into(),
                size: (mem::size_of::<MarkerVertex>() * Self::MAX_VERTICIES) as wgpu::BufferAddress,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::VERTEX,
                mapped_at_creation: false,
            })),
            vertex_count: 0,
            indices: Arc::new(gpu.device().create_buffer(&wgpu::BufferDescriptor {
                label: "markers-index-buffer".into(),
                size: (mem::size_of::<u32>() * Self::MAX_INDICES) as wgpu::BufferAddress,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::INDEX,
                mapped_at_creation: false,
            })),
            index_count: 0,
        }
    }

    pub fn vertex_span(&self) -> Range<u32> {
        0..self.vertex_count
    }

    pub fn index_span(&self) -> Range<u32> {
        0..self.index_count
    }

    pub fn vertex_buffer(&self) -> wgpu::BufferSlice {
        let sz = self.vertex_count as u64 * mem::size_of::<MarkerVertex>() as u64;
        self.vertices.slice(0..sz)
    }

    pub fn index_buffer(&self) -> wgpu::BufferSlice {
        let sz = self.index_count as u64 * mem::size_of::<MarkerVertex>() as u64;
        self.indices.slice(0..sz)
    }

    fn draw_point(
        view: &Matrix4<f64>,
        frame: &WorldSpaceFrame,
        pt: &MarkerPoint,
        vertices: &mut Vec<MarkerVertex>,
        indices: &mut Vec<u32>,
    ) {
        let sphere = Sphere::default().to_primitive(2);
        let center = (pt.position + frame.position().vec()).map(|v| v.f64());
        let s = pt.radius.f64();
        let base = vertices.len() as u32;
        for vertex in &sphere.verts {
            let pos = view * (center + vertex.position * s).to_homogeneous();
            vertices.push(MarkerVertex::new(pos.xyz(), vertex.normal, &pt.color));
        }
        for face in &sphere.faces {
            indices.push(base + face.index0);
            indices.push(base + face.index1);
            indices.push(base + face.index2);
        }
    }

    fn draw_box(
        view: &Matrix4<f64>,
        frame: &WorldSpaceFrame,
        aabb: &MarkerBox,
        vertices: &mut Vec<MarkerVertex>,
        indices: &mut Vec<u32>,
    ) {
        let x_axis = frame.facing() * Vector3::x_axis();
        let y_axis = frame.facing() * Vector3::y_axis();
        let z_axis = frame.facing() * Vector3::z_axis();
        let fp = frame.position().vec64();
        let facing = frame.facing().inverse();
        let hi_m = facing * aabb.aabb.hi().map(|v| v.f64());
        let lo_m = facing * aabb.aabb.lo().map(|v| v.f64());
        let hi_w = hi_m + fp;
        let lo_w = lo_m + fp;
        let hi_e = view * hi_w.to_homogeneous();
        let lo_e = view * lo_w.to_homogeneous();
        let lo = lo_e.xyz();
        let hi = hi_e.xyz();
        // let hi = aabb.aabb.hi().map(|v| v.f64());
        // let lo = aabb.aabb.lo().map(|v| v.f64());
        let a = [lo.x, hi.y, lo.z];
        let b = [hi.x, hi.y, lo.z];
        let c = [hi.x, lo.y, lo.z];
        let d = [hi.x, lo.y, hi.z];
        let e = [lo.x, hi.y, hi.z];
        let f = [lo.x, lo.y, hi.z];
        let lo = [lo.x, lo.y, lo.z];
        let hi = [hi.x, hi.y, hi.z];
        let faces = [
            ([lo, a, b, c], -z_axis),
            ([c, b, hi, d], x_axis),
            ([d, hi, e, f], z_axis),
            ([f, e, a, lo], -x_axis),
            ([a, e, hi, b], y_axis),
            ([f, lo, c, d], -y_axis),
        ];
        for (verts, normal) in faces {
            let base = vertices.len() as u32;
            for v in verts {
                let position = Vector3::new(v[0], v[1], v[2]);
                vertices.push(MarkerVertex::new(position.xyz(), normal.xyz(), &aabb.color));
            }
            for i in [0, 2, 1, 0, 3, 2] {
                indices.push(base + i);
            }
        }
    }

    fn draw_cylinder(
        view: &Matrix4<f64>,
        frame: &WorldSpaceFrame,
        cylinder: &MarkerCylinder,
        vertices: &mut Vec<MarkerVertex>,
        indices: &mut Vec<u32>,
    ) {
        // Positive y is up
        let cyl_len = cylinder.vector.map(|v| v.f64()).magnitude();
        let cyl = Cylinder::new(cyl_len, cylinder.radius.f64()).to_primitive(20);

        // Rotate from the y-up frame to whatever our actual direction is.
        let facing = if let Some(q) =
            UnitQuaternion::rotation_between(&Vector3::y(), &cylinder.vector.map(|v| v.f64()))
        {
            q
        } else {
            UnitQuaternion::from_axis_angle(&Vector3::x_axis(), PI)
        };

        // Rotate into frame such that y is still "up"
        let r = frame.facing() * facing;

        // Origin in world space; we'll take it into eye space below.
        let p = (cylinder.origin + frame.position().vec()).map(|v| v.f64());
        let base = vertices.len() as u32;
        for vert in &cyl.verts {
            let p0 = r * vert.position;
            let n0 = r * vert.normal;
            let p0 = view * (p + p0).to_homogeneous();
            vertices.push(MarkerVertex::new(p0.xyz(), n0, &cylinder.color));
        }
        for face in &cyl.faces {
            indices.push(base + face.index0);
            indices.push(base + face.index1);
            indices.push(base + face.index2);
        }
    }

    fn draw_arrow(
        view: &Matrix4<f64>,
        frame: &WorldSpaceFrame,
        arrow: &MarkerArrow,
        vertices: &mut Vec<MarkerVertex>,
        indices: &mut Vec<u32>,
    ) {
        // Positive y is up
        let total_len = arrow.vector.map(|v| v.f64()).magnitude();
        let cyl_len = (total_len - 1.).max(0.);
        let head_len = 1f64.min(total_len);
        let cyl = Cylinder::new(cyl_len, arrow.radius.f64()).to_primitive(20);
        let mut head =
            Cylinder::new_tapered(head_len, arrow.radius.f64() * 1.5, 0.0).to_primitive(20);
        for vert in head.verts.iter_mut() {
            vert.position.y += cyl_len;
        }

        // Rotate from the y-up frame to whatever our actual direction is.
        let facing = if let Some(q) =
            UnitQuaternion::rotation_between(&Vector3::y(), &arrow.vector.map(|v| v.f64()))
        {
            q
        } else {
            UnitQuaternion::from_axis_angle(&Vector3::x_axis(), PI)
        };

        // Rotate into frame such that y is still "up"
        let r = frame.facing() * facing;

        // Origin in world space; we'll take it into eye space below.
        let p = (arrow.origin + frame.position().vec()).map(|v| v.f64());

        for prim in &[cyl, head] {
            let base = vertices.len() as u32;
            for vert in &prim.verts {
                let p0 = r * vert.position;
                let n0 = r * vert.normal;
                let p0 = view * (p + p0).to_homogeneous();
                vertices.push(MarkerVertex::new(p0.xyz(), n0, &arrow.color));
            }
            for face in &prim.faces {
                indices.push(base + face.index0);
                indices.push(base + face.index1);
                indices.push(base + face.index2);
            }
        }
    }

    fn sys_upload_geometry(
        absolute_points: Query<(&EntityMarkers, &WorldSpaceFrame)>,
        mut markers: ResMut<Markers>,
        camera: Res<ScreenCamera>,
        gpu: Res<Gpu>,
        maybe_encoder: ResMut<Option<wgpu::CommandEncoder>>,
    ) {
        if let Some(encoder) = maybe_encoder.into_inner() {
            let mut upload_vertices = Vec::new();
            let mut upload_indices = Vec::new();
            let view = camera.view::<Meters>().to_homogeneous();
            for (ent_markers, frame) in absolute_points.iter() {
                for bx in ent_markers.boxes.values() {
                    Self::draw_box(&view, frame, bx, &mut upload_vertices, &mut upload_indices);
                }
                for pt in ent_markers.points.values() {
                    Self::draw_point(&view, frame, pt, &mut upload_vertices, &mut upload_indices);
                }
                for arrow in ent_markers.arrows.values() {
                    Self::draw_arrow(
                        &view,
                        frame,
                        arrow,
                        &mut upload_vertices,
                        &mut upload_indices,
                    );
                }
                for cylinder in ent_markers.cylinders.values() {
                    Self::draw_cylinder(
                        &view,
                        frame,
                        cylinder,
                        &mut upload_vertices,
                        &mut upload_indices,
                    );
                }
            }
            markers.vertex_count = upload_vertices.len() as u32;
            gpu.upload_slice_to(
                "marker-vertex-upload",
                &upload_vertices,
                markers.vertices.clone(),
                encoder,
            );
            markers.index_count = upload_indices.len() as u32;
            gpu.upload_slice_to(
                "marker-index-upload",
                &upload_indices,
                markers.indices.clone(),
                encoder,
            );
        }
    }

    fn sys_draw_markers(
        markers: Res<Markers>,
        globals: Res<GlobalParametersBuffer>,
        world: Res<WorldRenderPass>,
        maybe_encoder: ResMut<Option<wgpu::CommandEncoder>>,
    ) {
        if let Some(encoder) = maybe_encoder.into_inner() {
            let (color_attachments, depth_stencil_attachment) = world.offscreen_target_preserved();
            let render_pass_desc_ref = wgpu::RenderPassDescriptor {
                label: Some("shape-draw"),
                color_attachments: &color_attachments,
                depth_stencil_attachment,
            };
            let mut rpass = encoder.begin_render_pass(&render_pass_desc_ref);

            if markers.vertex_count > 0 {
                rpass.set_pipeline(&markers.pipeline);
                rpass.set_bind_group(Group::Globals.index(), globals.bind_group(), &[]);
                rpass.set_vertex_buffer(0, markers.vertex_buffer());
                rpass.set_index_buffer(markers.index_buffer(), wgpu::IndexFormat::Uint32);
                rpass.draw_indexed(markers.index_span(), 0, 0..1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
