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
    font_context::{FontContext, FontId},
    paint_context::PaintContext,
    size::{Extent, Position, Size},
    widget::{Labeled, Widget},
    widgets::label::Label,
};
use anyhow::Result;
use gpu::Gpu;
use input::GenericEvent;
use nitrous::Interpreter;
use parking_lot::RwLock;
use std::sync::Arc;

#[derive(Debug)]
pub struct Button {
    label: Arc<RwLock<Label>>,
    action: String,
}

impl Button {
    pub fn new_with_text<S: AsRef<str> + Into<String>>(s: S) -> Self {
        Button {
            label: Label::new(s).wrapped(),
            action: String::new(),
        }
    }

    pub fn with_action<S: Into<String>>(mut self, action: S) -> Self {
        self.action = action.into();
        self
    }

    pub fn wrapped(self) -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(self))
    }
}

impl Labeled for Button {
    fn set_text<S: AsRef<str> + Into<String>>(&mut self, content: S) {
        self.label.write().set_text(content);
    }

    fn set_size(&mut self, size: Size) {
        self.label.write().set_size(size);
    }

    fn set_color(&mut self, color: Color) {
        self.label.write().set_color(color);
    }

    fn set_font(&mut self, font_id: FontId) {
        self.label.write().set_font(font_id);
    }
}

impl Widget for Button {
    fn measure(&mut self, gpu: &Gpu, font_context: &mut FontContext) -> Result<Extent<Size>> {
        self.label.write().measure(gpu, font_context)
    }

    fn layout(
        &mut self,
        gpu: &Gpu,
        position: Position<Size>,
        extent: Extent<Size>,
        font_context: &mut FontContext,
    ) -> Result<()> {
        self.label
            .write()
            .layout(gpu, position, extent, font_context)
    }

    fn upload(&self, gpu: &Gpu, context: &mut PaintContext) -> Result<()> {
        self.label.read().upload(gpu, context)
    }

    fn handle_event(
        &mut self,
        _event: &GenericEvent,
        _focus: &str,
        _interpreter: Arc<RwLock<Interpreter>>,
    ) -> Result<()> {
        Ok(())
    }
}
