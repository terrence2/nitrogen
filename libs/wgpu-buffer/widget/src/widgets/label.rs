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
    layout::Layout,
    widget_vertex::WidgetVertex,
    widgets::{PaintContext, Widget},
    FontName,
};
use failure::Fallible;
use shader_shared::Group;

pub struct Label {
    content: String,
    size_em: f32,
    font: FontName,
}

impl Label {
    pub fn new<S: Into<String>>(markup: S) -> Self {
        //let mut cache = Vec::new();
        //Layout::span_to_triangles(&markup, glyph_cache, &mut cache);
        Self {
            content: markup.into(),
            size_em: 1.0,
            font: crate::FALLBACK_FONT_NAME.to_owned(),
        }
    }
}

impl Widget for Label {
    fn upload(&self, context: &mut PaintContext) {
        // FIXME: make a layout engine to share.
        for c in self.content.chars() {
            let frame = context
                .font_info
                .get_mut(&self.font)
                .unwrap()
                .load_glyph(c, self.size_em);
        }
    }

    // fn draw<'a>(&self, rpass: wgpu::RenderPass<'a>) -> Fallible<wgpu::RenderPass<'a>> {
    //     let mut rpass = rpass;
    //     for span in &self.spans {
    //         rpass.set_bind_group(
    //             Group::GlyphCache.index(),
    //             &span.glyph_cache().read().bind_group(),
    //             &[],
    //         );
    //     }
    //     Ok(rpass)
    // }
}
