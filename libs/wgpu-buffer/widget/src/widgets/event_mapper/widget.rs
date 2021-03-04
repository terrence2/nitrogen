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
        window::WindowEventKind,
    },
    PaintContext,
};
use failure::Fallible;
use gpu::GPU;
use input::{ElementState, GenericEvent, GenericWindowEvent, ModifiersState};
use nitrous::{Interpreter, Module, Value};
use nitrous_injector::{inject_nitrous_module, method, NitrousModule};
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

#[derive(Default, Debug, NitrousModule)]
pub struct EventMapper {
    bindings: HashMap<String, Arc<RwLock<Bindings>>>,
    state: State,
}

#[inject_nitrous_module]
impl EventMapper {
    pub fn new(interpreter: Arc<RwLock<Interpreter>>) -> Fallible<Arc<RwLock<Self>>> {
        let mapper = Arc::new(RwLock::new(Default::default()));
        interpreter
            .write()
            .put(interpreter.clone(), "mapper", Value::Module(mapper.clone()))?;
        Ok(mapper)
    }

    #[method]
    pub fn create_bindings(&mut self, name: &str) -> Fallible<Value> {
        let bindings = Arc::new(RwLock::new(Bindings::new(name)));
        self.bindings.insert(name.to_owned(), bindings.clone());
        Ok(Value::Module(bindings))
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

                for bindings in self.bindings.values() {
                    bindings.read().match_key(
                        key,
                        key_state,
                        &mut self.state,
                        interpreter.clone(),
                    )?;
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
                                for bindings in self.bindings.values() {
                                    bindings.read().match_axis(AxisKind::MouseMotion, inner)?;
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
                                for bindings in self.bindings.values() {
                                    bindings.read().match_axis(AxisKind::MouseWheel, inner)?;
                                }
                                Ok(Value::True())
                            },
                        )?;
                    }

                    GenericEvent::Window(evt) => match evt {
                        GenericWindowEvent::Resized { width, height } => {
                            interpreter.write().with_locals(
                                &[
                                    ("width", Value::Integer(*width as i64)),
                                    ("height", Value::Integer(*height as i64)),
                                ],
                                |inner| {
                                    for bindings in self.bindings.values() {
                                        bindings
                                            .read()
                                            .match_window_event(WindowEventKind::Resize, inner)?;
                                    }
                                    Ok(Value::True())
                                },
                            )?;
                        }

                        GenericWindowEvent::ScaleFactorChanged { scale } => {
                            interpreter.write().with_locals(
                                &[("scale", Value::Float(OrderedFloat(*scale)))],
                                |inner| {
                                    for bindings in self.bindings.values() {
                                        bindings.read().match_window_event(
                                            WindowEventKind::DpiChange,
                                            inner,
                                        )?;
                                    }
                                    Ok(Value::True())
                                },
                            )?;
                        }
                    },

                    _ => {
                        //println!("unexpected event: {:?}", event);
                    }
                };
            }
        }
        Ok(())
    }
}
