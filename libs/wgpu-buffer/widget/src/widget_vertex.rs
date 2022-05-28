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
use crate::region::{Extent, Position, Region};
use csscolorparser::Color;
use memoffset::offset_of;
use std::mem;
use window::{
    size::{AbsSize, AspectMath, RelSize, ScreenDir, Size},
    Window,
};
use zerocopy::{AsBytes, FromBytes};

#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Debug, Default)]
pub struct WidgetVertex {
    pub(crate) position: [f32; 3],
    pub(crate) tex_coord: [f32; 2],
    pub(crate) color: [u8; 4],
    pub(crate) widget_info_index: u32,
}

fn pack_color(c: &Color) -> [u8; 4] {
    let rgba = c.to_linear_rgba_u8();
    [rgba.0, rgba.1, rgba.2, rgba.3]
}

impl WidgetVertex {
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
                // tex_coord
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 12,
                    shader_location: 1,
                },
                // color
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Unorm8x4,
                    offset: 20,
                    shader_location: 2,
                },
                // info_index
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32,
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

    pub fn push_textured_quad_rel(
        [x0, y0]: [RelSize; 2],
        [x1, y1]: [RelSize; 2],
        z: f32,
        [s0, t0]: [f32; 2],
        [s1, t1]: [f32; 2],
        color: &Color,
        widget_info_index: u32,
        pool: &mut Vec<WidgetVertex>,
    ) {
        let x0 = x0.as_gpu();
        let y0 = y0.as_gpu();
        let x1 = x1.as_gpu();
        let y1 = y1.as_gpu();

        // Build 4 corner vertices.
        let v00 = WidgetVertex {
            position: [x0, y0, z],
            tex_coord: [s0, t0],
            color: pack_color(color),
            widget_info_index,
        };
        let v01 = WidgetVertex {
            position: [x0, y1, z],
            tex_coord: [s0, t1],
            color: pack_color(color),
            widget_info_index,
        };
        let v10 = WidgetVertex {
            position: [x1, y0, z],
            tex_coord: [s1, t0],
            color: pack_color(color),
            widget_info_index,
        };
        let v11 = WidgetVertex {
            position: [x1, y1, z],
            tex_coord: [s1, t1],
            color: pack_color(color),
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

    #[allow(clippy::too_many_arguments)]
    pub fn push_textured_quad(
        [x0, y0]: [Size; 2],
        [x1, y1]: [Size; 2],
        z: f32,
        [s0, t0]: [f32; 2],
        [s1, t1]: [f32; 2],
        color: &Color,
        widget_info_index: u32,
        win: &Window,
        pool: &mut Vec<WidgetVertex>,
    ) {
        let x0 = x0.as_gpu(win, ScreenDir::Horizontal);
        let y0 = y0.as_gpu(win, ScreenDir::Vertical);
        let x1 = x1.as_gpu(win, ScreenDir::Horizontal);
        let y1 = y1.as_gpu(win, ScreenDir::Vertical);

        // Build 4 corner vertices.
        let v00 = WidgetVertex {
            position: [x0, y0, z],
            tex_coord: [s0, t0],
            color: pack_color(color),
            widget_info_index,
        };
        let v01 = WidgetVertex {
            position: [x0, y1, z],
            tex_coord: [s0, t1],
            color: pack_color(color),
            widget_info_index,
        };
        let v10 = WidgetVertex {
            position: [x1, y0, z],
            tex_coord: [s1, t0],
            color: pack_color(color),
            widget_info_index,
        };
        let v11 = WidgetVertex {
            position: [x1, y1, z],
            tex_coord: [s1, t1],
            color: pack_color(color),
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

    pub fn push_partial_quad(
        [x0, y0]: [AbsSize; 2],
        [x1, y1]: [AbsSize; 2],
        color: &Color,
        pool: &mut Vec<WidgetVertex>,
    ) {
        // let x0 = x0.as_gpu(win, ScreenDir::Horizontal);
        // let y0 = y0.as_gpu(win, ScreenDir::Vertical);
        // let x1 = x1.as_gpu(win, ScreenDir::Horizontal);
        // let y1 = y1.as_gpu(win, ScreenDir::Vertical);
        let x0 = x0.as_px();
        let y0 = y0.as_px();
        let x1 = x1.as_px();
        let y1 = y1.as_px();
        let z = 0f32;
        let s0 = 0f32;
        let s1 = 0f32;
        let t0 = 0f32;
        let t1 = 0f32;
        let widget_info_index = 0;

        // Build 4 corner vertices.
        let v00 = WidgetVertex {
            position: [x0, y0, z],
            tex_coord: [s0, t0],
            color: pack_color(color),
            widget_info_index,
        };
        let v01 = WidgetVertex {
            position: [x0, y1, z],
            tex_coord: [s0, t1],
            color: pack_color(color),
            widget_info_index,
        };
        let v10 = WidgetVertex {
            position: [x1, y0, z],
            tex_coord: [s1, t0],
            color: pack_color(color),
            widget_info_index,
        };
        let v11 = WidgetVertex {
            position: [x1, y1, z],
            tex_coord: [s1, t1],
            color: pack_color(color),
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
        pos_low: [Size; 2],
        pos_high: [Size; 2],
        z: f32,
        color: &Color,
        widget_info_index: u32,
        win: &Window,
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
            win,
            pool,
        )
    }

    pub fn push_quad_ext(
        position: Position<Size>,
        extent: Extent<Size>,
        color: &Color,
        widget_info_index: u32,
        win: &Window,
        pool: &mut Vec<WidgetVertex>,
    ) {
        Self::push_textured_quad(
            [position.left(), position.bottom()],
            [
                position
                    .left()
                    .add(&extent.width(), win, ScreenDir::Horizontal),
                position
                    .bottom()
                    .add(&extent.height(), win, ScreenDir::Vertical),
            ],
            position.depth().as_depth(),
            [0f32, 0f32],
            [0f32, 0f32],
            color,
            widget_info_index,
            win,
            pool,
        )
    }

    pub fn push_region(
        region: Region<RelSize>,
        color: &Color,
        widget_info_index: u32,
        pool: &mut Vec<WidgetVertex>,
    ) {
        Self::push_textured_quad_rel(
            [region.position().left(), region.position().bottom()],
            [
                region.position().left() + region.extent().width(),
                region.position().bottom() + region.extent().height(),
            ],
            region.position().depth().as_depth(),
            [0f32, 0f32],
            [0f32, 0f32],
            color,
            widget_info_index,
            pool,
        )
    }
}
