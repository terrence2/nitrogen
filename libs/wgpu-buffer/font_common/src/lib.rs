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
mod font_interface;

pub use crate::font_interface::{FontAdvance, FontInterface};

use atlas::Frame;
use ordered_float::OrderedFloat;
use parking_lot::{Mutex, MutexGuard};
use std::{
    collections::HashMap,
    fmt,
    fmt::{Debug, Formatter},
    sync::Arc,
};

/// Combines a font interface with a glyph cache behind a handy interface.
#[derive(Clone)]
pub struct Font {
    inner: Arc<Mutex<FontInner>>,
}

impl Debug for Font {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.inner.lock().font.fmt(f)
    }
}

impl Font {
    pub fn new<T: FontInterface>(font: T) -> Self {
        Self {
            inner: Arc::new(Mutex::new(FontInner {
                font: Box::new(font),
                cache: HashMap::new(),
            })),
        }
    }

    pub fn interface(&self) -> MutexGuard<FontInner> {
        self.inner.lock()
    }
}

pub struct FontInner {
    font: Box<dyn FontInterface>,
    cache: HashMap<(char, OrderedFloat<f32>), Frame>,
}

impl FontInner {
    pub fn font(&self) -> &dyn FontInterface {
        self.font.as_ref()
    }

    pub fn get_cached_frame(&self, c: char, scale_pts: f32) -> Option<&Frame> {
        self.cache.get(&(c, OrderedFloat(scale_pts)))
    }

    pub fn cache_frame(&mut self, c: char, scale_pts: f32, frame: Frame) {
        self.cache.insert((c, OrderedFloat(scale_pts)), frame);
    }
}
