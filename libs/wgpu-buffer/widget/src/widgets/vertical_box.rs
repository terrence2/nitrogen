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
    paint_context::PaintContext,
    widget::{UploadMetrics, Widget},
    widget_info::WidgetInfo,
    widget_vertex::WidgetVertex,
};
use anyhow::Result;
use gpu::Gpu;
use input::GenericEvent;
use nitrous::Interpreter;
use parking_lot::RwLock;
use std::sync::Arc;

// Items packed from top to bottom.
#[derive(Debug, Default)]
pub struct VerticalBox {
    info: WidgetInfo,
    background_color: Color,
    override_width: Option<f32>,
    override_height: Option<f32>,
    children: Vec<BoxPacking>,
}

impl VerticalBox {
    pub fn with_children(children: &[Arc<RwLock<dyn Widget>>]) -> Self {
        Self {
            children: children
                .iter()
                .enumerate()
                .map(|(i, w)| BoxPacking::new(w.to_owned(), i))
                .collect::<Vec<_>>(),
            background_color: Color::Magenta,
            info: WidgetInfo::default(),
            override_width: None,
            override_height: None,
        }
    }

    pub fn info_mut(&mut self) -> &mut WidgetInfo {
        &mut self.info
    }

    pub fn with_background_color(mut self, color: Color) -> Self {
        self.background_color = color;
        self
    }

    pub fn with_width(mut self, width: f32) -> Self {
        self.override_width = Some(width);
        self
    }

    pub fn with_height(mut self, height: f32) -> Self {
        self.override_height = Some(height);
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
    fn upload(&self, gpu: &Gpu, context: &mut PaintContext) -> Result<UploadMetrics> {
        let widget_info_index = context.push_widget(&self.info);
        let mut widget_info_indexes = vec![widget_info_index];

        let mut width = 0f32;
        let mut height = 0f32;
        context.current_depth += PaintContext::BOX_DEPTH_SIZE;
        for pack in &self.children {
            // Pack at 0,0
            let mut child_metrics = pack.widget().read().upload(gpu, context)?;

            // Offset up to our current height.
            for &widget_info_index in &child_metrics.widget_info_indexes {
                context.widget_info_pool[widget_info_index as usize].position[1] -= height;
            }

            width = width.max(child_metrics.width);
            height += child_metrics.height;
            widget_info_indexes.append(&mut child_metrics.widget_info_indexes);
        }
        context.current_depth -= PaintContext::BOX_DEPTH_SIZE;

        if let Some(override_width) = self.override_width {
            width = override_width;
        }
        if let Some(override_height) = self.override_height {
            height = override_height;
        }

        WidgetVertex::push_quad(
            [0., -height],
            [width, 0.],
            context.current_depth,
            &self.background_color,
            widget_info_index,
            &mut context.background_pool,
        );

        Ok(UploadMetrics {
            widget_info_indexes,
            width,
            height,
        })
    }

    fn handle_event(
        &mut self,
        event: &GenericEvent,
        focus: &str,
        interpreter: Arc<RwLock<Interpreter>>,
    ) -> Result<()> {
        for child in &self.children {
            child
                .widget
                .write()
                .handle_event(event, focus, interpreter.clone())?;
        }
        Ok(())
    }
}
