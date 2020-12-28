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
    paint_context::PaintContext,
    widget::{UploadMetrics, Widget},
    widget_info::WidgetInfo,
    widget_vertex::WidgetVertex,
    Label, TextEdit, VerticalBox,
};
use gpu::GPU;
use parking_lot::RwLock;
use std::sync::Arc;

// Items packed from top to bottom.
pub struct Terminal {
    container: Arc<RwLock<VerticalBox>>,
}

impl Terminal {
    pub fn new() -> Arc<RwLock<Self>> {
        let output = TextEdit::new("Nitrogen Terminal\nType `help` for help.")
            .with_font("mono")
            .with_color(Color::Green)
            .wrapped();
        let edit = Label::new("testeteststestesttesttesttesttest")
            .with_font("mono")
            .with_color(Color::White)
            .wrapped();
        let container = VerticalBox::with_children(&[output, edit])
            .with_background_color(Color::Gray.darken(3.).opacity(0.8))
            .wrapped();
        Arc::new(RwLock::new(Self { container }))
    }
}

impl Widget for Terminal {
    fn upload(&self, gpu: &GPU, context: &mut PaintContext) -> UploadMetrics {
        self.container.read().upload(gpu, context)
    }
}
