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
mod generic;

pub use generic::{GenericEvent, GenericSystemEvent, GenericWindowEvent, MouseAxis};
pub use winit::event::{ButtonId, ElementState, ModifiersState, VirtualKeyCode};

use anyhow::{bail, Result};
use smallvec::SmallVec;
use std::{
    collections::HashMap,
    sync::mpsc::{channel, Receiver, TryRecvError},
};
use winit::{
    event::{
        DeviceEvent, DeviceId, Event, KeyboardInput, MouseScrollDelta, StartCause, WindowEvent,
    },
    event_loop::{ControlFlow, EventLoop, EventLoopProxy},
    window::Window,
};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MetaEvent {
    Stop,
}

#[derive(Debug, Default)]
pub struct InputState {
    modifiers_state: ModifiersState,
    cursor_in_window: HashMap<DeviceId, bool>,
    window_focused: bool,
}

pub struct InputController {
    proxy: EventLoopProxy<MetaEvent>,
    event_source: Receiver<GenericEvent>,
}

impl InputController {
    fn new(proxy: EventLoopProxy<MetaEvent>, event_source: Receiver<GenericEvent>) -> Self {
        Self {
            proxy,
            event_source,
        }
    }

    pub fn quit(&self) -> Result<()> {
        self.proxy.send_event(MetaEvent::Stop)?;
        Ok(())
    }

    pub fn poll_events(&self) -> Result<SmallVec<[GenericEvent; 8]>> {
        let mut out = SmallVec::new();
        let mut event_input = self.event_source.try_recv();
        while event_input.is_ok() {
            out.push(event_input?);
            event_input = self.event_source.try_recv();
        }
        match event_input.err().unwrap() {
            TryRecvError::Empty => Ok(out),
            TryRecvError::Disconnected => bail!("input system stopped"),
        }
    }
}

#[derive(Debug)]
pub struct InputSystem;

impl InputSystem {
    pub fn make_event_loop() -> EventLoop<MetaEvent> {
        EventLoop::<MetaEvent>::with_user_event()
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn run_forever<M, T>(
        event_loop: EventLoop<MetaEvent>,
        window: Window,
        mut window_loop: M,
        mut ctx: T,
    ) -> Result<()>
    where
        T: 'static + Send + Sync,
        M: 'static + Send + FnMut(&Window, &InputController, &mut T) -> Result<()>,
    {
        use web_sys::console;

        let (tx_event, rx_event) = channel();
        let input_controller = InputController::new(event_loop.create_proxy(), rx_event);

        // Hijack the main thread.
        let mut generic_events = Vec::new();
        let mut input_state: InputState = Default::default();
        event_loop.run(move |event, _target, control_flow| {
            *control_flow = ControlFlow::Wait;
            if event == Event::UserEvent(MetaEvent::Stop) {
                *control_flow = ControlFlow::Exit;
                return;
            }

            Self::wrap_event(&event, &mut input_state, &mut generic_events);
            for evt in generic_events.drain(..) {
                if let Err(e) = tx_event.send(evt) {
                    console::log_1(&format!("Game loop hung up ({}), exiting...", e).into());
                    *control_flow = ControlFlow::Exit;
                    return;
                }
            }
            if event == Event::MainEventsCleared {
                window_loop(&window, &input_controller, &mut ctx).unwrap();
            }

            /*
            let commands = match event {
                Event::WindowEvent { event, .. } => {
                    console::log_1(&format!("window event: {:?}", event).into());
                    Self::handle_window_event(event, &bindings, &mut button_state).unwrap()
                }
                Event::DeviceEvent { device_id, event } => {
                    console::log_1(&format!("device event: {:?}", event).into());
                    Self::handle_device_event(device_id, event, &bindings, &mut button_state)
                        .unwrap()
                }
                Event::MainEventsCleared => {
                    window_loop(&window, &input_controller, &mut ctx).unwrap();
                    smallvec![]
                }
                Event::RedrawRequested(_window_id) => smallvec![],
                Event::RedrawEventsCleared => smallvec![],
                Event::NewEvents(StartCause::WaitCancelled { .. }) => smallvec![],
                unhandled => {
                    console::log_1(&format!("don't know how to handle: {:?}", unhandled).into());
                    smallvec![]
                }
            };
            for command in &commands {
                if let Err(e) = tx_command.send(command.to_owned()) {
                    console::log_1(&format!("Game loop hung up ({}), exiting...", e).into());
                    *control_flow = ControlFlow::Exit;
                    return;
                }
            }
             */
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn run_forever<M>(mut window_main: M) -> Result<()>
    where
        M: 'static + Send + FnMut(Window, &InputController) -> Result<()>,
    {
        let event_loop = EventLoop::<MetaEvent>::with_user_event();
        let window = winit::window::WindowBuilder::new()
            .with_title("Nitrogen")
            .build(&event_loop)?;
        let (tx_event, rx_event) = channel();
        let input_controller = InputController::new(event_loop.create_proxy(), rx_event);

        // Spawn the game thread.
        std::thread::spawn(move || {
            if let Err(e) = window_main(window, &input_controller) {
                println!("Error: {}", e);
            }
            input_controller.quit().ok();
        });

        // Hijack the main thread.
        let mut generic_events = Vec::new();
        let mut input_state: InputState = Default::default();
        event_loop.run(move |event, _target, control_flow| {
            *control_flow = ControlFlow::Wait;
            if event == Event::UserEvent(MetaEvent::Stop) {
                *control_flow = ControlFlow::Exit;
                return;
            }

            Self::wrap_event(&event, &mut input_state, &mut generic_events);
            for evt in generic_events.drain(..) {
                if let Err(e) = tx_event.send(evt) {
                    println!("Game loop hung up ({}), exiting...", e);
                    *control_flow = ControlFlow::Exit;
                    return;
                }
            }
        });
    }

    fn wrap_event(e: &Event<MetaEvent>, input_state: &mut InputState, out: &mut Vec<GenericEvent>) {
        match e {
            Event::WindowEvent { event, .. } => Self::wrap_window_event(event, input_state, out),
            Event::DeviceEvent { device_id, event } => {
                Self::wrap_device_event(device_id, event, input_state, out)
            }
            Event::MainEventsCleared => {}
            Event::RedrawRequested(_window_id) => {}
            Event::RedrawEventsCleared => {}
            Event::NewEvents(StartCause::WaitCancelled { .. }) => {}
            unhandled => {
                log::warn!("don't know how to handle: {:?}", unhandled);
            }
        }
    }

    fn wrap_window_event(
        event: &WindowEvent,
        input_state: &mut InputState,
        out: &mut Vec<GenericEvent>,
    ) {
        match event {
            WindowEvent::Resized(s) => {
                out.push(GenericEvent::Window(GenericWindowEvent::Resized {
                    width: s.width,
                    height: s.height,
                }));
            }
            WindowEvent::Moved(_) => {}
            WindowEvent::Destroyed => {
                out.push(GenericEvent::System(GenericSystemEvent::Quit));
            }
            WindowEvent::CloseRequested => {
                out.push(GenericEvent::System(GenericSystemEvent::Quit));
            }
            WindowEvent::Focused(b) => {
                input_state.window_focused = *b;
            }
            WindowEvent::DroppedFile(_) => {}
            WindowEvent::HoveredFile(_) => {}
            WindowEvent::HoveredFileCancelled => {}
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                out.push(GenericEvent::Window(
                    GenericWindowEvent::ScaleFactorChanged {
                        scale: *scale_factor,
                    },
                ));
            }
            WindowEvent::CursorEntered { device_id } => {
                input_state.cursor_in_window.insert(*device_id, true);
            }
            WindowEvent::CursorLeft { device_id } => {
                input_state.cursor_in_window.insert(*device_id, false);
            }

            // Track real cursor position in the window including window system accel
            // warping, and other such; mostly useful for software mice, but also for
            // picking with a hardware mouse.
            WindowEvent::CursorMoved {
                position,
                device_id,
                ..
            } => {
                let in_window = *input_state
                    .cursor_in_window
                    .get(device_id)
                    .unwrap_or(&false);
                out.push(GenericEvent::CursorMove {
                    pixel_position: (position.x, position.y),
                    modifiers_state: input_state.modifiers_state,
                    in_window,
                    window_focused: input_state.window_focused,
                });
            }

            // We need to capture keyboard input both here and below because of web.
            WindowEvent::KeyboardInput {
                input:
                    KeyboardInput {
                        virtual_keycode,
                        scancode,
                        state,
                        ..
                    },
                ..
            } => {
                if let Some(vkey) = Self::guess_key(
                    *scancode,
                    *virtual_keycode,
                    input_state.modifiers_state.shift(),
                ) {
                    out.push(GenericEvent::KeyboardKey {
                        scancode: *scancode,
                        virtual_keycode: vkey,
                        press_state: *state,
                        modifiers_state: input_state.modifiers_state,
                        window_focused: true,
                    });
                }
            }

            // Ignore events duplicated by other capture methods.
            WindowEvent::ReceivedCharacter { .. } => {}
            WindowEvent::MouseInput { .. } => {}
            WindowEvent::MouseWheel { .. } => {}
            WindowEvent::AxisMotion { .. } => {}

            // Don't worry about touch just yet.
            WindowEvent::Touch(_) => {}
            WindowEvent::TouchpadPressure { .. } => {}

            WindowEvent::ModifiersChanged(modifiers_state) => {
                input_state.modifiers_state = *modifiers_state;
            }

            WindowEvent::ThemeChanged(_) => {}
        }
    }

    fn wrap_device_event(
        device_id: &DeviceId,
        event: &DeviceEvent,
        input_state: &mut InputState,
        out: &mut Vec<GenericEvent>,
    ) {
        match event {
            // Device change events
            DeviceEvent::Added => {
                out.push(GenericEvent::System(GenericSystemEvent::DeviceAdded {
                    dummy: 0,
                }));
            }
            DeviceEvent::Removed => {
                out.push(GenericEvent::System(GenericSystemEvent::DeviceRemoved {
                    dummy: 0,
                }));
            }

            // Mouse Motion: unfiltered, arbitrary units
            DeviceEvent::MouseMotion { delta: (dx, dy) } => {
                let in_window = *input_state
                    .cursor_in_window
                    .get(device_id)
                    .unwrap_or(&false);
                out.push(GenericEvent::MouseMotion {
                    dx: *dx,
                    dy: *dy,
                    modifiers_state: input_state.modifiers_state,
                    in_window,
                    window_focused: input_state.window_focused,
                });
            }

            // Mouse Wheel
            DeviceEvent::MouseWheel {
                delta: MouseScrollDelta::LineDelta(dh, dv),
            } => {
                let in_window = *input_state
                    .cursor_in_window
                    .get(device_id)
                    .unwrap_or(&false);
                out.push(GenericEvent::MouseWheel {
                    horizontal_delta: *dh as f64,
                    vertical_delta: *dv as f64,
                    modifiers_state: input_state.modifiers_state,
                    in_window,
                    window_focused: input_state.window_focused,
                });
            }
            DeviceEvent::MouseWheel {
                delta: MouseScrollDelta::PixelDelta(s),
            } => {
                let in_window = *input_state
                    .cursor_in_window
                    .get(device_id)
                    .unwrap_or(&false);
                out.push(GenericEvent::MouseWheel {
                    horizontal_delta: s.x,
                    vertical_delta: s.y,
                    modifiers_state: input_state.modifiers_state,
                    in_window,
                    window_focused: input_state.window_focused,
                });
            }

            // Mouse Button, maybe also joystick button?
            DeviceEvent::Button { button, state } => {
                let in_window = *input_state
                    .cursor_in_window
                    .get(device_id)
                    .unwrap_or(&false);
                out.push(GenericEvent::MouseButton {
                    button: *button,
                    press_state: *state,
                    modifiers_state: input_state.modifiers_state,
                    in_window,
                    window_focused: input_state.window_focused,
                })
            }

            // Match virtual keycodes.
            DeviceEvent::Key(KeyboardInput {
                virtual_keycode,
                scancode,
                state,
                ..
            }) => {
                // If not focused, send from the device event. Note that this is important for
                // keeping key states, e.g. when the mouse is not locked with focus-on-hover.
                if !input_state.window_focused {
                    if let Some(vkey) = Self::guess_key(
                        *scancode,
                        *virtual_keycode,
                        input_state.modifiers_state.shift(),
                    ) {
                        out.push(GenericEvent::KeyboardKey {
                            scancode: *scancode,
                            virtual_keycode: vkey,
                            press_state: *state,
                            modifiers_state: input_state.modifiers_state,
                            window_focused: false,
                        });
                    }
                }
            }

            // Includes both joystick and mouse axis motion.
            DeviceEvent::Motion { axis, value } => {
                out.push(GenericEvent::JoystickAxis {
                    id: *axis,
                    value: *value,
                    modifiers_state: input_state.modifiers_state,
                    window_focused: input_state.window_focused,
                });
            }

            // I'm not sure what this does?
            DeviceEvent::Text { .. } => {}
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn guess_key(
        _scancode: u32,
        virtual_key: Option<VirtualKeyCode>,
        _shift_pressed: bool,
    ) -> Option<VirtualKeyCode> {
        // The virtual_key is all we get with wasm... scancode looks like ascii?
        virtual_key
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn guess_key(
        scancode: u32,
        virtual_key: Option<VirtualKeyCode>,
        shift_pressed: bool,
    ) -> Option<VirtualKeyCode> {
        // The X11 driver is using XLookupString rather than XIM or xinput2, so gets modified
        // keys, losing the key code in the process. e.g. a press of the 5 key may return Key5 or
        // None, depending on whether shift is also pressed, because percent is not a physical key.
        let discovered = match (shift_pressed, scancode) {
            (_, 1) => Some(VirtualKeyCode::Escape),
            (_, 59) => Some(VirtualKeyCode::F1),
            (_, 60) => Some(VirtualKeyCode::F2),
            (_, 61) => Some(VirtualKeyCode::F3),
            (_, 62) => Some(VirtualKeyCode::F4),
            (_, 63) => Some(VirtualKeyCode::F5),
            (_, 64) => Some(VirtualKeyCode::F6),
            (_, 65) => Some(VirtualKeyCode::F7),
            (_, 66) => Some(VirtualKeyCode::F8),
            (_, 67) => Some(VirtualKeyCode::F9),
            (_, 68) => Some(VirtualKeyCode::F10),
            (_, 87) => Some(VirtualKeyCode::F11),
            (_, 88) => Some(VirtualKeyCode::F12),

            (_, 41) => Some(VirtualKeyCode::Grave),
            (_, 2) => Some(VirtualKeyCode::Key1),
            (_, 3) => Some(VirtualKeyCode::Key2),
            (_, 4) => Some(VirtualKeyCode::Key3),
            (_, 5) => Some(VirtualKeyCode::Key4),
            (_, 6) => Some(VirtualKeyCode::Key5),
            (_, 7) => Some(VirtualKeyCode::Key6),
            (_, 8) => Some(VirtualKeyCode::Key7),
            (_, 9) => Some(VirtualKeyCode::Key8),
            (_, 10) => Some(VirtualKeyCode::Key9),
            (_, 11) => Some(VirtualKeyCode::Key0),
            (_, 12) => Some(VirtualKeyCode::Minus),
            (false, 13) => Some(VirtualKeyCode::Equals),
            (true, 13) => Some(VirtualKeyCode::Plus),
            (_, 14) => Some(VirtualKeyCode::Back),

            (_, 15) => Some(VirtualKeyCode::Tab),
            (_, 16) => Some(VirtualKeyCode::Q),
            (_, 17) => Some(VirtualKeyCode::W),
            (_, 18) => Some(VirtualKeyCode::E),
            (_, 19) => Some(VirtualKeyCode::R),
            (_, 20) => Some(VirtualKeyCode::T),
            (_, 21) => Some(VirtualKeyCode::Y),
            (_, 22) => Some(VirtualKeyCode::U),
            (_, 23) => Some(VirtualKeyCode::I),
            (_, 24) => Some(VirtualKeyCode::O),
            (_, 25) => Some(VirtualKeyCode::P),
            (_, 26) => Some(VirtualKeyCode::LBracket),
            (_, 27) => Some(VirtualKeyCode::RBracket),
            (_, 43) => Some(VirtualKeyCode::Backslash),

            // (_, ) => Some(VirtualKeyCode::Capital), // ?
            (_, 30) => Some(VirtualKeyCode::A),
            (_, 31) => Some(VirtualKeyCode::S),
            (_, 32) => Some(VirtualKeyCode::D),
            (_, 33) => Some(VirtualKeyCode::F),
            (_, 34) => Some(VirtualKeyCode::G),
            (_, 35) => Some(VirtualKeyCode::H),
            (_, 36) => Some(VirtualKeyCode::J),
            (_, 37) => Some(VirtualKeyCode::K),
            (_, 38) => Some(VirtualKeyCode::L),
            (_, 39) => Some(VirtualKeyCode::Semicolon),
            (_, 40) => Some(VirtualKeyCode::Apostrophe),
            (_, 28) => Some(VirtualKeyCode::Return),

            (_, 42) => Some(VirtualKeyCode::LShift),
            (_, 44) => Some(VirtualKeyCode::Z),
            (_, 45) => Some(VirtualKeyCode::X),
            (_, 46) => Some(VirtualKeyCode::C),
            (_, 47) => Some(VirtualKeyCode::V),
            (_, 48) => Some(VirtualKeyCode::B),
            (_, 49) => Some(VirtualKeyCode::N),
            (_, 50) => Some(VirtualKeyCode::M),
            (_, 51) => Some(VirtualKeyCode::Comma),
            (_, 52) => Some(VirtualKeyCode::Period),
            (_, 53) => Some(VirtualKeyCode::Slash),
            (_, 54) => Some(VirtualKeyCode::RShift),

            (_, 29) => Some(VirtualKeyCode::LControl),
            (_, 56) => Some(VirtualKeyCode::LAlt),
            (_, 57) => Some(VirtualKeyCode::Space),
            (_, 100) => Some(VirtualKeyCode::RAlt),
            (_, 97) => Some(VirtualKeyCode::RControl),

            (_, 102) => Some(VirtualKeyCode::Home),
            (_, 103) => Some(VirtualKeyCode::Up),
            (_, 104) => Some(VirtualKeyCode::PageUp),
            (_, 105) => Some(VirtualKeyCode::Left),
            (_, 106) => Some(VirtualKeyCode::Right),
            (_, 107) => Some(VirtualKeyCode::End),
            (_, 108) => Some(VirtualKeyCode::Down),
            (_, 109) => Some(VirtualKeyCode::PageDown),
            (_, 110) => Some(VirtualKeyCode::Insert),
            (_, 111) => Some(VirtualKeyCode::Delete),

            //=> Some(VirtualKeyCode::),
            _ => None,
        };

        if virtual_key.is_some() {
            if virtual_key != discovered {
                println!(
                    "Warning: broken scancode map for: {}; got virtual: {:?}; expected: {:?}",
                    scancode, discovered, virtual_key
                );
            }
            debug_assert_eq!(virtual_key, discovered);
            return virtual_key;
        }

        discovered
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::path::PathBuf;
    use winit::{dpi::PhysicalSize, window::WindowId};

    fn physical_size() -> PhysicalSize<u32> {
        PhysicalSize {
            width: 8,
            height: 9,
        }
    }

    fn path() -> PathBuf {
        let mut buf = PathBuf::new();
        buf.push("a");
        buf.push("b");
        buf
    }

    fn win_evt(event: WindowEvent<'static>) -> Event<'static, MetaEvent> {
        Event::WindowEvent {
            window_id: unsafe { WindowId::dummy() },
            event,
        }
    }

    fn dev_evt(event: DeviceEvent) -> Event<'static, MetaEvent> {
        Event::DeviceEvent {
            device_id: unsafe { DeviceId::dummy() },
            event,
        }
    }

    #[test]
    fn test_handle_system_events() {
        let mut input_state: InputState = Default::default();
        let psz = physical_size();

        let mut evts = Vec::new();
        InputSystem::wrap_event(
            &win_evt(WindowEvent::Resized(psz)),
            &mut input_state,
            &mut evts,
        );
        assert!(matches!(
            evts[0],
            GenericEvent::Window(GenericWindowEvent::Resized { .. })
        ));

        let mut evts = Vec::new();
        InputSystem::wrap_event(
            &win_evt(WindowEvent::Destroyed),
            &mut input_state,
            &mut evts,
        );
        assert!(matches!(
            evts[0],
            GenericEvent::System(GenericSystemEvent::Quit)
        ));

        let mut evts = Vec::new();
        InputSystem::wrap_event(
            &win_evt(WindowEvent::CloseRequested),
            &mut input_state,
            &mut evts,
        );
        assert!(matches!(
            evts[0],
            GenericEvent::System(GenericSystemEvent::Quit)
        ));

        let mut evts = Vec::new();
        InputSystem::wrap_event(
            &win_evt(WindowEvent::DroppedFile(path())),
            &mut input_state,
            &mut evts,
        );
        assert!(evts.is_empty());

        let mut evts = Vec::new();
        InputSystem::wrap_event(
            &win_evt(WindowEvent::Focused(true)),
            &mut input_state,
            &mut evts,
        );
        assert!(evts.is_empty());

        let mut evts = Vec::new();
        InputSystem::wrap_event(&dev_evt(DeviceEvent::Added), &mut input_state, &mut evts);
        assert!(matches!(
            evts[0],
            GenericEvent::System(GenericSystemEvent::DeviceAdded { .. })
        ));

        let mut evts = Vec::new();
        InputSystem::wrap_event(&dev_evt(DeviceEvent::Removed), &mut input_state, &mut evts);
        assert!(matches!(
            evts[0],
            GenericEvent::System(GenericSystemEvent::DeviceRemoved { .. })
        ));

        let mut evts = Vec::new();
        InputSystem::wrap_event(
            &dev_evt(DeviceEvent::MouseMotion { delta: (8., 9.) }),
            &mut input_state,
            &mut evts,
        );
        assert!(matches!(evts[0], GenericEvent::MouseMotion { .. }));

        let mut evts = Vec::new();
        InputSystem::wrap_event(
            &dev_evt(DeviceEvent::MouseWheel {
                delta: MouseScrollDelta::LineDelta(8., 9.),
            }),
            &mut input_state,
            &mut evts,
        );
        assert!(matches!(evts[0], GenericEvent::MouseWheel { .. }));
    }
}
