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

pub trait FontInterface: Debug + Send + Sync + 'static {
    // global metrics
    fn units_per_em(&self) -> f32;

    // vertical metrics
    fn ascent(&self, scale: f32) -> f32;
    fn descent(&self, scale: f32) -> f32;
    fn line_gap(&self, scale: f32) -> f32;

    // horizontal metrics
    fn advance_width(&self, c: char, scale: f32) -> f32;
    fn left_side_bearing(&self, c: char, scale: f32) -> f32;
    fn pair_kerning(&self, a: char, b: char, scale: f32) -> f32;
    fn exact_bounding_box(&self, c: char, scale: f32) -> ((f32, f32), (f32, f32));
    fn pixel_bounding_box(&self, c: char, scale: f32) -> ((i32, i32), (i32, i32));

    // rendering
    fn render_glyph(&self, c: char, scale: f32) -> GrayImage;
}
