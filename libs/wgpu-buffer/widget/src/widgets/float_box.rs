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
    box_packing::{PositionH, PositionV},
    font_context::FontContext,
    paint_context::PaintContext,
    region::{Extent, Position, Region},
    widget::Widget,
};
use anyhow::{anyhow, Result};
use gpu::{
    size::{AbsSize, LeftBound, RelSize, ScreenDir, Size},
    Gpu,
};
use input::GenericEvent;
use nitrous::Interpreter;
use nitrous::Value;
use nitrous_injector::{inject_nitrous_module, method, NitrousModule};
use parking_lot::RwLock;
use std::{collections::HashMap, sync::Arc, time::Instant};

// Pack boxes at an edge.
#[derive(Debug)]
pub struct FloatPacking {
    name: String,
    widget: Arc<RwLock<dyn Widget>>,
    float_h: PositionH,
    float_v: PositionV,
    offset_x: Size,
    offset_y: Size,

    /// Whether to take all remaining space, or just the asked for amount
    expand: bool,
}

impl FloatPacking {
    pub fn new(name: &str, widget: Arc<RwLock<dyn Widget>>) -> Self {
        Self {
            name: name.to_owned(),
            widget,
            float_h: PositionH::Start,
            float_v: PositionV::Top,
            offset_x: Size::zero(),
            offset_y: Size::zero(),
            expand: true,
        }
    }

    pub fn widget(&self) -> Arc<RwLock<dyn Widget>> {
        self.widget.clone()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn set_float(&mut self, float_h: PositionH, float_v: PositionV) {
        self.float_h = float_h;
        self.float_v = float_v;
    }

    pub fn set_offset(&mut self, offset_x: Size, offset_y: Size) {
        self.offset_x = offset_x;
        self.offset_y = offset_y;
    }

    pub fn set_expand(&mut self, expand: bool) {
        self.expand = expand;
    }
}

// Items packed from top to bottom.
#[derive(Debug, NitrousModule)]
pub struct FloatBox {
    children: HashMap<String, FloatPacking>,

    position: Position<RelSize>,
    extent: Extent<RelSize>,
}

#[inject_nitrous_module]
impl FloatBox {
    pub fn new() -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(Self {
            children: HashMap::new(),
            position: Position::origin(),
            extent: Extent::zero(),
        }))
    }

    pub fn add_child(&mut self, name: &str, child: Arc<RwLock<dyn Widget>>) -> &mut FloatPacking {
        self.children
            .insert(name.to_owned(), FloatPacking::new(name, child));
        self.packing_mut(name).unwrap()
    }

    pub fn packing(&self, name: &str) -> Result<&FloatPacking> {
        self.children
            .get(name)
            .ok_or_else(|| anyhow!("request for unknown widget in float"))
    }

    pub fn packing_mut(&mut self, name: &str) -> Result<&mut FloatPacking> {
        self.children
            .get_mut(name)
            .ok_or_else(|| anyhow!("mut request for unknown widget in float"))
    }

    #[method]
    pub fn child_float(&mut self, name: &str, horizontal: &str, vertical: &str) -> Value {
        if let Some(child) = self.children.get_mut(name) {
            let float_h = match horizontal {
                "start" => PositionH::Start,
                "center" => PositionH::Center,
                "end" => PositionH::End,
                _ => {
                    return "unknown horizontal float, expected one of start, center, or end"
                        .to_owned()
                        .into()
                }
            };
            let float_v = match vertical {
                "top" => PositionV::Top,
                "center" => PositionV::Center,
                "bottom" => PositionV::Bottom,
                _ => {
                    return "unknown vertical float, expected one of top, center, or bottom"
                        .to_owned()
                        .into()
                }
            };
            child.set_float(float_h, float_v);
            Value::True()
        } else {
            format!("no such child: {}", name).into()
        }
    }

    #[method]
    pub fn child_offset(&mut self, name: &str, x: f64, y: f64) -> Value {
        if let Some(child) = self.children.get_mut(name) {
            child.set_offset(Size::from_percent(x as f32), Size::from_percent(y as f32));
            Value::True()
        } else {
            format!("no such child: {}", name).into()
        }
    }
}

impl Widget for FloatBox {
    fn measure(&mut self, _gpu: &Gpu, _font_context: &mut FontContext) -> Result<Extent<Size>> {
        Ok(Extent::zero())
    }

    fn layout(
        &mut self,
        now: Instant,
        region: Region<Size>,
        gpu: &Gpu,
        font_context: &mut FontContext,
    ) -> Result<()> {
        let position = region.position().as_rel(gpu);
        let extent = region.extent().as_rel(gpu);
        for pack in self.children.values() {
            let mut widget = pack.widget.write();
            let child_extent = widget.measure(gpu, font_context)?.as_rel(gpu);

            let left_offset = position.left()
                + match pack.float_h {
                    PositionH::Start => RelSize::from_percent(0.),
                    PositionH::Center => (extent.width() / 2.) - (child_extent.width() / 2.),
                    PositionH::End => extent.width() - child_extent.width(),
                }
                + pack.offset_x.as_rel(gpu, ScreenDir::Horizontal);
            let top_offset = position.bottom()
                + match pack.float_v {
                    PositionV::Top => extent.height() - child_extent.height(),
                    PositionV::Center => (extent.height() / 2.) - (child_extent.height() / 2.),
                    PositionV::Bottom => RelSize::zero(),
                }
                + pack.offset_y.as_rel(gpu, ScreenDir::Vertical);
            let mut remaining_extent = Extent::<Size>::new(
                (extent.width() - left_offset).into(),
                (extent.height() - top_offset).into(),
            );
            if !pack.expand {
                remaining_extent.set_width(Size::from_percent(
                    remaining_extent
                        .width()
                        .as_percent(gpu, ScreenDir::Horizontal)
                        .min(child_extent.width().as_percent()),
                ));
                remaining_extent.set_height(Size::from_percent(
                    remaining_extent
                        .height()
                        .as_percent(gpu, ScreenDir::Vertical)
                        .min(child_extent.height().as_percent()),
                ));
            }
            widget.layout(
                now,
                Region::new(
                    Position::new(left_offset.into(), top_offset.into()),
                    remaining_extent,
                ),
                gpu,
                font_context,
            )?;
        }
        self.position = position;
        self.extent = extent;

        Ok(())
    }

    // Webgpu: (-1, -1) maps to the bottom-left of the screen.
    // Widget: (0, 0) maps to the top-left of the widget.
    fn upload(&self, now: Instant, gpu: &Gpu, context: &mut PaintContext) -> Result<()> {
        // Upload all children
        for pack in self.children.values() {
            let widget = pack.widget.read();
            let _ = widget.upload(now, gpu, context)?;
        }

        Ok(())
    }

    fn handle_event(
        &mut self,
        now: Instant,
        event: &GenericEvent,
        focus: &str,
        cursor_position: Position<AbsSize>,
        interpreter: Interpreter,
    ) -> Result<()> {
        for packing in self.children.values() {
            packing.widget.write().handle_event(
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
