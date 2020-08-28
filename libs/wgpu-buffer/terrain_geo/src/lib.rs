// This file is part of OpenFA.
//
// OpenFA is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// OpenFA is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with OpenFA.  If not, see <http://www.gnu.org/licenses/>.
mod icosahedron;
mod index_dependency_lut;
mod patch;
mod patch_tree;
mod patch_winding;
mod queue;
mod terrain_vertex;
mod tri_strip_indices;
mod wireframe_indices;

pub mod tile;

use crate::{
    index_dependency_lut::get_index_dependency_lut, patch_tree::PatchTree, tile::TileManager,
    tri_strip_indices::get_tri_strip_index_buffer, wireframe_indices::get_wireframe_index_buffer,
};
pub use crate::{patch_winding::PatchWinding, terrain_vertex::TerrainVertex};

use absolute_unit::Kilometers;
use camera::Camera;
use catalog::Catalog;
use failure::Fallible;
use geodesy::{Cartesian, GeoCenter, Graticule};
use gpu::{FrameStateTracker, GPU};
use nalgebra::{Matrix4, Point3};
use std::{
    mem,
    num::NonZeroU64,
    ops::Range,
    sync::{Arc, RwLock},
};
use tokio::{runtime::Runtime, sync::RwLock as AsyncRwLock};
use zerocopy::{AsBytes, FromBytes};

#[allow(unused)]
const DBG_COLORS_BY_LEVEL: [[f32; 3]; 19] = [
    [0.75, 0.25, 0.25],
    [0.25, 0.75, 0.75],
    [0.75, 0.42, 0.25],
    [0.25, 0.58, 0.75],
    [0.75, 0.58, 0.25],
    [0.25, 0.42, 0.75],
    [0.75, 0.75, 0.25],
    [0.25, 0.25, 0.75],
    [0.58, 0.75, 0.25],
    [0.42, 0.25, 0.75],
    [0.58, 0.25, 0.75],
    [0.42, 0.75, 0.25],
    [0.25, 0.75, 0.25],
    [0.75, 0.25, 0.75],
    [0.25, 0.75, 0.42],
    [0.75, 0.25, 0.58],
    [0.25, 0.75, 0.58],
    [0.75, 0.25, 0.42],
    [0.10, 0.75, 0.72],
];

pub(crate) struct CpuDetail {
    max_level: usize,
    target_refinement: f64,
    desired_patch_count: usize,
}

impl CpuDetail {
    fn new(max_level: usize, target_refinement: f64, desired_patch_count: usize) -> Self {
        Self {
            max_level,
            target_refinement,
            desired_patch_count,
        }
    }
}

pub enum CpuDetailLevel {
    Low,
    Medium,
    High,
    Ultra,
}

impl CpuDetailLevel {
    // max-level, target-refinement, buffer-size
    fn parameters(&self) -> CpuDetail {
        match self {
            Self::Low => CpuDetail::new(8, 150.0, 200),
            Self::Medium => CpuDetail::new(15, 150.0, 300),
            Self::High => CpuDetail::new(16, 150.0, 400),
            Self::Ultra => CpuDetail::new(17, 150.0, 500),
        }
    }
}

pub(crate) struct GpuDetail {
    // Number of tesselation subdivision steps to compute on the GPU each frame.
    subdivisions: usize,

    // The number of tiles to store on the GPU.
    tile_cache_size: u32,
}

impl GpuDetail {
    fn new(subdivisions: usize, tile_cache_size: u32) -> Self {
        Self {
            subdivisions,
            tile_cache_size,
        }
    }
}

pub enum GpuDetailLevel {
    Low,
    Medium,
    High,
    Ultra,
}

impl GpuDetailLevel {
    // subdivisions
    fn parameters(&self) -> GpuDetail {
        match self {
            Self::Low => GpuDetail::new(3, 128), // 64MiB
            Self::Medium => GpuDetail::new(4, 256),
            Self::High => GpuDetail::new(6, 512),
            Self::Ultra => GpuDetail::new(7, 1024),
        }
    }

    fn vertices_per_subdivision(d: usize) -> usize {
        (((2f64.powf(d as f64) + 1.0) * (2f64.powf(d as f64) + 2.0)) / 2.0).floor() as usize
    }
}

#[repr(C)]
#[derive(AsBytes, FromBytes, Debug, Copy, Clone)]
pub struct SubdivisionContext {
    // Number of unique vertices in a patch in the target subdivision level. e.g. Skip past this
    // many vertices in a buffer to get to the next patch.
    target_stride: u32,

    // The final target subdivision level of the subdivision process.
    target_subdivision_level: u32,

    // Number of vertices in a subdivision
    pad: [u32; 2],
}

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

    pad: [u32; 1],
}

pub struct TerrainGeoBuffer {
    // Maximum number of patches for the patch buffer.
    desired_patch_count: usize,

    patch_tree: PatchTree,
    patch_windings: Vec<PatchWinding>,

    tile_manager: TileManager,

    patch_upload_buffer: Arc<Box<wgpu::Buffer>>,

    subdivide_context: SubdivisionContext,
    target_vertex_buffer: Arc<Box<wgpu::Buffer>>,

    wireframe_index_buffers: Vec<wgpu::Buffer>,
    wireframe_index_ranges: Vec<Range<u32>>,

    tristrip_index_buffers: Vec<wgpu::Buffer>,
    tristrip_index_ranges: Vec<Range<u32>>,

    subdivisions: usize,
    subdivide_prepare_pipeline: wgpu::ComputePipeline,
    subdivide_prepare_bind_group: wgpu::BindGroup,
    subdivide_expand_pipeline: wgpu::ComputePipeline,
    subdivide_expand_bind_groups: Vec<(SubdivisionExpandContext, wgpu::BindGroup)>,
}

impl TerrainGeoBuffer {
    pub fn new(
        catalog: &Catalog,
        cpu_detail_level: CpuDetailLevel,
        gpu_detail_level: GpuDetailLevel,
        gpu: &mut GPU,
    ) -> Fallible<Arc<RwLock<Self>>> {
        let cpu_detail = cpu_detail_level.parameters();
        let gpu_detail = gpu_detail_level.parameters();

        let patch_tree = PatchTree::new(
            cpu_detail.max_level,
            cpu_detail.target_refinement,
            cpu_detail.desired_patch_count,
        );
        let tile_manager = TileManager::new(catalog, &gpu_detail, gpu)?;

        let mut patch_windings = Vec::with_capacity(cpu_detail.desired_patch_count);
        patch_windings.resize(cpu_detail.desired_patch_count, PatchWinding::Full);

        let patch_upload_stride = 3; // 3 vertices per patch in the upload buffer.
        let patch_upload_byte_size = TerrainVertex::mem_size() * patch_upload_stride;
        let patch_upload_buffer_size =
            (patch_upload_byte_size * cpu_detail.desired_patch_count) as wgpu::BufferAddress;
        let patch_upload_buffer = Arc::new(Box::new(gpu.device().create_buffer(
            &wgpu::BufferDescriptor {
                label: Some("terrain-geo-patch-vertex-buffer"),
                size: patch_upload_buffer_size,
                usage: wgpu::BufferUsage::STORAGE | wgpu::BufferUsage::COPY_DST,
                mapped_at_creation: false,
            },
        )));

        // Create the context buffer for uploading uniform data to our subdivision process.
        let subdivide_context = SubdivisionContext {
            target_stride: GpuDetailLevel::vertices_per_subdivision(gpu_detail.subdivisions) as u32,
            target_subdivision_level: gpu_detail.subdivisions as u32,
            pad: [0u32; 2],
        };
        let subdivide_context_buffer = Arc::new(Box::new(gpu.push_data(
            "subdivision-context",
            &subdivide_context,
            wgpu::BufferUsage::UNIFORM,
        )));

        // Create target vertex buffer.
        let target_patch_byte_size =
            mem::size_of::<TerrainVertex>() * subdivide_context.target_stride as usize;
        assert_eq!(target_patch_byte_size % 4, 0);
        let target_vertex_buffer_size =
            (target_patch_byte_size * cpu_detail.desired_patch_count) as wgpu::BufferAddress;
        let target_vertex_buffer = Arc::new(Box::new(gpu.device().create_buffer(
            &wgpu::BufferDescriptor {
                label: Some("terrain-geo-sub-vertex-buffer"),
                size: target_vertex_buffer_size,
                usage: wgpu::BufferUsage::STORAGE | wgpu::BufferUsage::VERTEX,
                mapped_at_creation: false,
            },
        )));

        let subdivide_prepare_bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("terrain-geo-subdivide-bind-group-layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::UniformBuffer {
                                dynamic: false,
                                min_binding_size: NonZeroU64::new(
                                    mem::size_of::<SubdivisionContext>() as u64,
                                ),
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::StorageBuffer {
                                dynamic: false,
                                readonly: false,
                                min_binding_size: NonZeroU64::new(target_vertex_buffer_size),
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::StorageBuffer {
                                dynamic: false,
                                readonly: true,
                                min_binding_size: NonZeroU64::new(patch_upload_buffer_size),
                            },
                            count: None,
                        },
                    ],
                });

        let subdivide_prepare_shader =
            gpu.create_shader_module(include_bytes!("../target/subdivide_prepare.comp.spirv"))?;
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
                    compute_stage: wgpu::ProgrammableStageDescriptor {
                        module: &subdivide_prepare_shader,
                        entry_point: "main",
                    },
                });

        let subdivide_prepare_bind_group =
            gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("terrain-geo-subdivide-bind-group"),
                layout: &subdivide_prepare_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(subdivide_context_buffer.slice(..)),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Buffer(target_vertex_buffer.slice(..)),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Buffer(patch_upload_buffer.slice(..)),
                    },
                ],
            });

        // Create the index dependence lut.
        let index_dependency_lut_buffer_size = (mem::size_of::<u32>()
            * get_index_dependency_lut(gpu_detail.subdivisions).len())
            as wgpu::BufferAddress;
        let index_dependency_lut_buffer = gpu.push_slice(
            "terrain-geo-index-dependency-lut",
            get_index_dependency_lut(gpu_detail.subdivisions),
            wgpu::BufferUsage::STORAGE,
        );

        let subdivide_expand_bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("terrain-geo-subdivide-prepare-bind-group-layout"),
                    entries: &[
                        // Subdivide context
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::UniformBuffer {
                                dynamic: false,
                                min_binding_size: NonZeroU64::new(
                                    mem::size_of::<SubdivisionContext>() as u64,
                                ),
                            },
                            count: None,
                        },
                        // Subdivide expand context
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::UniformBuffer {
                                dynamic: false,
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
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::StorageBuffer {
                                dynamic: false,
                                readonly: false,
                                min_binding_size: NonZeroU64::new(target_vertex_buffer_size),
                            },
                            count: None,
                        },
                        // Index dependency LUT
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::StorageBuffer {
                                dynamic: false,
                                readonly: true,
                                min_binding_size: NonZeroU64::new(index_dependency_lut_buffer_size),
                            },
                            count: None,
                        },
                    ],
                });

        let subdivide_expand_shader =
            gpu.create_shader_module(include_bytes!("../target/subdivide_expand.comp.spirv"))?;
        let subdivide_expand_pipeline =
            gpu.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("terrain-subdivide-expand-pipeline"),
                    layout: Some(&gpu.device().create_pipeline_layout(
                        &wgpu::PipelineLayoutDescriptor {
                            label: Some("terrain-subdivide-expand-pipeline-layout"),
                            push_constant_ranges: &[],
                            bind_group_layouts: &[&subdivide_expand_bind_group_layout],
                        },
                    )),
                    compute_stage: wgpu::ProgrammableStageDescriptor {
                        module: &subdivide_expand_shader,
                        entry_point: "main",
                    },
                });

        let mut subdivide_expand_bind_groups = Vec::new();
        for i in 1..gpu_detail.subdivisions + 1 {
            let expand_context = SubdivisionExpandContext {
                current_target_subdivision_level: i as u32,
                skip_vertices_in_patch: GpuDetailLevel::vertices_per_subdivision(i - 1) as u32,
                compute_vertices_in_patch: (GpuDetailLevel::vertices_per_subdivision(i)
                    - GpuDetailLevel::vertices_per_subdivision(i - 1))
                    as u32,
                pad: [0u32; 1],
            };
            let expand_context_buffer = gpu.push_data(
                "terrain-geo-expand-context-SUB",
                &expand_context,
                wgpu::BufferUsage::UNIFORM,
            );
            let subdivide_expand_bind_group =
                gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("terrain-geo-subdivide-expand-bind-group"),
                    layout: &subdivide_expand_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(
                                subdivide_context_buffer.slice(..),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Buffer(
                                expand_context_buffer.slice(..),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Buffer(target_vertex_buffer.slice(..)),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::Buffer(
                                index_dependency_lut_buffer.slice(..),
                            ),
                        },
                    ],
                });
            subdivide_expand_bind_groups.push((expand_context, subdivide_expand_bind_group));
        }

        // Create each of the 4 wireframe index buffers at this subdivision level.
        let wireframe_index_buffers = PatchWinding::all_windings()
            .iter()
            .map(|&winding| {
                gpu.push_slice(
                    "terrain-geo-wireframe-indices-SUB",
                    get_wireframe_index_buffer(gpu_detail.subdivisions, winding),
                    wgpu::BufferUsage::INDEX,
                )
            })
            .collect::<Vec<_>>();

        let wireframe_index_ranges = PatchWinding::all_windings()
            .iter()
            .map(|&winding| {
                0u32..get_wireframe_index_buffer(gpu_detail.subdivisions, winding).len() as u32
            })
            .collect::<Vec<_>>();

        // Create each of the 4 tristrip index buffers at this subdivision level.
        let tristrip_index_buffers = PatchWinding::all_windings()
            .iter()
            .map(|&winding| {
                gpu.push_slice(
                    "terrain-geo-tristrip-indices-SUB",
                    get_tri_strip_index_buffer(gpu_detail.subdivisions, winding),
                    wgpu::BufferUsage::INDEX,
                )
            })
            .collect::<Vec<_>>();

        let tristrip_index_ranges = PatchWinding::all_windings()
            .iter()
            .map(|&winding| {
                0u32..get_tri_strip_index_buffer(gpu_detail.subdivisions, winding).len() as u32
            })
            .collect::<Vec<_>>();

        Ok(Arc::new(RwLock::new(Self {
            desired_patch_count: cpu_detail.desired_patch_count,
            patch_tree,
            patch_windings,

            tile_manager,

            patch_upload_buffer,

            target_vertex_buffer,
            wireframe_index_buffers,
            wireframe_index_ranges,
            tristrip_index_buffers,
            tristrip_index_ranges,

            subdivisions: gpu_detail.subdivisions,
            subdivide_context,
            subdivide_prepare_pipeline,
            subdivide_prepare_bind_group,
            subdivide_expand_pipeline,
            subdivide_expand_bind_groups,
        })))
    }

    pub fn make_upload_buffer(
        &mut self,
        camera: &Camera,
        catalog: Arc<AsyncRwLock<Catalog>>,
        async_rt: &mut Runtime,
        gpu: &GPU,
        tracker: &mut FrameStateTracker,
    ) -> Fallible<()> {
        // TODO: keep these allocations across frames
        let mut live_patches = Vec::with_capacity(self.desired_patch_count);
        self.patch_tree.optimize_for_view(camera, &mut live_patches);
        assert!(live_patches.len() <= self.desired_patch_count);

        let mut verts = Vec::with_capacity(3 * self.desired_patch_count);
        self.tile_manager.begin_update();
        let scale = Matrix4::new_scaling(1_000.0);
        let view = camera.view::<Kilometers>();
        for (offset, (i, winding)) in live_patches.iter().enumerate() {
            self.patch_windings[offset] = *winding;
            let patch = self.patch_tree.get_patch(*i);
            if offset >= self.desired_patch_count {
                continue;
            }
            let [pw0, pw1, pw2] = patch.points();
            let nv0 = view.to_homogeneous() * pw0.coords.normalize().to_homogeneous();
            let nv1 = view.to_homogeneous() * pw1.coords.normalize().to_homogeneous();
            let nv2 = view.to_homogeneous() * pw2.coords.normalize().to_homogeneous();

            // project patch verts from global coordinates into view space.
            let vv0 = scale * view.to_homogeneous() * pw0.to_homogeneous();
            let vv1 = scale * view.to_homogeneous() * pw1.to_homogeneous();
            let vv2 = scale * view.to_homogeneous() * pw2.to_homogeneous();
            let pv0 = Point3::from(vv0.xyz());
            let pv1 = Point3::from(vv1.xyz());
            let pv2 = Point3::from(vv2.xyz());
            let cart0 = Cartesian::<GeoCenter, Kilometers>::from(pw0.coords);
            let cart1 = Cartesian::<GeoCenter, Kilometers>::from(pw1.coords);
            let cart2 = Cartesian::<GeoCenter, Kilometers>::from(pw2.coords);
            let g0 = Graticule::<GeoCenter>::from(cart0);
            let g1 = Graticule::<GeoCenter>::from(cart1);
            let g2 = Graticule::<GeoCenter>::from(cart2);

            self.tile_manager.note_required(&g0);
            self.tile_manager.note_required(&g1);
            self.tile_manager.note_required(&g2);

            verts.push(TerrainVertex::new(&pv0, &nv0.xyz(), &g0));
            verts.push(TerrainVertex::new(&pv1, &nv1.xyz(), &g1));
            verts.push(TerrainVertex::new(&pv2, &nv2.xyz(), &g2));
        }
        self.tile_manager
            .finish_update(catalog, async_rt, gpu, tracker);

        while verts.len() < 3 * self.desired_patch_count {
            verts.push(TerrainVertex::empty());
        }
        gpu.upload_slice_to(
            "terrain-geo-patch-vertex-upload-buffer",
            &verts,
            self.patch_upload_buffer.clone(),
            wgpu::BufferUsage::all(),
            tracker,
        );

        //println!("dt: {:?}", Instant::now() - loop_start);
        Ok(())
    }

    pub fn paint_atlas_indices(&self, encoder: wgpu::CommandEncoder) -> wgpu::CommandEncoder {
        self.tile_manager.paint_atlas_indices(encoder)
    }

    pub fn tesselate<'a>(
        &'a self,
        mut cpass: wgpu::ComputePass<'a>,
    ) -> Fallible<wgpu::ComputePass<'a>> {
        cpass.set_pipeline(&self.subdivide_prepare_pipeline);
        cpass.set_bind_group(0, &self.subdivide_prepare_bind_group, &[]);
        cpass.dispatch(3 * self.desired_patch_count as u32, 1, 1);

        for i in 0..self.subdivisions {
            let (expand, bind_group) = &self.subdivide_expand_bind_groups[i];
            let iteration_count =
                expand.compute_vertices_in_patch * self.desired_patch_count as u32;
            cpass.set_pipeline(&self.subdivide_expand_pipeline);
            cpass.set_bind_group(0, bind_group, &[]);
            cpass.dispatch(iteration_count, 1, 1);
        }

        // for bind_group in self.tile_manager.height_bind_groups() {
        //     cpass.set_pipeline(&self.apply_height_pipeline);
        //     cpass.set_bind_group(0, bind_group, &[]);
        //     cpass.dispatch()
        // }

        Ok(cpass)
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        self.tile_manager.bind_group_layout()
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        self.tile_manager.bind_group()
    }

    pub fn num_patches(&self) -> i32 {
        self.desired_patch_count as i32
    }

    pub fn vertex_buffer(&self) -> wgpu::BufferSlice {
        self.target_vertex_buffer.slice(..)
    }

    pub fn patch_upload_buffer(&self) -> &wgpu::Buffer {
        &self.patch_upload_buffer
    }

    pub fn patch_vertex_buffer_offset(&self, patch_number: i32) -> i32 {
        assert!(patch_number >= 0);
        (patch_number as u32 * self.subdivide_context.target_stride) as i32
    }

    pub fn patch_winding(&self, patch_number: i32) -> PatchWinding {
        assert!(patch_number >= 0);
        assert!(patch_number < self.num_patches());
        self.patch_windings[patch_number as usize]
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_subdivision_vertex_counts() {
        let expect = vec![3, 6, 15, 45, 153, 561, 2145, 8385];
        for (i, &value) in expect.iter().enumerate() {
            assert_eq!(value, GpuDetailLevel::vertices_per_subdivision(i));
        }
    }

    #[test]
    fn test_built_index_lut() {
        // let lut = TerrainGeoBuffer::build_index_dependence_lut();
        // for (i, (j, k)) in lut.iter().skip(3).enumerate() {
        //     println!("at offset: {}: {}, {}", i + 3, j, k);
        //     assert!((i as u32) + 3 > *j);
        //     assert!((i as u32) + 3 > *k);
        // }
        // assert_eq!(lut[0], (0, 0));
        // assert_eq!(lut[1], (0, 0));
        // assert_eq!(lut[2], (0, 0));
        // assert_eq!(lut[3], (0, 1));
        // assert_eq!(lut[4], (1, 2));
        // assert_eq!(lut[5], (2, 0));
        for i in 0..300 {
            let patch_id = i / 3;
            let offset = i % 3;
            assert_eq!(i, patch_id * 3 + offset);
        }
    }
}
