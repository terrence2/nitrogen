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
    paint_context::PaintContext,
    region::{Border, Extent, Position, Region},
    widget_vertex::WidgetVertex,
    WidgetInfo,
};
use anyhow::Result;
use bevy_ecs::prelude::*;
use csscolorparser::Color;
use gpu::Gpu;
use nitrous::{inject_nitrous_component, method, HeapMut, NitrousComponent};
use parking_lot::Mutex;
use std::{str::FromStr, sync::Arc, time::Instant};
use window::{
    size::{LeftBound, RelSize, ScreenDir, Size},
    Window,
};

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
    Layout(Arc<Mutex<LayoutNode>>),
}

impl LayoutLink {
    pub fn widget(&self) -> Entity {
        match self {
            LayoutLink::Leaf(widget) => *widget,
            LayoutLink::Layout(layout) => layout.lock().widget,
        }
    }
}

// Determine how the given widget should be packed into its box.
// Owned by the parent layout, not the child.
#[derive(Component, NitrousComponent, Clone)]
#[Name = "packing"]
pub struct LayoutPacking {
    // non-client area
    padding: Border<RelSize>,
    margin: Border<RelSize>,
    border: Border<RelSize>,

    display: bool,
    background_color: Option<Color>,
    border_color: Option<Color>,

    expand: Expand,
    float_h: PositionH,
    float_v: PositionV,
}

impl Default for LayoutPacking {
    fn default() -> Self {
        Self {
            padding: Border::empty(),
            margin: Border::empty(),
            border: Border::empty(),
            display: true,
            background_color: None,
            border_color: None,
            expand: Expand::Shrink,
            float_h: PositionH::Start,
            float_v: PositionV::Top,
        }
    }
}

#[inject_nitrous_component]
impl LayoutPacking {
    #[method]
    pub fn float_start(&mut self) -> &mut Self {
        self.float_h = PositionH::Start;
        self
    }

    #[method]
    pub fn float_end(&mut self) -> &mut Self {
        self.float_h = PositionH::End;
        self
    }

    #[method]
    pub fn float_middle(&mut self) -> &mut Self {
        self.float_h = PositionH::Center;
        self
    }

    #[method]
    pub fn float_top(&mut self) -> &mut Self {
        self.float_v = PositionV::Top;
        self
    }

    #[method]
    pub fn float_bottom(&mut self) -> &mut Self {
        self.float_v = PositionV::Bottom;
        self
    }

    #[method]
    pub fn float_center(&mut self) -> &mut Self {
        self.float_v = PositionV::Center;
        self
    }

    #[method]
    pub fn set_display(&mut self, display: bool) -> &mut Self {
        self.display = display;
        self
    }

    #[method]
    pub fn set_background(&mut self, color: &str) -> Result<&mut Self> {
        self.background_color = Some(color.parse()?);
        Ok(self)
    }

    #[method]
    pub fn set_border_color(&mut self, color: &str) -> Result<&mut Self> {
        self.border_color = Some(color.parse()?);
        Ok(self)
    }

    pub fn padding_mut(&mut self) -> &mut Border<RelSize> {
        &mut self.padding
    }

    #[method]
    pub fn set_padding(&mut self, s: &str, heap: HeapMut) -> Result<&mut Self> {
        let win = heap.resource::<Window>();
        let sz = Size::from_str(s)?;
        self.padding.set_left(sz.as_rel(win, ScreenDir::Horizontal));
        self.padding
            .set_right(sz.as_rel(win, ScreenDir::Horizontal));
        self.padding.set_top(sz.as_rel(win, ScreenDir::Vertical));
        self.padding.set_bottom(sz.as_rel(win, ScreenDir::Vertical));
        Ok(self)
    }

    #[method]
    pub fn set_padding_left(&mut self, s: &str, heap: HeapMut) -> Result<&mut Self> {
        let win = heap.resource::<Window>();
        let sz = Size::from_str(s)?;
        self.padding.set_left(sz.as_rel(win, ScreenDir::Horizontal));
        Ok(self)
    }

    #[method]
    pub fn set_padding_right(&mut self, s: &str, heap: HeapMut) -> Result<&mut Self> {
        let win = heap.resource::<Window>();
        let sz = Size::from_str(s)?;
        self.padding
            .set_right(sz.as_rel(win, ScreenDir::Horizontal));
        Ok(self)
    }

    #[method]
    pub fn set_padding_top(&mut self, s: &str, heap: HeapMut) -> Result<&mut Self> {
        let win = heap.resource::<Window>();
        let sz = Size::from_str(s)?;
        self.padding.set_top(sz.as_rel(win, ScreenDir::Vertical));
        Ok(self)
    }

    #[method]
    pub fn set_padding_bottom(&mut self, s: &str, heap: HeapMut) -> Result<&mut Self> {
        let win = heap.resource::<Window>();
        let sz = Size::from_str(s)?;
        self.padding.set_bottom(sz.as_rel(win, ScreenDir::Vertical));
        Ok(self)
    }

    #[method]
    pub fn set_margin(&mut self, s: &str, heap: HeapMut) -> Result<&mut Self> {
        let win = heap.resource::<Window>();
        let sz = Size::from_str(s)?;
        self.margin.set_left(sz.as_rel(win, ScreenDir::Horizontal));
        self.margin.set_right(sz.as_rel(win, ScreenDir::Horizontal));
        self.margin.set_top(sz.as_rel(win, ScreenDir::Vertical));
        self.margin.set_bottom(sz.as_rel(win, ScreenDir::Vertical));
        Ok(self)
    }

    #[method]
    pub fn set_margin_left(&mut self, s: &str, heap: HeapMut) -> Result<&mut Self> {
        let win = heap.resource::<Window>();
        let sz = Size::from_str(s)?;
        self.margin.set_left(sz.as_rel(win, ScreenDir::Horizontal));
        Ok(self)
    }

    #[method]
    pub fn set_margin_right(&mut self, s: &str, heap: HeapMut) -> Result<&mut Self> {
        let win = heap.resource::<Window>();
        let sz = Size::from_str(s)?;
        self.margin.set_right(sz.as_rel(win, ScreenDir::Horizontal));
        Ok(self)
    }

    #[method]
    pub fn set_margin_top(&mut self, s: &str, heap: HeapMut) -> Result<&mut Self> {
        let win = heap.resource::<Window>();
        let sz = Size::from_str(s)?;
        self.margin.set_top(sz.as_rel(win, ScreenDir::Vertical));
        Ok(self)
    }

    #[method]
    pub fn set_margin_bottom(&mut self, s: &str, heap: HeapMut) -> Result<&mut Self> {
        let win = heap.resource::<Window>();
        let sz = Size::from_str(s)?;
        self.margin.set_bottom(sz.as_rel(win, ScreenDir::Vertical));
        Ok(self)
    }

    #[method]
    pub fn set_border_left(&mut self, s: &str, heap: HeapMut) -> Result<&mut Self> {
        let win = heap.resource::<Window>();
        let sz = Size::from_str(s)?;
        self.border.set_left(sz.as_rel(win, ScreenDir::Horizontal));
        Ok(self)
    }

    #[method]
    pub fn set_border_right(&mut self, s: &str, heap: HeapMut) -> Result<&mut Self> {
        let win = heap.resource::<Window>();
        let sz = Size::from_str(s)?;
        self.border.set_right(sz.as_rel(win, ScreenDir::Horizontal));
        Ok(self)
    }

    #[method]
    pub fn set_border_top(&mut self, s: &str, heap: HeapMut) -> Result<&mut Self> {
        let win = heap.resource::<Window>();
        let sz = Size::from_str(s)?;
        self.border.set_top(sz.as_rel(win, ScreenDir::Vertical));
        Ok(self)
    }

    #[method]
    pub fn set_border_bottom(&mut self, s: &str, heap: HeapMut) -> Result<&mut Self> {
        let win = heap.resource::<Window>();
        let sz = Size::from_str(s)?;
        self.border.set_bottom(sz.as_rel(win, ScreenDir::Vertical));
        Ok(self)
    }
}

/// Post layout screen-space decisions about where to place widgets.
///
/// Widget implementations must set `child_extent` _before_ WidgetBufferStep::LayoutWidgets.
/// Widget implementations should draw themselves clipped to `child_allocation` _before_
/// WidgetRenderStep::EnsureUploaded. The non-child area is handled by the layout system.
#[derive(Component, Clone, Debug)]
pub struct LayoutMeasurements {
    // Measured size of the child area
    child_extent: Extent<RelSize>,

    // Required size of the full area with padding, margin, and borders accounted for.
    total_extent: Extent<RelSize>,

    // Actual client region allocated to the child.
    child_allocation: Region<RelSize>,

    // Amount allocated to the widget in total.
    total_allocation: Region<RelSize>,

    // Whether parent container is hidden.
    display: bool,
}

impl Default for LayoutMeasurements {
    fn default() -> Self {
        Self {
            child_extent: Extent::new(RelSize::Percent(0.), RelSize::Percent(0.)),
            total_extent: Extent::new(RelSize::Percent(0.), RelSize::Percent(0.)),
            child_allocation: Region::empty(),
            total_allocation: Region::empty(),
            display: true,
        }
    }
}

impl LayoutMeasurements {
    pub fn set_child_extent(&mut self, child_extent: Extent<RelSize>, packing: &LayoutPacking) {
        self.child_extent = child_extent;

        // Get the total allocation by adding the packing borders.
        self.total_extent = child_extent;
        self.total_extent.expand_with_border_rel(&packing.margin);
        self.total_extent.expand_with_border_rel(&packing.border);
        self.total_extent.expand_with_border_rel(&packing.padding);
    }

    pub fn set_depth(&mut self, depth: f32) {
        self.child_allocation
            .position_mut()
            .set_depth(RelSize::Gpu(depth));
        self.total_allocation
            .position_mut()
            .set_depth(RelSize::Gpu(depth));
    }

    pub fn set_display(&mut self, display: bool) {
        self.display = display;
    }

    pub fn display(&self) -> bool {
        self.display
    }

    pub fn child_allocation(&self) -> &Region<RelSize> {
        &self.child_allocation
    }

    pub(crate) fn set_total_allocation(
        &mut self,
        total_allocation: Region<RelSize>,
        packing: &LayoutPacking,
    ) {
        self.total_allocation = total_allocation.clone();

        // Get the child allocation by shaving off the borders.
        self.child_allocation = total_allocation;
        self.child_allocation.remove_border_rel(&packing.margin);
        self.child_allocation.remove_border_rel(&packing.border);
        self.child_allocation.remove_border_rel(&packing.padding);
    }
}

#[derive(Clone, Debug)]
pub struct LayoutNode {
    kind: LayoutKind,
    widget: Entity,
    children: Vec<LayoutLink>,
}

impl LayoutNode {
    pub fn new(kind: LayoutKind, name: &str, mut heap: HeapMut) -> Result<Self> {
        let widget = heap
            .spawn_named(name)?
            .insert_named(LayoutPacking::default())?
            .insert(LayoutMeasurements::default())
            .id();
        Ok(Self {
            kind,
            widget,
            children: Vec::new(),
        })
    }

    pub fn id(&self) -> Entity {
        self.widget
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

    pub fn push_widget(&mut self, widget: Entity) -> Result<()> {
        self.children.push(LayoutLink::Leaf(widget));
        Ok(())
    }

    pub fn push_layout(&mut self, layout: LayoutNode) -> Result<()> {
        self.children
            .push(LayoutLink::Layout(Arc::new(Mutex::new(layout))));
        Ok(())
    }

    pub fn pack_axis(&self) -> Option<ScreenDir> {
        match self.kind {
            LayoutKind::HBox => Some(ScreenDir::Horizontal),
            LayoutKind::VBox => Some(ScreenDir::Vertical),
            _ => None,
        }
    }

    /// Recursively measure layouts by using the child measurements. This will set
    /// the LayoutMeasurements on all layouts in the tree.
    ///
    /// Requirement: all widgets (components with LayoutPacking + LayoutMeasurements)
    ///              have already been measured by setting up a measurement system that
    ///              runs before WidgetRenderStep::LayoutWidgets
    pub fn measure_layout(
        &mut self,
        packings: &Query<&LayoutPacking>,
        measures: &mut Query<&mut LayoutMeasurements>,
    ) -> Result<()> {
        let maybe_dir = self.pack_axis();
        let mut own_extent = Extent::new(RelSize::Percent(0.), RelSize::Percent(0.));
        for link in &mut self.children {
            let child_total_extent = match link {
                LayoutLink::Leaf(entity) => measures.get(*entity)?.total_extent,
                LayoutLink::Layout(layout) => {
                    // Recurse to measure the child container before continuing
                    layout.lock().measure_layout(packings, measures)?;
                    measures.get(layout.lock().widget)?.total_extent
                }
            };

            // Accumulate our child into our own min size allocation.
            if let Some(dir) = maybe_dir {
                *own_extent.axis_mut(dir) += child_total_extent.axis(dir);
                *own_extent.axis_mut(dir.other()) = own_extent
                    .axis(dir.other())
                    .max_rel(&child_total_extent.axis(dir.other()));
            }
        }

        // Set our child extent (and compute the total extent for our parent)
        measures
            .get_mut(self.widget)?
            .set_child_extent(own_extent, packings.get(self.widget)?);

        Ok(())
    }

    fn do_box_layout(
        &mut self,
        region: Region<RelSize>,
        depth: f32,
        packings: &Query<&LayoutPacking>,
        measures: &mut Query<&mut LayoutMeasurements>,
    ) -> Result<()> {
        let dir = self.pack_axis().expect("box only");

        // Figure out how much size we need to actually allocate to our widgets.
        let total_shrink_size = measures.get(self.widget)?.child_extent;
        let mut fill_count = 0;
        for link in &self.children {
            match packings.get(link.widget())?.expand {
                Expand::Shrink => {}
                Expand::Fill => fill_count += 1,
            }
        }
        // We know our requested child area, but we may not have gotten it, or we
        // may have gotten allocated more. This tells us how much more to give to each
        // widget that is packed as fill.
        let extra_fill_allocation =
            (region.extent().axis(dir) - total_shrink_size.axis(dir)) / fill_count as f32;

        // Allocation cursor within region.
        let mut pos = *region.position();
        *pos.axis_mut(dir) += region.extent().axis(dir);

        // Box Packing Algorithm
        // For both horizontal and vertical boxes, using axis.
        for link in &mut self.children {
            measures.get_mut(link.widget())?.set_depth(depth);

            let packing = packings.get(link.widget())?;

            // Compute our actual allocation.
            // Shrink gets exactly what was asked for.
            // Fill gets what was asked for, plus one unit of extra fill
            // This only ever expands.
            let mut child_alloc = measures.get(link.widget())?.total_extent.axis(dir);
            child_alloc += match packing.expand {
                Expand::Shrink => RelSize::Percent(0.),
                Expand::Fill => extra_fill_allocation,
            };

            // Region is bottom left corner to top right corner.
            *pos.axis_mut(dir) -= child_alloc;

            // Compute the region for the total
            let mut child_total_extent = measures.get(link.widget())?.total_extent;
            child_total_extent.set_axis(dir, child_alloc);
            let child_total_alloc = Region::new(pos, child_total_extent);
            measures
                .get_mut(link.widget())?
                .set_total_allocation(child_total_alloc, packings.get(link.widget())?);

            // Recurse into any layout children to do layout.
            if let LayoutLink::Layout(layout) = link {
                let child_region = measures.get(layout.lock().widget)?.child_allocation.clone();
                layout
                    .lock()
                    .perform_layout(child_region, depth + 1., packings, measures)?;
            }
        }

        Ok(())
    }

    fn do_float_layout(
        &mut self,
        region: Region<RelSize>,
        depth: f32,
        packings: &Query<&LayoutPacking>,
        measures: &mut Query<&mut LayoutMeasurements>,
    ) -> Result<()> {
        for (i, link) in self.children.iter().enumerate() {
            measures.get_mut(link.widget())?.set_depth(depth);

            let packing = packings.get(link.widget())?;
            let child_total_extent = measures.get(link.widget())?.total_extent;

            let left_offset = region.position().left()
                + match packing.float_h {
                    PositionH::Start => RelSize::from_percent(0.),
                    PositionH::Center => {
                        (region.extent().width() / 2.) - (child_total_extent.width() / 2.)
                    }
                    PositionH::End => region.extent().width() - child_total_extent.width(),
                };
            let top_offset = region.position().bottom()
                + match packing.float_v {
                    PositionV::Top => region.extent().height() - child_total_extent.height(),
                    PositionV::Center => {
                        (region.extent().height() / 2.) - (child_total_extent.height() / 2.)
                    }
                    PositionV::Bottom => RelSize::zero(),
                };

            let d = depth + 1. + 0.1 * i as f32;
            let position = Position::new_with_depth(left_offset, top_offset, RelSize::Gpu(d));
            let child_total_alloc = Region::new(position, child_total_extent);
            measures
                .get_mut(link.widget())?
                .set_total_allocation(child_total_alloc, packings.get(link.widget())?);

            // Recurse into any layout children to do layout.
            if let LayoutLink::Layout(layout) = link {
                let child_region = measures.get(layout.lock().widget)?.child_allocation.clone();
                layout
                    .lock()
                    .perform_layout(child_region, depth + 1., packings, measures)?;
            }
        }

        Ok(())
    }

    /// Decide what children will get drawn where. Set the regions for all children,
    /// based on previous measurements and a the region given to us by the parent.
    ///
    /// Note: the region that is allocated is _only_ for the child area for this
    /// widget, it should not take into account the margin, border, and padding of
    /// this widget (though it will need to account for that in its children).
    ///
    /// Requirement: measure_layout has been called
    pub fn perform_layout(
        &mut self,
        region: Region<RelSize>,
        depth: f32,
        packings: &Query<&LayoutPacking>,
        measures: &mut Query<&mut LayoutMeasurements>,
    ) -> Result<()> {
        match self.kind {
            LayoutKind::VBox | LayoutKind::HBox => {
                self.do_box_layout(region, depth, packings, measures)
            }
            LayoutKind::Float => self.do_float_layout(region, depth, packings, measures),
        }
    }

    // Draw backgrounds and borders, as requested.
    pub fn draw_non_client(
        &self,
        _now: Instant,
        packings: &Query<&LayoutPacking>,
        measures: &Query<&LayoutMeasurements>,
        _win: &Window,
        _gpu: &Gpu,
        context: &mut PaintContext,
    ) -> Result<()> {
        for link in &self.children {
            // We don't care if the child is a layout or a widget, we want to handle
            // the background and border drawing for our children.
            let packing = packings.get(link.widget())?;
            let measure = measures.get(link.widget())?;

            if !packing.display {
                continue;
            }

            let mut info = WidgetInfo::default();
            if let Some(color) = &packing.background_color {
                if color.a < 1. {
                    info.set_glass_background(true);
                }
            }
            let id = context.push_widget(&info);
            if let Some(color) = &packing.border_color {
                let mut rect = measure.total_allocation.clone_with_depth_adjust(-0.02);
                rect.remove_border_rel(&packing.margin);
                WidgetVertex::push_region(rect, color, id, &mut context.background_pool);
            }
            if let Some(color) = &packing.background_color {
                let mut rect = measure.total_allocation.clone_with_depth_adjust(-0.01);
                rect.remove_border_rel(&packing.margin);
                rect.remove_border_rel(&packing.border);
                WidgetVertex::push_region(rect, color, id, &mut context.background_pool);
            }

            // Recurse into child layouts
            if let LayoutLink::Layout(layout) = link {
                layout
                    .lock()
                    .draw_non_client(_now, packings, measures, _win, _gpu, context)?;
            }
        }

        Ok(())
    }
}
