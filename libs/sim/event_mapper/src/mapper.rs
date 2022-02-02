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
use crate::{
    bindings::Bindings,
    input::{Input, InputSet},
};
use anyhow::{bail, Result};
use bevy_ecs::prelude::*;
use input::{ElementState, InputEvent, InputEventVec, InputFocus, ModifiersState};
use nitrous::{inject_nitrous_resource, method, NitrousResource, Value};
use ordered_float::OrderedFloat;
use runtime::{Extension, Runtime, ScriptHerder, SimStage};
use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    marker::PhantomData,
    str::FromStr,
};

#[derive(Debug, Default)]
pub struct State {
    pub modifiers_state: ModifiersState,
    pub input_states: HashMap<Input, ElementState>,
    pub active_chords: HashSet<InputSet>,
}

#[derive(Default, Debug, NitrousResource)]
pub struct EventMapper<T>
where
    T: InputFocus,
    <T as FromStr>::Err: Debug,
{
    bindings: HashMap<T, Bindings>,
    state: State,
    phantom_data: PhantomData<T>,
}

impl<T> Extension for EventMapper<T>
where
    T: InputFocus,
    <T as FromStr>::Err: Debug,
{
    fn init(runtime: &mut Runtime) -> Result<()> {
        runtime.insert_named_resource("bindings", EventMapper::<T>::new());
        runtime
            .sim_stage_mut(SimStage::HandleInput)
            .add_system(Self::sys_handle_input_events);
        Ok(())
    }
}

#[inject_nitrous_resource]
impl<T> EventMapper<T>
where
    T: InputFocus,
    <T as FromStr>::Err: Debug,
{
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
            state: State::default(),
            phantom_data: Default::default(),
        }
    }

    pub fn bind_in_focus(&mut self, focus: T, event_name: &str, script_raw: &str) -> Result<()> {
        let bindings = self
            .bindings
            .entry(focus)
            .or_insert(Bindings::new(focus.name()));
        bindings.bind(event_name, script_raw)?;
        Ok(())
    }

    #[method]
    pub fn bind_in(&mut self, focus_name: &str, event_name: &str, script_raw: &str) -> Result<()> {
        let focus = match T::from_str(focus_name) {
            Ok(focus) => focus,
            Err(e) => bail!("{:?}", e),
        };
        self.bind_in_focus(focus, event_name, script_raw)
    }

    #[method]
    pub fn bind(&mut self, event_name: &str, script_raw: &str) -> Result<()> {
        self.bind_in_focus(T::default(), event_name, script_raw)
    }

    pub fn sys_handle_input_events(
        events: Res<InputEventVec>,
        input_focus: Res<T>,
        mut herder: ResMut<ScriptHerder>,
        mut mapper: ResMut<EventMapper<T>>,
    ) {
        mapper
            .handle_events(&events, *input_focus, &mut herder)
            .expect("EventMapper::handle_events");
    }

    pub fn handle_events(
        &mut self,
        events: &[InputEvent],
        focus: T,
        herder: &mut ScriptHerder,
    ) -> Result<()> {
        for event in events {
            self.handle_event(event, focus, herder)?;
        }
        Ok(())
    }

    fn handle_event(
        &mut self,
        event: &InputEvent,
        focus: T,
        herder: &mut ScriptHerder,
    ) -> Result<()> {
        let input = Input::from_event(event);
        if input.is_none() {
            return Ok(());
        }
        let input = input.unwrap();

        let mut variables = HashMap::with_capacity(8);
        variables.insert("window_focused", Value::Boolean(event.is_window_focused()));

        if let Some(press_state) = event.press_state() {
            self.state.input_states.insert(input, press_state);
            // Note: pressed variable is set later, since we need to disable masked input sets.
        }

        if let Some(modifiers_state) = event.modifiers_state() {
            self.state.modifiers_state = modifiers_state;
            variables.insert("shift_pressed", Value::Boolean(modifiers_state.shift()));
            variables.insert("alt_pressed", Value::Boolean(modifiers_state.alt()));
            variables.insert("ctrl_pressed", Value::Boolean(modifiers_state.ctrl()));
            variables.insert("logo_pressed", Value::Boolean(modifiers_state.logo()));
        }

        // Break *after* maintaining state.
        if focus.is_terminal_focused() {
            return Ok(());
        }

        // Collect variables to inject.
        match event {
            InputEvent::MouseMotion {
                dx, dy, in_window, ..
            } => {
                variables.insert("dx", Value::Float(OrderedFloat(*dx)));
                variables.insert("dy", Value::Float(OrderedFloat(*dy)));
                variables.insert("in_window", Value::Boolean(*in_window));
            }
            InputEvent::MouseWheel {
                horizontal_delta,
                vertical_delta,
                in_window,
                ..
            } => {
                variables.insert(
                    "horizontal_delta",
                    Value::Float(OrderedFloat(*horizontal_delta)),
                );
                variables.insert(
                    "vertical_delta",
                    Value::Float(OrderedFloat(*vertical_delta)),
                );
                variables.insert("in_window", Value::Boolean(*in_window));
            }
            InputEvent::DeviceAdded { dummy } => {
                variables.insert("device_id", Value::Integer(*dummy as i64));
            }
            InputEvent::DeviceRemoved { dummy } => {
                variables.insert("device_id", Value::Integer(*dummy as i64));
            }
            // FIXME: set variables for button state, key state, joy state, etc
            _ => {}
        }

        let locals = variables.into();
        for bindings in self.bindings.values() {
            bindings.match_input(input, event.press_state(), &mut self.state, &locals, herder)?
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use input::{
        test_make_input_events as mkinp, DemoFocus, InputEvent, ModifiersState, VirtualKeyCode,
        VirtualKeyCode as VKC,
    };
    use nitrous::{inject_nitrous_resource, method, NitrousResource};
    use runtime::Runtime;

    #[derive(Debug, Default, NitrousResource)]
    struct Player {
        walking: bool,
        running: bool,
    }

    #[inject_nitrous_resource]
    impl Player {
        #[method]
        fn walk(&mut self, pressed: bool) {
            self.walking = pressed;
        }

        #[method]
        fn run(&mut self, pressed: bool) {
            self.running = pressed;
        }
    }

    fn press(key: VirtualKeyCode, ms: &mut ModifiersState) -> InputEvent {
        match key {
            VKC::LShift | VKC::RShift => *ms |= ModifiersState::SHIFT,
            VKC::LAlt | VKC::RAlt => *ms |= ModifiersState::ALT,
            VKC::LControl | VKC::RControl => *ms |= ModifiersState::CTRL,
            VKC::LWin | VKC::RWin => *ms |= ModifiersState::LOGO,
            _ => {}
        }
        InputEvent::KeyboardKey {
            scancode: 0,
            virtual_keycode: key,
            press_state: ElementState::Pressed,
            modifiers_state: *ms,
            window_focused: true,
        }
    }

    fn release(key: VirtualKeyCode, ms: &mut ModifiersState) -> InputEvent {
        match key {
            VKC::LShift | VKC::RShift => ms.remove(ModifiersState::SHIFT),
            VKC::LAlt | VKC::RAlt => ms.remove(ModifiersState::ALT),
            VKC::LControl | VKC::RControl => ms.remove(ModifiersState::CTRL),
            VKC::LWin | VKC::RWin => ms.remove(ModifiersState::LOGO),
            _ => {}
        }
        InputEvent::KeyboardKey {
            scancode: 0,
            virtual_keycode: key,
            press_state: ElementState::Released,
            modifiers_state: *ms,
            window_focused: true,
        }
    }

    fn prepare() -> Result<Runtime> {
        let mut runtime = Runtime::default();
        runtime
            .insert_resource(DemoFocus::default())
            .insert_named_resource("player", Player::default())
            .load_extension::<EventMapper<DemoFocus>>()?;
        runtime.resource_mut::<ScriptHerder>().run_string(
            r#"
                bindings.bind("w", "player.walk(pressed)");
                bindings.bind("shift+w", "player.run(pressed)");
            "#,
        )?;
        runtime.run_startup();
        Ok(runtime)
    }

    #[test]
    fn test_basic() -> Result<()> {
        let mut runtime = prepare()?;
        let mut state = ModifiersState::empty();
        let ms = &mut state;

        runtime.insert_resource(mkinp(vec![press(VKC::W, ms)]));
        runtime.run_sim_once();
        assert_eq!(runtime.resource::<Player>().walking, true);
        runtime.insert_resource(mkinp(vec![release(VKC::W, ms)]));
        runtime.run_sim_once();
        assert_eq!(runtime.resource::<Player>().walking, false);
        runtime.insert_resource(mkinp(vec![press(VKC::LShift, ms), press(VKC::W, ms)]));
        runtime.run_sim_once();
        assert_eq!(runtime.resource::<Player>().walking, false);
        assert_eq!(runtime.resource::<Player>().running, true);
        runtime.insert_resource(mkinp(vec![release(VKC::LShift, ms), release(VKC::W, ms)]));
        runtime.run_sim_once();
        assert_eq!(runtime.resource::<Player>().walking, false);
        assert_eq!(runtime.resource::<Player>().running, false);

        Ok(())
    }

    #[test]
    fn test_modifier_planes() -> Result<()> {
        let mut runtime = prepare()?;
        let mut state = ModifiersState::empty();
        let ms = &mut state;

        // modifier planes should mask pressed keys
        runtime.insert_resource(mkinp(vec![press(VKC::W, ms)]));
        runtime.run_sim_once();
        assert_eq!(runtime.resource::<Player>().walking, true);
        assert_eq!(runtime.resource::<Player>().running, false);
        runtime.insert_resource(mkinp(vec![press(VKC::LShift, ms)]));
        runtime.run_sim_once();
        assert_eq!(runtime.resource::<Player>().walking, false);
        assert_eq!(runtime.resource::<Player>().running, true);
        runtime.insert_resource(mkinp(vec![release(VKC::LShift, ms)]));
        runtime.run_sim_once();
        assert_eq!(runtime.resource::<Player>().walking, true);
        assert_eq!(runtime.resource::<Player>().running, false);
        runtime.insert_resource(mkinp(vec![release(VKC::W, ms)]));
        runtime.run_sim_once();
        assert_eq!(runtime.resource::<Player>().walking, false);
        assert_eq!(runtime.resource::<Player>().running, false);

        Ok(())
    }

    #[test]
    #[ignore]
    fn test_exact_modifer_matching() -> Result<()> {
        env_logger::init();
        let mut runtime = prepare()?;
        let mut state = ModifiersState::empty();
        let ms = &mut state;

        // Match modifier planes exactly
        // FIXME: the control does not mask properly because we don't visit it because it's not
        //        part of any binding... which is not quite right.
        runtime.insert_resource(mkinp(vec![
            press(VKC::W, ms),
            press(VKC::LShift, ms),
            press(VKC::LControl, ms),
        ]));
        runtime.run_sim_once();
        println!("MS: {:?}", ms);
        assert_eq!(runtime.resource::<Player>().walking, false);
        assert_eq!(runtime.resource::<Player>().running, false);
        runtime.insert_resource(mkinp(vec![release(VKC::LControl, ms)]));
        runtime.run_sim_once();
        assert_eq!(runtime.resource::<Player>().walking, false);
        assert_eq!(runtime.resource::<Player>().running, true);
        runtime.insert_resource(mkinp(vec![release(VKC::LShift, ms)]));
        runtime.run_sim_once();
        assert_eq!(runtime.resource::<Player>().walking, true);
        assert_eq!(runtime.resource::<Player>().running, false);
        runtime.insert_resource(mkinp(vec![release(VKC::W, ms)]));
        runtime.run_sim_once();
        assert_eq!(runtime.resource::<Player>().walking, false);
        assert_eq!(runtime.resource::<Player>().running, false);

        Ok(())
    }

    /*
    #[test]
    fn test_masking() -> Result<()> {
        let mut runtime = Runtime::default();
        // let mut interpreter = Interpreter::default();
        let player = Arc::new(RwLock::new(Player::default()));
        // interpreter.put_global("player", Value::Module(player.clone()));

        let w_key = Input::KeyboardKey(VirtualKeyCode::W);
        let shift_key = Input::KeyboardKey(VirtualKeyCode::LShift);

        let mut state: State = Default::default();
        let bindings = Bindings::new("test")
            .with_bind("w", "player.walk(pressed)")?
            .with_bind("shift+w", "player.run(pressed)")?;

        state.input_states.insert(w_key, ElementState::Pressed);
        bindings.match_input(
            w_key,
            Some(ElementState::Pressed),
            &mut state,
            &LocalNamespace::empty(),
            &mut runtime.resource_mut::<ScriptHerder>(),
        )?;
        runtime.run_sim_once();
        assert!(player.read().walking);
        assert!(!player.read().running);

        state.input_states.insert(shift_key, ElementState::Pressed);
        state.modifiers_state |= ModifiersState::SHIFT;
        bindings.match_input(
            shift_key,
            Some(ElementState::Pressed),
            &mut state,
            &LocalNamespace::empty(),
            &mut runtime.resource_mut::<ScriptHerder>(),
        )?;
        assert!(player.read().running);
        assert!(!player.read().walking);

        state.input_states.insert(shift_key, ElementState::Released);
        state.modifiers_state -= ModifiersState::SHIFT;
        bindings.match_input(
            shift_key,
            Some(ElementState::Released),
            &mut state,
            &LocalNamespace::empty(),
            &mut runtime.resource_mut::<ScriptHerder>(),
        )?;
        assert!(player.read().walking);
        assert!(!player.read().running);

        state.input_states.insert(shift_key, ElementState::Pressed);
        state.modifiers_state |= ModifiersState::SHIFT;
        bindings.match_input(
            shift_key,
            Some(ElementState::Pressed),
            &mut state,
            &LocalNamespace::empty(),
            &mut runtime.resource_mut::<ScriptHerder>(),
        )?;
        assert!(!player.read().walking);
        assert!(player.read().running);

        state.input_states.insert(w_key, ElementState::Released);
        bindings.match_input(
            w_key,
            Some(ElementState::Released),
            &mut state,
            &LocalNamespace::empty(),
            &mut runtime.resource_mut::<ScriptHerder>(),
        )?;
        assert!(!player.read().walking);
        assert!(!player.read().running);

        state.input_states.insert(w_key, ElementState::Pressed);
        bindings.match_input(
            w_key,
            Some(ElementState::Pressed),
            &mut state,
            &LocalNamespace::empty(),
            &mut runtime.resource_mut::<ScriptHerder>(),
        )?;
        assert!(!player.read().walking);
        assert!(player.read().running);

        Ok(())
    }
     */
}
