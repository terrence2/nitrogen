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
    region::{Border, Extent, Position, Region},
    widget::Widget,
    widget_info::WidgetInfo,
    widget_vertex::WidgetVertex,
};
use anyhow::Result;
use gpu::Gpu;
use input::{InputEvent, InputFocus};
use parking_lot::RwLock;
use runtime::ScriptHerder;
use std::{sync::Arc, time::Instant};
use window::{
    size::{AbsSize, ScreenDir, Size},
    Window,
};

// Items packed from top to bottom.
#[derive(Debug)]
pub struct VerticalBox {
    children: Vec<BoxPacking>,
    background_color: Option<Color>,
    border: Border<Size>,
    border_color: Option<Color>,
    override_extent: Option<Extent<Size>>,
    padding: Border<Size>,

    info: WidgetInfo,
    allocated_region: Region<Size>,
    child_region: Region<Size>,
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
            border: Border::empty(),
            border_color: None,
            override_extent: None,
            padding: Border::empty(),

            info: WidgetInfo::default(),
            allocated_region: Region::empty(),
            child_region: Region::empty(),
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

    pub fn with_border(mut self, color: Color, border: Border<Size>) -> Self {
        self.border = border;
        self.border_color = Some(color);
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

    pub fn with_padding(mut self, padding: Border<Size>) -> Self {
        self.padding = padding;
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
    fn measure(&mut self, win: &Window, font_context: &mut FontContext) -> Result<Extent<Size>> {
        // Note: we need to measure children for layout, even if we have a fixed extent.
        let mut size =
            BoxPacking::measure(&mut self.children, ScreenDir::Vertical, win, font_context)?;
        size.expand_with_border(&self.border, win);
        size.expand_with_border(&self.padding, win);
        if let Some(extent) = self.override_extent {
            return Ok(extent);
        }
        Ok(size)
    }

    fn layout(
        &mut self,
        now: Instant,
        mut region: Region<Size>,
        win: &Window,
        font_context: &mut FontContext,
    ) -> Result<()> {
        self.allocated_region = region.clone();
        region.extent_mut().remove_border(&self.border, win);
        region.extent_mut().remove_border(&self.padding, win);
        region.position_mut().offset_by_border(&self.border, win);
        region.position_mut().offset_by_border(&self.padding, win);
        BoxPacking::layout(
            &mut self.children,
            ScreenDir::Vertical,
            now,
            region.clone(),
            win,
            font_context,
        )?;
        self.child_region = region;
        Ok(())
    }

    fn upload(
        &self,
        now: Instant,
        win: &Window,
        gpu: &Gpu,
        context: &mut PaintContext,
    ) -> Result<()> {
        let widget_info_index = context.push_widget(&self.info);

        context.current_depth += PaintContext::BOX_DEPTH_SIZE;
        for packing in &self.children {
            packing.widget_mut().upload(now, win, gpu, context)?;
        }
        context.current_depth -= PaintContext::BOX_DEPTH_SIZE;

        if let Some(border_color) = self.border_color {
            WidgetVertex::push_quad_ext(
                self.allocated_region
                    .position()
                    .with_depth(context.current_depth + PaintContext::BORDER_DEPTH),
                *self.allocated_region.extent(),
                &border_color,
                widget_info_index,
                win,
                &mut context.background_pool,
            );
        }

        if let Some(background_color) = self.background_color {
            let mut pos = self
                .allocated_region
                .position()
                .with_depth(context.current_depth + PaintContext::BACKGROUND_DEPTH);
            pos.offset_by_border(&self.border, win);
            let mut ext = *self.allocated_region.extent();
            ext.remove_border(&self.border, win);
            WidgetVertex::push_quad_ext(
                pos,
                ext,
                &background_color,
                widget_info_index,
                win,
                &mut context.background_pool,
            );
        }

        Ok(())
    }

    fn handle_event(
        &mut self,
        event: &InputEvent,
        focus: InputFocus,
        cursor_position: Position<AbsSize>,
        herder: &mut ScriptHerder,
    ) -> Result<()> {
        for child in &self.children {
            child
                .widget_mut()
                .handle_event(event, focus, cursor_position, herder)?;
        }
        Ok(())
    }
}
