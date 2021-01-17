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
use crate::color::Color;
use memoffset::offset_of;
use std::mem;
use zerocopy::{AsBytes, FromBytes};

#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Debug, Default)]
pub struct WidgetVertex {
    pub(crate) position: [f32; 3],
    pub(crate) tex_coord: [f32; 2],
    pub(crate) color: [u8; 4],
    pub(crate) widget_info_index: u32,
}

impl WidgetVertex {
    pub fn descriptor() -> wgpu::VertexBufferDescriptor<'static> {
        let tmp = wgpu::VertexBufferDescriptor {
            stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::InputStepMode::Vertex,
            attributes: &[
                // position
                wgpu::VertexAttributeDescriptor {
                    format: wgpu::VertexFormat::Float3,
                    offset: 0,
                    shader_location: 0,
                },
                // tex_coord
                wgpu::VertexAttributeDescriptor {
                    format: wgpu::VertexFormat::Float3,
                    offset: 12,
                    shader_location: 1,
                },
                // color
                wgpu::VertexAttributeDescriptor {
                    format: wgpu::VertexFormat::Uchar4Norm,
                    offset: 20,
                    shader_location: 2,
                },
                // info_index
                wgpu::VertexAttributeDescriptor {
                    format: wgpu::VertexFormat::Uint,
                    offset: 24,
                    shader_location: 3,
                },
            ],
        };

        assert_eq!(
            tmp.attributes[0].offset,
            offset_of!(WidgetVertex, position) as wgpu::BufferAddress
        );
        assert_eq!(
            tmp.attributes[1].offset,
            offset_of!(WidgetVertex, tex_coord) as wgpu::BufferAddress
        );
        assert_eq!(
            tmp.attributes[2].offset,
            offset_of!(WidgetVertex, color) as wgpu::BufferAddress
        );
        assert_eq!(
            tmp.attributes[3].offset,
            offset_of!(WidgetVertex, widget_info_index) as wgpu::BufferAddress
        );

        tmp
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push_textured_quad(
        [x0, y0]: [f32; 2],
        [x1, y1]: [f32; 2],
        z: f32,
        [s0, t0]: [f32; 2],
        [s1, t1]: [f32; 2],
        color: &Color,
        widget_info_index: u32,
        pool: &mut Vec<WidgetVertex>,
    ) {
        // Build 4 corner vertices.
        let v00 = WidgetVertex {
            position: [x0, y0, z],
            tex_coord: [s0, t0],
            color: color.to_u8_array(),
            widget_info_index,
        };
        let v01 = WidgetVertex {
            position: [x0, y1, z],
            tex_coord: [s0, t1],
            color: color.to_u8_array(),
            widget_info_index,
        };
        let v10 = WidgetVertex {
            position: [x1, y0, z],
            tex_coord: [s1, t0],
            color: color.to_u8_array(),
            widget_info_index,
        };
        let v11 = WidgetVertex {
            position: [x1, y1, z],
            tex_coord: [s1, t1],
            color: color.to_u8_array(),
            widget_info_index,
        };

        // Push 2 triangles
        pool.push(v00);
        pool.push(v10);
        pool.push(v01);
        pool.push(v01);
        pool.push(v10);
        pool.push(v11);
    }

    pub fn push_quad(
        pos_low: [f32; 2],
        pos_high: [f32; 2],
        z: f32,
        color: &Color,
        widget_info_index: u32,
        pool: &mut Vec<WidgetVertex>,
    ) {
        Self::push_textured_quad(
            pos_low,
            pos_high,
            z,
            [0f32, 0f32],
            [0f32, 0f32],
            color,
            widget_info_index,
            pool,
        )
    }
}
