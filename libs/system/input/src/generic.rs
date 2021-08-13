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
use winit::{
    dpi::LogicalSize,
    event::{ButtonId, ElementState, ModifiersState, VirtualKeyCode},
};

#[derive(Debug, Copy, Clone)]
pub enum MouseAxis {
    X,
    Y,
    ScrollH,
    ScrollV,
    Tilt,
}

// Every platform winit supports provides a slightly different perspective on how events show up
// in this system. Our needs are less than completely general, so we can get away with projecting
// that onto a simpler surface. In particular:
//   * There is a single window that is generally fullscreen.
//   * There are limited interactions with the system outside of mouse/keyboard/joystick/gpu.
// As such, we try to make the following guarantees, papering over platform differences:
//   * Keyboard events may only fire when the window is focused.
//   * Keyboard events contain the current state of modifiers when the event happens.
//   * Keyboard events contain both a scancode and a "virtual" keycode.
//   * The virtual keycode is just a friendly name for the physical key that was interacted.
//     e.g. Events in alternate planes reflect the plane in the ModifiersState, not in the keycode.
//   * Mouse movement and button events fire always, but are marked with the window in-out state.
//   * There is only one "user-caused-window-to-close" signal.
#[derive(Debug, Clone)]
pub enum GenericEvent {
    KeyboardKey {
        scancode: u32,
        virtual_keycode: VirtualKeyCode,
        press_state: ElementState,
        modifiers_state: ModifiersState,
        window_focused: bool,
    },

    MouseButton {
        button: ButtonId,
        press_state: ElementState,
        modifiers_state: ModifiersState,
        in_window: bool,
        window_focused: bool,
    },

    JoystickButton {
        dummy: u32,
        press_state: ElementState,
        modifiers_state: ModifiersState,
        window_focused: bool,
    },

    CursorMove {
        pixel_position: (f64, f64),
        modifiers_state: ModifiersState,
        in_window: bool,
        window_focused: bool,
    },

    MouseWheel {
        horizontal_delta: f64,
        vertical_delta: f64,
        modifiers_state: ModifiersState,
        in_window: bool,
        window_focused: bool,
    },

    MouseMotion {
        dx: f64,
        dy: f64,
        modifiers_state: ModifiersState,
        in_window: bool,
        window_focused: bool,
    },

    JoystickAxis {
        id: u32,
        value: f64,
        modifiers_state: ModifiersState,
        window_focused: bool,
    },

    Window(GenericWindowEvent),
    System(GenericSystemEvent),
}

impl GenericEvent {
    pub fn press_state(&self) -> Option<ElementState> {
        match self {
            Self::KeyboardKey { press_state, .. } => Some(*press_state),
            Self::MouseButton { press_state, .. } => Some(*press_state),
            Self::JoystickButton { press_state, .. } => Some(*press_state),
            _ => None,
        }
    }

    pub fn modifiers_state(&self) -> Option<ModifiersState> {
        match self {
            Self::KeyboardKey {
                modifiers_state, ..
            } => Some(*modifiers_state),
            Self::MouseButton {
                modifiers_state, ..
            } => Some(*modifiers_state),
            Self::MouseMotion {
                modifiers_state, ..
            } => Some(*modifiers_state),
            Self::MouseWheel {
                modifiers_state, ..
            } => Some(*modifiers_state),
            Self::CursorMove {
                modifiers_state, ..
            } => Some(*modifiers_state),
            Self::JoystickAxis {
                modifiers_state, ..
            } => Some(*modifiers_state),
            Self::JoystickButton {
                modifiers_state, ..
            } => Some(*modifiers_state),
            _ => None,
        }
    }

    pub fn is_window_focused(&self) -> bool {
        matches!(
            self,
            Self::KeyboardKey {
                window_focused: true,
                ..
            } | Self::MouseButton {
                window_focused: true,
                ..
            } | Self::JoystickButton {
                window_focused: true,
                ..
            } | Self::MouseMotion {
                window_focused: true,
                ..
            } | Self::MouseWheel {
                window_focused: true,
                ..
            } | Self::CursorMove {
                window_focused: true,
                ..
            } | Self::JoystickAxis {
                window_focused: true,
                ..
            } | Self::Window(_)
                | Self::System(_)
        )
    }

    pub fn pixel_position(&self) -> Option<(f64, f64)> {
        match self {
            Self::CursorMove { pixel_position, .. } => Some(*pixel_position),
            _ => None,
        }
    }

    pub fn gpu_position(&self, logical_size: LogicalSize<f64>) -> Option<(f32, f32)> {
        self.pixel_position().map(|(x, y)| {
            (
                (x / logical_size.width * 2.0) as f32,
                (y / logical_size.height * 2.0) as f32,
            )
        })
    }

    pub fn is_primary_mouse_down(&self) -> bool {
        match self {
            Self::MouseButton {
                button,
                press_state,
                ..
            } => {
                if *button == 1 && *press_state == ElementState::Pressed {
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum GenericWindowEvent {
    // Note that the sizes passed here may race with the ones returned by the surface/window,
    // so code should be careful to use these values instead of the ones returned by those apis.
    Resized { width: u32, height: u32 },

    // Note that the scale factor passed here may race with the one given back by the surface
    // so code that responds to this should be careful to use this value instead of the one there.
    ScaleFactorChanged { scale: f64 },
}

#[derive(Debug, Clone)]
pub enum GenericSystemEvent {
    // Aggregate of various "user wants the program to go away" interactions. Close button (the X)
    // pressed in the window's bar or task bar, Win+F4 pressed, File+Quit, etc.
    Quit,

    // We do not generally care about individual mice or keyboards: the events will still come
    // through automatically. This will be very important for Joystick management, however, as we
    // expect those to cycle relatively frequently during gameplay.
    DeviceAdded { dummy: u32 },
    DeviceRemoved { dummy: u32 },
}
