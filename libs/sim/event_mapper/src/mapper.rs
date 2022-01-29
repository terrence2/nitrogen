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
use nitrous::Value;
use nitrous_injector::{inject_nitrous, method, NitrousResource};
use ordered_float::OrderedFloat;
use parking_lot::RwLock;
use runtime::{Extension, Runtime, ScriptHerder, SimStage};
use std::{
    collections::{HashMap, HashSet},
    marker::PhantomData,
    sync::Arc,
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
{
    bindings: HashMap<String, Arc<RwLock<Bindings>>>,
    state: State,
    phantom_data: PhantomData<T>,
}

impl<T> Extension for EventMapper<T>
where
    T: InputFocus,
{
    fn init(runtime: &mut Runtime) -> Result<()> {
        runtime.insert_module("mapper", EventMapper::<T>::new());
        runtime
            .sim_stage_mut(SimStage::HandleInput)
            .add_system(Self::sys_handle_input_events);
        Ok(())
    }
}

#[inject_nitrous]
impl<T> EventMapper<T>
where
    T: InputFocus,
{
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
            state: State::default(),
            phantom_data: Default::default(),
        }
    }

    #[method]
    pub fn create_bindings(&mut self, name: &str) -> Result<Value> {
        ensure!(
            !self.bindings.contains_key(name),
            format!("already have a bindings set named {}", name)
        );
        let bindings = Arc::new(RwLock::new(Bindings::new(name)));
        self.bindings.insert(name.to_owned(), bindings.clone());
        // FIXME: re-orient bindings to work against GameState input
        // Ok(Value::Module(bindings))
        Ok(Value::True())
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
            bindings.read().match_input(
                input,
                event.press_state(),
                &mut self.state,
                &locals,
                herder,
            )?
        }

        Ok(())
    }
}
