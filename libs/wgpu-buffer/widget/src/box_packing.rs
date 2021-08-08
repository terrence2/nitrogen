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
    size::{AspectMath, Extent, LeftBound, Position, ScreenDir, Size},
    widget::Widget,
};
use anyhow::Result;
use gpu::Gpu;
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::sync::Arc;

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

// Determine how the given widget should be packed into its box.
#[derive(Debug)]
pub struct BoxPacking {
    widget: Arc<RwLock<dyn Widget>>,
    offset: usize,
    expand: Expand,
    extent: Extent<Size>,
}

impl BoxPacking {
    pub fn new(widget: Arc<RwLock<dyn Widget>>, offset: usize) -> Self {
        Self {
            widget,
            offset,
            expand: Expand::Shrink,
            extent: Extent::zero(),
        }
    }

    pub fn set_fill(&mut self) {
        self.expand = Expand::Fill;
    }

    pub fn set_shrink(&mut self) {
        self.expand = Expand::Shrink;
    }

    pub fn widget(&self) -> RwLockReadGuard<dyn Widget> {
        self.widget.read()
    }

    pub fn widget_mut(&self) -> RwLockWriteGuard<dyn Widget> {
        self.widget.write()
    }

    pub fn set_extent(&mut self, extent: Extent<Size>) {
        self.extent = extent;
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn expand(&self) -> Expand {
        self.expand
    }

    pub fn extent(&self) -> &Extent<Size> {
        &self.extent
    }

    pub fn measure(
        children: &mut [BoxPacking],
        screen_dir: ScreenDir,
        gpu: &Gpu,
        font_context: &mut FontContext,
    ) -> Result<Extent<Size>> {
        let off_dir = screen_dir.other();

        // Note: we're getting the native shrunken size, so don't apply box filling in this loop.
        let mut size = Extent::<Size>::zero();
        for packing in children {
            let child_extent = packing.widget_mut().measure(gpu, font_context)?;
            size.set_axis(
                screen_dir,
                size.axis(screen_dir)
                    .add(&child_extent.axis(screen_dir), gpu, screen_dir),
            );
            size.set_axis(
                off_dir,
                size.axis(off_dir)
                    .max(&child_extent.axis(off_dir), gpu, off_dir),
            );
            packing.set_extent(child_extent);
        }
        Ok(size)
    }

    pub fn layout(
        children: &mut [BoxPacking],
        dir: ScreenDir,
        gpu: &Gpu,
        position: Position<Size>,
        extent: Extent<Size>,
        font_context: &mut FontContext,
    ) -> Result<()> {
        // Figure out how much size we need to actually allocate to our widgets.
        let mut total_shrink_size = Size::zero();
        let mut fill_count = 0;
        for packing in children.iter() {
            match packing.expand() {
                Expand::Shrink => {
                    total_shrink_size = total_shrink_size.add(&packing.extent().axis(dir), gpu, dir)
                }
                Expand::Fill => fill_count += 1,
            }
        }
        let fill_allocation =
            extent.axis(dir).sub(&total_shrink_size, gpu, dir) / fill_count as f32;

        let mut tmp_extent = extent;
        let mut pos = position;
        *pos.axis_mut(dir) = pos.axis(dir).add(&extent.axis(dir), gpu, dir);
        for packing in children {
            let child_alloc = match packing.expand() {
                Expand::Shrink => packing.extent().axis(dir),
                Expand::Fill => fill_allocation,
            };
            *pos.axis_mut(dir) = pos.axis(dir).sub(&child_alloc, gpu, dir);
            tmp_extent.set_axis(dir, child_alloc);
            packing
                .widget_mut()
                .layout(gpu, pos, tmp_extent, font_context)?;
        }

        Ok(())
    }
}
