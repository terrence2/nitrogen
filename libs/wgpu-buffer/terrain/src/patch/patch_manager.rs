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
use crate::{
    patch::{PatchIndex, PatchTree, PatchWinding, TerrainUploadVertex, TerrainVertex},
    tables::{get_index_dependency_lut, get_tri_strip_index_buffer, get_wireframe_index_buffer},
    GpuDetail, VisiblePatch,
};
use absolute_unit::{degrees, meters, radians, Angle, Kilometers, Radians};
use anyhow::Result;
use camera::Camera;
use geodesy::{Cartesian, GeoCenter, Graticule};
use gpu::{Gpu, UploadTracker};
use nalgebra::{Matrix4, Point3};
use static_assertions::{assert_eq_align, assert_eq_size};
use std::{f64::consts::FRAC_PI_2, fmt, mem, num::NonZeroU64, ops::Range, sync::Arc};
use zerocopy::{AsBytes, FromBytes};

#[repr(C)]
#[derive(AsBytes, FromBytes, Debug, Copy, Clone)]
pub struct SubdivisionContext {
    // Number of unique vertices in a patch in the target subdivision level. e.g. Skip past this
    // many vertices in a buffer to get to the next patch.
    target_stride: u32,

    // The final target subdivision level of the subdivision process.
    target_subdivision_level: u32,
}
assert_eq_size!(SubdivisionContext, [u32; 2]);
assert_eq_align!(SubdivisionContext, [f32; 4]);

#[repr(C)]
#[derive(AsBytes, FromBytes, Debug, Copy, Clone)]
pub struct SubdivisionExpandContext {
    // The target subdivision level after this expand call.
    current_target_subdivision_level: u32,

    // The number of vertices to skip at the start of each patch. This is always the number of
    // vertices in the previous subdivision level.
    skip_vertices_in_patch: u32,

    // The number of vertices to compute per patch in this expand phase. This will always be the
    // number of vertices in this subdivision level *minus* the number of vertices in the previous
    // expansion level.
    compute_vertices_in_patch: u32,
}
assert_eq_size!(SubdivisionExpandContext, [u32; 3]);
assert_eq_align!(SubdivisionExpandContext, [f32; 4]);

pub(crate) struct PatchManager {
    // The frame-coherent optimal patch tree. e.g. the hard part.
    patch_tree: PatchTree,

    // Hot cache for CPU patch and vertex generation. Note that we read from live_patches
    // for patch-winding when doing the draw later.
    desired_patch_count: usize,
    live_patches: Vec<(PatchIndex, PatchWinding)>,
    live_vertices: Vec<TerrainUploadVertex>,

    // CPU generated patch corner vertices. Input to subdivision.
    patch_upload_buffer: Arc<wgpu::Buffer>,

    // Metadata about the subdivision. We upload this in a buffer, then save the uploaded context
    // in our manager as a reference so that the CPU and GPU can share some constant (per run)
    // metadata about the subdivision and buffers that are not hard-coded.
    subdivide_context: SubdivisionContext,

    // Subdivision process.
    subdivide_prepare_pipeline: wgpu::ComputePipeline,
    subdivide_prepare_bind_group: wgpu::BindGroup,
    subdivide_expand_pipeline: wgpu::ComputePipeline,
    subdivide_expand_bind_groups: Vec<(SubdivisionExpandContext, wgpu::BindGroup)>,

    // Height displacement bind group for use by the height tiles.
    displace_height_bind_group_layout: wgpu::BindGroupLayout,
    displace_height_bind_group: wgpu::BindGroup,

    // The final buffer containing the fully tessellated and height-offset vertices.
    target_vertex_count: u32,
    target_vertex_buffer: Arc<wgpu::Buffer>,

    // Index buffers for each patch size and winding.
    wireframe_index_buffers: Vec<wgpu::Buffer>,
    wireframe_index_ranges: Vec<Range<u32>>,
    tristrip_index_buffers: Vec<wgpu::Buffer>,
    tristrip_index_ranges: Vec<Range<u32>>,
}

impl fmt::Debug for PatchManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PatchManger")
    }
}

impl PatchManager {
    pub fn new(
        max_level: usize,
        target_refinement: f64,
        desired_patch_count: usize,
        max_subdivisions: usize,
        gpu: &Gpu,
    ) -> Result<Self> {
        let patch_upload_stride = 3; // 3 vertices per patch in the upload buffer.
        let patch_upload_byte_size = TerrainUploadVertex::mem_size() * patch_upload_stride;
        let patch_upload_buffer_size =
            (patch_upload_byte_size * desired_patch_count) as wgpu::BufferAddress;
        let patch_upload_buffer = Arc::new(gpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("terrain-geo-patch-vertex-buffer"),
            size: patch_upload_buffer_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));

        // Create the context buffer for uploading uniform data to our subdivision process.
        let subdivide_context = SubdivisionContext {
            target_stride: GpuDetail::vertices_per_subdivision(max_subdivisions) as u32,
            target_subdivision_level: max_subdivisions as u32,
        };
        let subdivide_context_buffer = Arc::new(gpu.push_data(
            "subdivision-context",
            &subdivide_context,
            wgpu::BufferUsages::UNIFORM,
        ));

        // Create target vertex buffer.
        let target_vertex_count = subdivide_context.target_stride * desired_patch_count as u32;
        let target_patch_byte_size =
            TerrainVertex::mem_size(0) * subdivide_context.target_stride as usize;
        assert_eq!(target_patch_byte_size % 4, 0);
        let target_vertex_buffer_size =
            (target_patch_byte_size * desired_patch_count) as wgpu::BufferAddress;
        let target_vertex_buffer = Arc::new(gpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("terrain-geo-sub-vertex-buffer"),
            size: target_vertex_buffer_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::VERTEX,
            mapped_at_creation: false,
        }));

        let subdivide_prepare_bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("terrain-geo-subdivide-bind-group-layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: NonZeroU64::new(
                                    mem::size_of::<SubdivisionContext>() as u64,
                                ),
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: false },
                                has_dynamic_offset: false,
                                min_binding_size: NonZeroU64::new(target_vertex_buffer_size),
                                //min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: NonZeroU64::new(patch_upload_buffer_size),
                                //min_binding_size: None,
                            },
                            count: None,
                        },
                    ],
                });

        let subdivide_prepare_shader = gpu.create_shader_module(
            "subdivide_prepare.comp",
            include_bytes!("../../target/subdivide_prepare.comp.spirv"),
        )?;
        let subdivide_prepare_pipeline =
            gpu.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("terrain-subdivide-prepare-pipeline"),
                    layout: Some(&gpu.device().create_pipeline_layout(
                        &wgpu::PipelineLayoutDescriptor {
                            label: Some("terrain-subdivide-prepare-pipeline-layout"),
                            push_constant_ranges: &[],
                            bind_group_layouts: &[&subdivide_prepare_bind_group_layout],
                        },
                    )),
                    module: &subdivide_prepare_shader,
                    entry_point: "main",
                });

        let subdivide_prepare_bind_group =
            gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("terrain-geo-subdivide-bind-group"),
                layout: &subdivide_prepare_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &subdivide_context_buffer,
                            offset: 0,
                            size: NonZeroU64::new(mem::size_of::<SubdivisionContext>() as u64),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &target_vertex_buffer,
                            offset: 0,
                            size: None,
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &patch_upload_buffer,
                            offset: 0,
                            size: None,
                        }),
                    },
                ],
            });

        // Create the index dependence lut.
        let index_dependency_lut_buffer_size = (mem::size_of::<u32>()
            * get_index_dependency_lut(max_subdivisions).len())
            as wgpu::BufferAddress;
        let index_dependency_lut_buffer = gpu.push_slice(
            "terrain-geo-index-dependency-lut",
            get_index_dependency_lut(max_subdivisions),
            wgpu::BufferUsages::STORAGE,
        );

        let subdivide_expand_bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("terrain-geo-subdivide-prepare-bind-group-layout"),
                    entries: &[
                        // Subdivide context
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: NonZeroU64::new(
                                    mem::size_of::<SubdivisionContext>() as u64,
                                ),
                            },
                            count: None,
                        },
                        // Subdivide expand context
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: NonZeroU64::new(mem::size_of::<
                                    SubdivisionExpandContext,
                                >(
                                )
                                    as u64),
                            },
                            count: None,
                        },
                        // Target vertex buffer
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: false },
                                has_dynamic_offset: false,
                                min_binding_size: NonZeroU64::new(target_vertex_buffer_size),
                            },
                            count: None,
                        },
                        // Index dependency LUT
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: NonZeroU64::new(index_dependency_lut_buffer_size),
                            },
                            count: None,
                        },
                    ],
                });

        let subdivide_expand_shader = gpu.create_shader_module(
            "subdivide_expand.comp",
            include_bytes!("../../target/subdivide_expand.comp.spirv"),
        )?;
        let subdivide_expand_pipeline =
            gpu.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("terrain-geo-subdivide-expand-pipeline"),
                    layout: Some(&gpu.device().create_pipeline_layout(
                        &wgpu::PipelineLayoutDescriptor {
                            label: Some("terrain-geo-subdivide-expand-pipeline-layout"),
                            push_constant_ranges: &[],
                            bind_group_layouts: &[&subdivide_expand_bind_group_layout],
                        },
                    )),
                    module: &subdivide_expand_shader,
                    entry_point: "main",
                });

        let mut subdivide_expand_bind_groups = Vec::new();
        for i in 1..max_subdivisions + 1 {
            let expand_context = SubdivisionExpandContext {
                current_target_subdivision_level: i as u32,
                skip_vertices_in_patch: GpuDetail::vertices_per_subdivision(i - 1) as u32,
                compute_vertices_in_patch: (GpuDetail::vertices_per_subdivision(i)
                    - GpuDetail::vertices_per_subdivision(i - 1))
                    as u32,
            };
            let expand_context_buffer = gpu.push_data(
                "terrain-geo-expand-context-SUB",
                &expand_context,
                wgpu::BufferUsages::UNIFORM,
            );
            let subdivide_expand_bind_group =
                gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("terrain-geo-subdivide-expand-bind-group"),
                    layout: &subdivide_expand_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &subdivide_context_buffer,
                                offset: 0,
                                size: None,
                            }),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &expand_context_buffer,
                                offset: 0,
                                size: None,
                            }),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &target_vertex_buffer,
                                offset: 0,
                                size: None,
                            }),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &index_dependency_lut_buffer,
                                offset: 0,
                                size: None,
                            }),
                        },
                    ],
                });
            subdivide_expand_bind_groups.push((expand_context, subdivide_expand_bind_group));
        }

        let displace_height_bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("terrain-geo-displace-height-bind-group-layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: NonZeroU64::new(target_vertex_buffer_size),
                        },
                        count: None,
                    }],
                });
        let displace_height_bind_group =
            gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("terrain-geo-displace-height-bind-group"),
                layout: &displace_height_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &target_vertex_buffer,
                        offset: 0,
                        size: None,
                    }),
                }],
            });

        let patch_tree = PatchTree::new(max_level, target_refinement, desired_patch_count);

        // Create each of the 4 wireframe index buffers at this subdivision level.
        let wireframe_index_buffers = PatchWinding::all_windings()
            .iter()
            .map(|&winding| {
                gpu.push_slice(
                    "terrain-geo-wireframe-indices-SUB",
                    get_wireframe_index_buffer(max_subdivisions, winding),
                    wgpu::BufferUsages::INDEX,
                )
            })
            .collect::<Vec<_>>();

        let wireframe_index_ranges = PatchWinding::all_windings()
            .iter()
            .map(|&winding| {
                0u32..get_wireframe_index_buffer(max_subdivisions, winding).len() as u32
            })
            .collect::<Vec<_>>();

        // Create each of the 4 tristrip index buffers at this subdivision level.
        let tristrip_index_buffers = PatchWinding::all_windings()
            .iter()
            .map(|&winding| {
                gpu.push_slice(
                    "terrain-geo-tristrip-indices-SUB",
                    get_tri_strip_index_buffer(max_subdivisions, winding),
                    wgpu::BufferUsages::INDEX,
                )
            })
            .collect::<Vec<_>>();

        let tristrip_index_ranges = PatchWinding::all_windings()
            .iter()
            .map(|&winding| {
                0u32..get_tri_strip_index_buffer(max_subdivisions, winding).len() as u32
            })
            .collect::<Vec<_>>();

        let live_patches = Vec::with_capacity(desired_patch_count);
        let live_vertices = Vec::with_capacity(3 * desired_patch_count);

        Ok(PatchManager {
            patch_tree,
            desired_patch_count,
            live_patches,
            live_vertices,
            patch_upload_buffer,
            subdivide_context,
            subdivide_prepare_pipeline,
            subdivide_prepare_bind_group,
            subdivide_expand_pipeline,
            subdivide_expand_bind_groups,
            displace_height_bind_group_layout,
            displace_height_bind_group,
            target_vertex_count,
            target_vertex_buffer,
            wireframe_index_buffers,
            wireframe_index_ranges,
            tristrip_index_buffers,
            tristrip_index_ranges,
        })
    }

    // Detect when a patch crosses a seam and re-order the graticules so that it overlaps,
    // preventing what will become texture coordinates from going backwards.
    fn relap_for_seam(
        lon0: &mut Angle<Radians>,
        lon1: &mut Angle<Radians>,
        lon2: &mut Angle<Radians>,
    ) {
        const LIM: f64 = FRAC_PI_2;
        if *lon0 > radians!(LIM) && (*lon1 < radians!(-LIM) || *lon2 < radians!(-LIM))
            || *lon1 > radians!(LIM) && (*lon0 < radians!(-LIM) || *lon2 < radians!(-LIM))
            || *lon2 > radians!(LIM) && (*lon0 < radians!(-LIM) || *lon1 < radians!(-LIM))
        {
            if lon0.sign() < 0 {
                *lon0 += degrees!(360);
            }
            if lon1.sign() < 0 {
                *lon1 += degrees!(360);
            }
            if lon2.sign() < 0 {
                *lon2 += degrees!(360);
            }
        }
    }

    pub fn track_state_changes(
        &mut self,
        camera: &Camera,
        optimize_camera: &Camera,
        visible_regions: &mut Vec<VisiblePatch>,
    ) -> Result<()> {
        // Select optimal live patches from our coherent patch tree.
        self.live_patches.clear();
        self.patch_tree
            .optimize_for_view(optimize_camera, &mut self.live_patches);
        assert!(self.live_patches.len() <= self.desired_patch_count);

        // Build CPU vertices for upload. Make sure to track visibility for our tile loader.
        self.live_vertices.clear();
        let scale = Matrix4::new_scaling(1_000.0);
        let view = camera.view::<Kilometers>();
        for (offset, (i, _)) in self.live_patches.iter().enumerate() {
            if offset >= self.desired_patch_count {
                continue;
            }
            let patch = self.patch_tree.get_patch(*i);

            // Points in geocenter KM f64 for precision reasons.
            let [pw0, pw1, pw2] = patch.points();

            // Move normals into view space, still in KM f64.
            let nv0 = view.to_homogeneous() * pw0.coords.normalize().to_homogeneous();
            let nv1 = view.to_homogeneous() * pw1.coords.normalize().to_homogeneous();
            let nv2 = view.to_homogeneous() * pw2.coords.normalize().to_homogeneous();

            // Move verts from global coordinates into view space, meters in f64.
            let vv0 = scale * view.to_homogeneous() * pw0.to_homogeneous();
            let vv1 = scale * view.to_homogeneous() * pw1.to_homogeneous();
            let vv2 = scale * view.to_homogeneous() * pw2.to_homogeneous();
            let pv0 = Point3::from(vv0.xyz());
            let pv1 = Point3::from(vv1.xyz());
            let pv2 = Point3::from(vv2.xyz());

            // Convert from geocenter f64 kilometers into graticules.
            let cart0 = Cartesian::<GeoCenter, Kilometers>::from(pw0.coords);
            let cart1 = Cartesian::<GeoCenter, Kilometers>::from(pw1.coords);
            let cart2 = Cartesian::<GeoCenter, Kilometers>::from(pw2.coords);
            let mut g0 = Graticule::<GeoCenter>::from(cart0);
            let mut g1 = Graticule::<GeoCenter>::from(cart1);
            let mut g2 = Graticule::<GeoCenter>::from(cart2);
            // FIXME: we're using a different coordinate system somewhere, but not sure where.
            g0.longitude = -g0.longitude;
            g1.longitude = -g1.longitude;
            g2.longitude = -g2.longitude;
            Self::relap_for_seam(&mut g0.longitude, &mut g1.longitude, &mut g2.longitude);

            // Use the patch vertices to sample the tile tree, re-using the existing visibility and
            // solid-angle calculations to avoid having to re-do them for the patch tree as well.
            let segments = 2i32.pow(self.target_patch_subdivision_level());
            let edge_length = meters!((pv0 - pv1).magnitude() / segments as f64);
            visible_regions.push(VisiblePatch {
                g0,
                g1,
                g2,
                edge_length,
            });

            self.live_vertices
                .push(TerrainUploadVertex::new(&pv0, &nv0.xyz(), &g0));
            self.live_vertices
                .push(TerrainUploadVertex::new(&pv1, &nv1.xyz(), &g1));
            self.live_vertices
                .push(TerrainUploadVertex::new(&pv2, &nv2.xyz(), &g2));
        }
        while self.live_vertices.len() < 3 * self.desired_patch_count {
            self.live_vertices.push(TerrainUploadVertex::empty());
        }

        //println!("dt: {:?}", Instant::now() - loop_start);
        Ok(())
    }

    pub fn ensure_uploaded(&self, gpu: &Gpu, tracker: &UploadTracker) {
        gpu.upload_slice_to(
            "terrain-geo-patch-vertex-upload-buffer",
            &self.live_vertices,
            self.patch_upload_buffer.clone(),
            tracker,
        );
    }

    pub fn tessellate<'a>(&'a self, mut cpass: wgpu::ComputePass<'a>) -> wgpu::ComputePass<'a> {
        // Copy our upload buffer into seed positions for subdivisions.
        let patch_count = 3 * self.desired_patch_count as u32;
        assert!(patch_count < u16::MAX as u32);
        cpass.set_pipeline(&self.subdivide_prepare_pipeline);
        cpass.set_bind_group(0, &self.subdivide_prepare_bind_group, &[]);
        cpass.dispatch(patch_count, 1, 1);

        // Iterative subdivision by recursion level
        cpass.set_pipeline(&self.subdivide_expand_pipeline);
        for i in 0usize..self.target_patch_subdivision_level() as usize {
            let (expand, bind_group) = &self.subdivide_expand_bind_groups[i];
            let iteration_count =
                expand.compute_vertices_in_patch * self.desired_patch_count as u32;
            const WORKGROUP_WIDTH: u32 = 65536;
            let wg_x = (iteration_count % WORKGROUP_WIDTH).max(1);
            let wg_y = (iteration_count / WORKGROUP_WIDTH).max(1);
            cpass.set_bind_group(0, bind_group, &[]);
            cpass.dispatch(wg_x, wg_y, 1);
        }

        cpass
    }

    pub(crate) fn displace_height_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.displace_height_bind_group_layout
    }

    pub(crate) fn displace_height_bind_group(&self) -> &wgpu::BindGroup {
        &self.displace_height_bind_group
    }

    pub(crate) fn target_vertex_count(&self) -> u32 {
        self.target_vertex_count
    }

    pub(crate) fn patch_winding(&self, patch_number: i32) -> PatchWinding {
        assert!(patch_number >= 0);
        if patch_number < self.live_patches.len() as i32 {
            self.live_patches[patch_number as usize].1
        } else {
            PatchWinding::Full
        }
    }

    pub fn patch_vertex_buffer_offset(&self, patch_number: i32) -> i32 {
        assert!(patch_number >= 0);
        (patch_number as u32 * self.subdivide_context.target_stride) as i32
    }

    pub fn target_patch_subdivision_level(&self) -> u32 {
        self.subdivide_context.target_subdivision_level
    }

    pub fn num_patches(&self) -> i32 {
        self.desired_patch_count as i32
    }

    pub(crate) fn vertex_buffer(&self) -> wgpu::BufferSlice {
        self.target_vertex_buffer.slice(..)
    }

    pub fn wireframe_index_buffer(&self, winding: PatchWinding) -> wgpu::BufferSlice {
        self.wireframe_index_buffers[winding.index()].slice(..)
    }

    pub fn wireframe_index_range(&self, winding: PatchWinding) -> Range<u32> {
        self.wireframe_index_ranges[winding.index()].clone()
    }

    pub fn tristrip_index_buffer(&self, winding: PatchWinding) -> wgpu::BufferSlice {
        self.tristrip_index_buffers[winding.index()].slice(..)
    }

    pub fn tristrip_index_range(&self, winding: PatchWinding) -> Range<u32> {
        self.tristrip_index_ranges[winding.index()].clone()
    }
}
