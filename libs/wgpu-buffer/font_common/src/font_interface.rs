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
use image::GrayImage;
use std::fmt::Debug;
use window::size::AbsSize;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum FontAdvance {
    Mono,
    Sans,
}

// Note: scale is pixels in ascender - descender: e.g. the same as gnome.
pub trait FontInterface: Debug + Send + Sync + 'static {
    // global metrics
    fn units_per_em(&self) -> f32;
    fn advance_style(&self) -> FontAdvance;

    // vertical metrics
    fn ascent(&self, scale: AbsSize) -> AbsSize;
    fn descent(&self, scale: AbsSize) -> AbsSize;
    fn line_gap(&self, scale: AbsSize) -> AbsSize;

    // horizontal metrics
    fn advance_width(&self, c: char, scale: AbsSize) -> AbsSize;
    fn left_side_bearing(&self, c: char, scale: AbsSize) -> AbsSize;
    fn pair_kerning(&self, a: char, b: char, scale: AbsSize) -> AbsSize;
    fn exact_bounding_box(
        &self,
        c: char,
        scale: AbsSize,
    ) -> ((AbsSize, AbsSize), (AbsSize, AbsSize));
    fn pixel_bounding_box(
        &self,
        c: char,
        scale: AbsSize,
    ) -> ((AbsSize, AbsSize), (AbsSize, AbsSize));

    // rendering
    fn render_glyph(&self, c: char, scale: AbsSize) -> GrayImage;
}
