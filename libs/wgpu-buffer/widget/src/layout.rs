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
use crate::{widget_vertex::WidgetVertex, widgets::FontContext};
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
        let scale_px = 2.0 * size_pts * gpu.guess_dpi() as f32 / 72.0;

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

/*
// Note that each layout has its own vertex/index buffer and a tiny transform
// buffer that might get updated every frame. This is costly per layout. However,
// these are screen text layouts, so there will hopefully never be too many of them
// if we do end up creating lots, we'll need to do some sort of layout caching.
pub struct Layout {
    // The externally exposed handle, for ease of use.
    // layout_handle: LayoutHandle,

    // The font used for rendering this layout.
    // glyph_cache: Arc<RwLock<GlyphCache>>,

    // Cached per-frame render state.
    content: String,
    position_x: TextPositionH,
    position_y: TextPositionV,
    anchor_x: TextAnchorH,
    anchor_y: TextAnchorV,
    color: [f32; 4],

    // Gpu resources
    text_render_context: Option<LayoutTextRenderContext>,
    layout_data_buffer: Arc<Box<wgpu::Buffer>>,
    bind_group: Arc<Box<wgpu::BindGroup>>,
}

impl Layout {
    pub(crate) fn new(
        // layout_handle: LayoutHandle,
        text: &str,
        // glyph_cache: Arc<RwLock<GlyphCache>>,
        bind_group_layout: &wgpu::BindGroupLayout,
        gpu: &GPU,
    ) -> Fallible<Self> {
        let size = mem::size_of::<LayoutData>() as wgpu::BufferAddress;
        let layout_data_buffer = Arc::new(Box::new(gpu.device().create_buffer(
            &wgpu::BufferDescriptor {
                label: Some("text-layout-data-buffer"),
                size,
                usage: wgpu::BufferUsage::all(),
                mapped_at_creation: false,
            },
        )));

        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("text-layout-bind-group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(layout_data_buffer.slice(..)),
            }],
        });

        // let text_render_context = Self::build_text_span(text, &glyph_cache.read(), gpu)?;
        Ok(Self {
            // layout_handle,
            //glyph_cache,
            content: text.to_owned(),
            position_x: TextPositionH::Center,
            position_y: TextPositionV::Center,
            anchor_x: TextAnchorH::Left,
            anchor_y: TextAnchorV::Top,
            color: [1f32, 0f32, 1f32, 1f32],

            text_render_context: None,
            layout_data_buffer,
            bind_group: Arc::new(Box::new(bind_group)),
        })
    }

    pub fn with_span(&mut self, span: &str) -> &mut Self {
        self.set_span(span);
        self
    }

    pub fn with_color(&mut self, clr: &[f32; 4]) -> &mut Self {
        self.set_color(clr);
        self
    }

    pub fn with_horizontal_position(&mut self, pos: TextPositionH) -> &mut Self {
        self.set_horizontal_position(pos);
        self
    }

    pub fn with_vertical_position(&mut self, pos: TextPositionV) -> &mut Self {
        self.set_vertical_position(pos);
        self
    }

    pub fn with_horizontal_anchor(&mut self, anchor: TextAnchorH) -> &mut Self {
        self.set_horizontal_anchor(anchor);
        self
    }

    pub fn with_vertical_anchor(&mut self, anchor: TextAnchorV) -> &mut Self {
        self.set_vertical_anchor(anchor);
        self
    }

    pub fn set_horizontal_position(&mut self, pos: TextPositionH) {
        self.position_x = pos;
    }

    pub fn set_vertical_position(&mut self, pos: TextPositionV) {
        self.position_y = pos;
    }

    pub fn set_horizontal_anchor(&mut self, anchor: TextAnchorH) {
        self.anchor_x = anchor;
    }

    pub fn set_vertical_anchor(&mut self, anchor: TextAnchorV) {
        self.anchor_y = anchor;
    }

    pub fn set_color(&mut self, color: &[f32; 4]) {
        self.color = *color;
    }

    pub fn set_span(&mut self, text: &str) {
        self.text_render_context = None;
        self.content = text.to_owned();
    }

    // pub fn handle(&self) -> LayoutHandle {
    //     self.layout_handle
    // }

    /*
    pub(crate) fn make_upload_buffer(
        &mut self,
        // glyph_cache: &GlyphCache,
        gpu: &GPU,
        tracker: &mut UploadTracker,
    ) -> Fallible<()> {
        if self.text_render_context.is_none() {
            self.text_render_context =
                Some(Layout::build_text_span(&self.content, &glyph_cache, gpu)?);
        }

        let x = self.position_x.to_vulkan();
        let y = self.position_y.to_vulkan();

        let dx = match self.anchor_x {
            TextAnchorH::Left => 0f32,
            TextAnchorH::Right => -self.text_render_context.as_ref().unwrap().render_width,
            TextAnchorH::Center => -self.text_render_context.as_ref().unwrap().render_width / 2f32,
        };

        let dy = match self.anchor_y {
            TextAnchorV::Top => 0f32,
            TextAnchorV::Bottom => -glyph_cache.render_height(),
            TextAnchorV::Center => -glyph_cache.render_height() / 2f32,
        };

        let buffer = gpu.push_slice(
            "text-layout-upload-buffer",
            &[LayoutData {
                text_layout_position: [x + dx, y + dy, 0f32, 0f32],
                text_layout_color: self.color,
            }],
            wgpu::BufferUsage::all(),
        );
        tracker.upload(
            buffer,
            self.layout_data_buffer.clone(),
            mem::size_of::<LayoutData>(),
        );

        Ok(())
    }
     */

    pub fn vertex_buffer(&self) -> wgpu::BufferSlice {
        self.text_render_context
            .as_ref()
            .unwrap()
            .vertex_buffer
            .slice(..)
    }

    pub fn index_buffer(&self) -> wgpu::BufferSlice {
        self.text_render_context
            .as_ref()
            .unwrap()
            .index_buffer
            .slice(..)
    }

    pub fn index_range(&self) -> Range<u32> {
        0u32..self.text_render_context.as_ref().unwrap().index_count
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }
}
*/
