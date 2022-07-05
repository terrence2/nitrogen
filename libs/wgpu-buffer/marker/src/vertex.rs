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
use csscolorparser::Color;
use memoffset::offset_of;
use nalgebra::Vector3;
use std::mem;
use zerocopy::{AsBytes, FromBytes};

#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Debug)]
pub struct MarkerVertex {
    position: [f32; 3],
    normal: [f32; 3],
    color: [f32; 4],
}

impl MarkerVertex {
    #[allow(clippy::unneeded_field_pattern)]
    pub fn descriptor() -> wgpu::VertexBufferLayout<'static> {
        let tmp = wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // position
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 0,
                    shader_location: 0,
                },
                // normal
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 12,
                    shader_location: 1,
                },
                // color
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 24,
                    shader_location: 2,
                },
            ],
        };

        assert_eq!(
            tmp.attributes[0].offset,
            offset_of!(MarkerVertex, position) as wgpu::BufferAddress
        );
        assert_eq!(
            tmp.attributes[1].offset,
            offset_of!(MarkerVertex, normal) as wgpu::BufferAddress
        );
        assert_eq!(
            tmp.attributes[2].offset,
            offset_of!(MarkerVertex, color) as wgpu::BufferAddress
        );

        tmp
    }

    pub(crate) fn new(position: Vector3<f64>, normal: Vector3<f64>, color: &Color) -> Self {
        let (r, g, b, a) = color.to_linear_rgba();
        Self {
            position: [position[0] as f32, position[1] as f32, position[2] as f32],
            normal: [normal[0] as f32, normal[1] as f32, normal[2] as f32],
            color: [r as f32, g as f32, b as f32, a as f32],
        }
    }
}
