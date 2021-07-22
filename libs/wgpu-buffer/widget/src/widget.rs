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
use crate::paint_context::PaintContext;
use anyhow::Result;
use gpu::Gpu;
use input::GenericEvent;
use nitrous::Interpreter;
use parking_lot::RwLock;
use std::{fmt::Debug, sync::Arc};

/// Linear extents defined with units and conversions.
#[derive(Copy, Clone, Debug)]
pub enum Size {
    /// In 0.0 to 2.0, to fit into the -1 to 1 webgpu frame.
    Gpu(f32),

    /// As percentage of screen
    Percent(f32),

    /// Font "points", not actually 1/72 of an inch; font dependent.
    Pts(f32),

    /// Pixels of screen real-estate. Not exact.
    Px(f32),
}

impl Size {
    pub fn as_gpu(self, gpu: &Gpu) -> f32 {
        match self {
            Self::Gpu(v) => v,
            Self::Percent(pct) => pct / 100.0 * 2.0,
            Self::Pts(pts) => {
                let px = pts as f64 * 96.0 / 72.0;
                (px / gpu.logical_size().width * 2.) as f32
            }
            Self::Px(px) => (px as f64 / gpu.logical_size().width * 2.) as f32,
        }
    }

    pub fn as_px(self, gpu: &Gpu) -> f32 {
        match self {
            Self::Gpu(v) => v / 2.0 * gpu.logical_size().width as f32,
            Self::Percent(pct) => pct / 100.0 * gpu.logical_size().width as f32,
            Self::Pts(pts) => pts * 96.0 / 72.0,
            Self::Px(px) => px,
        }
    }

    pub fn as_pts(self, gpu: &Gpu) -> f32 {
        match self {
            Self::Gpu(_) => self.as_px(gpu) * 72.0 / 96.0,
            Self::Percent(_) => self.as_px(gpu) * 72.0 / 96.0,
            Self::Pts(pts) => pts,
            Self::Px(px) => px * 72.0 / 96.0,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Padding {
    top: Size,
    bottom: Size,
    left: Size,
    right: Size,
}

impl Padding {
    pub fn new_uniform(size: Size) -> Self {
        Self {
            top: size,
            bottom: size,
            left: size,
            right: size,
        }
    }

    pub fn left(&self) -> Size {
        self.left
    }

    pub fn right(&self) -> Size {
        self.right
    }

    pub fn top(&self) -> Size {
        self.top
    }

    pub fn bottom(&self) -> Size {
        self.bottom
    }

    pub fn left_gpu(&self, gpu: &Gpu) -> f32 {
        self.left.as_gpu(gpu)
    }

    pub fn right_gpu(&self, gpu: &Gpu) -> f32 {
        self.right.as_gpu(gpu)
    }

    pub fn top_gpu(&self, gpu: &Gpu) -> f32 {
        self.top.as_gpu(gpu)
    }

    pub fn bottom_gpu(&self, gpu: &Gpu) -> f32 {
        self.bottom.as_gpu(gpu)
    }
}

#[derive(Clone, Debug, Default)]
pub struct UploadMetrics {
    pub widget_info_indexes: Vec<u32>,
    pub width: f32,
    pub height: f32,
}

impl UploadMetrics {
    pub fn adjust_height(&self, height: Size, gpu: &Gpu, context: &mut PaintContext) {
        // Offset up to our current height.
        for &widget_info_index in &self.widget_info_indexes {
            context.widget_info_pool[widget_info_index as usize].position[1] += height.as_gpu(gpu);
        }
    }
}

pub trait Widget: Debug + Send + Sync + 'static {
    /// Mutate paint context to reflect the presence of this widget.
    fn upload(&self, gpu: &Gpu, context: &mut PaintContext) -> Result<UploadMetrics>;

    /// Low level event handler.
    fn handle_event(
        &mut self,
        event: &GenericEvent,
        focus: &str,
        interpreter: Arc<RwLock<Interpreter>>,
    ) -> Result<()>;
}
