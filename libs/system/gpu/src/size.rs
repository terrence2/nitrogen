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
use crate::Gpu;
use std::cmp::Ordering;
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
            ScreenDir::Horizontal => 1.,
            _ => panic!("can only convert H/V to abs"),
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

    pub fn ceil(&self) -> Self {
        match self {
            Self::Px(px) => Self::Px(px.ceil()),
            Self::Pts(pts) => Self::Pts(pts.ceil()),
        }
    }

    pub fn round(&self) -> Self {
        match self {
            Self::Px(px) => Self::Px(px.round()),
            Self::Pts(pts) => Self::Pts(pts.round()),
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

impl PartialEq for AbsSize {
    fn eq(&self, other: &Self) -> bool {
        self.as_px() == other.as_px()
    }
}

impl PartialOrd for AbsSize {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.as_px() < other.as_px() {
            Some(Ordering::Less)
        } else if self.as_px() > other.as_px() {
            Some(Ordering::Greater)
        } else if (self.as_px() - other.as_px()).abs() < f32::EPSILON {
            Some(Ordering::Equal)
        } else {
            None
        }
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

    pub fn as_percent(self, gpu: &Gpu, screen_dir: ScreenDir) -> f32 {
        match self {
            Self::Rel(v) => v.as_percent(),
            Self::Abs(v) => v.as_rel(gpu, screen_dir).as_percent(),
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
