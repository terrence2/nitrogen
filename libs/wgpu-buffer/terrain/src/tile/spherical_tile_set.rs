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
        spherical_common::SphericalTileSetCommon, tile_manager::TileSet, DataSetCoordinates,
        DataSetDataKind,
    },
    GpuDetail, VisiblePatch,
};
use anyhow::Result;
use catalog::Catalog;
use global_data::GlobalParametersBuffer;
use gpu::wgpu::{BindGroup, CommandEncoder, ComputePass};
use gpu::{UploadTracker, GPU};
use shader_shared::Group;
use std::sync::Arc;
use tokio::{runtime::Runtime, sync::RwLock};

#[derive(Debug)]
pub(crate) struct SphericalHeightTileSet {
    common: SphericalTileSetCommon,
    displace_height_pipeline: wgpu::ComputePipeline,
}

impl SphericalHeightTileSet {
    pub(crate) fn new(
        // Note: patch manager owns the vertex buffer, so owns the layout here
        displace_height_bind_group_layout: &wgpu::BindGroupLayout,
        catalog: &Catalog,
        prefix: &str,
        gpu_detail: &GpuDetail,
        gpu: &GPU,
    ) -> Result<Self> {
        let common =
            SphericalTileSetCommon::new(catalog, prefix, DataSetDataKind::Height, gpu_detail, gpu)?;

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
                    )?,
                    entry_point: "main",
                });

        Ok(Self {
            common,
            displace_height_pipeline,
        })
    }
}

impl TileSet for SphericalHeightTileSet {
    fn kind(&self) -> DataSetDataKind {
        DataSetDataKind::Height
    }

    fn coordinates(&self) -> DataSetCoordinates {
        DataSetCoordinates::Spherical
    }

    fn begin_update(&mut self) {
        self.common.begin_update()
    }

    fn note_required(&mut self, visible_patch: &VisiblePatch) {
        self.common.note_required(visible_patch)
    }

    fn finish_update(
        &mut self,
        catalog: Arc<RwLock<Catalog>>,
        async_rt: &Runtime,
        gpu: &GPU,
        tracker: &mut UploadTracker,
    ) {
        self.common.finish_update(catalog, async_rt, gpu, tracker)
    }

    fn snapshot_index(&mut self, async_rt: &Runtime, gpu: &mut GPU) {
        self.common.snapshot_index(async_rt, gpu)
    }

    fn paint_atlas_index(&self, encoder: &mut CommandEncoder) {
        self.common.paint_atlas_index(encoder)
    }

    fn displace_height<'a>(
        &'a self,
        vertex_count: u32,
        mesh_bind_group: &'a BindGroup,
        mut cpass: ComputePass<'a>,
    ) -> Result<ComputePass<'a>> {
        cpass.set_pipeline(&self.displace_height_pipeline);
        cpass.set_bind_group(0, mesh_bind_group, &[]);
        cpass.set_bind_group(1, self.common.bind_group(), &[]);
        cpass.dispatch(vertex_count, 1, 1);
        Ok(cpass)
    }

    fn accumulate_normals<'a>(
        &'a self,
        _cpass: ComputePass<'a>,
        _extent: &wgpu::Extent3d,
        _globals_buffer: &'a GlobalParametersBuffer,
        _accumulate_common_bind_group: &'a wgpu::BindGroup,
    ) -> Result<ComputePass<'a>> {
        unimplemented!()
    }

    fn accumulate_colors<'a>(
        &'a self,
        _cpass: ComputePass<'a>,
        _extent: &wgpu::Extent3d,
        _globals_buffer: &'a GlobalParametersBuffer,
        _accumulate_common_bind_group: &'a wgpu::BindGroup,
    ) -> Result<ComputePass<'a>> {
        unimplemented!()
    }
}

#[derive(Debug)]
pub(crate) struct SphericalColorTileSet {
    common: SphericalTileSetCommon,
    accumulate_spherical_colors_pipeline: wgpu::ComputePipeline,
}

impl SphericalColorTileSet {
    pub(crate) fn new(
        accumulate_common_bind_group_layout: &wgpu::BindGroupLayout,
        catalog: &Catalog,
        prefix: &str,
        globals_buffer: &GlobalParametersBuffer,
        gpu_detail: &GpuDetail,
        gpu: &GPU,
    ) -> Result<Self> {
        let common =
            SphericalTileSetCommon::new(catalog, prefix, DataSetDataKind::Color, gpu_detail, gpu)?;

        let accumulate_spherical_colors_pipeline =
            gpu.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("terrain-accumulate-spherical-colors-pipeline"),
                    layout: Some(&gpu.device().create_pipeline_layout(
                        &wgpu::PipelineLayoutDescriptor {
                            label: Some("terrain-accumulate-pipeline-layout"),
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
                    )?,
                    entry_point: "main",
                });

        Ok(Self {
            common,
            accumulate_spherical_colors_pipeline,
        })
    }
}

impl TileSet for SphericalColorTileSet {
    fn kind(&self) -> DataSetDataKind {
        DataSetDataKind::Color
    }

    fn coordinates(&self) -> DataSetCoordinates {
        DataSetCoordinates::Spherical
    }

    fn begin_update(&mut self) {
        self.common.begin_update()
    }

    fn note_required(&mut self, visible_patch: &VisiblePatch) {
        self.common.note_required(visible_patch)
    }

    fn finish_update(
        &mut self,
        catalog: Arc<RwLock<Catalog>>,
        async_rt: &Runtime,
        gpu: &GPU,
        tracker: &mut UploadTracker,
    ) {
        self.common.finish_update(catalog, async_rt, gpu, tracker)
    }

    fn snapshot_index(&mut self, async_rt: &Runtime, gpu: &mut GPU) {
        self.common.snapshot_index(async_rt, gpu)
    }

    fn paint_atlas_index(&self, encoder: &mut CommandEncoder) {
        self.common.paint_atlas_index(encoder)
    }

    fn displace_height<'a>(
        &'a self,
        _vertex_count: u32,
        _mesh_bind_group: &'a BindGroup,
        _cpass: ComputePass<'a>,
    ) -> Result<ComputePass<'a>> {
        unimplemented!()
    }

    fn accumulate_normals<'a>(
        &'a self,
        _cpass: ComputePass<'a>,
        _extent: &wgpu::Extent3d,
        _globals_buffer: &'a GlobalParametersBuffer,
        _accumulate_common_bind_group: &'a wgpu::BindGroup,
    ) -> Result<ComputePass<'a>> {
        unimplemented!()
    }

    fn accumulate_colors<'a>(
        &'a self,
        mut cpass: ComputePass<'a>,
        extent: &wgpu::Extent3d,
        globals_buffer: &'a GlobalParametersBuffer,
        accumulate_common_bind_group: &'a wgpu::BindGroup,
    ) -> Result<ComputePass<'a>> {
        cpass.set_pipeline(&self.accumulate_spherical_colors_pipeline);
        cpass.set_bind_group(Group::Globals.index(), globals_buffer.bind_group(), &[]);
        cpass.set_bind_group(Group::TerrainAcc.index(), accumulate_common_bind_group, &[]);
        cpass.set_bind_group(Group::TerrainTileSet.index(), self.common.bind_group(), &[]);
        cpass.dispatch(extent.width / 8, extent.height / 8, 1);
        Ok(cpass)
    }
}

#[derive(Debug)]
pub(crate) struct SphericalNormalsTileSet {
    common: SphericalTileSetCommon,
    accumulate_spherical_normals_pipeline: wgpu::ComputePipeline,
}

impl SphericalNormalsTileSet {
    pub(crate) fn new(
        accumulate_common_bind_group_layout: &wgpu::BindGroupLayout,
        catalog: &Catalog,
        prefix: &str,
        globals_buffer: &GlobalParametersBuffer,
        gpu_detail: &GpuDetail,
        gpu: &GPU,
    ) -> Result<Self> {
        let common =
            SphericalTileSetCommon::new(catalog, prefix, DataSetDataKind::Normal, gpu_detail, gpu)?;

        let accumulate_spherical_normals_pipeline =
            gpu.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("terrain-accumulate-spherical-normals-pipeline"),
                    layout: Some(&gpu.device().create_pipeline_layout(
                        &wgpu::PipelineLayoutDescriptor {
                            label: Some("terrain-accumulate-pipeline-layout"),
                            push_constant_ranges: &[],
                            bind_group_layouts: &[
                                globals_buffer.bind_group_layout(),
                                &accumulate_common_bind_group_layout,
                                common.bind_group_layout(),
                            ],
                        },
                    )),
                    module: &gpu.create_shader_module(
                        "accumulate_spherical_normals.comp",
                        include_bytes!("../../target/accumulate_spherical_normals.comp.spirv"),
                    )?,
                    entry_point: "main",
                });

        Ok(Self {
            common,
            accumulate_spherical_normals_pipeline,
        })
    }
}

impl TileSet for SphericalNormalsTileSet {
    fn kind(&self) -> DataSetDataKind {
        DataSetDataKind::Normal
    }

    fn coordinates(&self) -> DataSetCoordinates {
        DataSetCoordinates::Spherical
    }

    fn begin_update(&mut self) {
        self.common.begin_update()
    }

    fn note_required(&mut self, visible_patch: &VisiblePatch) {
        self.common.note_required(visible_patch)
    }

    fn finish_update(
        &mut self,
        catalog: Arc<RwLock<Catalog>>,
        async_rt: &Runtime,
        gpu: &GPU,
        tracker: &mut UploadTracker,
    ) {
        self.common.finish_update(catalog, async_rt, gpu, tracker)
    }

    fn snapshot_index(&mut self, async_rt: &Runtime, gpu: &mut GPU) {
        self.common.snapshot_index(async_rt, gpu)
    }

    fn paint_atlas_index(&self, encoder: &mut CommandEncoder) {
        self.common.paint_atlas_index(encoder)
    }

    fn displace_height<'a>(
        &'a self,
        _vertex_count: u32,
        _mesh_bind_group: &'a BindGroup,
        _cpass: ComputePass<'a>,
    ) -> Result<ComputePass<'a>> {
        unimplemented!()
    }

    fn accumulate_normals<'a>(
        &'a self,
        mut cpass: ComputePass<'a>,
        extent: &wgpu::Extent3d,
        globals_buffer: &'a GlobalParametersBuffer,
        accumulate_common_bind_group: &'a wgpu::BindGroup,
    ) -> Result<ComputePass<'a>> {
        cpass.set_pipeline(&self.accumulate_spherical_normals_pipeline);
        cpass.set_bind_group(Group::Globals.index(), globals_buffer.bind_group(), &[]);
        cpass.set_bind_group(Group::TerrainAcc.index(), accumulate_common_bind_group, &[]);
        cpass.set_bind_group(
            Group::TerrainTileSet.index(),
            &self.common.bind_group(),
            &[],
        );
        cpass.dispatch(extent.width / 8, extent.height / 8, 1);

        Ok(cpass)
    }

    fn accumulate_colors<'a>(
        &'a self,
        _cpass: ComputePass<'a>,
        _extent: &wgpu::Extent3d,
        _globals_buffer: &'a GlobalParametersBuffer,
        _accumulate_common_bind_group: &'a wgpu::BindGroup,
    ) -> Result<ComputePass<'a>> {
        unimplemented!()
    }
}
