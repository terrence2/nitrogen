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
    region::Position,
    text_run::{SpanSelection, TextSpan},
    widget_vertex::WidgetVertex,
    SANS_FONT_NAME,
};
use anyhow::Result;
use atlas::{AtlasPacker, Frame};
use font_common::{FontAdvance, FontInterface};
use gpu::Gpu;
use image::Luma;
use nitrous::Value;
use ordered_float::OrderedFloat;
use parking_lot::RwLock;
use std::hint::unreachable_unchecked;
use std::{borrow::Borrow, collections::HashMap, env, path::PathBuf, sync::Arc};
use window::{
    size::{AbsSize, LeftBound, RelSize, ScreenDir},
    Window,
};

#[derive(Debug)]
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

#[derive(Clone, Debug, Default)]
pub struct TextSpanMetrics {
    // The width of the span.
    pub width: AbsSize,

    // The height of the span from top of ascent to bottom of descent.
    pub height: AbsSize,

    // Distance from the origin to the top and bottom of the span, respectively.
    pub ascent: AbsSize,
    pub descent: AbsSize,

    // Expected additional builtin line gap (baseline to baseline) for this span.
    pub line_gap: AbsSize,
}

#[derive(Debug)]
pub struct FontContext {
    glyph_sheet: AtlasPacker<Luma<u8>>,
    trackers: HashMap<FontId, GlyphTracker>,
    name_manager: FontNameManager,
    dump_texture_path: Option<PathBuf>,
}

impl FontContext {
    pub fn new(gpu: &Gpu) -> Self {
        Self {
            glyph_sheet: AtlasPacker::new(
                "glyph_sheet",
                gpu,
                256 * 4,
                256,
                wgpu::TextureFormat::R8Unorm,
                wgpu::FilterMode::Linear,
            ),
            trackers: HashMap::new(),
            name_manager: Default::default(),
            dump_texture_path: None,
        }
    }

    pub fn handle_dump_texture(&mut self, gpu: &mut Gpu) -> Result<()> {
        if let Some(dump_path) = &self.dump_texture_path {
            self.glyph_sheet.dump_texture(gpu, dump_path)?;
        }
        self.dump_texture_path = None;
        Ok(())
    }

    pub fn maintain_font_atlas(&mut self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder) {
        self.glyph_sheet.encode_frame_uploads(gpu, encoder);
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

    pub fn load_glyph(&mut self, fid: FontId, c: char, scale: AbsSize, gpu: &Gpu) -> Result<Frame> {
        if let Some(frame) = self.trackers[&fid]
            .glyphs
            .get(&(c, OrderedFloat(scale.as_pts())))
        {
            return Ok(*frame);
        }
        // Note: cannot delegate to GlyphTracker because of the second mutable borrow.
        let img = self.trackers[&fid].font.read().render_glyph(c, scale);

        let frame = self.glyph_sheet.push_image(&img, gpu)?;
        self.trackers
            .get_mut(&fid)
            .unwrap()
            .glyphs
            .insert((c, OrderedFloat(scale.as_pts())), frame);
        Ok(frame)
    }

    pub fn cache_ascii_glyphs(&mut self, fid: FontId, scale: AbsSize, gpu: &Gpu) -> Result<()> {
        for c in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789`-=[[]\\;',./!@#$%^&*()_+{}|:\"<>?".chars() {
            self.load_glyph(fid, c, scale, gpu)?;
        }
        Ok(())
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

    pub fn dump_glyphs(&mut self) -> Result<()> {
        let mut path = env::current_dir()?;
        path.push("__dump__");
        path.push("font_context_glyphs.png");
        self.dump_texture_path = Some(path);
        Ok(())
    }

    fn align_to_px(phys_w: f32, x_pos: &mut AbsSize) {
        *x_pos = AbsSize::from_px((x_pos.as_px() * phys_w).floor() / phys_w);
    }

    pub fn measure_text(&mut self, span: &mut TextSpan, win: &Window) -> Result<TextSpanMetrics> {
        if let Some(metrics) = span.metrics() {
            debug_assert_eq!(
                span.layout_cache().len(),
                span.content().chars().count() * 6
            );
            return Ok(metrics.to_owned());
        }
        debug_assert_eq!(span.layout_cache().len(), 0);

        let phys_w = win.width() as f32;
        // let scale_px = (span.size() * win.dpi_scale_factor() as f32)
        //     .as_abs(win, ScreenDir::Horizontal)
        //     .ceil();
        let scale_px = span.size().as_abs(win, ScreenDir::Horizontal).ceil();

        // Font rendering is based around the baseline. We want it based around the top-left
        // corner instead, so move down by the ascent.
        let font_p = self.get_font(span.font());
        let font = font_p.read();
        let descent = font.descent(scale_px);
        let ascent = font.ascent(scale_px);
        let line_gap = font.line_gap(scale_px);
        let advance = font.advance_style();

        let mut x_pos = AbsSize::from_px(0.);
        let mut prior = None;
        let mut cache = Vec::new();
        for c in span.content().chars() {
            let ((lo_x, lo_y), (mut hi_x, hi_y)) = font.pixel_bounding_box(c, scale_px);
            let lsb = font.left_side_bearing(c, scale_px);
            let adv = font.advance_width(c, scale_px);
            let kerning = prior
                .map(|p| font.pair_kerning(p, c, scale_px))
                .unwrap_or_else(AbsSize::zero);
            prior = Some(c);

            if advance != FontAdvance::Mono {
                x_pos += kerning;
            }
            Self::align_to_px(phys_w, &mut x_pos);

            // Since we use the start and end span, make sure that our degenerate rect is
            // at least wide enough to serve as a placeholder char.
            if c == ' ' {
                hi_x += adv;
            }

            let x0 = x_pos + lo_x;
            let x1 = x_pos + hi_x;
            let y0 = lo_y;
            let y1 = hi_y;

            WidgetVertex::push_partial_quad([x0, y0], [x1, y1], span.color(), &mut cache);

            x_pos += match advance {
                FontAdvance::Mono => adv,
                FontAdvance::Sans => adv - lsb,
            };
        }

        let metrics = TextSpanMetrics {
            width: x_pos,
            height: ascent - descent,
            ascent,
            descent,
            line_gap,
        };
        span.set_metrics(&metrics);
        span.layout_cache_mut().append(&mut cache);
        Ok(metrics)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn layout_text(
        &mut self,
        span: &TextSpan,
        widget_info_index: u32,
        offset: Position<AbsSize>,
        selection_area: SpanSelection,
        win: &Window,
        gpu: &Gpu,
        text_pool: &mut Vec<WidgetVertex>,
        background_pool: &mut Vec<WidgetVertex>,
    ) -> Result<()> {
        let gs_width = self.glyph_sheet_width();
        let gs_height = self.glyph_sheet_height();

        // Use the physical width to re-align all pixel boxes to pixel boundaries.
        let phys_w = win.width() as f32;

        // The font system expects scales in pixels.
        // let scale_px = (span.size() * win.dpi_scale_factor() as f32)
        //     .as_abs(win, ScreenDir::Horizontal)
        //     .ceil();
        let scale_px = span.size().as_abs(win, ScreenDir::Horizontal).ceil();

        // Font rendering is based around the baseline. We want it based around the top-left
        // corner instead, so move down by the ascent.
        let ascent = span.metrics().unwrap().ascent;
        let descent = span.metrics().unwrap().descent;

        let mut sel_range_start = None;
        let mut sel_range_end = None;
        let mut bx0 = AbsSize::zero();
        let mut bx1 = AbsSize::zero();
        let x_base = offset.left();
        let y_pos = offset.bottom();
        let z_depth = offset.depth().as_depth();
        let mut cache_offset = 0;
        for (i, c) in span.content().chars().enumerate() {
            let frame = self.load_glyph(span.font(), c, scale_px, gpu)?;

            let s0 = frame.s0(gs_width);
            let t0 = frame.t0(gs_height);
            let s1 = frame.s1(gs_width);
            let t1 = frame.t1(gs_height);
            for j in 0..6 {
                let mut v = span.layout_cache()[cache_offset + j];

                v.position[0] += x_base.as_px();
                if j == 0 {
                    bx0 = AbsSize::from_px(v.position[0]);
                }
                if j == 5 {
                    bx1 = AbsSize::from_px(v.position[0]);
                }
                v.position[0] = AbsSize::from_px(v.position[0])
                    .as_rel(win, ScreenDir::Horizontal)
                    .as_gpu();

                v.position[1] += y_pos.as_px();
                v.position[1] = AbsSize::from_px(v.position[1])
                    .as_rel(win, ScreenDir::Vertical)
                    .as_gpu();

                v.position[2] = z_depth;

                v.widget_info_index = widget_info_index;
                v.tex_coord = match j {
                    0 => [s0, t0],
                    1 => [s1, t0],
                    2 => [s0, t1],
                    3 => [s0, t1],
                    4 => [s1, t0],
                    5 => [s1, t1],
                    _ => unsafe { unreachable_unchecked() },
                };
                text_pool.push(v);
            }
            cache_offset += 6;

            // Apply cursor or selection
            if let SpanSelection::Cursor { position } = selection_area {
                if i == position {
                    // Draw cursor, pixel aligned.
                    Self::align_to_px(phys_w, &mut bx0);
                    bx0 -= AbsSize::from_px(1.);
                    let bx1 = bx0 + AbsSize::from_px(2.);
                    let by0 = offset.bottom() + descent;
                    let by1 = offset.bottom() + ascent;
                    let bz = offset.depth() - RelSize::from_percent(0.1);

                    WidgetVertex::push_quad(
                        [bx0.into(), by0.into()],
                        [bx1.into(), by1.into()],
                        bz.as_depth(),
                        &Color::White.opacity(0.8),
                        widget_info_index,
                        win,
                        background_pool,
                    );
                }
            }
            if let SpanSelection::Select { range } = &selection_area {
                if i == range.start {
                    sel_range_start = Some(bx0);
                }
                if i == range.end - 1 {
                    sel_range_end = Some(bx1);
                }
                if i == range.end {
                    sel_range_end = Some(bx0);
                }
            }
        }

        if let Some(bx0) = sel_range_start {
            if let Some(bx1) = sel_range_end {
                let by0 = offset.bottom() + descent;
                let by1 = offset.bottom() + ascent;
                let bz = offset.depth() - RelSize::from_percent(0.1);
                WidgetVertex::push_quad(
                    [bx0.into(), by0.into()],
                    [bx1.into(), by1.into()],
                    bz.as_depth(),
                    &Color::Blue,
                    widget_info_index,
                    win,
                    background_pool,
                );
            }
        }

        if let SpanSelection::Cursor { position } = selection_area {
            if position == span.content().len() {
                // Draw cursor, pixel aligned.
                bx0 = bx1;
                Self::align_to_px(phys_w, &mut bx0);
                bx0 -= AbsSize::from_px(1.);
                let bx1 = bx0 + AbsSize::from_px(2.);
                let by0 = offset.bottom() + descent;
                let by1 = offset.bottom() + ascent;
                let bz = offset.depth() - RelSize::from_percent(0.1);

                WidgetVertex::push_quad(
                    [bx0.into(), by0.into()],
                    [bx1.into(), by1.into()],
                    bz.as_depth(),
                    &Color::White,
                    widget_info_index,
                    win,
                    background_pool,
                );
            }
        }

        Ok(())
    }
}

#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct FontId(u32);

impl FontId {
    pub fn from_value(v: Value) -> Result<Self> {
        Ok(Self(v.to_int()? as u32))
    }

    pub fn as_value(&self) -> Value {
        Value::Integer(self.0 as i64)
    }
}

pub const SANS_FONT_ID: FontId = FontId(0);

/// Enables tracking of font information without leaking lifetimes everywhere or taking String
/// allocations everywhere. Generally the top-level widget system will hand this out with
/// any operation that deals with fonts.
#[derive(Clone, Debug, Default)]
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
