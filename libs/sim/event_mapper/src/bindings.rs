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
    input::{Input, InputSet},
    State,
};
use anyhow::Result;
use input::ElementState;
use log::{debug, trace};
use nitrous::{LocalNamespace, NitrousScript, Value};
use runtime::ScriptHerder;
use smallvec::{smallvec, SmallVec};
use std::collections::HashMap;

// Map from key, buttons, and axes to commands.
#[derive(Debug)]
pub struct Bindings {
    pub name: String,
    press_chords: HashMap<Input, Vec<InputSet>>,
    script_map: HashMap<InputSet, Vec<NitrousScript>>,
}

impl Bindings {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            press_chords: HashMap::new(),
            script_map: HashMap::new(),
        }
    }

    pub fn with_bind(mut self, keyset_or_axis: &str, script_raw: &str) -> Result<Self> {
        self.bind(keyset_or_axis, script_raw)?;
        Ok(self)
    }

    pub fn bind(&mut self, event_name: &str, script_raw: &str) -> Result<()> {
        let script = NitrousScript::compile(script_raw)?;

        for ks in InputSet::from_binding(event_name)?.drain(..) {
            trace!("binding {} to\n{}", ks, script);
            self.script_map
                .entry(ks.clone())
                .or_insert_with(Vec::new)
                .push(script.clone());

            for key in &ks.keys {
                let sets = self.press_chords.entry(*key).or_insert_with(Vec::new);

                sets.push(ks.to_owned());
                sets.sort_by_key(|ks| usize::max_value() - ks.keys.len());
            }
        }
        Ok(())
    }

    pub fn match_input(
        &self,
        input: Input,
        press_state: Option<ElementState>,
        state: &mut State,
        locals: &LocalNamespace,
        herder: &mut ScriptHerder,
    ) -> Result<()> {
        match press_state {
            Some(ElementState::Pressed) => self.handle_press(input, state, locals, herder)?,
            Some(ElementState::Released) => self.handle_release(input, state, locals, herder)?,
            None => self.handle_edge(input, state, locals, herder)?,
        }
        Ok(())
    }

    fn handle_edge(
        &self,
        input: Input,
        state: &mut State,
        locals: &LocalNamespace,
        herder: &mut ScriptHerder,
    ) -> Result<()> {
        if let Some(possible_chord_list) = self.press_chords.get(&input) {
            for chord in possible_chord_list {
                if chord.is_pressed(Some(input), state) {
                    // Note: chord is in possible chord list, so must be present.
                    for script in &self.script_map[chord] {
                        herder.run_binding(locals.to_owned(), script.to_owned());
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_press(
        &self,
        input: Input,
        state: &mut State,
        locals: &LocalNamespace,
        herder: &mut ScriptHerder,
    ) -> Result<()> {
        // The press chords gives us a quick map from a key press to all chords which could become
        // active in the case that it is pressed so that we don't have to look at everything.
        if let Some(possible_chord_list) = self.press_chords.get(&input) {
            for chord in possible_chord_list {
                if chord.is_pressed(None, state) {
                    self.maybe_activate_chord(chord, state, locals, herder)?;
                }
            }
        }
        Ok(())
    }

    fn chord_is_masked(chord: &InputSet, state: &State) -> bool {
        for active_chord in &state.active_chords {
            if chord.is_subset_of(active_chord) {
                return true;
            }
        }
        false
    }

    fn maybe_activate_chord(
        &self,
        chord: &InputSet,
        state: &mut State,
        locals: &LocalNamespace,
        herder: &mut ScriptHerder,
    ) -> Result<()> {
        // We may have multiple binding sets active for the same KeySet, in which case the first
        // binding in the set wins and checks for subsequent activations should exit early.
        if state.active_chords.contains(chord) {
            debug!("chord {} is already active", chord);
            return Ok(());
        }

        // The press_chords list implicitly filters out keys not in this bindings.
        assert!(self.script_map.contains_key(chord));

        // If the chord is masked, do not activate.
        if Self::chord_is_masked(chord, state) {
            debug!("chord {} is masked", chord);
            return Ok(());
        }

        // If any chord will become masked, deactivate it.
        let mut masked_chords: SmallVec<[InputSet; 4]> = smallvec![];
        for active_chord in &state.active_chords {
            if active_chord.is_subset_of(chord) {
                debug!("masking chord: {}", active_chord);
                masked_chords.push(active_chord.to_owned());
            }
        }
        for masked in &masked_chords {
            state.active_chords.remove(masked);
            if let Some(scripts) = self.script_map.get(masked) {
                self.deactiveate_chord(locals, scripts, herder)?;
            }
        }

        // Activate the chord and run the command.
        state.active_chords.insert(chord.to_owned());
        self.activate_chord(locals, &self.script_map[chord], herder)?;

        Ok(())
    }

    fn handle_release(
        &self,
        key: Input,
        state: &mut State,
        locals: &LocalNamespace,
        herder: &mut ScriptHerder,
    ) -> Result<()> {
        // Remove any chords that have been released.
        let mut released_chords: SmallVec<[InputSet; 4]> = smallvec![];
        for active_chord in &state.active_chords {
            if active_chord.contains_key(&key) {
                // Note: unlike with press, we do not implicitly filter out keys we don't care about.
                if let Some(scripts) = self.script_map.get(active_chord) {
                    released_chords.push(active_chord.to_owned());
                    self.deactiveate_chord(locals, scripts, herder)?;
                }
            }
        }
        for chord in &released_chords {
            state.active_chords.remove(chord);
        }

        // If we removed a chord, then it may have been masking an active command. Re-enable any
        // masked commands that were unmasked by this change.
        for released_chord in &released_chords {
            for (chord, scripts) in &self.script_map {
                if chord.is_subset_of(released_chord) && chord.is_pressed(None, state) {
                    state.active_chords.insert(chord.to_owned());
                    self.activate_chord(locals, scripts, herder)?;
                }
            }
        }

        Ok(())
    }

    fn activate_chord(
        &self,
        locals: &LocalNamespace,
        scripts: &[NitrousScript],
        herder: &mut ScriptHerder,
    ) -> Result<()> {
        for script in scripts {
            let mut locals = locals.to_owned();
            locals.put("pressed", Value::True());
            herder.run_binding(locals, script);
        }
        Ok(())
    }

    fn deactiveate_chord(
        &self,
        locals: &LocalNamespace,
        scripts: &[NitrousScript],
        herder: &mut ScriptHerder,
    ) -> Result<()> {
        for script in scripts {
            let mut locals = locals.to_owned();
            locals.put("pressed", Value::False());
            herder.run_binding(locals, script);
        }
        Ok(())
    }
}
