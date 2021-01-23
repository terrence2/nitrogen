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
use crate::{
    color::Color,
    text_run::{SpanSelection, TextSpan},
    widget_vertex::WidgetVertex,
    SANS_FONT_NAME,
};
use atlas::{AtlasPacker, Frame};
use failure::Fallible;
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
    // The width of the span.
    pub width: f32,

    // The height of the span from top of ascent to bottom of descent.
    pub height: f32,

    // Distance from the origin to the top and bottom of the span, respectively.
    pub ascent: f32,
    pub descent: f32,

    // Expected additional builtin line gap (baseline to baseline) for this span.
    pub line_gap: f32,

    // Initial position of text and background buffers that we push into.
    pub initial_text_offset: usize,
    pub initial_background_offset: usize,
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

    pub fn load_glyph(&mut self, fid: FontId, c: char, scale: f32) -> Fallible<Frame> {
        if let Some(frame) = self.trackers[&fid].glyphs.get(&(c, OrderedFloat(scale))) {
            return Ok(*frame);
        }
        // Note: cannot delegate to GlyphTracker because of the second mutable borrow.
        let img = self.trackers[&fid].font.read().render_glyph(c, scale);
        let frame = self.glyph_sheet.push_image(&img)?;
        self.trackers
            .get_mut(&fid)
            .unwrap()
            .glyphs
            .insert((c, OrderedFloat(scale)), frame);
        Ok(frame)
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

    #[allow(clippy::too_many_arguments)]
    pub fn layout_text(
        &mut self,
        span: &TextSpan,
        widget_info_index: u32,
        offset: [f32; 3],
        selection_area: SpanSelection,
        gpu: &GPU,
        text_pool: &mut Vec<WidgetVertex>,
        background_pool: &mut Vec<WidgetVertex>,
    ) -> Fallible<TextSpanMetrics> {
        let initial_text_offset = text_pool.len();
        let initial_background_offset = background_pool.len();

        let w = self.glyph_sheet_width();
        let h = self.glyph_sheet_height();

        let phys_w = gpu.physical_size().width as f32;

        let px_scaling = if span.size_pts() <= 12.0 { 4.0 } else { 2.0 };

        // Use ttf standard formula to adjust scale by pts to figure out base rendering size.
        // Note that we add some extra scaling and use linear filtering to help account for
        // our lack of sub-pixel and pixel alignment techniques.
        let scale_px = px_scaling * span.size_pts() * gpu.scale_factor() as f32;

        // We used guess_dpi to project from logical to physical pixels for rendering, so scale
        // vertices proportional to physical size for vertex layout. Note that the extra factor of
        // 2 here is to account for the fact that vertex ranges are between [-1,1], not to account
        // for the scaling of scale_px above.
        let scale_y =
            Self::GNOME_SCALE_FACTOR * 4.0 / gpu.physical_size().height as f32 / px_scaling;
        let scale_x = scale_y * gpu.aspect_ratio_f32();

        // Font rendering is based around the baseline. We want it based around the top-left
        // corner instead, so move down by the ascent.
        let font = self.get_font(span.font());
        let descent = font.read().descent(scale_px);
        let ascent = font.read().ascent(scale_px);
        let line_gap = font.read().line_gap(scale_px);

        let mut x_pos = 0f32;
        let mut prior = None;
        for (i, c) in span.content().chars().enumerate() {
            let frame = self.load_glyph(span.font(), c, scale_px)?;
            let font = self.get_font(span.font());
            let ((lo_x, lo_y), (hi_x, hi_y)) = font.read().pixel_bounding_box(c, scale_px);
            let lsb = font.read().left_side_bearing(c, scale_px);
            let adv = font.read().advance_width(c, scale_px);
            let kerning = prior
                .map(|p| font.read().pair_kerning(p, c, scale_px))
                .unwrap_or(0f32);
            prior = Some(c);

            x_pos += kerning;
            x_pos = (x_pos * phys_w).floor() / phys_w;

            let px0 = x_pos + lo_x as f32;
            let px1 = x_pos + hi_x as f32;
            let mut x0 = offset[0] + px0 * scale_x;
            let mut x1 = offset[0] + px1 * scale_x;
            x0 = (x0 * phys_w).floor() / phys_w;
            x1 = (x1 * phys_w).floor() / phys_w;

            let y0 = offset[1] + lo_y as f32 * scale_y;
            let y1 = offset[1] + hi_y as f32 * scale_y;
            let z = offset[2];

            let s0 = frame.s0(w);
            let s1 = frame.s1(w);
            let t0 = frame.t0(h);
            let t1 = frame.t1(h);

            WidgetVertex::push_textured_quad(
                [x0, y0],
                [x1, y1],
                z,
                [s0, t0],
                [s1, t1],
                span.color(),
                widget_info_index,
                text_pool,
            );

            // Apply cursor or selection
            if let SpanSelection::Cursor { position } = selection_area {
                if i == position {
                    // Draw cursor, pixel aligned.
                    let mut bx0 = offset[0] + x_pos * scale_x;
                    bx0 = (bx0 * phys_w).floor() / phys_w;
                    let bx1 = bx0 + px_scaling / gpu.physical_size().width as f32;
                    let by0 = offset[1] + descent * scale_y;
                    let by1 = offset[1] + ascent * scale_y;
                    let bz = offset[2] - 0.1;

                    WidgetVertex::push_quad(
                        [bx0, by0],
                        [bx1, by1],
                        bz,
                        &Color::White,
                        widget_info_index,
                        background_pool,
                    );
                }
            }
            if let SpanSelection::Select { range } = &selection_area {
                if range.contains(&i) {
                    let bx0 = offset[0] + x_pos * scale_x;
                    let bx1 = offset[0] + (x_pos + kerning + lo_x as f32 + adv) * scale_x;
                    let by0 = offset[1] + descent * scale_y;
                    let by1 = offset[1] + ascent * scale_y;
                    let bz = offset[2] - 0.1;

                    WidgetVertex::push_quad(
                        [bx0, by0],
                        [bx1, by1],
                        bz,
                        &Color::Blue,
                        widget_info_index,
                        background_pool,
                    );
                }
            }

            x_pos += adv - lsb;
        }

        if let SpanSelection::Cursor { position } = selection_area {
            if position == span.content().len() {
                // Draw cursor, pixel aligned.
                let mut bx0 = offset[0] + x_pos * scale_x;
                bx0 = (bx0 * phys_w).floor() / phys_w;
                let bx1 = bx0 + px_scaling / gpu.physical_size().width as f32;
                let by0 = offset[1] + descent * scale_y;
                let by1 = offset[1] + ascent * scale_y;
                let bz = offset[2] - 0.1;

                WidgetVertex::push_quad(
                    [bx0, by0],
                    [bx1, by1],
                    bz,
                    &Color::White,
                    widget_info_index,
                    background_pool,
                );
            }
        }

        Ok(TextSpanMetrics {
            width: x_pos * scale_x,
            height: (ascent - descent) * scale_y,
            ascent: ascent * scale_y,
            descent: descent * scale_y,
            line_gap,
            initial_text_offset,
            initial_background_offset,
        })
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
