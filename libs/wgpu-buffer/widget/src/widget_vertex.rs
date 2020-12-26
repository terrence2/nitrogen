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
use memoffset::offset_of;
use std::mem;
use zerocopy::{AsBytes, FromBytes};

#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Debug, Default)]
pub struct WidgetVertex {
    pub(crate) position: [f32; 3],
    pub(crate) tex_coord: [f32; 2],
    pub(crate) widget_info_index: u32,
}

impl WidgetVertex {
    // #[allow(clippy::unneeded_field_pattern)]
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
                // info_index
                wgpu::VertexAttributeDescriptor {
                    format: wgpu::VertexFormat::Uint,
                    offset: 20,
                    shader_location: 2,
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
            offset_of!(WidgetVertex, widget_info_index) as wgpu::BufferAddress
        );

        tmp
    }
}
