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
pub mod size;

use parking_lot::{Mutex, MutexGuard};
use std::sync::Arc;

pub use winit::{
    dpi::{LogicalSize, PhysicalSize},
    window::Window,
};

#[derive(Clone, Debug)]
pub struct WindowHandle {
    window: Arc<Mutex<Window>>,
}

impl WindowHandle {
    pub fn new(window: Window) -> Self {
        Self {
            window: Arc::new(Mutex::new(window)),
        }
    }

    // Grab the raw window for sync reads.
    pub fn lock(&self) -> MutexGuard<Window> {
        self.window.lock()
    }

    pub fn aspect_ratio(&self) -> f64 {
        let sz = self.logical_size();
        sz.height.floor() / sz.width.floor()
    }

    pub fn aspect_ratio_f32(&self) -> f32 {
        self.aspect_ratio() as f32
    }

    pub fn scale_factor(&self) -> f64 {
        self.window.lock().scale_factor()
    }

    pub fn logical_size(&self) -> LogicalSize<f64> {
        let win = self.window.lock();
        win.inner_size().to_logical(win.scale_factor())
    }

    // pub fn logical_width(&self) -> f64 {
    //     let win = self.window.lock();
    //     win.inner_size().to_logical(win.scale_factor()).width
    // }

    pub fn physical_size(&self) -> PhysicalSize<u32> {
        self.window.lock().inner_size()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn it_works() -> Result<()> {
        use winit::platform::unix::EventLoopExtUnix;
        let event_loop = winit::event_loop::EventLoop::<()>::new_any_thread();
        let window = winit::window::WindowBuilder::new()
            .with_title("Nitrogen Engine")
            .build(&event_loop)?;
        let _win_handle = WindowHandle::new(window);
        Ok(())
    }
}
