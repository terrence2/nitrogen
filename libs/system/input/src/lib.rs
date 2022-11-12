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

pub use generic::{InputEvent, MouseAxis, SystemEvent};
pub use winit::event::{ButtonId, ElementState, ModifiersState, VirtualKeyCode};

use anyhow::{bail, Result};
use bevy_ecs::prelude::*;
use gilrs::{Button as GilButton, Event as GilEvent, GilrsBuilder};
use log::warn;
use nitrous::{inject_nitrous_resource, method, NitrousResource};
use parking_lot::Mutex;
use runtime::{Extension, Runtime};
use smallvec::SmallVec;
use std::{
    collections::HashMap,
    fmt::Debug,
    hash::Hash,
    str::FromStr,
    sync::{
        mpsc::{channel, Receiver, TryRecvError},
        Arc,
    },
    time::Instant,
};
use winit::{
    event::{
        DeviceEvent, DeviceId, Event, KeyboardInput, MouseScrollDelta, StartCause, WindowEvent,
    },
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy},
    window::WindowBuilder,
};

pub type InputEventVec = SmallVec<[InputEvent; 8]>;
pub type SystemEventVec = SmallVec<[SystemEvent; 8]>;

pub fn test_make_input_events(mut events: Vec<InputEvent>) -> InputEventVec {
    let mut out = InputEventVec::new();
    for evt in events.drain(..) {
        out.push(evt);
    }
    out
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum InputFocus {
    Menu,
    GameMenu,
    Game,
    Edit,
}

impl InputFocus {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Menu => "menu",
            Self::GameMenu => "game_menu",
            Self::Game => "game",
            Self::Edit => "edit",
        }
    }
}

impl FromStr for InputFocus {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "menu" => Self::Menu,
            "game_menu" => Self::GameMenu,
            "game" => Self::Game,
            "edit" => Self::Edit,
            _ => bail!("not an input focus"),
        })
    }
}

impl ToString for InputFocus {
    fn to_string(&self) -> String {
        self.name().to_owned()
    }
}

impl Default for InputFocus {
    fn default() -> Self {
        Self::Game
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum InputTargetSimStep {
    ToggleTerminal,
}

/// Controls terminal visibility and key access. Should be installed before Terminal and anything
/// that processes input, like EventMapper and WidgetBuffer.
#[derive(NitrousResource, Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct InputTarget {
    terminal_active: bool,
    input_focus: InputFocus,
}

impl Extension for InputTarget {
    fn init(runtime: &mut Runtime) -> Result<()> {
        runtime.insert_named_resource("input_target", InputTarget::default());
        runtime.add_input_system(
            Self::sys_handle_toggle_terminal.label(InputTargetSimStep::ToggleTerminal),
        );
        Ok(())
    }
}

impl Default for InputTarget {
    fn default() -> Self {
        Self {
            terminal_active: false,
            input_focus: InputFocus::Edit,
        }
    }
}

#[inject_nitrous_resource]
impl InputTarget {
    // Handle terminal separately from bindings in case things get messed up
    pub fn sys_handle_toggle_terminal(events: Res<InputEventVec>, mut target: ResMut<InputTarget>) {
        if events
            .iter()
            .any(|event| target.is_toggle_terminal_event(event))
        {
            target.toggle_terminal();
        }
    }

    fn is_toggle_terminal_event(&self, event: &InputEvent) -> bool {
        if let InputEvent::KeyboardKey {
            virtual_keycode,
            press_state,
            modifiers_state,
            ..
        } = event
        {
            if self.terminal_active && *virtual_keycode == VirtualKeyCode::Escape
                || *virtual_keycode == VirtualKeyCode::Grave
                    && *modifiers_state == ModifiersState::CTRL
                    && *press_state == ElementState::Pressed
            {
                return true;
            }
        }
        false
    }

    #[method]
    pub fn terminal_active(&self) -> bool {
        self.terminal_active
    }

    #[method]
    pub fn set_terminal_active(&mut self, active: bool) {
        self.terminal_active = active;
    }

    #[method]
    pub fn toggle_terminal(&mut self) {
        self.terminal_active = !self.terminal_active;
    }

    pub fn focus(&self) -> String {
        self.input_focus.to_string()
    }

    pub fn input_focus(&self) -> InputFocus {
        self.input_focus
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MetaEvent {
    Stop,
}

#[derive(Debug, Default)]
pub struct GlobalInputState {
    modifiers_state: ModifiersState,
    cursor_in_window: HashMap<DeviceId, bool>,
    window_focused: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum InputStep {
    ReadInput,
    ReadSystem,
}

pub struct InputController {
    proxy: EventLoopProxy<MetaEvent>,
    input_event_source: Receiver<InputEvent>,
    system_event_source: Receiver<SystemEvent>,
}

impl InputController {
    fn new(
        proxy: EventLoopProxy<MetaEvent>,
        input_event_source: Receiver<InputEvent>,
        system_event_source: Receiver<SystemEvent>,
        runtime: &mut Runtime,
    ) -> Arc<Mutex<Self>> {
        let input_controller = Arc::new(Mutex::new(Self {
            proxy,
            input_event_source,
            system_event_source,
        }));

        // Hack so that our window APIs work properly from the get-go.
        // TODO: is this needed (or even working) on all platforms?
        #[cfg(unix)]
        input_controller.lock().wait_for_window_configuration().ok();

        runtime.insert_resource(input_controller.clone());
        runtime.insert_resource(InputEventVec::new());
        runtime.insert_resource(SystemEventVec::new());

        runtime.add_input_system(Self::sys_read_input_events.label(InputStep::ReadInput));
        runtime.add_frame_system(Self::sys_read_system_events.label(InputStep::ReadSystem));

        input_controller
    }

    pub fn sys_read_input_events(
        input_controller: Res<Arc<Mutex<InputController>>>,
        mut input_events: ResMut<InputEventVec>,
    ) {
        // Note: if we are stopping, the queue might have shut down, in which case we don't
        // really care about the output anymore.
        *input_events = if let Ok(events) = input_controller.lock().poll_input_events() {
            events
        } else {
            InputEventVec::new()
        };
    }

    pub fn sys_read_system_events(
        input_controller: Res<Arc<Mutex<InputController>>>,
        mut system_events: ResMut<SystemEventVec>,
    ) {
        // Note: if we are stopping, the queue might have shut down, in which case we don't
        // really care about the output anymore.
        *system_events = if let Ok(events) = input_controller.lock().poll_system_events() {
            events
        } else {
            SystemEventVec::new()
        };
    }

    pub fn for_test() -> Result<Runtime> {
        use winit::{platform::run_return::EventLoopExtRunReturn, window::Window};

        #[cfg(unix)]
        use winit::platform::unix::EventLoopBuilderExtUnix;
        #[cfg(windows)]
        use winit::platform::windows::EventLoopBuilderExtWindows;

        let mut event_loop = EventLoopBuilder::<MetaEvent>::with_user_event()
            .with_any_thread(true)
            .build();
        // let mut event_loop = EventLoop::<MetaEvent>::new_any_thread();
        let os_window = Window::new(&event_loop).unwrap();
        let mut have_config = 0;
        while have_config <= 0 && have_config > -100 {
            event_loop.run_return(|evt, _tgt, flow| {
                if matches!(
                    evt,
                    Event::WindowEvent {
                        event: WindowEvent::Resized(_),
                        ..
                    }
                ) {
                    have_config = 1;
                } else {
                    have_config -= 1;
                }
                *flow = winit::event_loop::ControlFlow::Exit;
            });
        }
        let mut runtime = Runtime::default();
        let (_, rx_input_event) = channel();
        let (_, rx_system_event) = channel();
        InputController::new(
            event_loop.create_proxy(),
            rx_input_event,
            rx_system_event,
            &mut runtime,
        );
        let event_loop = Arc::new(Mutex::new(event_loop));
        runtime.insert_non_send_resource(event_loop);
        os_window.focus_window();
        runtime.insert_resource(os_window);
        Ok(runtime)
    }

    pub fn quit(&self) -> Result<()> {
        self.proxy.send_event(MetaEvent::Stop)?;
        Ok(())
    }

    /// This is deeply cursed. Winit doesn't know our window size until X tells us.
    /// FIXME: do we need this on all platforms?
    pub fn wait_for_window_configuration(&mut self) -> Result<()> {
        let start = Instant::now();
        loop {
            for evt in self.poll_system_events()? {
                if matches!(evt, SystemEvent::WindowResized { .. }) {
                    warn!("Waited {:?} for size event", start.elapsed());
                    return Ok(());
                }
            }
        }
    }

    pub fn poll_input_events(&self) -> Result<InputEventVec> {
        let mut out = SmallVec::new();
        let mut maybe_event_input = self.input_event_source.try_recv();
        while maybe_event_input.is_ok() {
            let event_input = maybe_event_input?;
            out.push(event_input);
            maybe_event_input = self.input_event_source.try_recv();
        }
        match maybe_event_input.err().unwrap() {
            TryRecvError::Empty => Ok(out),
            TryRecvError::Disconnected => bail!("input system stopped"),
        }
    }

    pub fn poll_system_events(&self) -> Result<SystemEventVec> {
        let mut out = SmallVec::new();
        let mut maybe_system_event = self.system_event_source.try_recv();
        while maybe_system_event.is_ok() {
            let event_input = maybe_system_event?;
            out.push(event_input);
            maybe_system_event = self.system_event_source.try_recv();
        }
        match maybe_system_event.err().unwrap() {
            TryRecvError::Empty => Ok(out),
            TryRecvError::Disconnected => bail!("input system stopped"),
        }
    }
}

#[derive(Debug)]
pub struct InputSystem;

impl InputSystem {
    pub fn make_event_loop() -> EventLoop<MetaEvent> {
        EventLoopBuilder::<MetaEvent>::with_user_event().build()
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
        let mut input_state: GlobalInputState = Default::default();
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
    pub fn run_forever<O, M>(
        opt: O,
        window_builder: WindowBuilder,
        mut window_main: M,
    ) -> Result<()>
    where
        O: Clone + Send + Sync + 'static,
        M: 'static + Send + FnMut(Runtime) -> Result<()>,
    {
        let event_loop = EventLoopBuilder::<MetaEvent>::with_user_event().build();
        let window = window_builder.build(&event_loop)?;
        let (tx_input_event, rx_input_event) = channel();
        let (tx_system_event, rx_system_event) = channel();
        let event_loop_proxy = event_loop.create_proxy();

        // Spawn the game thread.
        std::thread::spawn(move || {
            let mut runtime = Runtime::default();
            runtime.insert_resource(opt);
            runtime.insert_resource(window);

            let input_controller = InputController::new(
                event_loop_proxy,
                rx_input_event,
                rx_system_event,
                &mut runtime,
            );

            if let Err(e) = window_main(runtime) {
                println!("Error: {:?}", e);
            }
            input_controller.lock().quit().ok();
        });

        // Spawn a thread to play with joysticks
        std::thread::spawn(move || {
            let mut gilrs = GilrsBuilder::new()
                .add_included_mappings(true)
                .add_env_mappings(true)
                .build()
                .expect("Gilrs load");
            let mut _active_gamepad = None;
            for (id, gamepad) in gilrs.gamepads() {
                println!("{}: {} is {:?}", id, gamepad.name(), gamepad.power_info());
                println!("MAPPING: {:?}", gamepad.mapping_source());
                println!("STATE: {:?}", gamepad.state());
                println!("BUTTON: {:?}", gamepad.button_code(GilButton::LeftTrigger));
            }
            while let Some(GilEvent { id, event, time }) = gilrs.next_event() {
                println!("{:?} New event from {}: {:?}", time, id, event);
                _active_gamepad = Some(id);
            }
        });

        // Hijack the main thread.
        let mut input_events = Vec::new();
        let mut system_events = Vec::new();
        let mut input_state: GlobalInputState = Default::default();
        event_loop.run(move |event, _target, control_flow| {
            *control_flow = ControlFlow::Wait;
            if event == Event::UserEvent(MetaEvent::Stop) {
                *control_flow = ControlFlow::Exit;
                return;
            }

            Self::wrap_event(
                &event,
                &mut input_state,
                &mut input_events,
                &mut system_events,
            );
            for evt in input_events.drain(..) {
                if let Err(e) = tx_input_event.send(evt) {
                    println!("Game loop hung up ({}), exiting...", e);
                    *control_flow = ControlFlow::Exit;
                    return;
                }
            }
            for evt in system_events.drain(..) {
                if let Err(e) = tx_system_event.send(evt) {
                    println!("Game loop hung up ({}), exiting...", e);
                    *control_flow = ControlFlow::Exit;
                    return;
                }
            }
        });
    }

    fn wrap_event(
        e: &Event<MetaEvent>,
        input_state: &mut GlobalInputState,
        input_events: &mut Vec<InputEvent>,
        system_events: &mut Vec<SystemEvent>,
    ) {
        match e {
            Event::WindowEvent { event, .. } => {
                Self::wrap_window_event(event, input_state, input_events, system_events)
            }
            Event::DeviceEvent { device_id, event } => {
                Self::wrap_device_event(device_id, event, input_state, input_events)
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

    // Uh, clippy? It's mutable: we need the vec-ness?
    #[allow(clippy::ptr_arg)]
    fn wrap_window_event(
        event: &WindowEvent,
        input_state: &mut GlobalInputState,
        out: &mut Vec<InputEvent>,
        system_events: &mut Vec<SystemEvent>,
    ) {
        match event {
            WindowEvent::Ime(_) => {}
            WindowEvent::Occluded(_) => {}
            WindowEvent::Resized(s) => {
                system_events.push(SystemEvent::WindowResized {
                    width: s.width,
                    height: s.height,
                });
            }
            WindowEvent::Moved(_) => {}
            WindowEvent::Destroyed => {
                system_events.push(SystemEvent::Quit);
            }
            WindowEvent::CloseRequested => {
                system_events.push(SystemEvent::Quit);
            }
            WindowEvent::Focused(b) => {
                input_state.window_focused = *b;
            }
            WindowEvent::DroppedFile(_) => {}
            WindowEvent::HoveredFile(_) => {}
            WindowEvent::HoveredFileCancelled => {}
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                system_events.push(SystemEvent::ScaleFactorChanged {
                    scale: *scale_factor,
                });
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
                out.push(InputEvent::CursorMove {
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
                    out.push(InputEvent::KeyboardKey {
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
        input_state: &mut GlobalInputState,
        out: &mut Vec<InputEvent>,
    ) {
        match event {
            // Device change events
            DeviceEvent::Added => {
                out.push(InputEvent::DeviceAdded { dummy: 0 });
            }
            DeviceEvent::Removed => {
                out.push(InputEvent::DeviceRemoved { dummy: 0 });
            }

            // Mouse Motion: unfiltered, arbitrary units
            DeviceEvent::MouseMotion { delta: (dx, dy) } => {
                let in_window = *input_state
                    .cursor_in_window
                    .get(device_id)
                    .unwrap_or(&false);
                out.push(InputEvent::MouseMotion {
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
                let horizontal_delta = *dh as f64;
                let vertical_delta = *dv as f64;
                #[cfg(windows)]
                let vertical_delta = -vertical_delta;
                let in_window = *input_state
                    .cursor_in_window
                    .get(device_id)
                    .unwrap_or(&false);
                out.push(InputEvent::MouseWheel {
                    horizontal_delta,
                    vertical_delta,
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
                out.push(InputEvent::MouseWheel {
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
                out.push(InputEvent::MouseButton {
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
                        out.push(InputEvent::KeyboardKey {
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
                out.push(InputEvent::JoystickAxis {
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
            (false, 3) => Some(VirtualKeyCode::Key2),
            (true, 3) => Some(VirtualKeyCode::At),
            (_, 4) => Some(VirtualKeyCode::Key3),
            (_, 5) => Some(VirtualKeyCode::Key4),
            (_, 6) => Some(VirtualKeyCode::Key5),
            (_, 7) => Some(VirtualKeyCode::Key6),
            (_, 8) => Some(VirtualKeyCode::Key7),
            (false, 9) => Some(VirtualKeyCode::Key8),
            (true, 9) => Some(VirtualKeyCode::Asterisk),
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
            (false, 39) => Some(VirtualKeyCode::Semicolon),
            (true, 39) => Some(VirtualKeyCode::Colon),
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

    pub fn code_to_char(virtual_keycode: &VirtualKeyCode) -> (Option<char>, Option<char>) {
        match virtual_keycode {
            VirtualKeyCode::Numpad0 => (Some('0'), None),
            VirtualKeyCode::Numpad1 => (Some('1'), None),
            VirtualKeyCode::Numpad2 => (Some('2'), None),
            VirtualKeyCode::Numpad3 => (Some('3'), None),
            VirtualKeyCode::Numpad4 => (Some('4'), None),
            VirtualKeyCode::Numpad5 => (Some('5'), None),
            VirtualKeyCode::Numpad6 => (Some('6'), None),
            VirtualKeyCode::Numpad7 => (Some('7'), None),
            VirtualKeyCode::Numpad8 => (Some('8'), None),
            VirtualKeyCode::Numpad9 => (Some('9'), None),
            VirtualKeyCode::Key0 => (Some('0'), Some(')')),
            VirtualKeyCode::Key1 => (Some('1'), Some('!')),
            VirtualKeyCode::Key2 => (Some('2'), Some('@')),
            VirtualKeyCode::Key3 => (Some('3'), Some('#')),
            VirtualKeyCode::Key4 => (Some('4'), Some('$')),
            VirtualKeyCode::Key5 => (Some('5'), Some('%')),
            VirtualKeyCode::Key6 => (Some('6'), Some('^')),
            VirtualKeyCode::Key7 => (Some('7'), Some('&')),
            VirtualKeyCode::Key8 => (Some('8'), Some('*')),
            VirtualKeyCode::Key9 => (Some('9'), Some('(')),
            VirtualKeyCode::A => (Some('a'), Some('A')),
            VirtualKeyCode::B => (Some('b'), Some('B')),
            VirtualKeyCode::C => (Some('c'), Some('C')),
            VirtualKeyCode::D => (Some('d'), Some('D')),
            VirtualKeyCode::E => (Some('e'), Some('E')),
            VirtualKeyCode::F => (Some('f'), Some('F')),
            VirtualKeyCode::G => (Some('g'), Some('G')),
            VirtualKeyCode::H => (Some('h'), Some('H')),
            VirtualKeyCode::I => (Some('i'), Some('I')),
            VirtualKeyCode::J => (Some('j'), Some('J')),
            VirtualKeyCode::K => (Some('k'), Some('K')),
            VirtualKeyCode::L => (Some('l'), Some('L')),
            VirtualKeyCode::M => (Some('m'), Some('M')),
            VirtualKeyCode::N => (Some('n'), Some('N')),
            VirtualKeyCode::O => (Some('o'), Some('O')),
            VirtualKeyCode::P => (Some('p'), Some('P')),
            VirtualKeyCode::Q => (Some('q'), Some('Q')),
            VirtualKeyCode::R => (Some('r'), Some('R')),
            VirtualKeyCode::S => (Some('s'), Some('S')),
            VirtualKeyCode::T => (Some('t'), Some('T')),
            VirtualKeyCode::U => (Some('u'), Some('U')),
            VirtualKeyCode::V => (Some('v'), Some('V')),
            VirtualKeyCode::W => (Some('w'), Some('W')),
            VirtualKeyCode::X => (Some('x'), Some('X')),
            VirtualKeyCode::Y => (Some('y'), Some('Y')),
            VirtualKeyCode::Z => (Some('z'), Some('Z')),
            VirtualKeyCode::Space => (Some(' '), None),
            VirtualKeyCode::Caret => (Some('^'), None),
            VirtualKeyCode::NumpadAdd => (Some('+'), None),
            VirtualKeyCode::NumpadDivide => (Some('/'), None),
            VirtualKeyCode::NumpadDecimal => (Some('.'), None),
            VirtualKeyCode::NumpadComma => (Some(','), None),
            VirtualKeyCode::NumpadEquals => (Some('='), None),
            VirtualKeyCode::NumpadMultiply => (Some('*'), None),
            VirtualKeyCode::NumpadSubtract => (Some('-'), None),
            VirtualKeyCode::Apostrophe => (Some('\''), Some('"')),
            VirtualKeyCode::Asterisk => (Some('*'), None),
            VirtualKeyCode::At => (Some('@'), None),
            VirtualKeyCode::Backslash => (Some('\\'), Some('|')),
            VirtualKeyCode::Colon => (Some(':'), None),
            VirtualKeyCode::Comma => (Some(','), Some('<')),
            VirtualKeyCode::Equals => (Some('='), Some('+')),
            VirtualKeyCode::Grave => (Some('`'), Some('~')),
            VirtualKeyCode::LBracket => (Some('['), Some('{')),
            VirtualKeyCode::Minus => (Some('-'), Some('_')),
            VirtualKeyCode::Period => (Some('.'), Some('>')),
            VirtualKeyCode::Plus => (Some('+'), None),
            VirtualKeyCode::RBracket => (Some(']'), Some('}')),
            VirtualKeyCode::Semicolon => (Some(';'), Some(':')),
            VirtualKeyCode::Slash => (Some('/'), Some('?')),

            // Move to top level?
            // Tab,
            // Copy,
            // Paste,
            // Cut,
            _ => (None, None),
        }
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
        let mut input_state: GlobalInputState = Default::default();
        let psz = physical_size();

        let mut evts = Vec::new();
        let mut syss = Vec::new();
        InputSystem::wrap_event(
            &win_evt(WindowEvent::Resized(psz)),
            &mut input_state,
            &mut evts,
            &mut syss,
        );
        assert!(matches!(syss[0], SystemEvent::WindowResized { .. }));

        let mut evts = Vec::new();
        let mut syss = Vec::new();
        InputSystem::wrap_event(
            &win_evt(WindowEvent::Destroyed),
            &mut input_state,
            &mut evts,
            &mut syss,
        );
        assert!(matches!(syss[0], SystemEvent::Quit));

        let mut evts = Vec::new();
        let mut syss = Vec::new();
        InputSystem::wrap_event(
            &win_evt(WindowEvent::CloseRequested),
            &mut input_state,
            &mut evts,
            &mut syss,
        );
        assert!(matches!(syss[0], SystemEvent::Quit));

        let mut evts = Vec::new();
        let mut syss = Vec::new();
        InputSystem::wrap_event(
            &win_evt(WindowEvent::DroppedFile(path())),
            &mut input_state,
            &mut evts,
            &mut syss,
        );
        assert!(evts.is_empty());

        let mut evts = Vec::new();
        let mut syss = Vec::new();
        InputSystem::wrap_event(
            &win_evt(WindowEvent::Focused(true)),
            &mut input_state,
            &mut evts,
            &mut syss,
        );
        assert!(evts.is_empty());

        let mut evts = Vec::new();
        let mut syss = Vec::new();
        InputSystem::wrap_event(
            &dev_evt(DeviceEvent::Added),
            &mut input_state,
            &mut evts,
            &mut syss,
        );
        assert!(matches!(evts[0], InputEvent::DeviceAdded { .. }));

        let mut evts = Vec::new();
        let mut syss = Vec::new();
        InputSystem::wrap_event(
            &dev_evt(DeviceEvent::Removed),
            &mut input_state,
            &mut evts,
            &mut syss,
        );
        assert!(matches!(evts[0], InputEvent::DeviceRemoved { .. }));

        let mut evts = Vec::new();
        let mut syss = Vec::new();
        InputSystem::wrap_event(
            &dev_evt(DeviceEvent::MouseMotion { delta: (8., 9.) }),
            &mut input_state,
            &mut evts,
            &mut syss,
        );
        assert!(matches!(evts[0], InputEvent::MouseMotion { .. }));

        let mut evts = Vec::new();
        let mut syss = Vec::new();
        InputSystem::wrap_event(
            &dev_evt(DeviceEvent::MouseWheel {
                delta: MouseScrollDelta::LineDelta(8., 9.),
            }),
            &mut input_state,
            &mut evts,
            &mut syss,
        );
        assert!(matches!(evts[0], InputEvent::MouseWheel { .. }));
    }
}
