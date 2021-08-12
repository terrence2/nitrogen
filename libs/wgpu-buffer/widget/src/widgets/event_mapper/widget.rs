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
    font_context::FontContext,
    region::{Extent, Position, Region},
    widget::Widget,
    widgets::event_mapper::{
        bindings::Bindings,
        input::{Input, InputSet},
    },
    PaintContext,
};
use anyhow::{ensure, Result};
use gpu::{
    size::{AbsSize, Size},
    Gpu,
};
use input::{ElementState, GenericEvent, GenericSystemEvent, GenericWindowEvent, ModifiersState};
use nitrous::{Interpreter, Value};
use nitrous_injector::{inject_nitrous_module, method, NitrousModule};
use ordered_float::OrderedFloat;
use parking_lot::RwLock;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Instant,
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
}

impl Widget for EventMapper {
    fn measure(&mut self, _gpu: &Gpu, _font_context: &mut FontContext) -> Result<Extent<Size>> {
        Ok(Extent::zero())
    }

    fn layout(
        &mut self,
        _region: Region<Size>,
        _gpu: &Gpu,
        _font_context: &mut FontContext,
    ) -> Result<()> {
        Ok(())
    }

    fn upload(&self, _now: Instant, _gpu: &Gpu, _context: &mut PaintContext) -> Result<()> {
        Ok(())
    }

    fn handle_event(
        &mut self,
        _now: Instant,
        event: &GenericEvent,
        focus: &str,
        _cursor_position: Position<AbsSize>,
        interpreter: Arc<RwLock<Interpreter>>,
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
        if focus != "mapper" {
            return Ok(());
        }

        // Collect variables to inject.
        match event {
            GenericEvent::MouseMotion {
                dx, dy, in_window, ..
            } => {
                variables.push(("dx", Value::Float(OrderedFloat(*dx))));
                variables.push(("dy", Value::Float(OrderedFloat(*dy))));
                variables.push(("in_window", Value::Boolean(*in_window)));
            }
            GenericEvent::MouseWheel {
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
            GenericEvent::Window(evt) => match evt {
                GenericWindowEvent::Resized { width, height } => {
                    variables.push(("width", Value::Integer(*width as i64)));
                    variables.push(("height", Value::Integer(*height as i64)));
                }
                GenericWindowEvent::ScaleFactorChanged { scale } => {
                    variables.push(("scale", Value::Float(OrderedFloat(*scale))));
                }
            },
            GenericEvent::System(evt) => match evt {
                GenericSystemEvent::Quit => {}
                GenericSystemEvent::DeviceAdded { dummy } => {
                    variables.push(("device_id", Value::Integer(*dummy as i64)));
                }
                GenericSystemEvent::DeviceRemoved { dummy } => {
                    variables.push(("device_id", Value::Integer(*dummy as i64)));
                }
            },
            _ => {}
        }

        interpreter.write().with_locals(&variables, |inner| {
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
