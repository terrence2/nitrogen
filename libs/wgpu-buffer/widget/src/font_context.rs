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
use crate::SANS_FONT_NAME;
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
}
