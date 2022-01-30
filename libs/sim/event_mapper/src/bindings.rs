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
    script_map: HashMap<InputSet, NitrousScript>,
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
            trace!("binding {} => {}", ks, script);
            self.script_map.insert(ks.clone(), script.clone());

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
                    herder.run_with_locals(locals.to_owned(), self.script_map[chord].clone());
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
            if let Some(script) = self.script_map.get(masked) {
                self.deactiveate_chord(locals, script, herder)?;
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
                if let Some(script) = self.script_map.get(active_chord) {
                    released_chords.push(active_chord.to_owned());
                    self.deactiveate_chord(locals, script, herder)?;
                }
            }
        }
        for chord in &released_chords {
            state.active_chords.remove(chord);
        }

        // If we removed a chord, then it may have been masking an active command. Re-enable any
        // masked commands that were unmasked by this change.
        for released_chord in &released_chords {
            for (chord, script) in &self.script_map {
                if chord.is_subset_of(released_chord) && chord.is_pressed(None, state) {
                    state.active_chords.insert(chord.to_owned());
                    self.activate_chord(locals, script, herder)?;
                }
            }
        }

        Ok(())
    }

    fn activate_chord(
        &self,
        locals: &LocalNamespace,
        script: &NitrousScript,
        herder: &mut ScriptHerder,
    ) -> Result<()> {
        let mut locals = locals.to_owned();
        locals.put("pressed", Value::True());
        herder.run_with_locals(locals, script);
        Ok(())
    }

    fn deactiveate_chord(
        &self,
        locals: &LocalNamespace,
        script: &NitrousScript,
        herder: &mut ScriptHerder,
    ) -> Result<()> {
        let mut locals = locals.to_owned();
        locals.put("pressed", Value::False());
        herder.run_with_locals(locals, script);
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use anyhow::bail;
    use input::{ModifiersState, VirtualKeyCode};
    use nitrous::{Module, Value};
    use parking_lot::RwLock;
    use runtime::Runtime;
    use std::sync::Arc;

    #[derive(Debug, Default)]
    struct Player {
        walking: bool,
        running: bool,
    }

    impl Module for Player {
        fn module_name(&self) -> String {
            "Player".to_owned()
        }

        fn call_method(&mut self, name: &str, args: &[Value]) -> Result<Value> {
            println!("Call: {}({})", name, args[0]);
            Ok(match name {
                "walk" => {
                    self.walking = args[0] == Value::True();
                    Value::Integer(0)
                }
                "run" => {
                    self.running = args[0] == Value::True();
                    Value::Integer(0)
                }
                _ => unimplemented!(),
            })
        }

        fn put(
            &mut self,
            _module: Arc<RwLock<dyn Module>>,
            _name: &str,
            _value: Value,
        ) -> Result<()> {
            unimplemented!()
        }

        fn get(&self, module: Arc<RwLock<dyn Module>>, name: &str) -> Result<Value> {
            Ok(match name {
                "walk" => Value::ResourceMethod(module, name.to_owned()),
                "run" => Value::ResourceMethod(module, name.to_owned()),
                _ => bail!("get unknown '{}'", name),
            })
        }

        fn names(&self) -> Vec<&str> {
            vec!["walk", "run"]
        }
    }

    #[test]
    fn test_modifier_planes_disable_bare() -> Result<()> {
        let mut herder = ScriptHerder::default();
        // let mut interpreter = Interpreter::default();
        let player = Arc::new(RwLock::new(Player::default()));
        // interpreter.put_global("player", Value::Module(player.clone()));

        let w_key = Input::KeyboardKey(VirtualKeyCode::W);
        let shift_key = Input::KeyboardKey(VirtualKeyCode::LShift);

        let mut state: State = Default::default();
        let bindings = Bindings::new("test").with_bind("w", "player.never_executed()")?;

        state.input_states.insert(shift_key, ElementState::Pressed);
        state.modifiers_state |= ModifiersState::SHIFT;
        bindings.match_input(
            shift_key,
            Some(ElementState::Pressed),
            &mut state,
            &LocalNamespace::empty(),
            &mut herder,
        )?;
        assert!(!player.read().walking);

        state.input_states.insert(w_key, ElementState::Pressed);
        bindings.match_input(
            w_key,
            Some(ElementState::Pressed),
            &mut state,
            &LocalNamespace::empty(),
            &mut herder,
        )?;
        assert!(!player.read().walking);

        Ok(())
    }

    #[test]
    fn test_matches_exact_modifier_plane() -> Result<()> {
        let mut herder = ScriptHerder::default();
        // let mut interpreter = Interpreter::default();
        let player = Arc::new(RwLock::new(Player::default()));
        // interpreter.put_global("player", Value::Module(player.clone()));

        let w_key = Input::KeyboardKey(VirtualKeyCode::W);
        let shift_key = Input::KeyboardKey(VirtualKeyCode::LShift);
        let ctrl_key = Input::KeyboardKey(VirtualKeyCode::RControl);

        let mut state: State = Default::default();
        let bindings = Bindings::new("test").with_bind("Shift+w", "player.never_executed()")?;

        state.input_states.insert(ctrl_key, ElementState::Pressed);
        state.input_states.insert(shift_key, ElementState::Pressed);
        state.modifiers_state |= ModifiersState::CTRL;
        state.modifiers_state |= ModifiersState::SHIFT;
        state.input_states.insert(w_key, ElementState::Pressed);
        bindings.match_input(
            w_key,
            Some(ElementState::Pressed),
            &mut state,
            &LocalNamespace::empty(),
            &mut herder,
        )?;
        assert!(!player.read().walking);

        Ok(())
    }

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
}
