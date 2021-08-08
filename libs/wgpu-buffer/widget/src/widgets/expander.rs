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
    size::{AspectMath, Border, Extent, Position, ScreenDir, Size},
    widget::Widget,
    widget_vertex::WidgetVertex,
    widgets::button::Button,
    WidgetInfo,
};
use anyhow::Result;
use gpu::Gpu;
use input::GenericEvent;
use nitrous::Interpreter;
use parking_lot::RwLock;
use std::sync::Arc;

#[derive(Debug)]
pub struct Expander {
    header: Arc<RwLock<Button>>,
    child: Arc<RwLock<dyn Widget>>,
    expanded: bool,

    border: Border<Size>,
    border_color: Option<Color>,
    padding: Border<Size>,
    background_color: Option<Color>,

    info: WidgetInfo,
    allocated_position: Position<Size>,
    allocated_extent: Extent<Size>,
    header_extent: Extent<Size>,
}

impl Expander {
    pub fn new_with_child<S: AsRef<str> + Into<String>>(
        s: S,
        child: Arc<RwLock<dyn Widget>>,
    ) -> Self {
        Expander {
            header: Button::new_with_text(s).wrapped(),
            child,
            expanded: false,

            border: Border::empty(),
            border_color: None,
            padding: Border::empty(),
            background_color: None,

            info: WidgetInfo::default(),
            allocated_position: Position::origin(),
            allocated_extent: Extent::zero(),
            header_extent: Extent::zero(),
        }
    }

    pub fn with_font(self, font_id: FontId) -> Self {
        self.header.write().set_font(font_id);
        self
    }

    pub fn with_foreground_color(self, color: Color) -> Self {
        self.header.write().set_color(color);
        self
    }

    pub fn with_background_color(mut self, color: Color) -> Self {
        self.background_color = Some(color);
        self
    }

    pub fn with_glass_background(mut self) -> Self {
        self.info.set_glass_background(true);
        self
    }

    pub fn with_border(mut self, color: Color, border: Border<Size>) -> Self {
        self.border = border;
        self.border_color = Some(color);
        self
    }

    pub fn with_padding(mut self, padding: Border<Size>) -> Self {
        self.padding = padding;
        self
    }

    pub fn wrapped(self) -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(self))
    }
}

impl Widget for Expander {
    fn measure(&mut self, gpu: &Gpu, font_context: &mut FontContext) -> Result<Extent<Size>> {
        self.header_extent = self.header.write().measure(gpu, font_context)?;
        if self.expanded {
            let child = self.child.write().measure(gpu, font_context)?;
            self.header_extent
                .width_mut()
                .max(&child.width(), gpu, ScreenDir::Horizontal);
            self.header_extent
                .height_mut()
                .add(&child.height(), gpu, ScreenDir::Vertical);
        }
        self.header_extent.add_border(&self.border, gpu);
        self.header_extent.add_border(&self.padding, gpu);
        Ok(self.header_extent)
    }

    fn layout(
        &mut self,
        gpu: &Gpu,
        position: Position<Size>,
        extent: Extent<Size>,
        font_context: &mut FontContext,
    ) -> Result<()> {
        let mut pos = position;
        pos.add_border(&self.border, gpu);
        pos.add_border(&self.padding, gpu);
        self.header.write().layout(gpu, pos, extent, font_context)?;
        *pos.bottom_mut() =
            pos.bottom()
                .add(&self.header_extent.height(), gpu, ScreenDir::Vertical);
        if self.expanded {
            self.child
                .write()
                .layout(gpu, position, extent, font_context)?;
        }

        self.allocated_position = position;
        self.allocated_extent = extent;

        Ok(())
    }

    fn upload(&self, gpu: &Gpu, context: &mut PaintContext) -> Result<()> {
        let widget_info_index = context.push_widget(&self.info);

        self.header.read().upload(gpu, context)?;
        if self.expanded {
            self.child.read().upload(gpu, context)?;
        }

        if let Some(border_color) = self.border_color {
            WidgetVertex::push_quad_ext(
                self.allocated_position
                    .with_depth(context.current_depth + PaintContext::BORDER_DEPTH),
                self.allocated_extent,
                &border_color,
                widget_info_index,
                gpu,
                &mut context.background_pool,
            );
        }
        if let Some(background_color) = self.background_color {
            let mut pos = self
                .allocated_position
                .with_depth(context.current_depth + PaintContext::BACKGROUND_DEPTH);
            pos.add_border(&self.border, gpu);
            let mut ext = self.allocated_extent;
            ext.remove_border(&self.border, gpu);
            WidgetVertex::push_quad_ext(
                pos,
                ext,
                &background_color,
                widget_info_index,
                gpu,
                &mut context.background_pool,
            );
        }

        Ok(())
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
