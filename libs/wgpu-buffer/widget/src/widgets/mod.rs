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

use crate::widget_vertex::WidgetVertex;
use failure::Fallible;

// Stored on the GPU, one per widget. Widget vertices reference one of these slots so that
// pipelines can get at the data they need.
pub struct WidgetInfo {
    border_color: [f32; 4],
    background_color: [f32; 4],
}

#[derive(Debug, Default)]
pub struct PaintContext {
    background_pool: Vec<WidgetVertex>,
    text_pool: Vec<WidgetVertex>,
    image_pool: Vec<WidgetVertex>,
}

pub trait Widget {
    fn upload(&self, context: &mut PaintContext);
    //fn draw<'a>(&self, rpass: wgpu::RenderPass<'a>) -> Fallible<wgpu::RenderPass<'a>>;
}
