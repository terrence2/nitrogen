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
    region::{Border, Extent, Position, Region},
    widget::{Labeled, Widget},
    widget_vertex::WidgetVertex,
    widgets::label::Label,
    WidgetInfo,
};
use anyhow::Result;
use gpu::{
    size::{AbsSize, AspectMath, ScreenDir, Size},
    Gpu,
};
use input::GenericEvent;
use nitrous::Interpreter;
use parking_lot::RwLock;
use std::{sync::Arc, time::Instant};

#[derive(Debug)]
pub struct Expander {
    header: Arc<RwLock<Label>>,
    child: Arc<RwLock<dyn Widget>>,
    expanded: bool,

    border: Border<Size>,
    border_color: Option<Color>,
    padding: Border<Size>,
    background_color: Option<Color>,

    info: WidgetInfo,
    allocated_region: Region<Size>,
    header_position: Position<AbsSize>,
    header_extent: Extent<AbsSize>,
}

impl Expander {
    pub fn new_with_child<S: AsRef<str> + Into<String>>(
        s: S,
        child: Arc<RwLock<dyn Widget>>,
    ) -> Self {
        Expander {
            header: Label::new(s).wrapped(),
            child,
            expanded: false,

            border: Border::empty(),
            border_color: None,
            padding: Border::empty(),
            background_color: None,

            info: WidgetInfo::default(),
            allocated_region: Region::empty(),
            header_position: Position::origin(),
            header_extent: Extent::zero(),
        }
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

impl Labeled for Expander {
    fn set_text<S: AsRef<str> + Into<String>>(&mut self, content: S) {
        self.header.write().set_text(content);
    }

    fn set_size(&mut self, size: Size) {
        self.header.write().set_size(size);
    }

    fn set_color(&mut self, color: Color) {
        self.header.write().set_color(color);
    }

    fn set_font(&mut self, font_id: FontId) {
        self.header.write().set_font(font_id);
    }
}

impl Widget for Expander {
    fn measure(&mut self, gpu: &Gpu, font_context: &mut FontContext) -> Result<Extent<Size>> {
        // Measure label and add border and padding from the box.
        let mut extent = self.header.write().measure(gpu, font_context)?;
        extent.add_border(&self.border, gpu);
        extent.add_border(&self.padding, gpu);

        // Copy this to what we use for hit testing.
        self.header_extent = extent.as_abs(gpu);

        // If we are expanded, add the full size of the child.
        if self.expanded {
            // TODO: what about internal border / line between?
            let child = self.child.write().measure(gpu, font_context)?;
            extent
                .width_mut()
                .max(&child.width(), gpu, ScreenDir::Horizontal);
            extent
                .height_mut()
                .add(&child.height(), gpu, ScreenDir::Vertical);
        }
        Ok(extent)
    }

    fn layout(
        &mut self,
        region: Region<Size>,
        gpu: &Gpu,
        font_context: &mut FontContext,
    ) -> Result<()> {
        // TODO: This is almost certainly wrong when content is expanded.
        {
            // Put the label inside the box at the proper border and padding offset.
            let mut pos = *region.position();
            pos.add_border(&self.border, gpu);
            pos.add_border(&self.padding, gpu);
            self.header.write().layout(
                Region::new(pos, self.header_extent.into()),
                gpu,
                font_context,
            )?;
        }

        // Layout the content at the bottom of the box.
        // TODO: what about an internal border? What about bottom and left borders?
        let mut pos = *region.position();
        *pos.bottom_mut() = pos
            .bottom()
            .add(&region.extent().height(), gpu, ScreenDir::Vertical);
        if self.expanded {
            self.child
                .write()
                .layout(region.clone(), gpu, font_context)?;
        }

        // note: for full extent of header, rather than just the label's
        let mut pos = *region.position();
        *pos.bottom_mut() = pos.bottom().add(
            &self.header_extent.height().into(),
            gpu,
            ScreenDir::Vertical,
        );
        self.header_position = region.position().as_abs(gpu);

        self.allocated_region = region;

        Ok(())
    }

    fn upload(&self, now: Instant, gpu: &Gpu, context: &mut PaintContext) -> Result<()> {
        let widget_info_index = context.push_widget(&self.info);

        self.header.read().upload(now, gpu, context)?;
        if self.expanded {
            self.child.read().upload(now, gpu, context)?;
        }

        if let Some(border_color) = self.border_color {
            WidgetVertex::push_quad_ext(
                self.allocated_region
                    .position()
                    .with_depth(context.current_depth + PaintContext::BORDER_DEPTH),
                *self.allocated_region.extent(),
                &border_color,
                widget_info_index,
                gpu,
                &mut context.background_pool,
            );
        }
        if let Some(background_color) = self.background_color {
            let mut pos = self
                .allocated_region
                .position()
                .with_depth(context.current_depth + PaintContext::BACKGROUND_DEPTH);
            pos.add_border(&self.border, gpu);
            let mut ext = *self.allocated_region.extent();
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
        _now: Instant,
        _event: &GenericEvent,
        _focus: &str,
        cursor_position: Position<AbsSize>,
        _interpreter: Arc<RwLock<Interpreter>>,
    ) -> Result<()> {
        if cursor_position.left() >= self.header_position.left()
            && cursor_position.left() <= (self.header_position.left() + self.header_extent.width())
            && cursor_position.bottom() >= self.header_position.bottom()
            && cursor_position.bottom()
                <= (self.header_position.bottom() + self.header_extent.height())
        {
            println!("in ame column");
        }

        Ok(())
    }
}
