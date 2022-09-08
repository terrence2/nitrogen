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
    tile::tile_builder::{HeightsTileSet, TileSet},
    VisiblePatch,
};
use bevy_ecs::prelude::*;
use camera::ScreenCamera;
use catalog::Catalog;
use gpu::wgpu::{BindGroup, CommandEncoder};
use gpu::Gpu;
use nitrous::{inject_nitrous_component, NitrousComponent};
use shader_shared::Group;
use std::any::Any;

#[derive(Debug, Component, NitrousComponent)]
#[Name = "null_height_tile_set"]
pub(crate) struct NullHeightTileSet {
    displace_height_pipeline: wgpu::ComputePipeline,
}

#[inject_nitrous_component]
impl NullHeightTileSet {
    pub(crate) fn new(
        // Note: patch manager owns the vertex buffer, so owns the layout here
        displace_height_bind_group_layout: &wgpu::BindGroupLayout,
        gpu: &Gpu,
    ) -> Self {
        let displace_height_pipeline =
            gpu.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("terrain-null-ts-displace-height-pipeline"),
                    layout: Some(&gpu.device().create_pipeline_layout(
                        &wgpu::PipelineLayoutDescriptor {
                            label: Some("terrain-null-ts-displace-height-pipeline-layout"),
                            push_constant_ranges: &[],
                            bind_group_layouts: &[displace_height_bind_group_layout],
                        },
                    )),
                    module: &gpu.create_shader_module(
                        "displace_null_height.comp",
                        include_bytes!("../../target/displace_null_height.comp.spirv"),
                    ),
                    entry_point: "main",
                });

        Self {
            displace_height_pipeline,
        }
    }
}

impl HeightsTileSet for NullHeightTileSet {
    fn displace_height(
        &self,
        vertex_count: u32,
        mesh_bind_group: &BindGroup,
        encoder: &mut CommandEncoder,
    ) {
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("terrain-null-ts-displace-height-cpass"),
        });
        cpass.set_pipeline(&self.displace_height_pipeline);
        cpass.set_bind_group(Group::TerrainDisplaceMesh.index(), mesh_bind_group, &[]);
        const WORKGROUP_WIDTH: u32 = 65536;
        let wg_x = (vertex_count % WORKGROUP_WIDTH).max(1);
        let wg_y = (vertex_count / WORKGROUP_WIDTH).max(1);
        cpass.dispatch_workgroups(wg_x, wg_y, 1);
    }
}

impl TileSet for NullHeightTileSet {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn begin_visibility_update(&mut self) {
        // self.common.begin_visibility_update();
    }

    fn note_required(&mut self, _visible_patch: &VisiblePatch) {
        // self.common.note_required(visible_patch)
    }

    fn finish_visibility_update(&mut self, _camera: &ScreenCamera, _catalog: &mut Catalog) {
        // self.common.finish_visibility_update(catalog);
    }

    fn encode_uploads(&mut self, _gpu: &Gpu, _encoder: &mut wgpu::CommandEncoder) {
        // self.common.encode_uploads(gpu, encoder);
    }

    fn snapshot_index(&mut self, _gpu: &mut Gpu) {
        // self.common.snapshot_index(gpu)
    }

    fn paint_atlas_index(&self, _encoder: &mut wgpu::CommandEncoder) {
        // self.common.paint_atlas_index(encoder)
    }

    fn shutdown_safely(&mut self) {
        // self.common.shutdown_safely();
    }
}
