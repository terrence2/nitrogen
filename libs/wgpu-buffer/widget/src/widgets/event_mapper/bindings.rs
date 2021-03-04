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
use crate::widgets::event_mapper::{
    axis::AxisKind,
    keyset::{Key, KeySet},
    window::WindowEventKind,
    State,
};
use failure::Fallible;
use input::ElementState;
use log::trace;
use nitrous::{Interpreter, Script, Value};
use nitrous_injector::{inject_nitrous_module, method, NitrousModule};
use parking_lot::RwLock;
use smallvec::{smallvec, SmallVec};
use std::{collections::HashMap, sync::Arc};

// Map from key, buttons, and axes to commands.
#[derive(Debug, NitrousModule)]
pub struct Bindings {
    pub name: String,
    press_chords: HashMap<Key, Vec<KeySet>>,
    script_map: HashMap<KeySet, Script>,
    axis_map: HashMap<AxisKind, Script>,
    windows_event_map: HashMap<WindowEventKind, Script>,
}

#[inject_nitrous_module]
impl Bindings {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            press_chords: HashMap::new(),
            script_map: HashMap::new(),
            axis_map: HashMap::new(),
            windows_event_map: HashMap::new(),
        }
    }

    pub fn with_bind(mut self, keyset_or_axis: &str, script_raw: &str) -> Fallible<Self> {
        self.bind(keyset_or_axis, script_raw)?;
        Ok(self)
    }

    #[method]
    pub fn bind(&mut self, event_name: &str, script_raw: &str) -> Fallible<()> {
        let script = Script::compile(script_raw)?;

        if let Ok(kind) = WindowEventKind::from_virtual(event_name) {
            trace!("binding {:?} => {}", kind, script);
            self.windows_event_map.insert(kind, script);
            return Ok(());
        }

        if let Ok(axis) = AxisKind::from_virtual(event_name) {
            trace!("binding {:?} => {}", axis, script);
            self.axis_map.insert(axis, script);
            return Ok(());
        }

        for ks in KeySet::from_virtual(event_name)?.drain(..) {
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

    pub fn match_window_event(
        &self,
        event: WindowEventKind,
        interpreter: &mut Interpreter,
    ) -> Fallible<()> {
        if let Some(script) = self.windows_event_map.get(&event) {
            interpreter.interpret(script)?;
        }
        Ok(())
    }

    pub fn match_axis(&self, axis: AxisKind, interpreter: &mut Interpreter) -> Fallible<()> {
        if let Some(script) = self.axis_map.get(&axis) {
            interpreter.interpret(script)?;
        }
        Ok(())
    }

    pub fn match_key(
        &self,
        key: Key,
        key_state: ElementState,
        state: &mut State,
        interpreter: Arc<RwLock<Interpreter>>,
    ) -> Fallible<()> {
        match key_state {
            ElementState::Pressed => self.handle_press(key, state, interpreter)?,
            ElementState::Released => self.handle_release(key, state, interpreter)?,
        }
        Ok(())
    }

    fn handle_press(
        &self,
        key: Key,
        state: &mut State,
        interpreter: Arc<RwLock<Interpreter>>,
    ) -> Fallible<()> {
        // The press chords gives us a quick map from a key press to all chords which could become
        // active in the case that it is pressed so that we don't have to look at everything.
        if let Some(possible_chord_list) = self.press_chords.get(&key) {
            for chord in possible_chord_list {
                if chord.is_pressed(&state) {
                    self.maybe_activate_chord(chord, state, interpreter.clone())?;
                }
            }
        }
        Ok(())
    }

    fn chord_is_masked(chord: &KeySet, state: &State) -> bool {
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
        state: &mut State,
        interpreter: Arc<RwLock<Interpreter>>,
    ) -> Fallible<()> {
        // We may have multiple binding sets active for the same KeySet, in which case the first
        // binding in the set wins and checks for subsequent activations should exit early.
        if state.active_chords.contains(&chord) {
            return Ok(());
        }

        // The press_chords list implicitly filters out keys not in this bindings.
        assert!(self.script_map.contains_key(chord));

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
            if let Some(script) = self.script_map.get(masked) {
                self.deactiveate_chord(script, interpreter.clone())?;
            }
        }

        // Activate the chord and run the command.
        state.active_chords.insert(chord.to_owned());
        self.activate_chord(&self.script_map[chord], interpreter)?;

        Ok(())
    }

    fn handle_release(
        &self,
        key: Key,
        state: &mut State,
        interpreter: Arc<RwLock<Interpreter>>,
    ) -> Fallible<()> {
        // Remove any chords that have been released.
        let mut released_chords: SmallVec<[KeySet; 4]> = smallvec![];
        for active_chord in &state.active_chords {
            if active_chord.contains_key(&key) {
                // Note: unlike with press, we do not implicitly filter out keys we don't care about.
                if let Some(script) = self.script_map.get(active_chord) {
                    released_chords.push(active_chord.to_owned());
                    self.deactiveate_chord(script, interpreter.clone())?;
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
                if chord.is_subset_of(released_chord) && chord.is_pressed(&state) {
                    state.active_chords.insert(chord.to_owned());
                    self.activate_chord(script, interpreter.clone())?;
                }
            }
        }

        Ok(())
    }

    fn activate_chord(
        &self,
        script: &Script,
        interpreter: Arc<RwLock<Interpreter>>,
    ) -> Fallible<()> {
        interpreter
            .write()
            .with_locals(&[("pressed", Value::True())], |inner| {
                inner.interpret(script)
            })?;
        Ok(())
    }

    fn deactiveate_chord(
        &self,
        script: &Script,
        interpreter: Arc<RwLock<Interpreter>>,
    ) -> Fallible<()> {
        interpreter
            .write()
            .with_locals(&[("pressed", Value::False())], |inner| {
                inner.interpret(script)
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use failure::bail;
    use input::{ModifiersState, VirtualKeyCode};
    use nitrous::{Module, Value};

    #[derive(Debug, Default)]
    struct Player {
        walking: bool,
        running: bool,
    }

    impl Module for Player {
        fn module_name(&self) -> String {
            "Player".to_owned()
        }

        fn call_method(&mut self, name: &str, args: &[Value]) -> Fallible<Value> {
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
        ) -> Fallible<()> {
            unimplemented!()
        }

        fn get(&self, module: Arc<RwLock<dyn Module>>, name: &str) -> Fallible<Value> {
            Ok(match name {
                "walk" => Value::Method(module, name.to_owned()),
                "run" => Value::Method(module, name.to_owned()),
                _ => bail!("get unknown '{}'", name),
            })
        }
    }

    #[test]
    fn test_modifier_planes_disable_bare() -> Fallible<()> {
        let interpreter = Interpreter::default().init()?;
        let player = Arc::new(RwLock::new(Player::default()));
        interpreter
            .write()
            .put(interpreter.clone(), "player", Value::Module(player.clone()))?;

        let w_key = Key::KeyboardKey(VirtualKeyCode::W);
        let shift_key = Key::KeyboardKey(VirtualKeyCode::LShift);

        let mut state: State = Default::default();
        let bindings = Bindings::new("test").with_bind("w", "player.never_executed()")?;

        state.key_states.insert(shift_key, ElementState::Pressed);
        state.modifiers_state |= ModifiersState::SHIFT;
        bindings.match_key(
            shift_key,
            ElementState::Pressed,
            &mut state,
            interpreter.clone(),
        )?;
        assert_eq!(player.read().walking, false);

        state.key_states.insert(w_key, ElementState::Pressed);
        bindings.match_key(w_key, ElementState::Pressed, &mut state, interpreter)?;
        assert_eq!(player.read().walking, false);

        Ok(())
    }

    #[test]
    fn test_matches_exact_modifier_plane() -> Fallible<()> {
        let interpreter = Interpreter::default().init()?;
        let player = Arc::new(RwLock::new(Player::default()));
        interpreter
            .write()
            .put(interpreter.clone(), "player", Value::Module(player.clone()))?;

        let w_key = Key::KeyboardKey(VirtualKeyCode::W);
        let shift_key = Key::KeyboardKey(VirtualKeyCode::LShift);
        let ctrl_key = Key::KeyboardKey(VirtualKeyCode::RControl);

        let mut state: State = Default::default();
        let bindings = Bindings::new("test").with_bind("Shift+w", "player.never_executed()")?;

        state.key_states.insert(ctrl_key, ElementState::Pressed);
        state.key_states.insert(shift_key, ElementState::Pressed);
        state.modifiers_state |= ModifiersState::CTRL;
        state.modifiers_state |= ModifiersState::SHIFT;
        state.key_states.insert(w_key, ElementState::Pressed);
        bindings.match_key(w_key, ElementState::Pressed, &mut state, interpreter)?;
        assert!(!player.read().walking);

        Ok(())
    }

    #[test]
    fn test_masking() -> Fallible<()> {
        let interpreter = Interpreter::default().init()?;
        let player = Arc::new(RwLock::new(Player::default()));
        interpreter
            .write()
            .put(interpreter.clone(), "player", Value::Module(player.clone()))?;

        let w_key = Key::KeyboardKey(VirtualKeyCode::W);
        let shift_key = Key::KeyboardKey(VirtualKeyCode::LShift);

        let mut state: State = Default::default();
        let bindings = Bindings::new("test")
            .with_bind("w", "player.walk(pressed)")?
            .with_bind("shift+w", "player.run(pressed)")?;

        state.key_states.insert(w_key, ElementState::Pressed);
        bindings.match_key(
            w_key,
            ElementState::Pressed,
            &mut state,
            interpreter.clone(),
        )?;
        assert!(player.read().walking);
        assert!(!player.read().running);

        state.key_states.insert(shift_key, ElementState::Pressed);
        state.modifiers_state |= ModifiersState::SHIFT;
        bindings.match_key(
            shift_key,
            ElementState::Pressed,
            &mut state,
            interpreter.clone(),
        )?;
        assert!(!player.read().walking);
        assert!(player.read().running);

        state.key_states.insert(shift_key, ElementState::Released);
        state.modifiers_state -= ModifiersState::SHIFT;
        bindings.match_key(
            shift_key,
            ElementState::Released,
            &mut state,
            interpreter.clone(),
        )?;
        assert!(player.read().walking);
        assert!(!player.read().running);

        state.key_states.insert(shift_key, ElementState::Pressed);
        state.modifiers_state |= ModifiersState::SHIFT;
        bindings.match_key(
            shift_key,
            ElementState::Pressed,
            &mut state,
            interpreter.clone(),
        )?;
        assert!(!player.read().walking);
        assert!(player.read().running);

        state.key_states.insert(w_key, ElementState::Released);
        bindings.match_key(
            w_key,
            ElementState::Released,
            &mut state,
            interpreter.clone(),
        )?;
        assert!(!player.read().walking);
        assert!(!player.read().running);

        state.key_states.insert(w_key, ElementState::Pressed);
        bindings.match_key(w_key, ElementState::Pressed, &mut state, interpreter)?;
        assert!(!player.read().walking);
        assert!(player.read().running);

        Ok(())
    }
}
