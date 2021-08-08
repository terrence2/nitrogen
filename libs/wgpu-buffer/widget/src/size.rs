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
use gpu::Gpu;
use std::fmt::Formatter;
use std::{
    fmt::{Debug, Display},
    ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign},
};

fn map_range((a, b): (f32, f32), (ap, bp): (f32, f32), v: f32) -> f32 {
    let f = (v - a) / (b - a);
    f * (bp - ap) + ap
}

pub trait LeftBound {
    fn zero() -> Self;
}

pub trait AspectMath {
    fn add(&self, other: &Self, gpu: &Gpu, dir: ScreenDir) -> Self;
    fn sub(&self, other: &Self, gpu: &Gpu, dir: ScreenDir) -> Self;
    fn max(&self, other: &Self, gpu: &Gpu, dir: ScreenDir) -> Self;
}

#[derive(Copy, Clone, Debug)]
pub enum ScreenDir {
    Vertical,
    Horizontal,
    Depth,
}

impl ScreenDir {
    pub fn other(&self) -> Self {
        match self {
            Self::Horizontal => Self::Vertical,
            Self::Vertical => Self::Horizontal,
            Self::Depth => panic!("no opposite for depth"),
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum RelSize {
    /// Webgpu: -1,-1 at bottom left corner, 1,1 at top right corner.
    Gpu(f32),

    /// As percentage of screen
    Percent(f32),
}

impl RelSize {
    const GPU_RANGE: (f32, f32) = (-1., 1.);
    const PCT_RANGE: (f32, f32) = (0., 100.);

    pub const fn from_percent(pct: f32) -> Self {
        Self::Percent(pct)
    }

    pub fn as_gpu(self) -> f32 {
        match self {
            Self::Gpu(v) => v,
            Self::Percent(pct) => map_range(Self::PCT_RANGE, Self::GPU_RANGE, pct),
        }
    }

    pub fn as_depth(self) -> f32 {
        (self.as_gpu() + 1f32) / 2f32
    }

    pub fn as_percent(self) -> f32 {
        match self {
            Self::Gpu(v) => map_range(Self::GPU_RANGE, Self::PCT_RANGE, v),
            Self::Percent(pct) => pct,
        }
    }

    pub fn as_abs(self, gpu: &Gpu, screen_dir: ScreenDir) -> AbsSize {
        let rng = match screen_dir {
            ScreenDir::Vertical => gpu.aspect_ratio_f32(),
            _ => 1.,
        };
        let f = map_range(Self::PCT_RANGE, (0., rng), self.as_percent());
        AbsSize::Px(f * gpu.logical_size().width as f32)
    }
}

impl LeftBound for RelSize {
    fn zero() -> Self {
        Self::Percent(0.)
    }
}

impl Div<f32> for RelSize {
    type Output = Self;

    fn div(self, rhs: f32) -> Self::Output {
        Self::Percent(self.as_percent() / rhs)
    }
}

impl Mul<f32> for RelSize {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self::Output {
        Self::Percent(self.as_percent() * rhs)
    }
}

impl Sub<RelSize> for RelSize {
    type Output = Self;

    fn sub(self, rhs: RelSize) -> Self::Output {
        Self::Percent(self.as_percent() - rhs.as_percent())
    }
}

impl SubAssign for RelSize {
    fn sub_assign(&mut self, rhs: Self) {
        *self = Self::Percent(self.as_percent() - rhs.as_percent())
    }
}

impl Add<RelSize> for RelSize {
    type Output = Self;

    fn add(self, rhs: RelSize) -> Self::Output {
        Self::Percent(self.as_percent() + rhs.as_percent())
    }
}

impl AddAssign for RelSize {
    fn add_assign(&mut self, rhs: Self) {
        *self = Self::Percent(self.as_percent() + rhs.as_percent())
    }
}

impl AspectMath for RelSize {
    fn add(&self, other: &Self, _gpu: &Gpu, _dir: ScreenDir) -> Self {
        *self + *other
    }

    fn sub(&self, other: &Self, _gpu: &Gpu, _dir: ScreenDir) -> Self {
        *self - *other
    }

    fn max(&self, other: &Self, _gpu: &Gpu, _dir: ScreenDir) -> Self {
        Self::Percent(self.as_percent().max(other.as_percent()))
    }
}

impl Display for RelSize {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gpu(v) => write!(f, "|{}|", v),
            Self::Percent(pct) => write!(f, "{}%", pct),
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum AbsSize {
    /// Font "points", not actually 1/72 of an inch; font dependent.
    Pts(f32),

    /// Pixels of screen real-estate. Not exact, given logical scaling.
    Px(f32),
}

impl AbsSize {
    const TTF_FONT_DPP: f32 = 72.0;
    const SCREEN_DPI: f32 = 96.0;
    const PTS_TO_PX: f32 = Self::SCREEN_DPI / Self::TTF_FONT_DPP;

    pub const fn from_px(px: f32) -> Self {
        Self::Px(px)
    }

    pub fn as_pts(self) -> f32 {
        match self {
            Self::Pts(pts) => pts,
            Self::Px(px) => px / Self::PTS_TO_PX,
        }
    }

    pub fn as_px(self) -> f32 {
        match self {
            Self::Pts(pts) => pts * Self::PTS_TO_PX,
            Self::Px(px) => px,
        }
    }

    /// This function takes pixel size as a percent of *WIDTH*. This may not be what is desired
    /// for all use cases. It will, for example preserve text size _in pixels_ rather than in
    /// screen extent, among other potential flaws.
    pub fn as_rel(self, gpu: &Gpu, screen_dir: ScreenDir) -> RelSize {
        let rng = match screen_dir {
            ScreenDir::Vertical => gpu.aspect_ratio_f32(),
            _ => 1.,
        };
        RelSize::Percent(map_range(
            (0., rng),
            RelSize::PCT_RANGE,
            (self.as_px() as f64 / gpu.logical_size().width) as f32,
        ))
    }

    pub fn max(&self, other: &Self) -> Self {
        match self {
            Self::Px(px) => Self::Px(px.max(other.as_px())),
            Self::Pts(pts) => Self::Pts(pts.max(other.as_pts())),
        }
    }

    pub fn min(&self, other: &Self) -> Self {
        match self {
            Self::Px(px) => Self::Px(px.min(other.as_px())),
            Self::Pts(pts) => Self::Pts(pts.min(other.as_pts())),
        }
    }
}

impl Default for AbsSize {
    fn default() -> Self {
        Self::Px(0.)
    }
}

impl LeftBound for AbsSize {
    fn zero() -> Self {
        Self::Px(0.)
    }
}

impl Div<f32> for AbsSize {
    type Output = Self;

    fn div(self, rhs: f32) -> Self::Output {
        match self {
            Self::Pts(v) => Self::Pts(v / rhs),
            Self::Px(v) => Self::Px(v / rhs),
        }
    }
}

impl Mul<f32> for AbsSize {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self::Output {
        match self {
            Self::Pts(v) => Self::Pts(v * rhs),
            Self::Px(v) => Self::Px(v * rhs),
        }
    }
}

impl Sub<AbsSize> for AbsSize {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        match self {
            Self::Pts(pts) => Self::Pts(pts - rhs.as_pts()),
            Self::Px(px) => Self::Px(px - rhs.as_px()),
        }
    }
}

impl Neg for AbsSize {
    type Output = Self;

    fn neg(self) -> Self::Output {
        match self {
            Self::Pts(pts) => Self::Pts(-pts),
            Self::Px(px) => Self::Px(-px),
        }
    }
}

impl SubAssign for AbsSize {
    fn sub_assign(&mut self, rhs: Self) {
        match self {
            Self::Pts(pts) => *pts -= rhs.as_pts(),
            Self::Px(px) => *px -= rhs.as_px(),
        }
    }
}

impl Add<AbsSize> for AbsSize {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        match self {
            Self::Pts(pts) => Self::Pts(pts + rhs.as_pts()),
            Self::Px(px) => Self::Px(px + rhs.as_px()),
        }
    }
}

impl AddAssign for AbsSize {
    fn add_assign(&mut self, rhs: Self) {
        match self {
            Self::Pts(pts) => *pts += rhs.as_pts(),
            Self::Px(px) => *px += rhs.as_px(),
        }
    }
}

impl AspectMath for AbsSize {
    fn add(&self, other: &Self, _gpu: &Gpu, _dir: ScreenDir) -> Self {
        *self + *other
    }

    fn sub(&self, other: &Self, _gpu: &Gpu, _dir: ScreenDir) -> Self {
        *self - *other
    }

    fn max(&self, other: &Self, _gpu: &Gpu, _dir: ScreenDir) -> Self {
        self.max(other)
    }
}

impl Display for AbsSize {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pts(pts) => write!(f, "{}pts", pts),
            Self::Px(px) => write!(f, "{}px", px),
        }
    }
}

/// Linear extents defined with units and conversions.
/// We frequently need to track sizes in screen relative extents like px and pts, as well
/// as screen relative sizes like percent.
#[derive(Copy, Clone, Debug)]
pub enum Size {
    Rel(RelSize),
    Abs(AbsSize),
}

impl Size {
    pub const fn from_percent(pct: f32) -> Self {
        Self::Rel(RelSize::Percent(pct))
    }

    pub const fn from_pts(pts: f32) -> Self {
        Self::Abs(AbsSize::Pts(pts))
    }

    pub const fn from_px(px: f32) -> Self {
        Self::Abs(AbsSize::Px(px))
    }

    pub fn as_rel(self, gpu: &Gpu, screen_dir: ScreenDir) -> RelSize {
        match self {
            Self::Rel(v) => v,
            Self::Abs(v) => v.as_rel(gpu, screen_dir),
        }
    }

    pub fn as_abs(self, gpu: &Gpu, screen_dir: ScreenDir) -> AbsSize {
        match self {
            Self::Rel(v) => v.as_abs(gpu, screen_dir),
            Self::Abs(v) => v,
        }
    }

    pub fn as_gpu(self, gpu: &Gpu, screen_dir: ScreenDir) -> f32 {
        match self {
            Self::Rel(v) => v.as_gpu(),
            Self::Abs(v) => v.as_rel(gpu, screen_dir).as_gpu(),
        }
    }

    pub fn as_px(self, gpu: &Gpu, screen_dir: ScreenDir) -> f32 {
        match self {
            Self::Rel(v) => v.as_abs(gpu, screen_dir).as_px(),
            Self::Abs(v) => v.as_px(),
        }
    }

    pub fn as_pts(self, gpu: &Gpu, screen_dir: ScreenDir) -> f32 {
        match self {
            Self::Rel(v) => v.as_abs(gpu, screen_dir).as_pts(),
            Self::Abs(v) => v.as_pts(),
        }
    }
}

impl AspectMath for Size {
    fn add(&self, other: &Self, gpu: &Gpu, screen_dir: ScreenDir) -> Self {
        match self {
            Self::Rel(v) => {
                Self::from_percent(v.as_percent() + other.as_rel(gpu, screen_dir).as_percent())
            }
            Self::Abs(v) => Self::from_px(v.as_px() + other.as_abs(gpu, screen_dir).as_px()),
        }
    }

    fn sub(&self, other: &Self, gpu: &Gpu, screen_dir: ScreenDir) -> Self {
        match self {
            Self::Rel(v) => {
                Self::from_percent(v.as_percent() - other.as_rel(gpu, screen_dir).as_percent())
            }
            Self::Abs(v) => Self::from_px(v.as_px() - other.as_abs(gpu, screen_dir).as_px()),
        }
    }

    fn max(&self, other: &Self, gpu: &Gpu, screen_dir: ScreenDir) -> Self {
        match self {
            Self::Rel(v) => Self::from_percent(
                v.as_percent()
                    .max(other.as_rel(gpu, screen_dir).as_percent()),
            ),
            Self::Abs(v) => Self::from_px(v.as_px().max(other.as_abs(gpu, screen_dir).as_px())),
        }
    }
}

impl Display for Size {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rel(rel) => write!(f, "{}", rel),
            Self::Abs(abs) => write!(f, "{}", abs),
        }
    }
}

impl LeftBound for Size {
    fn zero() -> Self {
        Self::Rel(RelSize::zero())
    }
}

impl From<RelSize> for Size {
    fn from(rel: RelSize) -> Self {
        Self::Rel(rel)
    }
}

impl From<AbsSize> for Size {
    fn from(abs: AbsSize) -> Self {
        Self::Abs(abs)
    }
}

impl Div<f32> for Size {
    type Output = Self;

    fn div(self, rhs: f32) -> Self::Output {
        match self {
            Self::Rel(v) => Self::Rel(v / rhs),
            Self::Abs(v) => Self::Abs(v / rhs),
        }
    }
}

impl Mul<f32> for Size {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self::Output {
        match self {
            Self::Rel(v) => Self::Rel(v * rhs),
            Self::Abs(v) => Self::Abs(v * rhs),
        }
    }
}

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

    pub fn with_border(mut self, border: &Border<T>, gpu: &Gpu) -> Self {
        self.add_border(border, gpu);
        self
    }

    pub fn add_border(&mut self, border: &Border<T>, gpu: &Gpu) {
        self.width = self.width.add(&border.left, gpu, ScreenDir::Horizontal);
        self.width = self.width.add(&border.right, gpu, ScreenDir::Horizontal);
        self.height = self.height.add(&border.top, gpu, ScreenDir::Vertical);
        self.height = self.height.add(&border.bottom, gpu, ScreenDir::Vertical);
    }

    pub fn remove_border(&mut self, border: &Border<T>, gpu: &Gpu) {
        self.width = self.width.sub(&border.left, gpu, ScreenDir::Horizontal);
        self.width = self.width.sub(&border.right, gpu, ScreenDir::Horizontal);
        self.height = self.height.sub(&border.top, gpu, ScreenDir::Vertical);
        self.height = self.height.sub(&border.bottom, gpu, ScreenDir::Vertical);
    }
}

impl From<Extent<AbsSize>> for Extent<Size> {
    fn from(abs: Extent<AbsSize>) -> Self {
        Extent::<Size>::new(abs.width().into(), abs.height().into())
    }
}

impl Extent<Size> {
    pub fn as_rel(self, gpu: &Gpu) -> Extent<RelSize> {
        Extent::<RelSize>::new(
            self.width.as_rel(gpu, ScreenDir::Horizontal),
            self.height.as_rel(gpu, ScreenDir::Vertical),
        )
    }

    pub fn as_abs(self, gpu: &Gpu, screen_dir: ScreenDir) -> Extent<AbsSize> {
        Extent::<AbsSize>::new(
            self.width.as_abs(gpu, screen_dir),
            self.height.as_abs(gpu, screen_dir),
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

    pub fn add_border(&mut self, border: &Border<T>, gpu: &Gpu) {
        self.bottom = self.bottom.add(&border.bottom, gpu, ScreenDir::Vertical);
        self.left = self.left.add(&border.left, gpu, ScreenDir::Horizontal);
    }

    pub fn with_border(mut self, border: &Border<T>, gpu: &Gpu) -> Self {
        self.add_border(border, gpu);
        self
    }
}

impl Position<Size> {
    pub fn as_rel(self, gpu: &Gpu) -> Position<RelSize> {
        Position::<RelSize>::new_with_depth(
            self.left.as_rel(gpu, ScreenDir::Horizontal),
            self.bottom.as_rel(gpu, ScreenDir::Vertical),
            self.depth,
        )
    }

    pub fn as_abs(self, gpu: &Gpu) -> Position<AbsSize> {
        Position::<AbsSize>::new_with_depth(
            self.left.as_abs(gpu, ScreenDir::Horizontal),
            self.bottom.as_abs(gpu, ScreenDir::Vertical),
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
