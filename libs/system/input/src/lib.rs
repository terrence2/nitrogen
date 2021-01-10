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
use command::{BindingState, Bindings, Command, Key};
use failure::{bail, Fallible};
use smallvec::{smallvec, SmallVec};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use winit::event::ModifiersState;
use winit::{
    event::{
        DeviceEvent, DeviceId, ElementState, Event, KeyboardInput, MouseScrollDelta, StartCause,
        VirtualKeyCode, WindowEvent,
    },
    event_loop::{ControlFlow, EventLoop, EventLoopProxy},
    window::{Window, WindowBuilder},
};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MetaEvent {
    Stop,
}

pub struct InputController {
    proxy: EventLoopProxy<MetaEvent>,
    raw_keyboard_source: Receiver<(KeyboardInput, ModifiersState)>,
    command_source: Receiver<Command>,
}

impl InputController {
    fn new(
        proxy: EventLoopProxy<MetaEvent>,
        raw_keyboard_source: Receiver<(KeyboardInput, ModifiersState)>,
        command_source: Receiver<Command>,
    ) -> Self {
        Self {
            proxy,
            raw_keyboard_source,
            command_source,
        }
    }

    pub fn quit(&self) -> Fallible<()> {
        self.proxy.send_event(MetaEvent::Stop)?;
        Ok(())
    }

    pub fn poll_commands(&self) -> Fallible<SmallVec<[Command; 8]>> {
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

    pub fn poll_keyboard(&self) -> Fallible<SmallVec<[(KeyboardInput, ModifiersState); 8]>> {
        let mut out = SmallVec::new();
        let mut kb_input = self.raw_keyboard_source.try_recv();
        while kb_input.is_ok() {
            out.push(kb_input?);
            kb_input = self.raw_keyboard_source.try_recv();
        }
        match kb_input.err().unwrap() {
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
        bindings: Vec<Bindings>,
        event_loop: EventLoop<MetaEvent>,
        window: Window,
        mut window_loop: M,
        mut ctx: T,
    ) -> Fallible<()>
    where
        T: 'static + Send + Sync,
        M: 'static + Send + FnMut(&Window, &InputController, &mut T) -> Fallible<()>,
    {
        use web_sys::console;

        let (tx_command, rx_command) = channel();
        let input_controller = InputController::new(event_loop.create_proxy(), rx_command);

        // Hijack the main thread.
        let mut button_state = HashMap::new();
        event_loop.run(move |event, _target, control_flow| {
            *control_flow = ControlFlow::Wait;
            if event == Event::UserEvent(MetaEvent::Stop) {
                *control_flow = ControlFlow::Exit;
                return;
            }
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
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn run_forever<M>(bindings: Vec<Bindings>, mut window_main: M) -> Fallible<()>
    where
        M: 'static + Send + FnMut(Window, &InputController) -> Fallible<()>,
    {
        let event_loop = EventLoop::<MetaEvent>::with_user_event();
        let window = WindowBuilder::new()
            .with_title("Nitrogen")
            .build(&event_loop)?;
        let (tx_event, rx_event) = channel();
        let (tx_command, rx_command) = channel();
        let input_controller =
            InputController::new(event_loop.create_proxy(), rx_event, rx_command);

        // Spawn the game thread.
        std::thread::spawn(move || {
            if let Err(e) = window_main(window, &input_controller) {
                println!("Error: {}", e);
            }
            input_controller.quit().ok();
        });

        // Hijack the main thread.
        let mut state: BindingState = Default::default();
        event_loop.run(move |mut event, _target, control_flow| {
            *control_flow = ControlFlow::Wait;
            if event == Event::UserEvent(MetaEvent::Stop) {
                *control_flow = ControlFlow::Exit;
                return;
            }
            // TODO: poll receive queue for bindings changes?

            // Process and send commands from the input thread.
            let commands = Self::handle_event(&mut event, &bindings, &mut state).unwrap();
            for command in &commands {
                log::trace!("send command: {}", command);
                if let Err(e) = tx_command.send(command.to_owned()) {
                    println!("Game loop hung up ({}), exiting...", e);
                    *control_flow = ControlFlow::Exit;
                    return;
                }
            }

            // Send any raw keyboard events.
            if let Event::WindowEvent {
                event: WindowEvent::KeyboardInput { input, .. },
                ..
            } = event
            {
                if let Err(e) = tx_event.send((input, state.modifiers_state)) {
                    println!("Game loop hung up ({}), exiting...", e);
                    *control_flow = ControlFlow::Exit;
                    return;
                }
            }
        });
    }

    pub fn is_close_command(command: &Command) -> bool {
        matches!(
            command.full(),
            "window.close" | "window.destroy" | "window.exit"
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn handle_event(
        e: &mut Event<MetaEvent>,
        bindings: &[Bindings],
        state: &mut BindingState,
    ) -> Fallible<SmallVec<[Command; 8]>> {
        Ok(match e {
            Event::WindowEvent { event, .. } => Self::handle_window_event(event, bindings, state)?,
            Event::DeviceEvent { device_id, event } => {
                Self::handle_device_event(device_id, event, bindings, state)?
            }
            Event::MainEventsCleared => smallvec![],
            Event::RedrawRequested(_window_id) => smallvec![],
            Event::RedrawEventsCleared => smallvec![],
            Event::NewEvents(StartCause::WaitCancelled { .. }) => smallvec![],
            unhandled => {
                log::warn!("don't know how to handle: {:?}", unhandled);
                smallvec![]
            }
        })
    }

    fn handle_window_event(
        event: &mut WindowEvent,
        bindings: &[Bindings],
        state: &mut BindingState,
    ) -> Fallible<SmallVec<[Command; 8]>> {
        Ok(match event {
            // System Stuff
            WindowEvent::Resized(s) => {
                smallvec![Command::parse("window.resize")?.with_arg((*s).into())]
            }
            WindowEvent::Moved(p) => {
                smallvec![Command::parse("window.move")?.with_arg((*p).into())]
            }
            WindowEvent::Destroyed => smallvec![Command::parse("window.destroy")?],
            WindowEvent::CloseRequested => smallvec![Command::parse("window.close")?],
            WindowEvent::Focused(b) => {
                smallvec![Command::parse("window.focus")?.with_arg((*b).into())]
            }
            WindowEvent::DroppedFile(p) => {
                smallvec![Command::parse("window.file-drop")?.with_arg(p.as_path().into())]
            }
            WindowEvent::HoveredFile(p) => {
                smallvec![Command::parse("window.file-hover")?.with_arg(p.as_path().into())]
            }
            WindowEvent::HoveredFileCancelled => {
                smallvec![Command::parse("window.file-hover-cancel")?]
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                smallvec![Command::parse("window.dpi-change")?.with_arg((*scale_factor).into())]
            }
            WindowEvent::CursorEntered { device_id } => {
                smallvec![Command::parse("window.cursor-entered")?.with_arg((*device_id).into())]
            }
            WindowEvent::CursorLeft { device_id } => {
                smallvec![Command::parse("window.cursor-left")?.with_arg((*device_id).into())]
            }

            // Track real cursor position in the window including window system accel
            // warping, and other such; mostly useful for software mice, but also for
            // picking with a hardware mouse.
            WindowEvent::CursorMoved { position, .. } => {
                smallvec![Command::parse("window.cursor-move")?.with_arg((*position).into())]
            }

            // We need to capture keyboard input both here and below because of web.
            WindowEvent::KeyboardInput {
                input:
                    KeyboardInput {
                        virtual_keycode: virtual_key,
                        scancode,
                        state: key_state,
                        ..
                    },
                ..
            } => {
                // Web backends deliver only Window::KeyboardInput events, so we need to track
                // state here too, but only on wasm.
                *virtual_key = Self::guess_key(*scancode, *virtual_key);
                #[cfg(target_arch = "wasm32")]
                {
                    state
                        .key_states
                        .insert(Key::Physical(*scancode), *key_state);
                }
                if let Some(key_code) = virtual_key {
                    #[cfg(target_arch = "wasm32")]
                    {
                        state.key_states.insert(Key::Virtual(key_code), *key_state);
                    }
                    Self::match_key(Key::Virtual(*key_code), *key_state, bindings, state)?
                } else {
                    smallvec![]
                }
            }

            // Ignore events duplicated by other capture methods.
            WindowEvent::ReceivedCharacter { .. } => smallvec![],
            WindowEvent::MouseInput { .. } => smallvec![],
            WindowEvent::MouseWheel { .. } => smallvec![],

            // Ignore events we don't get on the device.
            WindowEvent::Touch(_) => smallvec![],
            WindowEvent::TouchpadPressure { .. } => smallvec![],
            WindowEvent::AxisMotion { .. } => smallvec![],

            WindowEvent::ModifiersChanged(modifiers_state) => {
                state.modifiers_state = *modifiers_state;
                smallvec![]
            }

            WindowEvent::ThemeChanged(_) => smallvec![],
        })
    }

    fn handle_device_event(
        device_id: &DeviceId,
        event: &mut DeviceEvent,
        bindings: &[Bindings],
        state: &mut BindingState,
    ) -> Fallible<SmallVec<[Command; 8]>> {
        Ok(match event {
            // Device change events
            DeviceEvent::Added => {
                smallvec![Command::parse("device.added")?.with_arg((*device_id).into())]
            }
            DeviceEvent::Removed => {
                smallvec![Command::parse("device.removed")?.with_arg((*device_id).into())]
            }

            // Mouse Motion
            DeviceEvent::MouseMotion { delta } => {
                smallvec![Command::parse("device.mouse-move")?.with_arg((*delta).into())]
            }

            // Mouse Wheel
            DeviceEvent::MouseWheel {
                delta: MouseScrollDelta::LineDelta(x, y),
            } => smallvec![Command::parse("device.mouse-wheel")?.with_arg((*x, *y).into())],
            DeviceEvent::MouseWheel {
                delta: MouseScrollDelta::PixelDelta(s),
            } => smallvec![Command::parse("device.mouse-wheel")?.with_arg((*s).into())],

            // Mouse Button
            DeviceEvent::Button {
                button,
                state: key_state,
            } => {
                state
                    .key_states
                    .insert(Key::MouseButton(*button), *key_state);
                Self::match_key(Key::MouseButton(*button), *key_state, bindings, state)?
            }

            // Match virtual keycodes.
            DeviceEvent::Key(KeyboardInput {
                virtual_keycode: virtual_key,
                scancode,
                state: key_state,
                ..
            }) => {
                // Track key states on the device so that we don't lose our state if the mouse
                // happens to leave the window.
                *virtual_key = Self::guess_key(*scancode, *virtual_key);
                state
                    .key_states
                    .insert(Key::Physical(*scancode), *key_state);
                if let Some(key_code) = virtual_key {
                    state.key_states.insert(Key::Virtual(*key_code), *key_state);
                }
                smallvec![]
            }

            // Duplicate from MouseMotion for some reason?
            DeviceEvent::Motion { .. } => smallvec![],

            // I'm not sure what this does?
            DeviceEvent::Text { .. } => smallvec![],
        })
    }

    fn guess_key(scancode: u32, virtual_key: Option<VirtualKeyCode>) -> Option<VirtualKeyCode> {
        // The X11 driver is using XLookupString rather than XIM or xinput2, so gets modified
        // keys, losing the key code in the process. e.g. a press of the 5 key may return Key5 or
        // None, depending on whether shift is also pressed, because percent is not a physical key.
        let discovered = match scancode {
            1 => Some(VirtualKeyCode::Escape),
            59 => Some(VirtualKeyCode::F1),
            60 => Some(VirtualKeyCode::F2),
            61 => Some(VirtualKeyCode::F3),
            62 => Some(VirtualKeyCode::F4),
            63 => Some(VirtualKeyCode::F5),
            64 => Some(VirtualKeyCode::F6),
            65 => Some(VirtualKeyCode::F7),
            66 => Some(VirtualKeyCode::F8),
            67 => Some(VirtualKeyCode::F9),
            68 => Some(VirtualKeyCode::F10),
            87 => Some(VirtualKeyCode::F11),
            88 => Some(VirtualKeyCode::F12),

            41 => Some(VirtualKeyCode::Grave),
            2 => Some(VirtualKeyCode::Key1),
            3 => Some(VirtualKeyCode::Key2),
            4 => Some(VirtualKeyCode::Key3),
            5 => Some(VirtualKeyCode::Key4),
            6 => Some(VirtualKeyCode::Key5),
            7 => Some(VirtualKeyCode::Key6),
            8 => Some(VirtualKeyCode::Key7),
            9 => Some(VirtualKeyCode::Key8),
            10 => Some(VirtualKeyCode::Key9),
            11 => Some(VirtualKeyCode::Key0),
            12 => Some(VirtualKeyCode::Minus),
            13 => Some(VirtualKeyCode::Equals),
            14 => Some(VirtualKeyCode::Back),

            15 => Some(VirtualKeyCode::Tab),
            16 => Some(VirtualKeyCode::Q),
            17 => Some(VirtualKeyCode::W),
            18 => Some(VirtualKeyCode::E),
            19 => Some(VirtualKeyCode::R),
            20 => Some(VirtualKeyCode::T),
            21 => Some(VirtualKeyCode::Y),
            22 => Some(VirtualKeyCode::U),
            23 => Some(VirtualKeyCode::I),
            24 => Some(VirtualKeyCode::O),
            25 => Some(VirtualKeyCode::P),
            26 => Some(VirtualKeyCode::LBracket),
            27 => Some(VirtualKeyCode::RBracket),
            43 => Some(VirtualKeyCode::Backslash),

            // => Some(VirtualKeyCode::Capital), // ?
            30 => Some(VirtualKeyCode::A),
            31 => Some(VirtualKeyCode::S),
            32 => Some(VirtualKeyCode::D),
            33 => Some(VirtualKeyCode::F),
            34 => Some(VirtualKeyCode::G),
            35 => Some(VirtualKeyCode::H),
            36 => Some(VirtualKeyCode::J),
            37 => Some(VirtualKeyCode::K),
            38 => Some(VirtualKeyCode::L),
            39 => Some(VirtualKeyCode::Semicolon),
            40 => Some(VirtualKeyCode::Apostrophe),
            28 => Some(VirtualKeyCode::Return),

            42 => Some(VirtualKeyCode::LShift),
            44 => Some(VirtualKeyCode::Z),
            45 => Some(VirtualKeyCode::X),
            46 => Some(VirtualKeyCode::C),
            47 => Some(VirtualKeyCode::V),
            48 => Some(VirtualKeyCode::B),
            49 => Some(VirtualKeyCode::N),
            50 => Some(VirtualKeyCode::M),
            51 => Some(VirtualKeyCode::Comma),
            52 => Some(VirtualKeyCode::Period),
            53 => Some(VirtualKeyCode::Slash),
            54 => Some(VirtualKeyCode::RShift),

            29 => Some(VirtualKeyCode::LControl),
            56 => Some(VirtualKeyCode::LAlt),
            57 => Some(VirtualKeyCode::Space),
            100 => Some(VirtualKeyCode::RAlt),
            97 => Some(VirtualKeyCode::RControl),

            102 => Some(VirtualKeyCode::Home),
            103 => Some(VirtualKeyCode::Up),
            104 => Some(VirtualKeyCode::PageUp),
            105 => Some(VirtualKeyCode::Left),
            106 => Some(VirtualKeyCode::Right),
            107 => Some(VirtualKeyCode::End),
            108 => Some(VirtualKeyCode::Down),
            109 => Some(VirtualKeyCode::PageDown),
            110 => Some(VirtualKeyCode::Insert),
            111 => Some(VirtualKeyCode::Delete),

            //=> Some(VirtualKeyCode::),
            _ => None,
        };

        if virtual_key.is_some() {
            debug_assert_eq!(virtual_key, discovered);
            if virtual_key != discovered {
                println!(
                    "Warning: broken scancode map for: {}; got virtual: {:?}; expected: {:?}",
                    scancode, discovered, virtual_key
                );
            }
            return virtual_key;
        }

        discovered
    }

    fn match_key(
        key: Key,
        key_state: ElementState,
        bindings: &[Bindings],
        state: &mut BindingState,
    ) -> Fallible<SmallVec<[Command; 8]>> {
        let mut out = SmallVec::new();
        for bindings in bindings.iter().rev() {
            out.extend(bindings.match_key(key, key_state, state)?);
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
        let mut state = Default::default();

        let cmd = InputSystem::handle_event(
            &mut win_evt(WindowEvent::Resized(physical_size())),
            &binding_list,
            &mut state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "resize");
        assert_relative_eq!(cmd.displacement(0)?.0, 8f64);
        assert_relative_eq!(cmd.displacement(0)?.1, 9f64);

        let cmd = InputSystem::handle_event(
            &mut win_evt(WindowEvent::Destroyed),
            &binding_list,
            &mut state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "destroy");

        let cmd = InputSystem::handle_event(
            &mut win_evt(WindowEvent::CloseRequested),
            &binding_list,
            &mut state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "close");

        let cmd = InputSystem::handle_event(
            &mut win_evt(WindowEvent::DroppedFile(path())),
            &binding_list,
            &mut state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "file-drop");
        assert_eq!(cmd.path(0)?, path());

        let cmd = InputSystem::handle_event(
            &mut win_evt(WindowEvent::Focused(true)),
            &binding_list,
            &mut state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "focus");
        assert!(cmd.boolean(0)?);

        let cmd =
            InputSystem::handle_event(&mut dev_evt(DeviceEvent::Added), &binding_list, &mut state)?
                .first()
                .unwrap()
                .to_owned();
        assert_eq!(cmd.command(), "added");
        let cmd = InputSystem::handle_event(
            &mut dev_evt(DeviceEvent::Removed),
            &binding_list,
            &mut state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "removed");

        let cmd = InputSystem::handle_event(
            &mut dev_evt(DeviceEvent::MouseMotion { delta: (8., 9.) }),
            &binding_list,
            &mut state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "mouse-move");
        assert_relative_eq!(cmd.displacement(0)?.0, 8f64);
        assert_relative_eq!(cmd.displacement(0)?.1, 9f64);

        let cmd = InputSystem::handle_event(
            &mut dev_evt(DeviceEvent::MouseWheel {
                delta: MouseScrollDelta::LineDelta(8., 9.),
            }),
            &binding_list,
            &mut state,
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
        let mut state = Default::default();

        // FPS forward.
        let cmd = InputSystem::handle_event(
            &mut dev_evt(DeviceEvent::Key(vkey(VirtualKeyCode::W, true))),
            &binding_list,
            &mut state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "+move-forward");
        let cmd = InputSystem::handle_event(
            &mut dev_evt(DeviceEvent::Key(vkey(VirtualKeyCode::W, false))),
            &binding_list,
            &mut state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "-move-forward");

        // Mouse Button + find fire before click.
        let cmd = InputSystem::handle_event(
            &mut dev_evt(DeviceEvent::Button {
                button: 0,
                state: ElementState::Pressed,
            }),
            &binding_list,
            &mut state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "fire");
        let cmd = InputSystem::handle_event(
            &mut dev_evt(DeviceEvent::Button {
                button: 0,
                state: ElementState::Released,
            }),
            &binding_list,
            &mut state,
        )?;
        assert!(cmd.is_empty());

        // Multiple buttons + found shift from LShfit + find eject instead of exit
        let cmd = InputSystem::handle_event(
            &mut dev_evt(DeviceEvent::Key(vkey(VirtualKeyCode::LShift, true))),
            &binding_list,
            &mut state,
        )?;
        assert!(cmd.is_empty());
        let cmd = InputSystem::handle_event(
            &mut dev_evt(DeviceEvent::Key(vkey(VirtualKeyCode::E, true))),
            &binding_list,
            &mut state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "eject");

        // Let off e, drop fps, then hit again and get the other command
        let cmd = InputSystem::handle_event(
            &mut dev_evt(DeviceEvent::Key(vkey(VirtualKeyCode::E, false))),
            &binding_list,
            &mut state,
        )?;
        assert!(cmd.is_empty());
        binding_list.pop();
        let cmd = InputSystem::handle_event(
            &mut dev_evt(DeviceEvent::Key(vkey(VirtualKeyCode::E, true))),
            &binding_list,
            &mut state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "exit");
        let cmd = InputSystem::handle_event(
            &mut dev_evt(DeviceEvent::Key(vkey(VirtualKeyCode::LShift, false))),
            &binding_list,
            &mut state,
        )?;
        assert!(cmd.is_empty());

        // Push on a new command set and ensure that it masks.
        let flight = Bindings::new("flight").bind("player.+pickle", "mouse0")?;
        binding_list.push(flight);

        let cmd = InputSystem::handle_event(
            &mut dev_evt(DeviceEvent::Button {
                button: 0,
                state: ElementState::Pressed,
            }),
            &binding_list,
            &mut state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "+pickle");
        let cmd = InputSystem::handle_event(
            &mut dev_evt(DeviceEvent::Button {
                button: 0,
                state: ElementState::Released,
            }),
            &binding_list,
            &mut state,
        )?
        .first()
        .unwrap()
        .to_owned();
        assert_eq!(cmd.command(), "-pickle");

        Ok(())
    }
}
