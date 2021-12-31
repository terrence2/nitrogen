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

use anyhow::{bail, Result};
use input::SystemEvent;
use log::info;
use nitrous::{Interpreter, Value};
use nitrous_injector::{inject_nitrous_module, method, NitrousModule};
use parking_lot::RwLock;
use std::{fmt::Debug, str::FromStr, string::ToString, sync::Arc};
use structopt::StructOpt;

pub use winit::{
    dpi::{LogicalSize, PhysicalSize},
    window::Window as OsWindow,
};

#[derive(Debug, Default, StructOpt)]
pub struct DisplayOpts {
    /// Set the render width
    #[structopt(short, long)]
    width: Option<u32>,

    /// Set the render height
    #[structopt(short, long)]
    height: Option<u32>,

    /// Scale rendering resolution
    #[structopt(short, long)]
    scale: Option<f64>,

    /// Select how we output
    #[structopt(short, long)]
    display_mode: Option<DisplayMode>,
}

/// Fullscreen or windowed and how to do that.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum DisplayMode {
    /// Render: whatever size the window is right now (scaled by render scaling)
    /// Window: don't change what the OS gives us
    /// Monitor: leave alone
    ResizableWindowed,

    /// Render: at the configured size (scaled by render scaling)
    /// Window: attempt to set the size as given; on failure, letterbox.
    /// Monitor: leave alone
    Windowed,

    /// Render: at the specified size (scaled by render scaling)
    /// Window: Attempt to make the window cover the full screen, but don't be
    ///         obnoxious about it. Only present configuration options for resolution
    ///         that match the aspect ratio of the monitor. If there is a mismatch at
    ///         runtime, letterbox as appropriate.
    /// Monitor: leave alone
    Fullscreen,

    /// Render: at the specified size (scaled by render scaling)
    /// Window: Attempt to cover the full screen; be obnoxious about it to be
    ///         successful more often on common platforms. Only show configuration
    ///         options for resolutions that the monitor supports.
    /// Monitor: Resize to the indicated size. If the provided dimensions are not
    ///          supported by the monitor, fall back to SoftFullscreen transparently.
    ExclusiveFullscreen,
}

impl DisplayMode {
    fn to_string(self) -> &'static str {
        match self {
            Self::ResizableWindowed => "resizable_windowed",
            Self::Windowed => "windowed",
            Self::Fullscreen => "fullscreen",
            Self::ExclusiveFullscreen => "exclusive_fullscreen",
        }
    }
}

impl FromStr for DisplayMode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "resizable_windowed" | "resizable" => Self::ResizableWindowed,
            "windowed" | "window" => Self::Windowed,
            "fullscreen" | "full" => Self::Fullscreen,
            "exclusive_fullscreen" | "exclusive" => Self::ExclusiveFullscreen,
            _ => bail!("unrecognized display mode"),
        })
    }
}

#[derive(Clone, Debug)]
pub struct DisplayConfig {
    // Determines how we reflect our config on the system and vice versa, at a very high level.
    display_mode: DisplayMode,

    // This is the actual, current window size (as best we are able to tell). Requests for scaling
    // are generally provided by the configured render_extent.
    window_size: PhysicalSize<u32>,

    // The requested "render" dimensions. This is scaled by render_scale to produce the actual
    // buffers that we render to, but the base values are important as those are the requested
    // apparent size on the monitor.
    base_render_extent: PhysicalSize<u32>,

    // Decouples the resolution from the window and monitor.
    render_scale: f64,

    // Relevant for font rendering.
    dpi_scale_factor: f64,

    // Relevant for projections.
    render_aspect_ratio: f64,
}

impl DisplayConfig {
    pub fn discover(opt: &DisplayOpts, os_window: &OsWindow) -> Self {
        // FIXME: use a better default display mode
        let display_mode = opt.display_mode.unwrap_or(DisplayMode::ResizableWindowed);

        // if the resiable window mode is selected, use it as extent
        let base_render_extent = if display_mode == DisplayMode::ResizableWindowed {
            os_window.inner_size()
        } else if let Some(width) = opt.width {
            if let Some(height) = opt.height {
                PhysicalSize::new(width, height)
            } else {
                PhysicalSize::new(width, width * 9 / 16)
            }
        } else if let Some(height) = opt.height {
            PhysicalSize::new(height * 16 / 9, height)
        } else if let Some(monitor) = os_window.current_monitor() {
            if display_mode != DisplayMode::Windowed {
                monitor.size()
            } else {
                PhysicalSize::new(1280, 720)
            }
        } else {
            PhysicalSize::new(1280, 720)
        };

        Self {
            display_mode,
            window_size: os_window.inner_size(),
            base_render_extent,
            render_scale: opt.scale.unwrap_or(1.0),
            dpi_scale_factor: os_window.scale_factor(),
            render_aspect_ratio: base_render_extent.height as f64 / base_render_extent.width as f64,
        }
    }

    pub fn for_test() -> Self {
        Self {
            display_mode: DisplayMode::ResizableWindowed,
            window_size: PhysicalSize::new(1920, 1080),
            base_render_extent: PhysicalSize::new(1920, 1080),
            render_scale: 1.0,
            dpi_scale_factor: 1.2,
            render_aspect_ratio: 1080. / 1920.,
        }
    }

    /// The aspect ratio of the render extent as height / width.
    pub fn render_aspect_ratio(&self) -> f64 {
        self.render_aspect_ratio
    }

    pub fn window_size(&self) -> PhysicalSize<u32> {
        self.window_size
    }

    pub fn base_render_extent(&self) -> PhysicalSize<u32> {
        self.base_render_extent
    }

    pub fn render_extent(&self) -> PhysicalSize<u32> {
        if matches!(self.display_mode, DisplayMode::ResizableWindowed) {
            PhysicalSize::new(
                (self.window_size.width as f64 * self.render_scale).floor() as u32,
                (self.window_size.height as f64 * self.render_scale).floor() as u32,
            )
        } else {
            unimplemented!()
        }
    }

    fn on_window_resized(&mut self, new_size: PhysicalSize<u32>) {
        self.window_size = new_size;
        if matches!(self.display_mode, DisplayMode::ResizableWindowed) {
            self.base_render_extent = new_size;
            self.render_aspect_ratio =
                self.base_render_extent.height as f64 / self.base_render_extent.width as f64;
        }
    }
}

pub trait DisplayConfigChangeReceiver: Debug + Send + Sync + 'static {
    fn on_display_config_changed(&mut self, config: &DisplayConfig) -> Result<()>;
}

#[derive(Debug, NitrousModule)]
pub struct Window {
    os_window: OsWindow,
    config: DisplayConfig,
    config_changed: bool,
}

#[inject_nitrous_module]
impl Window {
    pub fn new(
        os_window: OsWindow,
        config: DisplayConfig,
        interpreter: &mut Interpreter,
    ) -> Result<Arc<RwLock<Self>>> {
        let win = Arc::new(RwLock::new(Self {
            os_window,
            config,
            config_changed: false,
        }));
        interpreter.put_global("window", Value::Module(win.clone()));
        Ok(win)
    }

    fn note_display_config_change(&mut self) {
        self.config_changed = true;
    }

    pub fn handle_system_events(&mut self, events: &[SystemEvent]) -> Option<DisplayConfig> {
        for event in events {
            match event {
                SystemEvent::ScaleFactorChanged { scale } => {
                    self.on_scale_factor_changed(*scale);
                }
                SystemEvent::WindowResized { width, height } => {
                    self.on_window_resized(*width, *height);
                }
                SystemEvent::Quit => {}
            }
        }
        if self.config_changed {
            self.config_changed = false;
            Some(self.config.clone())
        } else {
            None
        }
    }

    fn on_scale_factor_changed(&mut self, scale: f64) {
        self.config.dpi_scale_factor = scale;
        self.note_display_config_change();
    }

    fn on_window_resized(&mut self, width: u32, height: u32) /*-> Result<()>*/
    {
        info!(
            "received resize event: {}x{}; cached: {}x{}",
            width,
            height,
            self.os_window.inner_size().width,
            self.os_window.inner_size().height,
        );

        // On X11 (maybe others?), the w/h pair we get in the change event maybe has not
        // made it to / been fully processed by, the window, so try to make sure the window
        // knows what size the window is. :facepalm:
        let new_size = PhysicalSize {
            width: width as u32,
            height: height as u32,
        };
        self.os_window.set_inner_size(new_size);

        // note: the OS doesn't always give us the option to set the exact window size,
        // so use whatever is real, regardless of what happened above. It is possible
        // (AwesomeWM, X11) that the size change event reflects the full usable area
        // and not the ultimate client size, in which case using the new numbers passed
        // in the change event will cause us to resize every frame. :facepalm:
        let new_size = self.os_window.inner_size();
        info!(
            "after resize, size is: {}x{}",
            new_size.width, new_size.height
        );

        self.config.on_window_resized(new_size);
        self.note_display_config_change();
    }

    pub fn os_window(&self) -> &OsWindow {
        &self.os_window
    }

    pub fn config(&self) -> &DisplayConfig {
        &self.config
    }

    pub fn display_mode(&self) -> DisplayMode {
        self.config.display_mode
    }

    pub fn set_display_mode(&mut self, mode: DisplayMode) -> Result<()> {
        self.config.display_mode = mode;
        self.note_display_config_change();
        Ok(())
    }

    #[method]
    pub fn mode(&self) -> String {
        self.config.display_mode.to_string().to_owned()
    }

    #[method]
    pub fn width(&self) -> i64 {
        self.config.window_size.width as i64
    }

    #[method]
    pub fn height(&self) -> i64 {
        self.config.window_size.height as i64
    }

    pub fn render_aspect_ratio(&self) -> f64 {
        self.config.render_aspect_ratio()
    }

    pub fn render_aspect_ratio_f32(&self) -> f32 {
        self.config.render_aspect_ratio() as f32
    }

    pub fn dpi_scale_factor(&self) -> f64 {
        self.config.dpi_scale_factor
    }

    pub fn physical_size(&self) -> PhysicalSize<u32> {
        self.config.window_size
    }

    pub fn render_extent(&self) -> PhysicalSize<u32> {
        self.config.render_extent()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn it_works() -> Result<()> {
        let (os_window, _) = input::InputController::for_test_unix()?;
        let mut interpreter = Interpreter::default();
        let _win_handle = Window::new(os_window, DisplayConfig::for_test(), &mut interpreter)?;
        Ok(())
    }
}
