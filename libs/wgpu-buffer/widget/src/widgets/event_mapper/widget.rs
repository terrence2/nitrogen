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
    widget::{UploadMetrics, Widget},
    widgets::event_mapper::{
        axis::AxisKind,
        bindings::Bindings,
        keyset::{Key, KeySet},
    },
    PaintContext,
};
use failure::Fallible;
use gpu::GPU;
use input::{ElementState, GenericEvent, ModifiersState};
use nitrous::{Interpreter, Value};
use ordered_float::OrderedFloat;
use parking_lot::RwLock;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[derive(Debug, Default)]
pub struct State {
    pub modifiers_state: ModifiersState,
    pub key_states: HashMap<Key, ElementState>,
    pub active_chords: HashSet<KeySet>,
}

pub struct EventMapper {
    bindings: Vec<Bindings>,
    state: State,
}

impl EventMapper {
    pub fn with_bindings(bindings: Vec<Bindings>) -> Self {
        Self {
            bindings,
            state: Default::default(),
        }
    }

    pub fn wrapped(self) -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(self))
    }
}

impl Widget for EventMapper {
    fn upload(&self, _gpu: &GPU, _context: &mut PaintContext) -> Fallible<UploadMetrics> {
        Ok(UploadMetrics {
            widget_info_indexes: vec![],
            width: 0.,
            height: 0.,
        })
    }

    fn handle_events(
        &mut self,
        events: &[GenericEvent],
        interpreter: Arc<RwLock<Interpreter>>,
    ) -> Fallible<()> {
        for event in events {
            if !event.is_window_focused() {
                continue;
            }

            // FIXME: key-like elements are not the only elements we can bind.
            if let Some(key_state) = event.press_state() {
                let key = match event {
                    GenericEvent::KeyboardKey {
                        virtual_keycode, ..
                    } => Key::KeyboardKey(*virtual_keycode),
                    GenericEvent::MouseButton { button, .. } => Key::MouseButton(*button),
                    // GenericEvent::JoystickButton { button, .. } => Key::JoystickButton(*button),
                    _ => {
                        panic!("event has a press state, but is not a key or button kind of event")
                    }
                };

                self.state.key_states.insert(key, key_state);
                self.state.modifiers_state =
                    event.modifiers_state().expect("modifiers on key press");

                for bindings in &self.bindings {
                    bindings.match_key(key, key_state, &mut self.state, interpreter.clone())?;
                }
            } else {
                match event {
                    GenericEvent::MouseMotion {
                        dx,
                        dy,
                        modifiers_state,
                        in_window,
                        window_focused,
                    } => {
                        interpreter.write().with_locals(
                            &[
                                ("dx", Value::Float(OrderedFloat(*dx))),
                                ("dy", Value::Float(OrderedFloat(*dy))),
                                ("shift_pressed", Value::Boolean(modifiers_state.shift())),
                                ("alt_pressed", Value::Boolean(modifiers_state.alt())),
                                ("ctrl_pressed", Value::Boolean(modifiers_state.ctrl())),
                                ("logo_pressed", Value::Boolean(modifiers_state.logo())),
                                ("in_window", Value::Boolean(*in_window)),
                                ("window_focused", Value::Boolean(*window_focused)),
                            ],
                            |inner| {
                                for bindings in &self.bindings {
                                    bindings.match_axis(AxisKind::MouseMotion, inner)?;
                                }
                                Ok(Value::True())
                            },
                        )?;
                    }

                    GenericEvent::MouseWheel {
                        horizontal_delta,
                        vertical_delta,
                        modifiers_state,
                        in_window,
                        window_focused,
                    } => {
                        interpreter.write().with_locals(
                            &[
                                (
                                    "horizontal_delta",
                                    Value::Float(OrderedFloat(*horizontal_delta)),
                                ),
                                (
                                    "vertical_delta",
                                    Value::Float(OrderedFloat(*vertical_delta)),
                                ),
                                ("shift_pressed", Value::Boolean(modifiers_state.shift())),
                                ("alt_pressed", Value::Boolean(modifiers_state.alt())),
                                ("ctrl_pressed", Value::Boolean(modifiers_state.ctrl())),
                                ("logo_pressed", Value::Boolean(modifiers_state.logo())),
                                ("in_window", Value::Boolean(*in_window)),
                                ("window_focused", Value::Boolean(*window_focused)),
                            ],
                            |inner| {
                                for bindings in &self.bindings {
                                    bindings.match_axis(AxisKind::MouseWheel, inner)?;
                                }
                                Ok(Value::True())
                            },
                        )?;
                    }

                    _ => {
                        //println!("unexpected event: {:?}", event);
                    }
                };
            }
        }
        Ok(())
    }
}
