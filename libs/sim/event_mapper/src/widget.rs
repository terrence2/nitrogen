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
use anyhow::{ensure, Result};
use bevy_ecs::prelude::*;
use input::{ElementState, InputEvent, InputEventVec, InputFocus, ModifiersState};
use nitrous::{Interpreter, Value};
use nitrous_injector::{inject_nitrous_module, method, NitrousModule};
use ordered_float::OrderedFloat;
use parking_lot::RwLock;
use runtime::{Extension, Runtime};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[derive(Debug, Default)]
pub struct State {
    pub modifiers_state: ModifiersState,
    pub input_states: HashMap<Input, ElementState>,
    pub active_chords: HashSet<InputSet>,
}

#[derive(Default, Debug, NitrousModule)]
pub struct EventMapper {
    bindings: HashMap<String, Arc<RwLock<Bindings>>>,
    state: State,
}

// impl Extension for EventMapper {
//     fn init(runtime: &mut Runtime) -> Result<()> {
//         runtime.insert_module("mapper", EventMapper::new())
//     }
// }

#[inject_nitrous_module]
impl EventMapper {
    pub fn new(interpreter: &mut Interpreter) -> Arc<RwLock<Self>> {
        let mapper = Arc::new(RwLock::new(Self {
            bindings: HashMap::new(),
            state: State::default(),
        }));
        interpreter.put_global("mapper", Value::Module(mapper.clone()));
        mapper
    }

    #[method]
    pub fn create_bindings(&mut self, name: &str) -> Result<Value> {
        ensure!(
            !self.bindings.contains_key(name),
            format!("already have a bindings set named {}", name)
        );
        let bindings = Arc::new(RwLock::new(Bindings::new(name)));
        self.bindings.insert(name.to_owned(), bindings.clone());
        Ok(Value::Module(bindings))
    }

    pub fn sys_handle_input_events(
        events: Res<InputEventVec>,
        input_focus: Res<InputFocus>,
        mut interpreter: ResMut<Interpreter>,
        mapper: Res<Arc<RwLock<EventMapper>>>,
    ) {
        mapper
            .write()
            .handle_events(&events, *input_focus, &mut interpreter)
            .expect("EventMapper::handle_events");
    }

    pub fn handle_events(
        &mut self,
        events: &[InputEvent],
        focus: InputFocus,
        interpreter: &mut Interpreter,
    ) -> Result<()> {
        for event in events {
            self.handle_event(event, focus, interpreter)?;
        }
        Ok(())
    }

    fn handle_event(
        &mut self,
        event: &InputEvent,
        focus: InputFocus,
        interpreter: &mut Interpreter,
    ) -> Result<()> {
        let input = Input::from_event(event);
        if input.is_none() {
            return Ok(());
        }
        let input = input.unwrap();

        let mut variables = Vec::with_capacity(8);
        variables.push(("window_focused", Value::Boolean(event.is_window_focused())));

        if let Some(press_state) = event.press_state() {
            self.state.input_states.insert(input, press_state);
            // Note: pressed variable is set later, since we need to disable masked input sets.
        }

        if let Some(modifiers_state) = event.modifiers_state() {
            self.state.modifiers_state = modifiers_state;
            variables.push(("shift_pressed", Value::Boolean(modifiers_state.shift())));
            variables.push(("alt_pressed", Value::Boolean(modifiers_state.alt())));
            variables.push(("ctrl_pressed", Value::Boolean(modifiers_state.ctrl())));
            variables.push(("logo_pressed", Value::Boolean(modifiers_state.logo())));
        }

        // Break *after* maintaining state.
        if focus != InputFocus::Game {
            return Ok(());
        }

        // Collect variables to inject.
        match event {
            InputEvent::MouseMotion {
                dx, dy, in_window, ..
            } => {
                variables.push(("dx", Value::Float(OrderedFloat(*dx))));
                variables.push(("dy", Value::Float(OrderedFloat(*dy))));
                variables.push(("in_window", Value::Boolean(*in_window)));
            }
            InputEvent::MouseWheel {
                horizontal_delta,
                vertical_delta,
                in_window,
                ..
            } => {
                variables.push((
                    "horizontal_delta",
                    Value::Float(OrderedFloat(*horizontal_delta)),
                ));
                variables.push((
                    "vertical_delta",
                    Value::Float(OrderedFloat(*vertical_delta)),
                ));
                variables.push(("in_window", Value::Boolean(*in_window)));
            }
            InputEvent::DeviceAdded { dummy } => {
                variables.push(("device_id", Value::Integer(*dummy as i64)));
            }
            InputEvent::DeviceRemoved { dummy } => {
                variables.push(("device_id", Value::Integer(*dummy as i64)));
            }
            // FIXME: set variables for button state, key state, joy state, etc
            _ => {}
        }

        interpreter.with_locals(&variables, |inner| {
            for bindings in self.bindings.values() {
                bindings
                    .read()
                    .match_input(input, event.press_state(), &mut self.state, inner)?
            }
            Ok(Value::True())
        })?;

        Ok(())
    }
}
