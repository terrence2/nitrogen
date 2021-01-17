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
    font_context::FontContext,
    paint_context::PaintContext,
    widget::{UploadMetrics, Widget},
    LineEdit, TextEdit, VerticalBox,
};
use failure::Fallible;
use gpu::GPU;
use parking_lot::RwLock;
use std::sync::Arc;
use winit::event::{KeyboardInput, ModifiersState};

// Items packed from top to bottom.
pub struct Terminal {
    edit: Arc<RwLock<LineEdit>>,
    container: Arc<RwLock<VerticalBox>>,
}

impl Terminal {
    pub fn new(font_context: &FontContext) -> Arc<RwLock<Self>> {
        let output = TextEdit::new("")
            .with_default_font(font_context.font_id_for_name("mono"))
            .with_default_color(Color::Green)
            .with_text("Nitrogen Terminal\nType `help` for help.")
            .wrapped();
        let edit = LineEdit::empty()
            .with_default_font(font_context.font_id_for_name("mono"))
            .with_default_color(Color::White)
            .with_default_size_pts(12.0)
            .with_text("this is some test text for us to highlight")
            .wrapped();
        edit.write().line_mut().select_all();
        let container = VerticalBox::with_children(&[output, edit.clone()])
            .with_background_color(Color::Gray.darken(3.).opacity(0.8))
            .with_width(2.0)
            .with_height(1.5)
            .wrapped();
        container.write().info_mut().set_glass_background(true);
        Arc::new(RwLock::new(Self { edit, container }))
    }
}

impl Widget for Terminal {
    fn upload(&self, gpu: &GPU, context: &mut PaintContext) -> UploadMetrics {
        self.container.read().upload(gpu, context)
    }

    fn handle_keyboard(&mut self, events: &[(KeyboardInput, ModifiersState)]) -> Fallible<()> {
        self.edit.write().handle_keyboard(events)
    }
}
