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
use gpu::{Gpu, UploadTracker};
use image::Luma;
use nitrous::Value;
use ordered_float::OrderedFloat;
use parking_lot::RwLock;
use std::{borrow::Borrow, collections::HashMap, env, sync::Arc};
use tokio::runtime::Runtime;
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

#[derive(Debug, Default)]
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
}

impl FontContext {
    pub fn new(gpu: &Gpu) -> Result<Self> {
        Ok(Self {
            glyph_sheet: AtlasPacker::new(
                "glyph_sheet",
                gpu,
                256 * 4,
                256,
                wgpu::TextureFormat::R8Unorm,
                wgpu::FilterMode::Linear,
            )?,
            trackers: HashMap::new(),
            name_manager: Default::default(),
        })
    }

    pub fn make_upload_buffer(
        &mut self,
        gpu: &mut Gpu,
        async_rt: &Runtime,
        tracker: &mut UploadTracker,
    ) -> Result<()> {
        self.glyph_sheet.make_upload_buffer(gpu, async_rt, tracker)
    }

    pub fn maintain_font_atlas(
        &self,
        mut encoder: wgpu::CommandEncoder,
    ) -> Result<wgpu::CommandEncoder> {
        self.glyph_sheet.maintain_gpu_resources(&mut encoder)?;
        Ok(encoder)
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
        path.push("font_context");
        path.push("glyphs.png");
        self.glyph_sheet.dump(path);
        Ok(())
    }

    fn align_to_px(phys_w: f32, x_pos: &mut AbsSize) {
        *x_pos = AbsSize::from_px((x_pos.as_px() * phys_w).floor() / phys_w);
    }

    pub fn measure_text(&mut self, span: &TextSpan, win: &Window) -> Result<TextSpanMetrics> {
        let phys_w = win.width() as f32;
        let scale_px = (span.size() * win.dpi_scale_factor() as f32)
            .as_abs(win, ScreenDir::Horizontal)
            .ceil();

        // Font rendering is based around the baseline. We want it based around the top-left
        // corner instead, so move down by the ascent.
        let font = self.get_font(span.font());
        let descent = font.read().descent(scale_px);
        let ascent = font.read().ascent(scale_px);
        let line_gap = font.read().line_gap(scale_px);
        let advance = font.read().advance_style();

        let mut x_pos = AbsSize::from_px(0.);
        let mut prior = None;
        for c in span.content().chars() {
            let font = self.get_font(span.font());
            let lsb = font.read().left_side_bearing(c, scale_px);
            let adv = font.read().advance_width(c, scale_px);
            let kerning = prior
                .map(|p| font.read().pair_kerning(p, c, scale_px))
                .unwrap_or_else(AbsSize::zero);
            prior = Some(c);

            if advance != FontAdvance::Mono {
                x_pos += kerning;
            }
            Self::align_to_px(phys_w, &mut x_pos);

            x_pos += match advance {
                FontAdvance::Mono => adv,
                FontAdvance::Sans => adv - lsb,
            };
        }

        Ok(TextSpanMetrics {
            width: x_pos,
            height: ascent - descent,
            ascent,
            descent,
            line_gap,
        })
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
    ) -> Result<TextSpanMetrics> {
        let gs_width = self.glyph_sheet_width();
        let gs_height = self.glyph_sheet_height();

        // Use the physical width to re-align all pixel boxes to pixel boundaries.
        let phys_w = win.width() as f32;

        // The font system expects scales in pixels.
        let scale_px = (span.size() * win.dpi_scale_factor() as f32)
            .as_abs(win, ScreenDir::Horizontal)
            .ceil();

        // Font rendering is based around the baseline. We want it based around the top-left
        // corner instead, so move down by the ascent.
        let font = self.get_font(span.font());
        let descent = font.read().descent(scale_px);
        let ascent = font.read().ascent(scale_px);
        let line_gap = font.read().line_gap(scale_px);
        let advance = font.read().advance_style();

        let mut x_pos = offset.left();
        let y_pos = offset.bottom();
        let mut prior = None;
        for (i, c) in span.content().chars().enumerate() {
            let frame = self.load_glyph(span.font(), c, scale_px, gpu)?;
            let font = self.get_font(span.font());
            let ((lo_x, lo_y), (hi_x, hi_y)) = font.read().pixel_bounding_box(c, scale_px);
            let lsb = font.read().left_side_bearing(c, scale_px);
            let adv = font.read().advance_width(c, scale_px);
            let kerning = prior
                .map(|p| font.read().pair_kerning(p, c, scale_px))
                .unwrap_or_else(AbsSize::zero);
            prior = Some(c);

            if advance != FontAdvance::Mono {
                x_pos += kerning;
            }
            Self::align_to_px(phys_w, &mut x_pos);

            let x0 = x_pos + lo_x;
            let x1 = x_pos + hi_x;
            let y0 = y_pos + lo_y;
            let y1 = y_pos + hi_y;

            WidgetVertex::push_textured_quad(
                [x0.into(), y0.into()],
                [x1.into(), y1.into()],
                offset.depth().as_depth(),
                [frame.s0(gs_width), frame.t0(gs_height)],
                [frame.s1(gs_width), frame.t1(gs_height)],
                span.color(),
                widget_info_index,
                win,
                text_pool,
            );

            // Apply cursor or selection
            if let SpanSelection::Cursor { position } = selection_area {
                if i == position {
                    // Draw cursor, pixel aligned.
                    let mut bx0 = offset.left() + x_pos;
                    Self::align_to_px(phys_w, &mut bx0);
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
            if let SpanSelection::Select { range } = &selection_area {
                if range.contains(&i) {
                    let bx0 = offset.left() + x_pos;
                    let bx1 = offset.left() + x_pos + kerning + lo_x + adv;
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

            x_pos += match advance {
                FontAdvance::Mono => adv,
                FontAdvance::Sans => adv - lsb,
            };
        }

        if let SpanSelection::Cursor { position } = selection_area {
            if position == span.content().len() {
                // Draw cursor, pixel aligned.
                let mut bx0 = offset.left() + x_pos;
                Self::align_to_px(phys_w, &mut bx0);
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

        Ok(TextSpanMetrics {
            width: x_pos,
            height: ascent - descent,
            ascent,
            descent,
            line_gap,
        })
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
