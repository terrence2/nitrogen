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
pub(crate) mod float_box;
pub(crate) mod label;

use crate::{widget_vertex::WidgetVertex, FontName};
use atlas::{AtlasPacker, Frame};
use failure::Fallible;
use font_common::FontInterface;
use image::Rgba;
use std::collections::HashMap;

pub struct GlyphLoader {
    font: Box<dyn FontInterface>,
    glyphs: HashMap<char, Frame>,
}

impl GlyphLoader {
    pub fn new(font: Box<dyn FontInterface>) -> Self {
        Self {
            font,
            glyphs: HashMap::new(),
        }
    }

    pub fn load_glyph(&mut self, c: char, size_em: f32) -> Frame {
        unimplemented!()
    }
}

// Stored on the GPU, one per widget. Widget vertices reference one of these slots so that
// pipelines can get at the data they need.
pub struct WidgetInfo {
    border_color: [f32; 4],
    background_color: [f32; 4],
}

pub struct PaintContext {
    glyph_sheet: AtlasPacker<Rgba<u8>>,
    font_info: HashMap<FontName, GlyphLoader>,

    background_pool: Vec<WidgetVertex>,
    text_pool: Vec<WidgetVertex>,
    image_pool: Vec<WidgetVertex>,
}

impl PaintContext {
    pub fn new() -> Self {
        Self {
            glyph_sheet: AtlasPacker::new(
                512,
                512,
                Rgba([0; 4]),
                wgpu::TextureFormat::Rgba8Unorm,
                wgpu::TextureUsage::SAMPLED,
            ),
            font_info: HashMap::new(),
            background_pool: Vec::new(),
            image_pool: Vec::new(),
            text_pool: Vec::new(),
        }
    }

    pub fn add_font(&mut self, font_name: FontName, font: Box<dyn FontInterface>) {
        assert!(
            !self.font_info.contains_key(&font_name),
            "font already loaded"
        );
        self.font_info.insert(font_name, GlyphLoader::new(font));
    }
}

pub trait Widget {
    fn upload(&self, context: &mut PaintContext);
}
