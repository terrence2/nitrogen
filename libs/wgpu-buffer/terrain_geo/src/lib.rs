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
mod patch;
mod tables;

pub mod tile;

pub use crate::patch::{PatchWinding, TerrainVertex};
use crate::{patch::PatchManager, tile::TileManager};

use absolute_unit::{Length, Meters};
use camera::Camera;
use catalog::Catalog;
use command::Command;
use commandable::{command, commandable, Commandable};
use failure::Fallible;
use geodesy::{GeoCenter, Graticule};
use global_data::GlobalParametersBuffer;
use gpu::{texture_format_size, UploadTracker, GPU};
use image::{ImageBuffer, Rgb};
use shader_shared::Group;
use std::{ops::Range, sync::Arc};
use tokio::{runtime::Runtime, sync::RwLock as AsyncRwLock};
use zerocopy::LayoutVerified;

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
            Self::Low => GpuDetail::new(3, 32), // 64MiB
            Self::Medium => GpuDetail::new(4, 64),
            Self::High => GpuDetail::new(6, 128),
            Self::Ultra => GpuDetail::new(7, 256),
        }
    }

    fn vertices_per_subdivision(d: usize) -> usize {
        (((2f64.powf(d as f64) + 1.0) * (2f64.powf(d as f64) + 2.0)) / 2.0).floor() as usize
    }
}

pub(crate) struct VisiblePatch {
    g0: Graticule<GeoCenter>,
    g1: Graticule<GeoCenter>,
    g2: Graticule<GeoCenter>,
    edge_length: Length<Meters>,
}

#[derive(Commandable)]
pub struct TerrainGeoBuffer {
    patch_manager: PatchManager,
    tile_manager: TileManager,

    // Cache allocation for transferring visible allocations from patches to tiles.
    visible_regions: Vec<VisiblePatch>,

    deferred_texture_pipeline: wgpu::RenderPipeline,
    deferred_texture: wgpu::Texture,
    deferred_texture_view: wgpu::TextureView,
    empty_bind_group: wgpu::BindGroup,
    take_deferred_texture_snapshot: bool,
}

#[commandable]
impl TerrainGeoBuffer {
    const DEFERRED_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;
    const DEFERRED_TEXTURE_DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
    const NORMAL_ACCUMULATION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Sint;
    const COLOR_ACCUMULATION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Uint;

    pub fn new(
        catalog: &Catalog,
        cpu_detail_level: CpuDetailLevel,
        gpu_detail_level: GpuDetailLevel,
        globals_buffer: &GlobalParametersBuffer,
        gpu: &mut GPU,
    ) -> Fallible<Self> {
        let cpu_detail = cpu_detail_level.parameters();
        let gpu_detail = gpu_detail_level.parameters();

        let patch_manager = PatchManager::new(
            cpu_detail.max_level,
            cpu_detail.target_refinement,
            cpu_detail.desired_patch_count,
            gpu_detail.subdivisions,
            gpu,
        )?;

        let tile_manager = TileManager::new(
            patch_manager.displace_height_bind_group_layout(),
            catalog,
            &gpu_detail,
            gpu,
        )?;

        let empty_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("empty-bind-group-layout"),
                    entries: &[],
                });
        let empty_bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("empty-bind-group"),
            layout: &empty_layout,
            entries: &[],
        });
        let (deferred_texture, deferred_texture_view) = Self::_make_deferred_texture_target(gpu);
        let deferred_texture_pipeline =
            gpu.device()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("deferred-texture-pipeline"),
                    layout: Some(&gpu.device().create_pipeline_layout(
                        &wgpu::PipelineLayoutDescriptor {
                            label: Some("deferred-texture-pipeline-layout"),
                            push_constant_ranges: &[],
                            bind_group_layouts: &[
                                globals_buffer.bind_group_layout(),
                                &empty_layout, // atmosphere otherwise
                                tile_manager.bind_group_layout(),
                            ],
                        },
                    )),
                    vertex_stage: wgpu::ProgrammableStageDescriptor {
                        module: &gpu.create_shader_module(include_bytes!(
                            "../target/draw_deferred_texture.vert.spirv"
                        ))?,
                        entry_point: "main",
                    },
                    fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
                        module: &gpu.create_shader_module(include_bytes!(
                            "../target/draw_deferred_texture.frag.spirv"
                        ))?,
                        entry_point: "main",
                    }),
                    rasterization_state: Some(wgpu::RasterizationStateDescriptor {
                        front_face: wgpu::FrontFace::Cw,
                        cull_mode: wgpu::CullMode::Back,
                        depth_bias: 0,
                        depth_bias_slope_scale: 0.0,
                        depth_bias_clamp: 0.0,
                        clamp_depth: false,
                    }),
                    primitive_topology: wgpu::PrimitiveTopology::TriangleStrip,
                    color_states: &[wgpu::ColorStateDescriptor {
                        format: Self::DEFERRED_TEXTURE_FORMAT,
                        color_blend: wgpu::BlendDescriptor::REPLACE,
                        alpha_blend: wgpu::BlendDescriptor::REPLACE,
                        write_mask: wgpu::ColorWrite::ALL,
                    }],
                    depth_stencil_state: None,
                    vertex_state: wgpu::VertexStateDescriptor {
                        index_format: wgpu::IndexFormat::Uint32,
                        vertex_buffers: &[TerrainVertex::descriptor()],
                    },
                    sample_count: 1,
                    sample_mask: !0,
                    alpha_to_coverage_enabled: false,
                });

        Ok(Self {
            patch_manager,
            tile_manager,
            visible_regions: Vec::new(),
            deferred_texture_pipeline,
            deferred_texture,
            deferred_texture_view,
            empty_bind_group,
            take_deferred_texture_snapshot: false,
        })
    }

    fn _make_deferred_texture_target(gpu: &GPU) -> (wgpu::Texture, wgpu::TextureView) {
        let sz = gpu.physical_size();
        let target = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("deferred-texture-target"),
            size: wgpu::Extent3d {
                width: sz.width as u32,
                height: sz.height as u32,
                depth: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEFERRED_TEXTURE_FORMAT,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT | wgpu::TextureUsage::COPY_SRC,
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor {
            label: Some("deferred-texture-target-view"),
            format: None,
            dimension: None,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        });
        (target, view)
    }

    pub fn note_resize(&mut self, gpu: &GPU) {
        let (target, view) = Self::_make_deferred_texture_target(gpu);
        self.deferred_texture = target;
        self.deferred_texture_view = view;
    }

    pub fn make_upload_buffer(
        &mut self,
        camera: &Camera,
        catalog: Arc<AsyncRwLock<Catalog>>,
        async_rt: &mut Runtime,
        gpu: &mut GPU,
        tracker: &mut UploadTracker,
    ) -> Fallible<()> {
        // Upload patches and capture visibility regions.
        self.visible_regions.clear();
        self.patch_manager
            .make_upload_buffer(camera, gpu, tracker, &mut self.visible_regions)?;

        // Dispatch visibility to tiles so that they can manage the actively loaded set.
        self.tile_manager.begin_update();
        for visible_patch in &self.visible_regions {
            self.tile_manager.note_required(visible_patch);
        }
        self.tile_manager
            .finish_update(catalog, async_rt, gpu, tracker);

        if self.take_deferred_texture_snapshot {
            self.capture_and_save_deferred_texture_snapshot(async_rt, gpu)?;
            self.take_deferred_texture_snapshot = false;
        }

        Ok(())
    }

    fn capture_and_save_deferred_texture_snapshot(
        &mut self,
        async_rt: &mut Runtime,
        gpu: &mut GPU,
    ) -> Fallible<()> {
        fn write_image(extent: wgpu::Extent3d, format: wgpu::TextureFormat, data: Vec<u8>) {
            let pix_cnt = extent.width as usize * extent.height as usize;
            let img_len = pix_cnt * 3;
            let samples = LayoutVerified::<&[u8], [f32]>::new_slice(&data).expect("as [f32]");
            let src_stride = GPU::stride_for_row_size(extent.width * texture_format_size(format))
                / texture_format_size(format);
            let mut data = vec![0u8; img_len];
            for x in 0..extent.width as usize {
                for y in 0..extent.height as usize {
                    let src_offset = 4 * (x + (y * src_stride as usize));
                    let dst_offset = 3 * (x + (y * extent.width as usize));
                    let r = (samples[src_offset] * 255.0).floor() as u8;
                    let g = (samples[src_offset + 1] * 255.0).floor() as u8;
                    data[dst_offset] = r;
                    data[dst_offset + 1] = g;
                    data[dst_offset + 2] = 0;
                }
            }
            let img = ImageBuffer::<Rgb<u8>, _>::from_raw(extent.width, extent.height, data)
                .expect("built image");
            println!("writing to __dump__/terrain_geo_deferred_texture.png");
            img.save("__dump__/terrain_geo_deferred_texture.png")
                .expect("wrote file");
        }
        Ok(GPU::dump_texture(
            &self.deferred_texture,
            gpu.attachment_extent(),
            Self::DEFERRED_TEXTURE_FORMAT,
            async_rt,
            gpu,
            Box::new(write_image),
        )?)
    }

    pub fn paint_atlas_indices(&self, encoder: wgpu::CommandEncoder) -> wgpu::CommandEncoder {
        self.tile_manager.paint_atlas_indices(encoder)
    }

    pub fn tessellate<'a>(
        &'a self,
        mut cpass: wgpu::ComputePass<'a>,
    ) -> Fallible<wgpu::ComputePass<'a>> {
        // Use the CPU input mesh to tessellate on the GPU.
        cpass = self.patch_manager.tessellate(cpass)?;

        // Use our height tiles to displace mesh.
        self.tile_manager.displace_height(
            self.patch_manager.target_vertex_count(),
            &self.patch_manager.displace_height_bind_group(),
            cpass,
        )
    }

    pub fn deferred_texture_target(
        &self,
    ) -> (
        [wgpu::RenderPassColorAttachmentDescriptor; 1],
        Option<wgpu::RenderPassDepthStencilAttachmentDescriptor>,
    ) {
        (
            [wgpu::RenderPassColorAttachmentDescriptor {
                attachment: &self.deferred_texture_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::RED),
                    store: true,
                },
            }],
            None,
        )
    }

    pub fn deferred_texture<'a>(
        &'a self,
        mut rpass: wgpu::RenderPass<'a>,
        globals_buffer: &'a GlobalParametersBuffer,
    ) -> wgpu::RenderPass<'a> {
        rpass.set_pipeline(&self.deferred_texture_pipeline);
        rpass.set_bind_group(Group::Globals.index(), &globals_buffer.bind_group(), &[]);
        rpass.set_bind_group(1, &self.empty_bind_group, &[]);
        rpass.set_bind_group(Group::Terrain.index(), &self.tile_manager.bind_group(), &[]);
        rpass.set_vertex_buffer(0, self.patch_manager.vertex_buffer());
        for i in 0..self.patch_manager.num_patches() {
            let winding = self.patch_manager.patch_winding(i);
            let base_vertex = self.patch_manager.patch_vertex_buffer_offset(i);
            rpass.set_index_buffer(self.patch_manager.tristrip_index_buffer(winding));
            rpass.draw_indexed(
                self.patch_manager.tristrip_index_range(winding),
                base_vertex,
                0..1,
            );
        }

        rpass
    }

    #[command]
    pub fn snapshot_index(&mut self, _command: &Command) {
        self.tile_manager.snapshot_index();
    }

    #[command]
    pub fn snapshot_deferred_texture_buffer(&mut self, _command: &Command) {
        self.take_deferred_texture_snapshot = true;
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        self.tile_manager.bind_group_layout()
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        self.tile_manager.bind_group()
    }

    // pub fn num_patches(&self) -> i32 {
    //     self.patch_manager.num_patches()
    // }

    // pub fn vertex_buffer(&self) -> wgpu::BufferSlice {
    //     self.patch_manager.vertex_buffer()
    // }

    // pub fn patch_vertex_buffer_offset(&self, patch_number: i32) -> i32 {
    //     self.patch_manager.patch_vertex_buffer_offset(patch_number)
    // }

    // pub fn patch_winding(&self, patch_number: i32) -> PatchWinding {
    //     self.patch_manager.patch_winding(patch_number)
    // }

    pub fn wireframe_index_buffer(&self, winding: PatchWinding) -> wgpu::BufferSlice {
        self.patch_manager.wireframe_index_buffer(winding)
    }

    pub fn wireframe_index_range(&self, winding: PatchWinding) -> Range<u32> {
        self.patch_manager.wireframe_index_range(winding)
    }

    // pub fn tristrip_index_buffer(&self, winding: PatchWinding) -> wgpu::BufferSlice {
    //     self.patch_manager.tristrip_index_buffer(winding)
    // }

    // pub fn tristrip_index_range(&self, winding: PatchWinding) -> Range<u32> {
    //     self.patch_manager.tristrip_index_range(winding)
    // }
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
