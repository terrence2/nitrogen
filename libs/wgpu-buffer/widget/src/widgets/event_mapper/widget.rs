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
        bindings::Bindings,
        keyset::{Key, KeySet},
    },
    PaintContext,
};
use failure::Fallible;
use gpu::GPU;
use input::{ElementState, GenericEvent, ModifiersState};
use nitrous::Interpreter;
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
                for bindings in &self.bindings {
                    bindings.match_key(key, key_state, &mut self.state, interpreter.clone())?;
                }
            }
        }
        Ok(())
    }
}
