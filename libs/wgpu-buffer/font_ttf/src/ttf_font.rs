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
use failure::{err_msg, Fallible};
use font_common::FontInterface;
use image::{GrayImage, Luma};
use parking_lot::RwLock;
use rusttype::{Font, Point, Scale};
use std::sync::Arc;

#[derive(Debug)]
pub struct TtfFont {
    font: Font<'static>,
}

impl FontInterface for TtfFont {
    fn units_per_em(&self) -> f32 {
        self.font.units_per_em() as f32
    }

    fn ascent(&self, scale: f32) -> f32 {
        self.font.v_metrics(Scale::uniform(scale)).ascent
    }

    fn descent(&self, scale: f32) -> f32 {
        self.font.v_metrics(Scale::uniform(scale)).descent
    }

    fn line_gap(&self, scale: f32) -> f32 {
        self.font.v_metrics(Scale::uniform(scale)).line_gap
    }

    fn advance_width(&self, c: char, scale: f32) -> f32 {
        self.font
            .glyph(c)
            .scaled(Scale::uniform(scale))
            .h_metrics()
            .advance_width
    }

    fn left_side_bearing(&self, c: char, scale: f32) -> f32 {
        self.font
            .glyph(c)
            .scaled(Scale::uniform(scale))
            .h_metrics()
            .left_side_bearing
    }

    fn pair_kerning(&self, a: char, b: char, scale: f32) -> f32 {
        self.font.pair_kerning(Scale::uniform(scale), a, b)
    }

    fn exact_bounding_box(&self, c: char, scale: f32) -> ((f32, f32), (f32, f32)) {
        if let Some(bb) = self
            .font
            .glyph(c)
            .scaled(Scale::uniform(scale))
            .exact_bounding_box()
        {
            return ((bb.min.x, -bb.max.y), (bb.max.x, -bb.min.y));
        }
        Default::default()
    }

    fn pixel_bounding_box(&self, c: char, scale: f32) -> ((i32, i32), (i32, i32)) {
        if let Some(bb) = self
            .font
            .glyph(c)
            .scaled(Scale::uniform(scale))
            .positioned(Default::default())
            .pixel_bounding_box()
        {
            return ((bb.min.x, -bb.max.y), (bb.max.x, -bb.min.y));
        }
        Default::default()
    }

    fn render_glyph(&self, c: char, scale: f32) -> GrayImage {
        const ORIGIN: Point<f32> = Point { x: 0.0, y: 0.0 };
        let glyph = self
            .font
            .glyph(c)
            .scaled(Scale::uniform(scale))
            .positioned(ORIGIN);
        if let Some(bb) = glyph.pixel_bounding_box() {
            let w = (bb.max.x - bb.min.x) as u32;
            let h = (bb.max.y - bb.min.y) as u32;
            let mut image = GrayImage::from_pixel(w, h, Luma([0]));
            glyph.draw(|x, y, v| image.put_pixel(x, y, Luma([(v * 255.0) as u8])));
            image
        } else {
            GrayImage::from_pixel(1, 1, Luma([0]))
        }
    }
}

impl TtfFont {
    pub fn from_bytes(bytes: &'static [u8]) -> Fallible<Arc<RwLock<dyn FontInterface>>> {
        Ok(Arc::new(RwLock::new(Self {
            font: Font::try_from_bytes(bytes)
                .ok_or_else(|| err_msg("failed to load font from bytes"))?,
        })))
    }
}
