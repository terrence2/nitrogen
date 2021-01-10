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
use std::{borrow::Borrow, collections::HashMap, sync::Arc};

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

pub struct TextSpanMetrics {
    pub width: f32,
    pub baseline_height: f32,
    pub height: f32,
}

pub struct FontContext {
    glyph_sheet: AtlasPacker<Luma<u8>>,
    trackers: HashMap<FontId, GlyphTracker>,
    name_manager: FontNameManager,
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
            name_manager: Default::default(),
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

    pub fn get_font_by_name(&self, font_name: &str) -> Arc<RwLock<dyn FontInterface>> {
        self.get_font(self.font_id_for_name(font_name))
    }

    pub fn get_font(&self, font_id: FontId) -> Arc<RwLock<dyn FontInterface>> {
        self.trackers[&font_id].font()
    }

    pub fn add_font<S: Borrow<str> + Into<String>>(
        &mut self,
        font_name: S,
        font: Arc<RwLock<dyn FontInterface>>,
    ) {
        let fid = self.name_manager.allocate(font_name);
        self.trackers.insert(fid, GlyphTracker::new(font));
    }

    pub fn load_glyph(&mut self, fid: FontId, c: char, scale: f32) -> Frame {
        if let Some(frame) = self.trackers[&fid].glyphs.get(&(c, OrderedFloat(scale))) {
            return *frame;
        }
        // Note: cannot delegate to GlyphTracker because of the second mutable borrow.
        let img = self.trackers[&fid].font.read().render_glyph(c, scale);
        let frame = self.glyph_sheet.push_image(&img);
        self.trackers
            .get_mut(&fid)
            .unwrap()
            .glyphs
            .insert((c, OrderedFloat(scale)), frame);
        frame
    }

    pub fn font_id_for_name(&self, font_name: &str) -> FontId {
        if let Some(fid) = self.name_manager.get_by_name(font_name) {
            return fid;
        }
        debug_assert_eq!(
            self.name_manager.lookup_by_name(SANS_FONT_NAME),
            SANS_FONT_ID
        );
        SANS_FONT_ID
    }

    pub fn font_name_for_id(&self, font_id: FontId) -> &str {
        self.name_manager.lookup_by_id(font_id)
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
        span: SpanLayoutContext,
        gpu: &GPU,
        text_pool: &mut Vec<WidgetVertex>,
        background_pool: &mut Vec<WidgetVertex>,
    ) -> TextSpanMetrics {
        let w = self.glyph_sheet_width();
        let h = self.glyph_sheet_height();

        let px_scaling = if span.size_pts <= 12.0 { 4.0 } else { 2.0 };

        // Use ttf standard formula to adjust scale by pts to figure out base rendering size.
        // Note that we add some extra scaling and use linear filtering to help account for
        // our lack of sub-pixel and pixel alignment techniques.
        let scale_px = px_scaling * span.size_pts * gpu.scale_factor() as f32;

        // We used guess_dpi to project from logical to physical pixels for rendering, so scale
        // vertices proportional to physical size for vertex layout. Note that the extra factor of
        // 2 here is to account for the fact that vertex ranges are between [-1,1], not to account
        // for the scaling of scale_px above.
        let scale_y =
            Self::GNOME_SCALE_FACTOR * 4.0 / gpu.physical_size().height as f32 / px_scaling;
        let scale_x = scale_y * gpu.aspect_ratio_f32();

        // Font rendering is based around the baseline. We want it based around the top-left
        // corner instead, so move down by the ascent.
        let font = self.get_font(span.font_id);
        let descent = font.read().descent(scale_px);
        let ascent = font.read().ascent(scale_px);

        let mut offset = 0f32;
        let mut prior = None;
        for (i, c) in span.span.chars().enumerate() {
            let frame = self.load_glyph(span.font_id, c, scale_px);
            let font = self.get_font(span.font_id);
            let ((lo_x, lo_y), (hi_x, hi_y)) = font.read().exact_bounding_box(c, scale_px);
            let lsb = font.read().left_side_bearing(c, scale_px);
            let adv = font.read().advance_width(c, scale_px);
            let kerning = prior
                .map(|p| font.read().pair_kerning(p, c, scale_px))
                .unwrap_or(0f32);
            prior = Some(c);

            // Layout from 0-> and let our transform put us in the right spot.
            let x0 = span.offset[0] + (offset + kerning + lo_x) * scale_x;
            let x1 = span.offset[0] + (offset + kerning + hi_x) * scale_x;
            let y0 = span.offset[1] - (hi_y + ascent) * scale_y;
            let y1 = span.offset[1] - (lo_y + ascent) * scale_y;
            let z = span.offset[2];

            let s0 = frame.s0(w);
            let s1 = frame.s1(w);
            let t0 = frame.t0(h);
            let t1 = frame.t1(h);

            // Build 4 corner vertices.
            let v00 = WidgetVertex {
                position: [x0, y0, z],
                tex_coord: [s0, t0],
                widget_info_index: span.widget_info_index,
            };
            let v01 = WidgetVertex {
                position: [x0, y1, z],
                tex_coord: [s0, t1],
                widget_info_index: span.widget_info_index,
            };
            let v10 = WidgetVertex {
                position: [x1, y0, z],
                tex_coord: [s1, t0],
                widget_info_index: span.widget_info_index,
            };
            let v11 = WidgetVertex {
                position: [x1, y1, z],
                tex_coord: [s1, t1],
                widget_info_index: span.widget_info_index,
            };

            // Push 2 triangles
            text_pool.push(v00);
            text_pool.push(v10);
            text_pool.push(v01);
            text_pool.push(v01);
            text_pool.push(v10);
            text_pool.push(v11);

            // Apply cursor or selection
            if let Some(area) = &span.selection_area {
                if area.start == i && area.is_empty() {
                    // Draw cursor
                    let bx0 = span.offset[0] + offset * scale_x;
                    let bx1 = span.offset[0] + (offset + 2.) * scale_x;
                    let by0 = span.offset[1];
                    let by1 = span.offset[1] - (ascent + descent) * scale_y;
                    let bz = span.offset[2] + 0.1;
                    let bv00 = WidgetVertex {
                        position: [bx0, by0, bz],
                        tex_coord: [0.0, 0.0],
                        widget_info_index: span.widget_info_index,
                    };
                    let bv01 = WidgetVertex {
                        position: [bx0, by1, bz],
                        tex_coord: [0.0, 0.0],
                        widget_info_index: span.widget_info_index,
                    };
                    let bv10 = WidgetVertex {
                        position: [bx1, by0, bz],
                        tex_coord: [0.0, 0.0],
                        widget_info_index: span.widget_info_index,
                    };
                    let bv11 = WidgetVertex {
                        position: [bx1, by1, bz],
                        tex_coord: [0.0, 0.0],
                        widget_info_index: span.widget_info_index,
                    };

                    // Draw selection over item
                    background_pool.push(bv00);
                    background_pool.push(bv10);
                    background_pool.push(bv01);
                    background_pool.push(bv01);
                    background_pool.push(bv10);
                    background_pool.push(bv11);
                } else if area.contains(&i) {
                    let bx0 = span.offset[0] + offset * scale_x;
                    let bx1 = span.offset[0] + (offset + kerning + hi_x) * scale_x;
                    let by1 = span.offset[1];
                    let by0 = span.offset[1] - (ascent + descent) * scale_y;
                    let bz = span.offset[2] - 0.1;
                    let bv00 = WidgetVertex {
                        position: [bx0, by0, bz],
                        tex_coord: [0.0, 0.0],
                        widget_info_index: span.widget_info_index,
                    };
                    let bv01 = WidgetVertex {
                        position: [bx0, by1, bz],
                        tex_coord: [0.0, 0.0],
                        widget_info_index: span.widget_info_index,
                    };
                    let bv10 = WidgetVertex {
                        position: [bx1, by0, bz],
                        tex_coord: [0.0, 0.0],
                        widget_info_index: span.widget_info_index,
                    };
                    let bv11 = WidgetVertex {
                        position: [bx1, by1, bz],
                        tex_coord: [0.0, 0.0],
                        widget_info_index: span.widget_info_index,
                    };

                    // Draw selection over item
                    background_pool.push(bv00);
                    background_pool.push(bv10);
                    background_pool.push(bv01);
                    background_pool.push(bv01);
                    background_pool.push(bv10);
                    background_pool.push(bv11);
                }
            }

            offset += adv - lsb;
        }

        TextSpanMetrics {
            width: offset * scale_x,
            height: (ascent - descent) * scale_y,
            baseline_height: -descent * scale_y,
        }
    }
}

#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct FontId(u32);

pub const SANS_FONT_ID: FontId = FontId(0);

/// Enables tracking of font information without leaking lifetimes everywhere or taking String
/// allocations everywhere. Generally the top-level widget system will hand this out with
/// any operation that deals with fonts.
#[derive(Clone, Default)]
struct FontNameManager {
    last_id: usize,
    id_to_name: HashMap<FontId, String>,
    name_to_id: HashMap<String, FontId>,
}

impl FontNameManager {
    pub fn get_by_name(&self, name: &str) -> Option<FontId> {
        self.name_to_id.get(name).copied()
    }

    // panics if the name has not be allocated
    pub fn lookup_by_name(&self, name: &str) -> FontId {
        self.name_to_id[name]
    }

    // panics if the id has not be allocated
    pub fn lookup_by_id(&self, font_id: FontId) -> &str {
        &self.id_to_name[&font_id]
    }

    pub fn allocate<S: Borrow<str> + Into<String>>(&mut self, name: S) -> FontId {
        assert!(!self.name_to_id.contains_key(name.borrow()));
        assert!(self.last_id < std::u32::MAX as usize);
        let name = name.into();
        let fid = FontId(self.last_id as u32);
        self.last_id += 1;
        self.id_to_name.insert(fid, name.clone());
        self.name_to_id.insert(name, fid);
        fid
    }
}
