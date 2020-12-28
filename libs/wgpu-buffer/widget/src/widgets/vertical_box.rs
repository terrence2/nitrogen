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
use gpu::GPU;
use parking_lot::RwLock;
use std::sync::Arc;

// Items packed from top to bottom.
#[derive(Default)]
pub struct VerticalBox {
    background_color: Color,
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
            background_color: Color::Transparent,
        }
    }

    pub fn with_background_color(mut self, color: Color) -> Self {
        self.background_color = color;
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
    fn upload(&self, gpu: &GPU, context: &mut PaintContext) -> UploadMetrics {
        let info = WidgetInfo::default().with_background_color(self.background_color);
        let widget_info_index = context.push_widget(&info);
        let mut widget_info_indexes = vec![widget_info_index];

        let mut width = 2f32;
        let mut height = 0f32;
        for pack in &self.children {
            let mut child_metrics = pack.widget().read().upload(gpu, context);

            // Offset children by our current box offset.
            for &widget_info_index in &child_metrics.widget_info_indexes {
                context.widget_info_pool[widget_info_index as usize].position[1] -= height;
            }

            width = width.max(child_metrics.width);
            height += child_metrics.height;
            widget_info_indexes.append(&mut child_metrics.widget_info_indexes);
        }

        let v00 = WidgetVertex {
            position: [0., 0., context.current_depth],
            tex_coord: [0., 0.],
            widget_info_index,
        };
        let v01 = WidgetVertex {
            position: [0., -height, context.current_depth],
            tex_coord: [0., 0.],
            widget_info_index,
        };
        let v10 = WidgetVertex {
            position: [width, 0., context.current_depth],
            tex_coord: [0., 0.],
            widget_info_index,
        };
        let v11 = WidgetVertex {
            position: [width, -height, context.current_depth],
            tex_coord: [0., 0.],
            widget_info_index,
        };
        context.background_pool.push(v00);
        context.background_pool.push(v01);
        context.background_pool.push(v10);
        context.background_pool.push(v10);
        context.background_pool.push(v01);
        context.background_pool.push(v11);

        UploadMetrics {
            widget_info_indexes,
            width,
            baseline_height: height,
            height,
        }
    }
}
