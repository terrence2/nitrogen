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
use gpu::{UploadTracker, GPU};
use std::{ops::Range, sync::Arc};
use tokio::{runtime::Runtime, sync::RwLock as AsyncRwLock};

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

#[derive(Commandable)]
pub struct TerrainGeoBuffer {
    patch_manager: PatchManager,
    tile_manager: TileManager,

    // Cache allocation for transferring visible allocations from patches to tiles.
    visible_regions: Vec<(
        Graticule<GeoCenter>,
        Graticule<GeoCenter>,
        Graticule<GeoCenter>,
        Length<Meters>,
    )>,
}

#[commandable]
impl TerrainGeoBuffer {
    pub fn new(
        catalog: &Catalog,
        cpu_detail_level: CpuDetailLevel,
        gpu_detail_level: GpuDetailLevel,
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

        Ok(Self {
            patch_manager,
            tile_manager,
            visible_regions: Vec::new(),
        })
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
        for (g0, g1, g2, edge_length) in &self.visible_regions {
            self.tile_manager.note_required(g0, g1, g2, *edge_length);
        }
        self.tile_manager
            .finish_update(catalog, async_rt, gpu, tracker);

        Ok(())
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

    #[command]
    pub fn snapshot_index(&mut self, _command: &Command) {
        self.tile_manager.snapshot_index();
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        self.tile_manager.bind_group_layout()
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        self.tile_manager.bind_group()
    }

    pub fn num_patches(&self) -> i32 {
        self.patch_manager.num_patches()
    }

    pub fn vertex_buffer(&self) -> wgpu::BufferSlice {
        self.patch_manager.vertex_buffer()
    }

    pub fn patch_vertex_buffer_offset(&self, patch_number: i32) -> i32 {
        self.patch_manager.patch_vertex_buffer_offset(patch_number)
    }

    pub fn patch_winding(&self, patch_number: i32) -> PatchWinding {
        self.patch_manager.patch_winding(patch_number)
    }

    pub fn wireframe_index_buffer(&self, winding: PatchWinding) -> wgpu::BufferSlice {
        self.patch_manager.wireframe_index_buffer(winding)
    }

    pub fn wireframe_index_range(&self, winding: PatchWinding) -> Range<u32> {
        self.patch_manager.wireframe_index_range(winding)
    }

    pub fn tristrip_index_buffer(&self, winding: PatchWinding) -> wgpu::BufferSlice {
        self.patch_manager.tristrip_index_buffer(winding)
    }

    pub fn tristrip_index_range(&self, winding: PatchWinding) -> Range<u32> {
        self.patch_manager.tristrip_index_range(winding)
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
