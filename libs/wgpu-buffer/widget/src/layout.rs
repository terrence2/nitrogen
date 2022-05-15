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
    font_context::FontContext,
    region::{Border, Extent, Position, Region},
    widget::{Widget, WidgetComponent},
};
use animate::TimeStep;
use anyhow::Result;
use bevy_ecs::prelude::*;
use nitrous::{inject_nitrous_component, HeapMut, NitrousComponent};
use parking_lot::Mutex;
use std::{cell::RefCell, sync::Arc, time::Instant};
use window::{
    size::{AspectMath, LeftBound, RelSize, ScreenDir, Size},
    Window,
};

////////////////////
use crate::{text_run::TextRun, PaintContext};
use gpu::Gpu;

#[derive(Debug)]
struct Button {
    label: TextRun,
    action: String,
}

impl Button {
    pub fn new(text: &str) -> Self {
        Self {
            label: TextRun::from_text(text),
            action: "".into(),
        }
    }

    pub fn wrapped(self, name: &str, mut heap: HeapMut) -> Result<Entity> {
        Ok(heap
            .spawn_named(name)?
            .insert_named(WidgetComponent::new(self))?
            .id())
    }
}

impl Widget for Button {
    fn measure(&self, win: &Window, font_context: &FontContext) -> Result<Extent<Size>> {
        let measure = self.label.measure(win, font_context)?;
        Ok(measure.extent())
    }

    // fn layout(
    //     &mut self,
    //     now: Instant,
    //     region: Region<Size>,
    //     win: &Window,
    //     font_context: &mut FontContext,
    // ) -> Result<()> {
    //     todo!()
    // }

    fn upload(
        &self,
        now: Instant,
        win: &Window,
        gpu: &Gpu,
        context: &mut PaintContext,
    ) -> Result<()> {
        todo!()
    }
}
/////////////////////////////

#[derive(Copy, Clone, Debug)]
pub enum PositionH {
    Start,
    Center,
    End,
}

#[derive(Copy, Clone, Debug)]
pub enum PositionV {
    Top,
    Center,
    Bottom,
}

#[derive(Copy, Clone, Debug)]
pub enum Expand {
    Fill,
    Shrink,
}

#[derive(Copy, Clone, Debug)]
pub enum LayoutKind {
    VBox,
    HBox,
    Float,
}

#[derive(Clone, Debug)]
pub enum LayoutLink {
    Leaf(Entity),
    // Even though we are only used single-threaded, we have to be Send to store in a Resource
    Node(Arc<Mutex<LayoutNode>>),
}

impl LayoutLink {
    pub fn widget(&self) -> Entity {
        match self {
            LayoutLink::Leaf(widget) => *widget,
            LayoutLink::Node(layout) => layout.lock().widget,
        }
    }
}

// Post layout screen-space decisions about where to place widgets.
#[derive(Clone, Debug)]
pub struct LayoutMeasurements {
    // Measured size of the child area
    child_extent: Extent<RelSize>,

    // Required size of the full area with padding, margin, and borders accounted for.
    total_extent: Extent<RelSize>,

    // Actual client region allocated to the child.
    child_allocation: Region<RelSize>,

    // Amount allocated to the widget in total.
    total_allocation: Region<RelSize>,
}

impl Default for LayoutMeasurements {
    fn default() -> Self {
        Self {
            child_extent: Extent::new(RelSize::Percent(0.), RelSize::Percent(0.)),
            total_extent: Extent::new(RelSize::Percent(0.), RelSize::Percent(0.)),
            child_allocation: Region::empty(),
            total_allocation: Region::empty(),
        }
    }
}

/// Stored in the parent, about the child.
#[derive(Clone, Debug)]
pub struct LayoutChildInfo {
    /// The child pointer
    link: LayoutLink,

    /// Cache of measurement from the last layout operation. This is logically
    /// owned by the child, but is actually the mutablility secret sauce that
    /// lets us do layout like this in Rust.
    measures: LayoutMeasurements,
}

#[derive(Clone, Debug)]
pub struct LayoutNode {
    kind: LayoutKind,
    widget: Entity,
    children: Vec<LayoutChildInfo>,
}

impl LayoutNode {
    pub fn new(kind: LayoutKind, name: &str, mut heap: HeapMut) -> Result<Self> {
        let widget = heap
            .spawn_named(name)?
            .insert_named(LayoutPacking::default())?
            .id();
        Ok(Self {
            kind,
            widget,
            children: Vec::new(),
        })
    }

    pub fn new_float(name: &str, heap: HeapMut) -> Result<Self> {
        Self::new(LayoutKind::Float, name, heap)
    }

    pub fn new_vbox(name: &str, heap: HeapMut) -> Result<Self> {
        Self::new(LayoutKind::VBox, name, heap)
    }

    pub fn new_hbox(name: &str, heap: HeapMut) -> Result<Self> {
        Self::new(LayoutKind::HBox, name, heap)
    }

    pub fn push_widget(&mut self, widget: Entity, mut heap: HeapMut) -> Result<()> {
        heap.named_entity_mut(widget)
            .insert_named(LayoutPacking::default())?;
        let child = LayoutChildInfo {
            link: LayoutLink::Leaf(widget),
            measures: LayoutMeasurements::default(),
        };
        self.children.push(child);
        Ok(())
    }

    pub fn push_layout(&mut self, layout: LayoutNode, mut heap: HeapMut) -> Result<()> {
        let child = LayoutChildInfo {
            link: LayoutLink::Node(Arc::new(Mutex::new(layout))),
            measures: LayoutMeasurements::default(),
        };
        self.children.push(child);
        Ok(())
    }

    pub fn pack_axis(&self) -> Option<ScreenDir> {
        match self.kind {
            LayoutKind::HBox => Some(ScreenDir::Horizontal),
            LayoutKind::VBox => Some(ScreenDir::Vertical),
            _ => None,
        }
    }

    pub fn measure_layout(&mut self, mut heap: HeapMut) -> Result<Extent<RelSize>> {
        let maybe_dir = self.pack_axis();
        let mut extent = Extent::new(RelSize::Percent(0.), RelSize::Percent(0.));
        for info in &mut self.children {
            // Measure the child.
            info.measures.child_extent = match &info.link {
                // TODO: we should be using internal mutablitity for paint context
                LayoutLink::Leaf(widget) => heap
                    .get::<WidgetComponent>(*widget)
                    .inner()
                    .measure(
                        heap.resource::<Window>(),
                        &heap.resource::<PaintContext>().font_context,
                    )?
                    .as_rel(heap.resource::<Window>()),
                LayoutLink::Node(layout) => layout.lock().measure_layout(heap.as_mut())?,
            };

            // Account for packing properties that add size
            let win = heap.resource::<Window>();
            let packing = heap.get::<LayoutPacking>(info.link.widget());
            info.measures.total_extent = info.measures.child_extent;
            info.measures
                .total_extent
                .expand_with_border(&packing.padding, win);
            info.measures
                .total_extent
                .expand_with_border(&packing.margin, win);
            info.measures
                .total_extent
                .expand_with_border(&packing.border, win);

            // Accumulate our child into our own min size allocation.
            if let Some(dir) = maybe_dir {
                *extent.axis_mut(dir) += info.measures.total_extent.axis(dir);
                extent.set_axis(
                    dir.other(),
                    extent.axis(dir.other()).max(
                        &info.measures.total_extent.axis(dir.other()),
                        win,
                        dir.other(),
                    ),
                );
            }
        }
        Ok(extent)
    }

    pub fn perform_layout(&mut self, region: Region<RelSize>, mut heap: HeapMut) -> Result<()> {
        // Figure out how much size we need to actually allocate to our widgets.
        let maybe_dir = self.pack_axis();
        let mut total_shrink_size = RelSize::zero();
        let mut fill_count = 0;
        for info in &self.children {
            let packing = heap.get::<LayoutPacking>(info.link.widget());

            match packing.expand {
                Expand::Shrink => {
                    if let Some(dir) = maybe_dir {
                        total_shrink_size = total_shrink_size.add(
                            &info.measures.child_extent.axis(dir),
                            &heap.resource::<Window>(),
                            dir,
                        )
                    }
                }
                Expand::Fill => fill_count += 1,
            }
        }
        let fill_allocation = if let Some(dir) = maybe_dir {
            region
                .extent()
                .axis(dir)
                .sub(&total_shrink_size, &heap.resource::<Window>(), dir)
                / fill_count as f32
        } else {
            RelSize::Percent(0.)
        };

        let mut tmp_extent = *region.extent();
        let mut pos = *region.position();
        if let Some(dir) = maybe_dir {
            *pos.axis_mut(dir) += region.extent().axis(dir);
        }
        for info in &mut self.children {
            let packing = heap.get::<LayoutPacking>(info.link.widget());

            // Compute our actual allocation.
            if let Some(dir) = maybe_dir {
                let child_alloc = match packing.expand {
                    Expand::Shrink => info.measures.total_extent.axis(dir),
                    Expand::Fill => fill_allocation,
                };
                *pos.axis_mut(dir) -= child_alloc;
                tmp_extent.set_axis(dir, child_alloc);
            } else {
                Default::default()
            };
            let total_allocation = Region::new(pos, tmp_extent);
            let mut child_allocation = total_allocation.clone();
            let win = heap.resource::<Window>();
            // FIXME: need to bump the region offset as well?
            child_allocation
                .extent_mut()
                .remove_border(&packing.border, win);
            child_allocation
                .extent_mut()
                .remove_border(&packing.margin, win);
            child_allocation
                .extent_mut()
                .remove_border(&packing.padding, win);

            match &info.link {
                LayoutLink::Leaf(widget) => {
                    // Write back our actual client allocation for the draw pass.
                    info.measures.child_allocation = child_allocation;
                    info.measures.total_allocation = total_allocation;
                }
                LayoutLink::Node(layout) => layout
                    .lock()
                    .perform_layout(child_allocation, heap.as_mut())?,
            }
        }

        Ok(())
    }

    // Push data to the paint context
    pub fn draw_layout(&self, mut heap: HeapMut) {}
}

// Determine how the given widget should be packed into its box.
// Owned by the parent layout, not the child.
#[derive(Component, NitrousComponent, Clone, Debug)]
#[Name = "packing"]
pub struct LayoutPacking {
    expand: Expand,
    padding: Border<RelSize>,
    margin: Border<RelSize>,
    border: Border<RelSize>,
}

impl Default for LayoutPacking {
    fn default() -> Self {
        Self {
            expand: Expand::Shrink,
            padding: Border::empty(),
            margin: Border::empty(),
            border: Border::empty(),
        }
    }
}

#[inject_nitrous_component]
impl LayoutPacking {
    /*
    pub fn measure(
        children: &mut [LayoutPacking],
        screen_dir: ScreenDir,
        win: &Window,
        font_context: &mut FontContext,
    ) -> Result<Extent<Size>> {
        let off_dir = screen_dir.other();

        // Note: we're getting the native shrunken size, so don't apply box filling in this loop.
        let mut size = Extent::<Size>::zero();
        for packing in children {
            let child_extent = packing.widget_mut().measure(win, font_context)?;
            size.set_axis(
                screen_dir,
                size.axis(screen_dir)
                    .add(&child_extent.axis(screen_dir), win, screen_dir),
            );
            size.set_axis(
                off_dir,
                size.axis(off_dir)
                    .max(&child_extent.axis(off_dir), win, off_dir),
            );
            packing.set_extent(child_extent);
        }
        Ok(size)
    }

    pub fn layout(
        children: &mut [LayoutPacking],
        dir: ScreenDir,
        now: Instant,
        region: Region<Size>,
        win: &Window,
        font_context: &mut FontContext,
    ) -> Result<()> {
        // Figure out how much size we need to actually allocate to our widgets.
        let mut total_shrink_size = Size::zero();
        let mut fill_count = 0;
        for packing in children.iter() {
            match packing.expand() {
                Expand::Shrink => {
                    total_shrink_size = total_shrink_size.add(&packing.extent().axis(dir), win, dir)
                }
                Expand::Fill => fill_count += 1,
            }
        }
        let fill_allocation =
            region.extent().axis(dir).sub(&total_shrink_size, win, dir) / fill_count as f32;

        let mut tmp_extent = *region.extent();
        let mut pos = *region.position();
        *pos.axis_mut(dir) = pos.axis(dir).add(&region.extent().axis(dir), win, dir);
        for packing in children {
            let child_alloc = match packing.expand() {
                Expand::Shrink => packing.extent().axis(dir),
                Expand::Fill => fill_allocation,
            };
            *pos.axis_mut(dir) = pos.axis(dir).sub(&child_alloc, win, dir);
            tmp_extent.set_axis(dir, child_alloc);
            packing
                .widget_mut()
                .layout(now, Region::new(pos, tmp_extent), win, font_context)?;
        }

        Ok(())
    }
     */
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::WidgetBuffer;
    use gpu::Gpu;
    use input::DemoFocus;
    use platform_dirs::AppDirs;
    use runtime::Runtime;
    use std::time::Instant;

    #[test]
    fn it_can_build_a_layout() -> Result<()> {
        let mut runtime = Gpu::for_test_unix()?;
        runtime
            .insert_resource(AppDirs::new(Some("nitrogen"), true).unwrap())
            .insert_resource(TimeStep::new_60fps())
            .load_extension::<WidgetBuffer<DemoFocus>>()?;

        let button_float = runtime
            .spawn_named("test_button_float")?
            .insert_named(WidgetComponent::new(Button::new("Hello, world!")))?
            .id();

        let button_box1 = runtime
            .spawn_named("test_button_box1")?
            .insert_named(WidgetComponent::new(Button::new("Hello, world!")))?
            .id();
        let button_box2 = runtime
            .spawn_named("test_button_box2")?
            .insert_named(WidgetComponent::new(Button::new("Hello, world!")))?
            .id();

        let mut buttons = LayoutNode::new_vbox("buttons", runtime.heap_mut())?;
        buttons.push_widget(button_box1, runtime.heap_mut())?;
        buttons.push_widget(button_box2, runtime.heap_mut())?;

        let mut root = LayoutNode::new_hbox("root", runtime.heap_mut())?;
        root.push_widget(button_float, runtime.heap_mut())?;
        root.push_layout(buttons, runtime.heap_mut())?;

        // Long-running single threaded mutable action, recursive through the layout.
        root.measure_layout(runtime.heap_mut())?;

        // Purely internal to the layout, make mutable packing decisions about our children.
        root.perform_layout(Region::<RelSize>::full(), runtime.heap_mut())?;

        // Draw it all
        // root.draw_layout(runtime.heap_mut())?;

        Ok(())
    }
}
