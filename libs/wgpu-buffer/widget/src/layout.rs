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
use crate::{font_context::FontContext, widget_vertex::WidgetVertex};
use gpu::GPU;

pub struct LayoutEngine;

impl LayoutEngine {
    // Because of the indirection when rendering, we can't easily take advantage of sub-pixel
    // techniques, or even guarantee pixel-perfect placement. To help with text clarity, we thus
    // double our render size and use linear filtering. This is wasteful, however, so we scale
    // up a bit when rendering to get more use out of the pixels we place. Thus we take a hint
    // from Gnome's font rendering subsystem and assume a 96dpi screen compared to the 72 that
    // TTF assumes, to get the same nice look to what Gnome gives us.
    const TTF_FONT_DPI: f32 = 72.0;
    const GNOME_DPI: f32 = 96.0;
    const GNOME_SCALE_FACTOR: f32 = Self::TTF_FONT_DPI / Self::GNOME_DPI;

    pub fn span_to_triangles(
        gpu: &GPU,
        span: &str,
        font_context: &mut FontContext,
        font_name: &str,
        size_pts: f32,
        depth: f32,
        widget_info_index: u32,
        verts: &mut Vec<WidgetVertex>,
    ) {
        let w = font_context.glyph_sheet_width();
        let h = font_context.glyph_sheet_height();

        // Use ttf standard formula to adjust scale by pts to figure out base rendering size.
        // Note that we add an extra scale by 2x and use linear filtering to help account for
        // our lack of sub-pixel and pixel alignment techniques.
        let scale_px = 2.0 * size_pts * gpu.scale_factor() as f32;

        // We used guess_dpi to project from logical to physical pixels for rendering, so scale
        // vertices proportional to physical size for vertex layout. Note that the factor of 2
        // here is to account for the fact that vertex ranges are between [-1,1], not to account
        // for the scaling of scale_px above.
        let scale_y = Self::GNOME_SCALE_FACTOR * 2.0 / gpu.physical_size().height as f32;
        let scale_x = scale_y * gpu.aspect_ratio_f32();

        let mut offset = 0f32;
        let mut prior = None;
        for c in span.chars() {
            let frame = font_context.load_glyph(font_name, c, scale_px);
            let font = font_context.get_font(font_name);
            let ((lo_x, lo_y), (hi_x, hi_y)) = font.read().exact_bounding_box(c, scale_px);
            let lsb = font.read().left_side_bearing(c, scale_px);
            let adv = font.read().advance_width(c, scale_px);
            let kerning = prior
                .map(|p| font.read().pair_kerning(p, c, scale_px))
                .unwrap_or(0f32);
            prior = Some(c);

            // Layout from 0-> and let our transform put us in the right spot.
            let x0 = (offset + kerning + lo_x) * scale_x;
            let x1 = (offset + kerning + hi_x) * scale_x;
            let y0 = -hi_y * scale_y;
            let y1 = -lo_y * scale_y;

            let s0 = frame.s0(w);
            let s1 = frame.s1(w);
            let t0 = frame.t0(h);
            let t1 = frame.t1(h);

            // Build 4 corner vertices.
            let v00 = WidgetVertex {
                position: [x0, y0, depth],
                tex_coord: [s0, t0],
                widget_info_index,
            };
            let v01 = WidgetVertex {
                position: [x0, y1, depth],
                tex_coord: [s0, t1],
                widget_info_index,
            };
            let v10 = WidgetVertex {
                position: [x1, y0, depth],
                tex_coord: [s1, t0],
                widget_info_index,
            };
            let v11 = WidgetVertex {
                position: [x1, y1, depth],
                tex_coord: [s1, t1],
                widget_info_index,
            };

            // Push 2 triangles
            verts.push(v00);
            verts.push(v10);
            verts.push(v01);
            verts.push(v01);
            verts.push(v10);
            verts.push(v11);

            offset += adv - lsb;
        }
    }
}
