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
    box_packing::BoxPacking,
    color::Color,
    font_context::FontContext,
    paint_context::PaintContext,
    region::{Extent, Position},
    widget::Widget,
    widget_info::WidgetInfo,
    widget_vertex::WidgetVertex,
};
use anyhow::Result;
use gpu::{
    size::{AbsSize, ScreenDir, Size},
    Gpu,
};
use input::GenericEvent;
use nitrous::Interpreter;
use parking_lot::RwLock;
use std::{sync::Arc, time::Instant};

// Items packed from top to bottom.
#[derive(Debug)]
pub struct VerticalBox {
    info: WidgetInfo,
    position: Position<Size>,
    extent: Extent<Size>,

    background_color: Option<Color>,
    override_extent: Option<Extent<Size>>,
    children: Vec<BoxPacking>,
}

impl VerticalBox {
    pub fn new_with_children(children: &[Arc<RwLock<dyn Widget>>]) -> Self {
        Self {
            children: children
                .iter()
                .enumerate()
                .map(|(i, w)| BoxPacking::new(w.to_owned(), i))
                .collect::<Vec<_>>(),
            background_color: None,
            info: WidgetInfo::default(),
            position: Position::origin(),
            extent: Extent::zero(),
            override_extent: None,
        }
    }

    pub fn info_mut(&mut self) -> &mut WidgetInfo {
        &mut self.info
    }

    pub fn with_background_color(mut self, color: Color) -> Self {
        self.background_color = Some(color);
        self
    }

    pub fn with_glass_background(mut self) -> Self {
        self.info.set_glass_background(true);
        self
    }

    pub fn with_overridden_extent(mut self, extent: Extent<Size>) -> Self {
        self.override_extent = Some(extent);
        self
    }

    pub fn with_fill(mut self, offset: usize) -> Self {
        self.packing_mut(offset).set_fill();
        self
    }

    pub fn add_child(&mut self, child: Arc<RwLock<dyn Widget>>) -> &mut BoxPacking {
        let offset = self.children.len();
        self.children.push(BoxPacking::new(child, offset));
        self.packing_mut(offset)
    }

    pub fn packing(&self, offset: usize) -> &BoxPacking {
        &self.children[offset]
    }

    pub fn packing_mut(&mut self, offset: usize) -> &mut BoxPacking {
        &mut self.children[offset]
    }

    pub fn wrapped(self) -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(self))
    }
}

impl Widget for VerticalBox {
    fn measure(&mut self, gpu: &Gpu, font_context: &mut FontContext) -> Result<Extent<Size>> {
        // Note: we need to measure children for layout, even if we have a fixed extent.
        let size = BoxPacking::measure(&mut self.children, ScreenDir::Vertical, gpu, font_context)?;
        if let Some(extent) = self.override_extent {
            return Ok(extent);
        }
        Ok(size)
    }

    fn layout(
        &mut self,
        gpu: &Gpu,
        position: Position<Size>,
        extent: Extent<Size>,
        font_context: &mut FontContext,
    ) -> Result<()> {
        BoxPacking::layout(
            &mut self.children,
            ScreenDir::Vertical,
            gpu,
            position,
            extent,
            font_context,
        )?;
        self.position = position;
        self.extent = extent;

        Ok(())
    }

    fn upload(&self, now: Instant, gpu: &Gpu, context: &mut PaintContext) -> Result<()> {
        let widget_info_index = context.push_widget(&self.info);

        context.current_depth += PaintContext::BOX_DEPTH_SIZE;
        for packing in &self.children {
            packing.widget_mut().upload(now, gpu, context)?;
        }
        context.current_depth -= PaintContext::BOX_DEPTH_SIZE;

        if let Some(background_color) = self.background_color {
            WidgetVertex::push_quad_ext(
                self.position.with_depth(context.current_depth),
                self.extent,
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
        now: Instant,
        event: &GenericEvent,
        focus: &str,
        cursor_position: Position<AbsSize>,
        interpreter: Arc<RwLock<Interpreter>>,
    ) -> Result<()> {
        for child in &self.children {
            child.widget_mut().handle_event(
                now,
                event,
                focus,
                cursor_position,
                interpreter.clone(),
            )?;
        }
        Ok(())
    }
}
