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
    region::Position,
    text_run::{SpanSelection, TextSpan},
    widget_vertex::WidgetVertex,
    Extent, Size, SANS_FONT_NAME,
};
use anyhow::Result;
use atlas::{AtlasPacker, Frame};
use csscolorparser::Color;
use font_common::{Font, FontAdvance};
use gpu::Gpu;
use image::Luma;
use nitrous::Value;
use parking_lot::{Mutex, MutexGuard};
use std::{borrow::Borrow, collections::HashMap, env, hint::unreachable_unchecked, path::PathBuf};
use window::{
    size::{AbsSize, LeftBound, ScreenDir},
    Window,
};

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

impl TextSpanMetrics {
    pub fn extent(&self) -> Extent<Size> {
        Extent::new(self.width.into(), self.height.into())
    }
}

#[derive(Debug)]
pub struct FontContext {
    glyph_sheet: Mutex<AtlasPacker<Luma<u8>>>,
    trackers: HashMap<FontId, Font>,
    name_manager: FontNameManager,
    dump_texture_path: Option<PathBuf>,
}

impl FontContext {
    pub fn new(gpu: &Gpu) -> Self {
        Self {
            glyph_sheet: Mutex::new(AtlasPacker::new(
                "glyph_sheet",
                gpu,
                256 * 4,
                256,
                wgpu::TextureFormat::R8Unorm,
                wgpu::FilterMode::Linear,
            )),
            trackers: HashMap::new(),
            name_manager: Default::default(),
            dump_texture_path: None,
        }
    }

    pub fn handle_dump_texture(&mut self, gpu: &mut Gpu) -> Result<()> {
        if let Some(dump_path) = &self.dump_texture_path {
            self.glyph_sheet.lock().dump_texture(gpu, dump_path)?;
        }
        self.dump_texture_path = None;
        Ok(())
    }

    pub fn maintain_font_atlas(&mut self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder) {
        self.glyph_sheet.lock().encode_frame_uploads(gpu, encoder);
    }

    pub fn glyph_sheet_width(&self) -> u32 {
        self.glyph_sheet.lock().width()
    }

    pub fn glyph_sheet_height(&self) -> u32 {
        self.glyph_sheet.lock().height()
    }

    pub fn glyph_sheet_texture_layout_entry(&self, binding: u32) -> wgpu::BindGroupLayoutEntry {
        self.glyph_sheet.lock().texture_layout_entry(binding)
    }

    pub fn glyph_sheet_sampler_layout_entry(&self, binding: u32) -> wgpu::BindGroupLayoutEntry {
        self.glyph_sheet.lock().sampler_layout_entry(binding)
    }

    pub fn glyph_sheet(&self) -> MutexGuard<AtlasPacker<Luma<u8>>> {
        self.glyph_sheet.lock()
    }

    pub fn get_font_by_name(&self, font_name: &str) -> Font {
        self.get_font(self.font_id_for_name(font_name))
    }

    pub fn get_font(&self, font_id: FontId) -> Font {
        self.trackers[&font_id].clone()
    }

    pub fn add_font<S: Borrow<str> + Into<String>>(&mut self, font_name: S, font: Font) {
        let fid = self.name_manager.allocate(font_name);
        self.trackers.insert(fid, font);
    }

    pub fn load_glyph(&self, fid: FontId, c: char, scale: AbsSize, gpu: &Gpu) -> Result<Frame> {
        // We always have to take at least one mutex to protect the cache.
        let f0 = &self.trackers[&fid];
        let mut f = f0.interface();
        if let Some(frame) = f.get_cached_frame(c, scale.as_pts()) {
            return Ok(*frame);
        }
        let img = f.font().render_glyph(c, scale);
        // On cache miss we have to take a second mutex to allow inner mutability for the glyph
        // sheet, unless we want to move those to the font as well.
        let frame = self.glyph_sheet.lock().push_image(&img, gpu)?;
        f.cache_frame(c, scale.as_pts(), frame);
        Ok(frame)
    }

    pub fn cache_ascii_glyphs(&self, fid: FontId, scale: AbsSize, gpu: &Gpu) -> Result<()> {
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

    pub fn measure_text(&self, span: &TextSpan, win: &Window) -> Result<TextSpanMetrics> {
        if let Some(metrics) = span.metrics() {
            debug_assert_eq!(span.layout_cache_len(), span.content().chars().count() * 6);
            return Ok(metrics);
        }
        debug_assert_eq!(span.layout_cache_len(), 0);

        let phys_w = win.width() as f32;
        // let scale_px = (span.size() * win.dpi_scale_factor() as f32)
        //     .as_abs(win, ScreenDir::Horizontal)
        //     .ceil();
        let scale_px = span.size().as_abs(win, ScreenDir::Horizontal).ceil();

        // Font rendering is based around the baseline. We want it based around the top-left
        // corner instead, so move down by the ascent.
        let font_ref = self.get_font(span.font());
        let font_p = font_ref.interface();
        let font = font_p.font();
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
        span.set_span_cache(cache);
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
        debug_assert_eq!(span.layout_cache_len(), span.content().chars().count() * 6);

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
        let mut bx0 = offset.left();
        let mut bx1 = offset.left();
        let x_base = offset.left();
        let y_pos = offset.bottom();
        let mut z_depth = offset.depth().as_gpu();
        let mut cache_offset = 0;
        let span_cache = span.span_cache();
        for (i, c) in span.content().chars().enumerate() {
            let frame = self.load_glyph(span.font(), c, scale_px, gpu)?;

            let s0 = frame.s0(gs_width);
            let t0 = frame.t0(gs_height);
            let s1 = frame.s1(gs_width);
            let t1 = frame.t1(gs_height);
            for j in 0..6 {
                let mut v = span_cache.vertex(cache_offset + j);

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
                z_depth += 0.0001;

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
                    let bz = offset.depth().as_gpu() - 0.1;
                    WidgetVertex::push_quad(
                        [bx0.into(), by0.into()],
                        [bx1.into(), by1.into()],
                        bz,
                        &Color::from([1., 1., 1., 0.8]),
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
                let bz = offset.depth().as_gpu() - 0.1;
                WidgetVertex::push_quad(
                    [bx0.into(), by0.into()],
                    [bx1.into(), by1.into()],
                    bz,
                    &Color::from([0, 0, 255]),
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
                let bz = offset.depth().as_gpu() - 0.1;
                WidgetVertex::push_quad(
                    [bx0.into(), by0.into()],
                    [bx1.into(), by1.into()],
                    bz,
                    &Color::from([255, 255, 255]),
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
