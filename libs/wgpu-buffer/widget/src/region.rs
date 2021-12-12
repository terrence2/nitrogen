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
use std::{fmt::Debug, ops::Add};
use window::{
    size::{AbsSize, AspectMath, LeftBound, RelSize, ScreenDir, Size},
    PhysicalSize, Window,
};

#[derive(Copy, Clone, Debug)]
pub struct Extent<T> {
    width: T,
    height: T,
}

impl<T: Copy + Clone + LeftBound + AspectMath> Extent<T> {
    pub fn zero() -> Self {
        Extent {
            width: T::zero(),
            height: T::zero(),
        }
    }

    pub fn new(width: T, height: T) -> Self {
        Self { width, height }
    }

    pub fn width(&self) -> T {
        self.width
    }

    pub fn height(&self) -> T {
        self.height
    }

    pub fn axis(&self, dir: ScreenDir) -> T {
        match dir {
            ScreenDir::Horizontal => self.width,
            ScreenDir::Vertical => self.height,
            ScreenDir::Depth => panic!("no depth on extent"),
        }
    }

    pub fn set_width(&mut self, width: T) {
        self.width = width;
    }

    pub fn set_height(&mut self, height: T) {
        self.height = height;
    }

    pub fn width_mut(&mut self) -> &mut T {
        &mut self.width
    }

    pub fn height_mut(&mut self) -> &mut T {
        &mut self.height
    }

    pub fn set_axis(&mut self, dir: ScreenDir, v: T) {
        match dir {
            ScreenDir::Horizontal => self.width = v,
            ScreenDir::Vertical => self.height = v,
            ScreenDir::Depth => panic!("cannot set depth on extent"),
        }
    }

    pub fn with_border(mut self, border: &Border<T>, win: &Window) -> Self {
        self.expand_with_border(border, win);
        self
    }

    pub fn expand_with_border(&mut self, border: &Border<T>, win: &Window) {
        self.width = self.width.add(&border.left, win, ScreenDir::Horizontal);
        self.width = self.width.add(&border.right, win, ScreenDir::Horizontal);
        self.height = self.height.add(&border.top, win, ScreenDir::Vertical);
        self.height = self.height.add(&border.bottom, win, ScreenDir::Vertical);
    }

    pub fn remove_border(&mut self, border: &Border<T>, win: &Window) {
        self.width = self.width.sub(&border.left, win, ScreenDir::Horizontal);
        self.width = self.width.sub(&border.right, win, ScreenDir::Horizontal);
        self.height = self.height.sub(&border.top, win, ScreenDir::Vertical);
        self.height = self.height.sub(&border.bottom, win, ScreenDir::Vertical);
    }
}

impl From<Extent<AbsSize>> for Extent<Size> {
    fn from(abs: Extent<AbsSize>) -> Self {
        Extent::<Size>::new(abs.width().into(), abs.height().into())
    }
}

impl From<PhysicalSize<u32>> for Extent<AbsSize> {
    fn from(sz: PhysicalSize<u32>) -> Self {
        Extent::new(
            AbsSize::from_px(sz.width as f32),
            AbsSize::from_px(sz.height as f32),
        )
    }
}

impl Extent<Size> {
    pub fn as_rel(self, win: &Window) -> Extent<RelSize> {
        Extent::<RelSize>::new(
            self.width.as_rel(win, ScreenDir::Horizontal),
            self.height.as_rel(win, ScreenDir::Vertical),
        )
    }

    pub fn as_abs(self, win: &Window) -> Extent<AbsSize> {
        Extent::<AbsSize>::new(
            self.width.as_abs(win, ScreenDir::Horizontal),
            self.height.as_abs(win, ScreenDir::Vertical),
        )
    }
}

/// Position on screen as offsets from the origin: top left corner.
#[derive(Copy, Clone, Debug)]
pub struct Position<T> {
    left: T,
    bottom: T,
    depth: RelSize,
}

impl<T: Copy + Clone + LeftBound + AspectMath> Position<T> {
    pub fn origin() -> Self {
        Self {
            left: T::zero(),
            bottom: T::zero(),
            depth: RelSize::zero(),
        }
    }

    pub fn new(left: T, top: T) -> Self {
        Self {
            left,
            bottom: top,
            depth: RelSize::zero(),
        }
    }

    pub fn new_with_depth(left: T, bottom: T, depth: RelSize) -> Self {
        Self {
            left,
            bottom,
            depth,
        }
    }

    pub fn left(&self) -> T {
        self.left
    }

    pub fn bottom(&self) -> T {
        self.bottom
    }

    pub fn axis(&self, dir: ScreenDir) -> T {
        match dir {
            ScreenDir::Horizontal => self.left,
            ScreenDir::Vertical => self.bottom,
            ScreenDir::Depth => panic!("no generic depth"),
        }
    }

    pub fn left_mut(&mut self) -> &mut T {
        &mut self.left
    }

    pub fn bottom_mut(&mut self) -> &mut T {
        &mut self.bottom
    }

    pub fn axis_mut(&mut self, dir: ScreenDir) -> &mut T {
        match dir {
            ScreenDir::Horizontal => &mut self.left,
            ScreenDir::Vertical => &mut self.bottom,
            ScreenDir::Depth => panic!("no generic depth"),
        }
    }

    pub fn depth(&self) -> RelSize {
        self.depth
    }

    pub fn with_depth(mut self, depth: RelSize) -> Self {
        self.depth = depth;
        self
    }

    pub fn offset_by_border(&mut self, border: &Border<T>, win: &Window) {
        self.bottom = self.bottom.add(&border.bottom, win, ScreenDir::Vertical);
        self.left = self.left.add(&border.left, win, ScreenDir::Horizontal);
    }

    pub fn with_border(mut self, border: &Border<T>, win: &Window) -> Self {
        self.offset_by_border(border, win);
        self
    }
}

impl Position<Size> {
    pub fn as_rel(&self, win: &Window) -> Position<RelSize> {
        Position::<RelSize>::new_with_depth(
            self.left.as_rel(win, ScreenDir::Horizontal),
            self.bottom.as_rel(win, ScreenDir::Vertical),
            self.depth,
        )
    }

    pub fn as_abs(&self, win: &Window) -> Position<AbsSize> {
        Position::<AbsSize>::new_with_depth(
            self.left.as_abs(win, ScreenDir::Horizontal),
            self.bottom.as_abs(win, ScreenDir::Vertical),
            self.depth,
        )
    }
}

impl From<Position<AbsSize>> for Position<Size> {
    fn from(abs: Position<AbsSize>) -> Self {
        Position::<Size>::new_with_depth(abs.left().into(), abs.bottom().into(), abs.depth())
    }
}

#[derive(Clone, Debug)]
pub struct Border<T> {
    top: T,
    bottom: T,
    left: T,
    right: T,
}

impl<T: Copy + Clone + LeftBound> Border<T> {
    pub fn empty() -> Self {
        Self {
            top: T::zero(),
            bottom: T::zero(),
            left: T::zero(),
            right: T::zero(),
        }
    }

    pub fn new(top: T, bottom: T, left: T, right: T) -> Self {
        Self {
            top,
            bottom,
            left,
            right,
        }
    }

    pub fn new_uniform(size: T) -> Self {
        Self {
            top: size,
            bottom: size,
            left: size,
            right: size,
        }
    }

    #[allow(unused)]
    pub fn left(&self) -> T {
        self.left
    }

    #[allow(unused)]
    pub fn right(&self) -> T {
        self.right
    }

    #[allow(unused)]
    pub fn top(&self) -> T {
        self.top
    }

    #[allow(unused)]
    pub fn bottom(&self) -> T {
        self.bottom
    }
}

impl Border<Size> {
    pub fn as_rel(&self, win: &Window) -> Border<RelSize> {
        Border::<RelSize>::new(
            self.top.as_rel(win, ScreenDir::Vertical),
            self.bottom.as_rel(win, ScreenDir::Vertical),
            self.left.as_rel(win, ScreenDir::Horizontal),
            self.right.as_rel(win, ScreenDir::Horizontal),
        )
    }

    pub fn as_abs(&self, win: &Window) -> Border<AbsSize> {
        Border::<AbsSize>::new(
            self.top.as_abs(win, ScreenDir::Vertical),
            self.bottom.as_abs(win, ScreenDir::Vertical),
            self.left.as_abs(win, ScreenDir::Horizontal),
            self.right.as_abs(win, ScreenDir::Horizontal),
        )
    }
}

#[derive(Clone, Debug)]
pub struct Region<T> {
    position: Position<T>,
    extent: Extent<T>,
}

impl<T: Copy + Clone + AspectMath + LeftBound> Region<T> {
    pub fn empty() -> Self {
        Self {
            position: Position::origin(),
            extent: Extent::zero(),
        }
    }

    pub fn new(position: Position<T>, extent: Extent<T>) -> Self {
        Self { position, extent }
    }

    pub fn position(&self) -> &Position<T> {
        &self.position
    }

    pub fn extent(&self) -> &Extent<T> {
        &self.extent
    }

    pub fn position_mut(&mut self) -> &mut Position<T> {
        &mut self.position
    }

    pub fn extent_mut(&mut self) -> &mut Extent<T> {
        &mut self.extent
    }

    pub fn set_position(&mut self, position: Position<T>) {
        self.position = position;
    }

    pub fn set_extent(&mut self, extent: Extent<T>) {
        self.extent = extent;
    }

    pub fn with_extent(&self, extent: Extent<T>) -> Self {
        Self::new(*self.position(), extent)
    }
}

impl<T> Region<T>
where
    T: Copy + Clone + AspectMath + LeftBound + PartialEq + PartialOrd + Add<Output = T>,
{
    pub fn intersects(&self, p: &Position<T>) -> bool {
        p.left() >= self.position.left()
            && p.left() <= (self.position.left() + self.extent.width())
            && p.bottom() >= self.position.bottom()
            && p.bottom() <= (self.position.bottom() + self.extent.height())
    }
}

impl Region<Size> {
    pub fn as_abs(&self, win: &Window) -> Region<AbsSize> {
        Region::new(self.position.as_abs(win), self.extent.as_abs(win))
    }

    pub fn as_rel(&self, win: &Window) -> Region<RelSize> {
        Region::new(self.position.as_rel(win), self.extent.as_rel(win))
    }
}

impl From<Region<AbsSize>> for Region<Size> {
    fn from(abs: Region<AbsSize>) -> Self {
        Region::new((*abs.position()).into(), (*abs.extent()).into())
    }
}
