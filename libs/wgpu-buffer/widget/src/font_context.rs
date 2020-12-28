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
use crate::{paint_context::SpanLayoutContext, widget_vertex::WidgetVertex, SANS_FONT_NAME};
use atlas::{AtlasPacker, Frame};
use font_common::FontInterface;
use gpu::{UploadTracker, GPU};
use image::Luma;
use ordered_float::OrderedFloat;
use parking_lot::RwLock;
use std::{collections::HashMap, sync::Arc};

pub struct GlyphTracker {
    font: Arc<RwLock<dyn FontInterface>>,
    glyphs: HashMap<(char, OrderedFloat<f32>), Frame>,
}

impl GlyphTracker {
    pub fn new(font: Arc<RwLock<dyn FontInterface>>) -> Self {
        Self {
            font,
            glyphs: HashMap::new(),
        }
    }

    pub fn font(&self) -> Arc<RwLock<dyn FontInterface>> {
        self.font.clone()
    }
}

pub struct FontContext {
    glyph_sheet: AtlasPacker<Luma<u8>>,
    trackers: HashMap<String, GlyphTracker>,
}

pub struct TextSpanMetrics {
    pub width: f32,
    pub baseline_height: f32,
    pub height: f32,
}

impl FontContext {
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            glyph_sheet: AtlasPacker::new(
                device,
                256,
                256,
                Luma([0; 1]),
                wgpu::TextureFormat::R8Unorm,
                wgpu::TextureUsage::SAMPLED,
                wgpu::FilterMode::Linear,
            ),
            trackers: HashMap::new(),
        }
    }

    pub fn upload(&mut self, gpu: &GPU, tracker: &mut UploadTracker) {
        self.glyph_sheet.upload(gpu, tracker);
    }

    pub fn glyph_sheet_width(&self) -> u32 {
        self.glyph_sheet.width()
    }

    pub fn glyph_sheet_height(&self) -> u32 {
        self.glyph_sheet.height()
    }

    pub fn glyph_sheet_texture_layout_entry(&self, binding: u32) -> wgpu::BindGroupLayoutEntry {
        self.glyph_sheet.texture_layout_entry(binding)
    }

    pub fn glyph_sheet_sampler_layout_entry(&self, binding: u32) -> wgpu::BindGroupLayoutEntry {
        self.glyph_sheet.sampler_layout_entry(binding)
    }

    pub fn glyph_sheet_texture_binding(&self, binding: u32) -> wgpu::BindGroupEntry {
        self.glyph_sheet.texture_binding(binding)
    }

    pub fn glyph_sheet_sampler_binding(&self, binding: u32) -> wgpu::BindGroupEntry {
        self.glyph_sheet.sampler_binding(binding)
    }

    pub fn get_font(&self, font_name: &str) -> Arc<RwLock<dyn FontInterface>> {
        self.trackers[self.font_for(font_name)].font()
    }

    pub fn add_font(&mut self, font_name: String, font: Arc<RwLock<dyn FontInterface>>) {
        assert!(
            !self.trackers.contains_key(&font_name),
            "font already loaded"
        );
        self.trackers.insert(font_name, GlyphTracker::new(font));
    }

    pub fn load_glyph(&mut self, font_name: &str, c: char, scale: f32) -> Frame {
        let name = self.font_for(font_name);
        if let Some(frame) = self.trackers[name].glyphs.get(&(c, OrderedFloat(scale))) {
            return *frame;
        }
        // Note: cannot delegate to GlyphTracker because of the second mutable borrow.
        let img = self.trackers[name].font.read().render_glyph(c, scale);
        let frame = self.glyph_sheet.push_image(&img);
        self.trackers
            .get_mut(name)
            .unwrap()
            .glyphs
            .insert((c, OrderedFloat(scale)), frame);
        frame
    }

    fn font_for<'a>(&self, font_name: &'a str) -> &'a str {
        if self.trackers.contains_key(font_name) {
            font_name
        } else {
            SANS_FONT_NAME
        }
    }

    // Because of the indirection when rendering, we can't easily take advantage of sub-pixel
    // techniques, or even guarantee pixel-perfect placement. To help with text clarity, we thus
    // double our render size and use linear filtering. This is wasteful, however, so we scale
    // up a bit when rendering to get more use out of the pixels we place. Thus we take a hint
    // from Gnome's font rendering subsystem and assume a 96dpi screen compared to the 72 that
    // TTF assumes, to get the same nice look to what Gnome gives us.
    const TTF_FONT_DPI: f32 = 72.0;
    const GNOME_DPI: f32 = 96.0;
    const GNOME_SCALE_FACTOR: f32 = Self::TTF_FONT_DPI / Self::GNOME_DPI;

    pub fn layout_text(
        &mut self,
        ctx: SpanLayoutContext,
        gpu: &GPU,
        pool: &mut Vec<WidgetVertex>,
    ) -> TextSpanMetrics {
        let w = self.glyph_sheet_width();
        let h = self.glyph_sheet_height();

        let px_scaling = if ctx.size_pts <= 12.0 { 4.0 } else { 2.0 };

        // Use ttf standard formula to adjust scale by pts to figure out base rendering size.
        // Note that we add some extra scaling and use linear filtering to help account for
        // our lack of sub-pixel and pixel alignment techniques.
        let scale_px = px_scaling * ctx.size_pts * gpu.scale_factor() as f32;

        // We used guess_dpi to project from logical to physical pixels for rendering, so scale
        // vertices proportional to physical size for vertex layout. Note that the factor of 2
        // here is to account for the fact that vertex ranges are between [-1,1], not to account
        // for the scaling of scale_px above.
        let scale_y =
            Self::GNOME_SCALE_FACTOR * 4.0 / gpu.physical_size().height as f32 / px_scaling;
        let scale_x = scale_y * gpu.aspect_ratio_f32();

        // Font rendering is based around the baseline. We want it based around the top-left
        // corner instead, so move down by the ascent.
        let font = self.get_font(ctx.font_name);
        let descent = font.read().descent(scale_px);
        let ascent = font.read().ascent(scale_px);

        let mut offset = 0f32;
        let mut prior = None;
        for c in ctx.span.chars() {
            let frame = self.load_glyph(ctx.font_name, c, scale_px);
            let font = self.get_font(ctx.font_name);
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
            let y0 = -(hi_y + ascent) * scale_y;
            let y1 = -(lo_y + ascent) * scale_y;

            let s0 = frame.s0(w);
            let s1 = frame.s1(w);
            let t0 = frame.t0(h);
            let t1 = frame.t1(h);

            // Build 4 corner vertices.
            let v00 = WidgetVertex {
                position: [ctx.offset[0] + x0, ctx.offset[1] + y0, ctx.offset[2]],
                tex_coord: [s0, t0],
                widget_info_index: ctx.widget_info_index,
            };
            let v01 = WidgetVertex {
                position: [ctx.offset[0] + x0, ctx.offset[1] + y1, ctx.offset[2]],
                tex_coord: [s0, t1],
                widget_info_index: ctx.widget_info_index,
            };
            let v10 = WidgetVertex {
                position: [ctx.offset[0] + x1, ctx.offset[1] + y0, ctx.offset[2]],
                tex_coord: [s1, t0],
                widget_info_index: ctx.widget_info_index,
            };
            let v11 = WidgetVertex {
                position: [ctx.offset[0] + x1, ctx.offset[1] + y1, ctx.offset[2]],
                tex_coord: [s1, t1],
                widget_info_index: ctx.widget_info_index,
            };

            // Push 2 triangles
            pool.push(v00);
            pool.push(v10);
            pool.push(v01);
            pool.push(v01);
            pool.push(v10);
            pool.push(v11);

            offset += adv - lsb;
        }

        TextSpanMetrics {
            width: offset * scale_x,
            height: (ascent - descent) * scale_y,
            baseline_height: -descent * scale_y,
        }
    }
}
