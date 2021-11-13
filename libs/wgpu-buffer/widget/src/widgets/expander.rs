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
    size::{AbsSize, Size},
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
    allocated_region: Region<AbsSize>,
    header_region: Region<AbsSize>,
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
            header_region: Region::empty(),
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
        let mut extent = self.header.write().measure(gpu, font_context)?.as_abs(gpu);
        extent.expand_with_border(&self.border.as_abs(gpu), gpu);
        extent.expand_with_border(&self.padding.as_abs(gpu), gpu);

        // Copy the full area to what we use for hit testing.
        self.header_region.set_extent(extent);

        // If we are expanded, add the full size of the child.
        if self.expanded {
            // TODO: what about internal border / line between?
            let child = self.child.write().measure(gpu, font_context)?.as_abs(gpu);
            *extent.width_mut() = extent.width().max(&child.width());
            *extent.height_mut() = extent.height() + child.height();
        }
        Ok(extent.into())
    }

    fn layout(
        &mut self,
        now: Instant,
        region: Region<Size>,
        gpu: &Gpu,
        font_context: &mut FontContext,
    ) -> Result<()> {
        let region = region.as_abs(gpu);

        // Put the expanded content at the bottom of the box.
        let mut extent = *region.extent();
        *extent.height_mut() = extent.height() - self.header_region.extent().height();
        if self.expanded {
            self.child
                .write()
                .layout(now, region.with_extent(extent).into(), gpu, font_context)?;
        }

        // Recompute position from top using the header.
        let mut pos = *region.position();
        *pos.bottom_mut() = pos.bottom() + region.extent().height();
        *pos.bottom_mut() = pos.bottom() - self.header_region.extent().height();
        self.header_region.set_position(pos);
        pos.offset_by_border(&self.border.as_abs(gpu), gpu);
        pos.offset_by_border(&self.padding.as_abs(gpu), gpu);
        self.header.write().layout(
            now,
            Region::new(pos.into(), (*self.header_region.extent()).into()),
            gpu,
            font_context,
        )?;

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
                    .with_depth(context.current_depth + PaintContext::BORDER_DEPTH)
                    .into(),
                (*self.allocated_region.extent()).into(),
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
            pos.offset_by_border(&self.border.as_abs(gpu), gpu);
            let mut ext = *self.allocated_region.extent();
            ext.remove_border(&self.border.as_abs(gpu), gpu);
            WidgetVertex::push_quad_ext(
                pos.into(),
                ext.into(),
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
        event: &GenericEvent,
        _focus: &str,
        cursor_position: Position<AbsSize>,
        _interpreter: Interpreter,
    ) -> Result<()> {
        if event.is_primary_mouse_down() && self.header_region.intersects(&cursor_position) {
            self.expanded = !self.expanded;
        }

        Ok(())
    }
}
