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
use crate::{Command, Key, KeySet};
use failure::Fallible;
use log::trace;
use smallvec::{smallvec, SmallVec};
use std::collections::{HashMap, HashSet};
use winit::event::ElementState;

#[derive(Debug, Default)]
pub struct BindingState {
    pub key_states: HashMap<Key, ElementState>,
    active_chords: HashSet<KeySet>,
}

// Map from key, buttons, and axes to commands.
#[derive(Debug)]
pub struct Bindings {
    pub name: String,
    press_chords: HashMap<Key, Vec<KeySet>>,
    command_map: HashMap<KeySet, Command>,
}

impl Bindings {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            press_chords: HashMap::new(),
            command_map: HashMap::new(),
        }
    }

    pub fn bind(mut self, command_raw: &str, keyset: &str) -> Fallible<Self> {
        let command = Command::parse(command_raw)?;
        for ks in KeySet::from_virtual(keyset)?.drain(..) {
            self.command_map.insert(ks.clone(), command.clone());
            trace!("binding {} => {}", ks, command);

            for key in &ks.keys {
                let sets = self.press_chords.entry(*key).or_insert_with(Vec::new);

                sets.push(ks.to_owned());
                sets.sort_by_key(|ks| usize::max_value() - ks.keys.len());
            }
        }
        Ok(self)
    }

    pub fn match_key(
        &self,
        key: Key,
        key_state: ElementState,
        state: &mut BindingState,
    ) -> Fallible<SmallVec<[Command; 4]>> {
        match key_state {
            ElementState::Pressed => self.handle_press(key, state),
            ElementState::Released => self.handle_release(key, state),
        }
    }

    fn handle_press(&self, key: Key, state: &mut BindingState) -> Fallible<SmallVec<[Command; 4]>> {
        let mut commands = smallvec![];

        // The press chords gives us a quick map from a key press to all chords which could become
        // active in the case that it is pressed so that we don't have to look at everything.
        if let Some(possible_chord_list) = self.press_chords.get(&key) {
            for chord in possible_chord_list {
                if Self::chord_is_pressed(&chord.keys, &state.key_states) {
                    self.maybe_activate_chord(chord, state, &mut commands)?;
                }
            }
        }

        Ok(commands)
    }

    fn chord_is_masked(chord: &KeySet, state: &BindingState) -> bool {
        for active_chord in &state.active_chords {
            if chord.is_subset_of(active_chord) {
                return true;
            }
        }
        false
    }

    fn maybe_activate_chord(
        &self,
        chord: &KeySet,
        state: &mut BindingState,
        commands: &mut SmallVec<[Command; 4]>,
    ) -> Fallible<()> {
        // We may have multiple binding sets active for the same KeySet, in which case the first
        // binding in the set wins and checks for subsequent activations should exit early.
        if state.active_chords.contains(&chord) {
            return Ok(());
        }

        // The press_chords list implicitly filters out keys not in this bindings.
        assert!(self.command_map.contains_key(chord));

        // If the chord is masked, do not activate.
        if Self::chord_is_masked(chord, state) {
            return Ok(());
        }

        // If any chord will become masked, deactivate it.
        let mut masked_chords: SmallVec<[KeySet; 4]> = smallvec![];
        for active_chord in &state.active_chords {
            if active_chord.is_subset_of(chord) {
                masked_chords.push(active_chord.to_owned());
            }
        }
        for masked in &masked_chords {
            state.active_chords.remove(masked);
            if let Some(command) = self.command_map[masked].release_command()? {
                commands.push(command);
            }
        }

        // Activate the chord and run the command.
        state.active_chords.insert(chord.to_owned());
        commands.push(self.command_map[chord].to_owned());

        Ok(())
    }

    fn chord_is_pressed(binding_keys: &[Key], key_states: &HashMap<Key, ElementState>) -> bool {
        for binding_key in binding_keys.iter() {
            if let Some(current_state) = key_states.get(binding_key) {
                if *current_state == ElementState::Released {
                    return false;
                }
            } else {
                return false;
            }
        }
        true
    }

    fn handle_release(
        &self,
        key: Key,
        state: &mut BindingState,
    ) -> Fallible<SmallVec<[Command; 4]>> {
        let mut commands = smallvec![];

        // Remove any chords that have been released.
        let mut released_chords: SmallVec<[KeySet; 4]> = smallvec![];
        for active_chord in &state.active_chords {
            if active_chord.contains_key(&key) {
                // Note: unlike with press, we do not implicitly filter out keys we don't care about.
                if let Some(pressed_command) = self.command_map.get(active_chord) {
                    released_chords.push(active_chord.to_owned());
                    if let Some(command) = pressed_command.release_command()? {
                        commands.push(command);
                    }
                }
            }
        }
        for chord in &released_chords {
            state.active_chords.remove(chord);
        }

        // If we removed a chord, then it may have been masking an active command. Re-enable any
        // masked commands that were unmasked by this change.
        for released_chord in &released_chords {
            for (chord, command) in &self.command_map {
                if chord.is_subset_of(released_chord)
                    && Self::chord_is_pressed(&chord.keys, &state.key_states)
                {
                    state.active_chords.insert(chord.to_owned());
                    commands.push(command.to_owned());
                }
            }
        }

        Ok(commands)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use winit::event::VirtualKeyCode;

    #[test]
    fn test_masking() -> Fallible<()> {
        let w_key = Key::Virtual(VirtualKeyCode::W);
        let shift_key = Key::Virtual(VirtualKeyCode::LShift);

        let mut state: BindingState = Default::default();
        let bindings = Bindings::new("test")
            .bind("player.+walk", "w")?
            .bind("player.+run", "shift+w")?;

        state.key_states.insert(w_key, ElementState::Pressed);
        let cmds = bindings.match_key(w_key, ElementState::Pressed, &mut state)?;
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].full(), "player.+walk");

        state.key_states.insert(shift_key, ElementState::Pressed);
        let cmds = bindings.match_key(shift_key, ElementState::Pressed, &mut state)?;
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0].full(), "player.-walk");
        assert_eq!(cmds[1].full(), "player.+run");

        state.key_states.insert(shift_key, ElementState::Released);
        let cmds = bindings.match_key(shift_key, ElementState::Released, &mut state)?;
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0].full(), "player.-run");
        assert_eq!(cmds[1].full(), "player.+walk");

        state.key_states.insert(shift_key, ElementState::Pressed);
        let cmds = bindings.match_key(shift_key, ElementState::Pressed, &mut state)?;
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0].full(), "player.-walk");
        assert_eq!(cmds[1].full(), "player.+run");

        state.key_states.insert(w_key, ElementState::Released);
        let cmds = bindings.match_key(w_key, ElementState::Released, &mut state)?;
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].full(), "player.-run");

        state.key_states.insert(w_key, ElementState::Pressed);
        let cmds = bindings.match_key(w_key, ElementState::Pressed, &mut state)?;
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].full(), "player.+run");

        Ok(())
    }
}
