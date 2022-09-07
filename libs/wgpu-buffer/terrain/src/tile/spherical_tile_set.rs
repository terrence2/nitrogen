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
    tile::{
        spherical_common::SphericalTileSetCommon,
        tile_builder::{ColorsTileSet, HeightsTileSet, NormalsTileSet, TileSet},
        DataSetDataKind,
    },
    VisiblePatch,
};
use anyhow::Result;
use bevy_ecs::prelude::*;
use camera::ScreenCamera;
use catalog::Catalog;
use global_data::GlobalParametersBuffer;
use gpu::Gpu;
use nitrous::{inject_nitrous_component, method, NitrousComponent};
use shader_shared::Group;
use std::any::Any;

// TODO: tweak load depth of each type of tile... we don't need as much height data as normal data

#[derive(Debug, Component, NitrousComponent)]
#[Name = "tile_set"]
pub(crate) struct SphericalHeightTileSet {
    common: SphericalTileSetCommon,
    displace_height_pipeline: wgpu::ComputePipeline,
}

#[inject_nitrous_component]
impl SphericalHeightTileSet {
    pub(crate) fn new(
        // Note: patch manager owns the vertex buffer, so owns the layout here
        displace_height_bind_group_layout: &wgpu::BindGroupLayout,
        catalog: &Catalog,
        prefix: &str,
        tile_cache_size: u32,
        gpu: &Gpu,
    ) -> Result<Self> {
        let common = SphericalTileSetCommon::new(
            catalog,
            prefix,
            DataSetDataKind::Height,
            tile_cache_size,
            gpu,
        )?;

        let displace_height_pipeline =
            gpu.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("terrain-displace-height-pipeline"),
                    layout: Some(&gpu.device().create_pipeline_layout(
                        &wgpu::PipelineLayoutDescriptor {
                            label: Some("terrain-displace-height-pipeline-layout"),
                            push_constant_ranges: &[],
                            bind_group_layouts: &[
                                displace_height_bind_group_layout,
                                common.bind_group_layout(),
                            ],
                        },
                    )),
                    module: &gpu.create_shader_module(
                        "displace_spherical_height.comp",
                        include_bytes!("../../target/displace_spherical_height.comp.spirv"),
                    ),
                    entry_point: "main",
                });

        Ok(Self {
            common,
            displace_height_pipeline,
        })
    }

    #[method]
    pub fn dump_index(&mut self, path: &str) -> Result<()> {
        self.common.dump_index(path)
    }
}

impl TileSet for SphericalHeightTileSet {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn begin_visibility_update(&mut self) {
        self.common.begin_visibility_update();
    }

    fn note_required(&mut self, visible_patch: &VisiblePatch) {
        self.common.note_required(visible_patch)
    }

    fn finish_visibility_update(&mut self, _camera: &ScreenCamera, catalog: &mut Catalog) {
        self.common.finish_visibility_update(catalog);
    }

    fn encode_uploads(&mut self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder) {
        self.common.encode_uploads(gpu, encoder);
    }

    fn snapshot_index(&mut self, gpu: &mut Gpu) {
        self.common.snapshot_index(gpu)
    }

    fn paint_atlas_index(&self, encoder: &mut wgpu::CommandEncoder) {
        self.common.paint_atlas_index(encoder)
    }

    fn shutdown_safely(&mut self) {
        self.common.shutdown_safely();
    }
}

impl HeightsTileSet for SphericalHeightTileSet {
    fn displace_height(
        &self,
        vertex_count: u32,
        mesh_bind_group: &wgpu::BindGroup,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("terrain-displace-height-cpass"),
        });
        cpass.set_pipeline(&self.displace_height_pipeline);
        cpass.set_bind_group(Group::TerrainDisplaceMesh.index(), mesh_bind_group, &[]);
        cpass.set_bind_group(
            Group::TerrainDisplaceTileSet.index(),
            self.common.bind_group(),
            &[],
        );
        const WORKGROUP_WIDTH: u32 = 65536;
        let wg_x = (vertex_count % WORKGROUP_WIDTH).max(1);
        let wg_y = (vertex_count / WORKGROUP_WIDTH).max(1);
        cpass.dispatch_workgroups(wg_x, wg_y, 1);
    }
}

#[derive(Debug, Component, NitrousComponent)]
#[Name = "tile_set"]
pub(crate) struct SphericalColorTileSet {
    common: SphericalTileSetCommon,
    accumulate_spherical_colors_pipeline: wgpu::ComputePipeline,
}

#[inject_nitrous_component]
impl SphericalColorTileSet {
    pub(crate) fn new(
        accumulate_common_bind_group_layout: &wgpu::BindGroupLayout,
        catalog: &Catalog,
        prefix: &str,
        globals_buffer: &GlobalParametersBuffer,
        tile_cache_size: u32,
        gpu: &Gpu,
    ) -> Result<Self> {
        let common = SphericalTileSetCommon::new(
            catalog,
            prefix,
            DataSetDataKind::Color,
            tile_cache_size,
            gpu,
        )?;

        let accumulate_spherical_colors_pipeline =
            gpu.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("terrain-accumulate-spherical-colors-pipeline"),
                    layout: Some(&gpu.device().create_pipeline_layout(
                        &wgpu::PipelineLayoutDescriptor {
                            label: Some("terrain-accumulate-colors-pipeline-layout"),
                            push_constant_ranges: &[],
                            bind_group_layouts: &[
                                globals_buffer.bind_group_layout(),
                                accumulate_common_bind_group_layout,
                                common.bind_group_layout(),
                            ],
                        },
                    )),
                    module: &gpu.create_shader_module(
                        "accumulate_spherical_colors.comp",
                        include_bytes!("../../target/accumulate_spherical_colors.comp.spirv"),
                    ),
                    entry_point: "main",
                });

        Ok(Self {
            common,
            accumulate_spherical_colors_pipeline,
        })
    }

    #[method]
    pub fn dump_index(&mut self, path: &str) -> Result<()> {
        self.common.dump_index(path)
    }
}

impl TileSet for SphericalColorTileSet {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn begin_visibility_update(&mut self) {
        self.common.begin_visibility_update()
    }

    fn note_required(&mut self, visible_patch: &VisiblePatch) {
        self.common.note_required(visible_patch)
    }

    fn finish_visibility_update(&mut self, _camera: &ScreenCamera, catalog: &mut Catalog) {
        self.common.finish_visibility_update(catalog)
    }

    fn encode_uploads(&mut self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder) {
        self.common.encode_uploads(gpu, encoder);
    }

    fn snapshot_index(&mut self, gpu: &mut Gpu) {
        self.common.snapshot_index(gpu)
    }

    fn paint_atlas_index(&self, encoder: &mut wgpu::CommandEncoder) {
        self.common.paint_atlas_index(encoder)
    }

    fn shutdown_safely(&mut self) {
        self.common.shutdown_safely();
    }
}

impl ColorsTileSet for SphericalColorTileSet {
    fn accumulate_colors(
        &self,
        extent: &wgpu::Extent3d,
        globals: &GlobalParametersBuffer,
        accumulate_common_bind_group: &wgpu::BindGroup,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("terrain-spherical-colors-acc-cpass"),
        });
        cpass.set_pipeline(&self.accumulate_spherical_colors_pipeline);
        cpass.set_bind_group(Group::Globals.index(), globals.bind_group(), &[]);
        cpass.set_bind_group(
            Group::TerrainAccumulateCommon.index(),
            accumulate_common_bind_group,
            &[],
        );
        cpass.set_bind_group(
            Group::TerrainAccumulateTileSet.index(),
            self.common.bind_group(),
            &[],
        );
        cpass.dispatch_workgroups(extent.width / 8, extent.height / 8, 1);
    }
}

#[derive(Debug, Component, NitrousComponent)]
#[Name = "tile_set"]
pub(crate) struct SphericalNormalsTileSet {
    common: SphericalTileSetCommon,
    accumulate_spherical_normals_pipeline: wgpu::ComputePipeline,
}

#[inject_nitrous_component]
impl SphericalNormalsTileSet {
    pub(crate) fn new(
        accumulate_common_bind_group_layout: &wgpu::BindGroupLayout,
        catalog: &Catalog,
        prefix: &str,
        globals_buffer: &GlobalParametersBuffer,
        tile_cache_size: u32,
        gpu: &Gpu,
    ) -> Result<Self> {
        let common = SphericalTileSetCommon::new(
            catalog,
            prefix,
            DataSetDataKind::Normal,
            tile_cache_size,
            gpu,
        )?;

        let accumulate_spherical_normals_pipeline =
            gpu.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("terrain-accumulate-spherical-normals-pipeline"),
                    layout: Some(&gpu.device().create_pipeline_layout(
                        &wgpu::PipelineLayoutDescriptor {
                            label: Some("terrain-accumulate-normals-pipeline-layout"),
                            push_constant_ranges: &[],
                            bind_group_layouts: &[
                                globals_buffer.bind_group_layout(),
                                accumulate_common_bind_group_layout,
                                common.bind_group_layout(),
                            ],
                        },
                    )),
                    module: &gpu.create_shader_module(
                        "accumulate_spherical_normals.comp",
                        include_bytes!("../../target/accumulate_spherical_normals.comp.spirv"),
                    ),
                    entry_point: "main",
                });

        Ok(Self {
            common,
            accumulate_spherical_normals_pipeline,
        })
    }

    #[method]
    pub fn dump_index(&mut self, path: &str) -> Result<()> {
        self.common.dump_index(path)
    }
}

impl TileSet for SphericalNormalsTileSet {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn begin_visibility_update(&mut self) {
        self.common.begin_visibility_update();
    }

    fn note_required(&mut self, visible_patch: &VisiblePatch) {
        self.common.note_required(visible_patch);
    }

    fn finish_visibility_update(&mut self, _camera: &ScreenCamera, catalog: &mut Catalog) {
        self.common.finish_visibility_update(catalog);
    }

    fn encode_uploads(&mut self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder) {
        self.common.encode_uploads(gpu, encoder);
    }

    fn snapshot_index(&mut self, gpu: &mut Gpu) {
        self.common.snapshot_index(gpu)
    }

    fn paint_atlas_index(&self, encoder: &mut wgpu::CommandEncoder) {
        self.common.paint_atlas_index(encoder)
    }

    fn shutdown_safely(&mut self) {
        self.common.shutdown_safely();
    }
}

impl NormalsTileSet for SphericalNormalsTileSet {
    fn accumulate_normals(
        &self,
        extent: &wgpu::Extent3d,
        globals: &GlobalParametersBuffer,
        accumulate_common_bind_group: &wgpu::BindGroup,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("terrain-spherical-normals-acc-cpass"),
        });
        cpass.set_pipeline(&self.accumulate_spherical_normals_pipeline);
        cpass.set_bind_group(Group::Globals.index(), globals.bind_group(), &[]);
        cpass.set_bind_group(
            Group::TerrainAccumulateCommon.index(),
            accumulate_common_bind_group,
            &[],
        );
        cpass.set_bind_group(
            Group::TerrainAccumulateTileSet.index(),
            self.common.bind_group(),
            &[],
        );
        cpass.dispatch_workgroups(extent.width / 8, extent.height / 8, 1);
    }
}
