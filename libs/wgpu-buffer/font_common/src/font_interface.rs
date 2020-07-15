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
use crate::glyph_frame::GlyphFrame;

pub trait FontInterface {
    fn gpu_resources(&self) -> (&wgpu::TextureView, &wgpu::Sampler);
    fn render_height(&self) -> f32;
    fn can_render_char(&self, c: char) -> bool;
    fn frame_for(&self, c: char) -> &GlyphFrame;
    fn pair_kerning(&self, a: char, b: char) -> f32;
}
