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
use command::{Bindings, Command, Key};
use failure::{bail, Fallible};
use log::warn;
use smallvec::{smallvec, SmallVec};
use std::{
    collections::HashMap,
    sync::mpsc::{channel, Receiver, TryRecvError},
    thread,
};
use winit::{
    event::{
        DeviceEvent, DeviceId, ElementState, Event, KeyboardInput, MouseScrollDelta, StartCause,
        WindowEvent,
    },
    event_loop::{ControlFlow, EventLoop, EventLoopProxy},
    window::Window,
};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MetaEvent {
    Stop,
}

pub struct InputController {
    proxy: EventLoopProxy<MetaEvent>,
    command_source: Receiver<Command>,
}

impl InputController {
    fn new(proxy: EventLoopProxy<MetaEvent>, command_source: Receiver<Command>) -> Self {
        Self {
            proxy,
            command_source,
        }
    }

    pub fn quit(&self) -> Fallible<()> {
        self.proxy.send_event(MetaEvent::Stop)?;
        Ok(())
    }

    pub fn poll(&self) -> Fallible<SmallVec<[Command; 8]>> {
        let mut out = SmallVec::new();
        let mut command = self.command_source.try_recv();
        while command.is_ok() {
            out.push(command?);
            command = self.command_source.try_recv();
        }
        match command.err().unwrap() {
            TryRecvError::Empty => Ok(out),
            TryRecvError::Disconnected => bail!("input system stopped"),
        }
    }
}

pub struct InputSystem {}

impl InputSystem {
    pub fn run_forever<F>(bindings: Vec<Bindings>, mut game_loop: F) -> Fallible<()>
    where
        F: 'static + Send + FnMut(Window, &InputController) -> Fallible<()>,
    {
        let event_loop = EventLoop::<MetaEvent>::with_user_event();
        let window = Window::new(&event_loop).expect("unable to create window");
        let (tx_command, rx_command) = channel();
        let input_controller = InputController::new(event_loop.create_proxy(), rx_command);

        // Spawn the game thread.
        let _game_thread = thread::spawn(move || {
            if let Err(e) = game_loop(window, &input_controller) {
                println!("Error: {}", e);
            }
            input_controller.quit().ok();
        });

        // Hijack the main thread.
        let mut button_state = HashMap::new();
        event_loop.run(move |event, _target, control_flow| {
            *control_flow = ControlFlow::Wait;
            if event == Event::UserEvent(MetaEvent::Stop) {
                *control_flow = ControlFlow::Exit;
                return;
            }
            let commands = Self::handle_event(event, &bindings, &mut button_state).unwrap();
            for command in &commands {
                tx_command.send(command.to_owned()).expect("send okay");
            }
        });
    }

    /*
    pub fn push_bindings(&mut self, bindings: Bindings) {
        self.bindings.push(bindings);
    }

    pub fn pop_bindings(&mut self) -> Option<Bindings> {
        self.bindings.pop()
    }
     */

    fn handle_event(
        e: Event<MetaEvent>,
        binding_list: &[Bindings],
        button_state: &mut HashMap<Key, ElementState>,
    ) -> Fallible<SmallVec<[Command; 8]>> {
        Ok(match e {
            Event::WindowEvent { event, .. } => Self::handle_window_event(event)?,
            Event::DeviceEvent { device_id, event } => {
                Self::handle_device_event(device_id, event, binding_list, button_state)?
            }
            Event::MainEventsCleared => smallvec![],
            Event::RedrawRequested(_window_id) => smallvec![],
            Event::RedrawEventsCleared => smallvec![],
            Event::NewEvents(StartCause::WaitCancelled { .. }) => smallvec![],
            unhandled => {
                warn!("don't know how to handle: {:?}", unhandled);
                smallvec![]
            }
        })
    }

    fn handle_window_event(event: WindowEvent) -> Fallible<SmallVec<[Command; 8]>> {
        Ok(match event {
            // System Stuff
            WindowEvent::Resized(s) => {
                smallvec![Command::parse("window.resize")?.with_arg(s.into())]
            }
            WindowEvent::Moved(p) => smallvec![Command::parse("window.move")?.with_arg(p.into())],
            WindowEvent::Destroyed => smallvec![Command::parse("window.destroy")?],
            WindowEvent::CloseRequested => smallvec![Command::parse("window.close")?],
            WindowEvent::Focused(b) => {
                smallvec![Command::parse("window.focus")?.with_arg(b.into())]
            }
            WindowEvent::DroppedFile(p) => {
                smallvec![Command::parse("window.file-drop")?.with_arg(p.into())]
            }
            WindowEvent::HoveredFile(p) => {
                smallvec![Command::parse("window.file-hover")?.with_arg(p.into())]
            }
            WindowEvent::HoveredFileCancelled => {
                smallvec![Command::parse("window.file-hover-cancel")?]
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                smallvec![Command::parse("window.dpi-change")?.with_arg(scale_factor.into())]
            }
            WindowEvent::CursorEntered { device_id } => {
                smallvec![Command::parse("window.cursor-entered")?.with_arg(device_id.into())]
            }
            WindowEvent::CursorLeft { device_id } => {
                smallvec![Command::parse("window.cursor-left")?.with_arg(device_id.into())]
            }

            // Track real cursor position in the window including window system accel
            // warping, and other such; mostly useful for software mice, but also for
            // picking with a hardware mouse.
            WindowEvent::CursorMoved { position, .. } => {
                smallvec![Command::parse("window.cursor-move")?.with_arg(position.into())]
            }

            // Ignore events duplicated by device capture.
            WindowEvent::ReceivedCharacter { .. } => smallvec![],
            WindowEvent::KeyboardInput { .. } => smallvec![],
            WindowEvent::MouseInput { .. } => smallvec![],
            WindowEvent::MouseWheel { .. } => smallvec![],

            // Ignore events we don't get on the device.
            WindowEvent::Touch(_) => smallvec![],
            WindowEvent::TouchpadPressure { .. } => smallvec![],
            WindowEvent::AxisMotion { .. } => smallvec![],

            WindowEvent::ModifiersChanged { .. } => smallvec![],

            WindowEvent::ThemeChanged(_) => smallvec![],
        })
    }

    fn handle_device_event(
        device_id: DeviceId,
        event: DeviceEvent,
        binding_list: &[Bindings],
        button_state: &mut HashMap<Key, ElementState>,
    ) -> Fallible<SmallVec<[Command; 8]>> {
        Ok(match event {
            // Device change events
            DeviceEvent::Added => {
                smallvec![Command::parse("device.added")?.with_arg(device_id.into())]
            }
            DeviceEvent::Removed => {
                smallvec![Command::parse("device.removed")?.with_arg(device_id.into())]
            }

            // Mouse Motion
            DeviceEvent::MouseMotion { delta } => {
                smallvec![Command::parse("device.mouse-move")?.with_arg(delta.into())]
            }

            // Mouse Wheel
            DeviceEvent::MouseWheel {
                delta: MouseScrollDelta::LineDelta(x, y),
            } => smallvec![Command::parse("device.mouse-wheel")?.with_arg((x, y).into())],
            DeviceEvent::MouseWheel {
                delta: MouseScrollDelta::PixelDelta(s),
            } => smallvec![Command::parse("device.mouse-wheel")?.with_arg(s.into())],

            // Mouse Button
            DeviceEvent::Button { button, state } => {
                button_state.insert(Key::MouseButton(button), state);
                Self::match_key(Key::MouseButton(button), state, button_state, binding_list)?
            }

            // Match virtual keycodes.
            DeviceEvent::Key(KeyboardInput {
                virtual_keycode: Some(code),
                scancode,
                state,
                ..
            }) => {
                button_state.insert(Key::Physical(scancode), state);
                button_state.insert(Key::Virtual(code), state);
                Self::match_key(Key::Virtual(code), state, button_state, binding_list)?
            }

            // Match scancodes.
            DeviceEvent::Key(KeyboardInput {
                virtual_keycode: None,
                scancode,
                state,
                ..
            }) => {
                button_state.insert(Key::Physical(scancode), state);
                smallvec![]
            }

            // Duplicate from MouseMotion for some reason?
            DeviceEvent::Motion { .. } => smallvec![],

            // I'm not sure what this does?
            DeviceEvent::Text { .. } => smallvec![],
        })
    }

    fn match_key(
        key: Key,
        state: ElementState,
        button_state: &HashMap<Key, ElementState>,
        binding_list: &[Bindings],
    ) -> Fallible<SmallVec<[Command; 8]>> {
        let mut out = SmallVec::new();
        for bindings in binding_list.iter().rev() {
            out.extend(bindings.match_key(key, state, button_state)?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use approx::assert_relative_eq;
    use std::path::PathBuf;
    use winit::{
        dpi::PhysicalSize,
        event::{ModifiersState, VirtualKeyCode},
        window::WindowId,
    };

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

    fn vkey(key: VirtualKeyCode, state: bool) -> KeyboardInput {
        #[allow(deprecated)]
        KeyboardInput {
            scancode: 0,
            virtual_keycode: Some(key),
            state: if state {
                ElementState::Pressed
            } else {
                ElementState::Released
            },
            modifiers: ModifiersState::empty(),
        }
    }

    #[test]
    fn test_handle_system_events() -> Fallible<()> {
        let binding_list = vec![];
        let mut button_state = HashMap::new();

        let cmd = InputSystem::handle_event(
            win_evt(WindowEvent::Resized(physical_size())),
            &binding_list,
            &mut button_state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "resize");
        assert_relative_eq!(cmd.displacement(0)?.0, 8f64);
        assert_relative_eq!(cmd.displacement(0)?.1, 9f64);

        let cmd = InputSystem::handle_event(
            win_evt(WindowEvent::Destroyed),
            &binding_list,
            &mut button_state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "destroy");

        let cmd = InputSystem::handle_event(
            win_evt(WindowEvent::CloseRequested),
            &binding_list,
            &mut button_state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "close");

        let cmd = InputSystem::handle_event(
            win_evt(WindowEvent::DroppedFile(path())),
            &binding_list,
            &mut button_state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "file-drop");
        assert_eq!(cmd.path(0)?, path());

        let cmd = InputSystem::handle_event(
            win_evt(WindowEvent::Focused(true)),
            &binding_list,
            &mut button_state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "focus");
        assert!(cmd.boolean(0)?);

        let cmd = InputSystem::handle_event(
            dev_evt(DeviceEvent::Added),
            &binding_list,
            &mut button_state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "added");
        let cmd = InputSystem::handle_event(
            dev_evt(DeviceEvent::Removed),
            &binding_list,
            &mut button_state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "removed");

        let cmd = InputSystem::handle_event(
            dev_evt(DeviceEvent::MouseMotion { delta: (8., 9.) }),
            &binding_list,
            &mut button_state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "mouse-move");
        assert_relative_eq!(cmd.displacement(0)?.0, 8f64);
        assert_relative_eq!(cmd.displacement(0)?.1, 9f64);

        let cmd = InputSystem::handle_event(
            dev_evt(DeviceEvent::MouseWheel {
                delta: MouseScrollDelta::LineDelta(8., 9.),
            }),
            &binding_list,
            &mut button_state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "mouse-wheel");
        assert_relative_eq!(cmd.displacement(0)?.0, 8f64);
        assert_relative_eq!(cmd.displacement(0)?.1, 9f64);

        Ok(())
    }

    #[test]
    fn test_can_handle_nested_events() -> Fallible<()> {
        let menu = Bindings::new("menu")
            .bind("menu.+enter", "alt")?
            .bind("menu.exit", "shift+e")?
            .bind("menu.click", "mouse0")?;
        let fps = Bindings::new("fps")
            .bind("player.+move-forward", "w")?
            .bind("player.eject", "shift+e")?
            .bind("player.fire", "mouse0")?;

        let mut binding_list = vec![menu, fps];
        let mut button_state = HashMap::new();

        // FPS forward.
        let cmd = InputSystem::handle_event(
            dev_evt(DeviceEvent::Key(vkey(VirtualKeyCode::W, true))),
            &binding_list,
            &mut button_state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "+move-forward");
        let cmd = InputSystem::handle_event(
            dev_evt(DeviceEvent::Key(vkey(VirtualKeyCode::W, false))),
            &binding_list,
            &mut button_state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "-move-forward");

        // Mouse Button + find fire before click.
        let cmd = InputSystem::handle_event(
            dev_evt(DeviceEvent::Button {
                button: 0,
                state: ElementState::Pressed,
            }),
            &binding_list,
            &mut button_state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "fire");
        let cmd = InputSystem::handle_event(
            dev_evt(DeviceEvent::Button {
                button: 0,
                state: ElementState::Released,
            }),
            &binding_list,
            &mut button_state,
        )?;
        assert!(cmd.is_empty());

        // Multiple buttons + found shift from LShfit + find eject instead of exit
        let cmd = InputSystem::handle_event(
            dev_evt(DeviceEvent::Key(vkey(VirtualKeyCode::LShift, true))),
            &binding_list,
            &mut button_state,
        )?;
        assert!(cmd.is_empty());
        let cmd = InputSystem::handle_event(
            dev_evt(DeviceEvent::Key(vkey(VirtualKeyCode::E, true))),
            &binding_list,
            &mut button_state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "eject");

        // Let off e, drop fps, then hit again and get the other command
        let cmd = InputSystem::handle_event(
            dev_evt(DeviceEvent::Key(vkey(VirtualKeyCode::E, false))),
            &binding_list,
            &mut button_state,
        )?;
        assert!(cmd.is_empty());
        binding_list.pop();
        let cmd = InputSystem::handle_event(
            dev_evt(DeviceEvent::Key(vkey(VirtualKeyCode::E, true))),
            &binding_list,
            &mut button_state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "exit");
        let cmd = InputSystem::handle_event(
            dev_evt(DeviceEvent::Key(vkey(VirtualKeyCode::LShift, false))),
            &binding_list,
            &mut button_state,
        )?;
        assert!(cmd.is_empty());

        // Push on a new command set and ensure that it masks.
        let flight = Bindings::new("flight").bind("player.+pickle", "mouse0")?;
        binding_list.push(flight);

        let cmd = InputSystem::handle_event(
            dev_evt(DeviceEvent::Button {
                button: 0,
                state: ElementState::Pressed,
            }),
            &binding_list,
            &mut button_state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "+pickle");
        let cmd = InputSystem::handle_event(
            dev_evt(DeviceEvent::Button {
                button: 0,
                state: ElementState::Released,
            }),
            &binding_list,
            &mut button_state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "-pickle");

        Ok(())
    }
}
