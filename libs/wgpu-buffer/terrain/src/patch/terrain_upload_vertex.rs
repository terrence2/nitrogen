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
use absolute_unit::Radians;
use geodesy::{GeoCenter, Graticule};
use nalgebra::{Point3, Vector3};
use std::mem;
use zerocopy::{AsBytes, FromBytes};

#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Default)]
pub struct TerrainUploadVertex {
    position: [f32; 3],
    normal: [f32; 3],
    graticule: [f32; 2],
}

impl TerrainUploadVertex {
    pub fn empty() -> Self {
        Self {
            position: [0f32; 3],
            normal: [0f32; 3],
            graticule: [0f32; 2],
        }
    }

    pub fn new(v_view: &Point3<f64>, n0: &Vector3<f64>, graticule: &Graticule<GeoCenter>) -> Self {
        Self {
            position: [v_view[0] as f32, v_view[1] as f32, v_view[2] as f32],
            normal: [n0[0] as f32, n0[1] as f32, n0[2] as f32],
            graticule: graticule.lat_lon::<Radians, f32>(),
        }
    }

    pub fn mem_size() -> usize {
        mem::size_of::<Self>()
    }
}
