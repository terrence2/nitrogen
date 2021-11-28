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

use anyhow::Result;
use nitrous::{Interpreter, Value};
use nitrous_injector::{inject_nitrous_module, method, NitrousModule};
use parking_lot::{Mutex, MutexGuard, RwLock};
use std::sync::Arc;

pub use winit::{
    dpi::{LogicalSize, PhysicalSize},
    window::Window as OsWindow,
};

/// Fullscreen or windowed and how to do that.
#[derive(Copy, Clone, Debug)]
pub enum DisplayMode {
    /// Render: whatever size the window is right now (scaled by render scaling)
    /// Window: don't change what the OS gives us
    /// Monitor: leave alone
    ResizableWindowed,

    /// Render: at the given size (scaled by render scaling)
    /// Window: attempt to set the size as given; on failure, letterbox.
    /// Monitor: leave alone
    Windowed(PhysicalSize<u32>),

    /// Render: at the specified size (scaled by render scaling)
    /// Window: Attempt to make the window cover the full screen, but don't be
    ///         obnoxious about it. Only present configuration options for resolution
    ///         that match the aspect ratio of the monitor.
    /// Monitor: leave alone
    SoftFullscreen(PhysicalSize<u32>),

    /// Render: at the specified size (scaled by render scaling)
    /// Window: Attempt to cover the full screen; be obnoxious about it to be
    ///         successful more often on common platforms. Only show configuration
    ///         options for resolutions that the monitor supports.
    /// Monitor: Resize to the indicated size. If the provided dimensions are not
    ///          supported by the monitor, fall back to SoftFullscreen transparently.
    HardFullscreen(PhysicalSize<u32>),
}

#[derive(Clone, Debug)]
pub struct DisplayConfig {
    mode: DisplayMode,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            //mode: DisplayMode::SoftFullscreen(PhysicalSize::new(1920, 1080)),
            mode: DisplayMode::ResizableWindowed,
        }
    }
}

#[derive(Debug, NitrousModule)]
pub struct Window {
    os_window: OsWindow,
    config: DisplayConfig,
}

#[inject_nitrous_module]
impl Window {
    pub fn new(
        os_window: OsWindow,
        config: DisplayConfig,
        interpreter: &mut Interpreter,
    ) -> Result<Arc<RwLock<Self>>> {
        let win = Arc::new(RwLock::new(Self { os_window, config }));
        interpreter.put_global("window", Value::Module(win.clone()));

        // TODO: the renderer cares about screen size changes, but we don't really.
        // interpreter.interpret_once(
        //     r#"
        //         let bindings := mapper.create_bindings("window");
        //         bindings.bind("windowResized", "window.on_resize(width, height)");
        //         bindings.bind("windowDpiChanged", "window.on_dpi_change(scale)");
        //     "#,
        // )?;

        Ok(win)
    }

    pub fn display_mode(&self) -> &DisplayMode {
        &self.config.mode
    }

    // Grab the raw window for sync reads.
    pub fn os_window(&self) -> &OsWindow {
        &self.os_window
    }

    pub fn aspect_ratio(&self) -> f64 {
        let sz = self.logical_size();
        sz.height.floor() / sz.width.floor()
    }

    pub fn aspect_ratio_f32(&self) -> f32 {
        self.aspect_ratio() as f32
    }

    pub fn scale_factor(&self) -> f64 {
        self.os_window.scale_factor()
    }

    pub fn logical_size(&self) -> LogicalSize<f64> {
        self.os_window
            .inner_size()
            .to_logical(self.os_window.scale_factor())
    }

    pub fn physical_size(&self) -> PhysicalSize<u32> {
        self.os_window.inner_size()
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
        let _win_handle = Window::new(window);
        Ok(())
    }
}
