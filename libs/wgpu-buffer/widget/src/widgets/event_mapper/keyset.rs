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
use crate::widgets::event_mapper::State;
use anyhow::{bail, ensure, Result};
use input::{ButtonId, ElementState, ModifiersState, VirtualKeyCode};
use log::warn;
use once_cell::sync::Lazy;
use smallvec::SmallVec;
use std::{
    collections::{HashMap, HashSet},
    fmt,
};
use unicase::{eq_ascii, Ascii};

// When providing keys via a typed in command ala `bind +moveleft a`, we are
// talking about a virtual key name. When we poke a key in order to set a bind
// in the gui, we want to capture the actual scancode, because we have no idea
// what's painted on the front of the keycap.
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum Key {
    KeyboardKey(VirtualKeyCode),
    MouseButton(ButtonId),
}

static MIRROR_MODIFIERS: Lazy<HashSet<Ascii<&'static str>>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert(Ascii::new("Control"));
    s.insert(Ascii::new("Alt"));
    s.insert(Ascii::new("Win"));
    s.insert(Ascii::new("Shift"));
    s
});

#[rustfmt::skip]
static KEYCODES: Lazy<HashMap<Ascii<&'static str>, Key>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert(Ascii::new("A"), Key::KeyboardKey(VirtualKeyCode::A));
    m.insert(Ascii::new("B"), Key::KeyboardKey(VirtualKeyCode::B));
    m.insert(Ascii::new("C"), Key::KeyboardKey(VirtualKeyCode::C));
    m.insert(Ascii::new("D"), Key::KeyboardKey(VirtualKeyCode::D));
    m.insert(Ascii::new("E"), Key::KeyboardKey(VirtualKeyCode::E));
    m.insert(Ascii::new("F"), Key::KeyboardKey(VirtualKeyCode::F));
    m.insert(Ascii::new("G"), Key::KeyboardKey(VirtualKeyCode::G));
    m.insert(Ascii::new("H"), Key::KeyboardKey(VirtualKeyCode::H));
    m.insert(Ascii::new("I"), Key::KeyboardKey(VirtualKeyCode::I));
    m.insert(Ascii::new("J"), Key::KeyboardKey(VirtualKeyCode::J));
    m.insert(Ascii::new("K"), Key::KeyboardKey(VirtualKeyCode::K));
    m.insert(Ascii::new("L"), Key::KeyboardKey(VirtualKeyCode::L));
    m.insert(Ascii::new("M"), Key::KeyboardKey(VirtualKeyCode::M));
    m.insert(Ascii::new("N"), Key::KeyboardKey(VirtualKeyCode::N));
    m.insert(Ascii::new("O"), Key::KeyboardKey(VirtualKeyCode::O));
    m.insert(Ascii::new("P"), Key::KeyboardKey(VirtualKeyCode::P));
    m.insert(Ascii::new("Q"), Key::KeyboardKey(VirtualKeyCode::Q));
    m.insert(Ascii::new("R"), Key::KeyboardKey(VirtualKeyCode::R));
    m.insert(Ascii::new("S"), Key::KeyboardKey(VirtualKeyCode::S));
    m.insert(Ascii::new("T"), Key::KeyboardKey(VirtualKeyCode::T));
    m.insert(Ascii::new("U"), Key::KeyboardKey(VirtualKeyCode::U));
    m.insert(Ascii::new("V"), Key::KeyboardKey(VirtualKeyCode::V));
    m.insert(Ascii::new("W"), Key::KeyboardKey(VirtualKeyCode::W));
    m.insert(Ascii::new("X"), Key::KeyboardKey(VirtualKeyCode::X));
    m.insert(Ascii::new("Y"), Key::KeyboardKey(VirtualKeyCode::Y));
    m.insert(Ascii::new("Z"), Key::KeyboardKey(VirtualKeyCode::Z));
    m.insert(Ascii::new("Key1"), Key::KeyboardKey(VirtualKeyCode::Key1));
    m.insert(Ascii::new("Key2"), Key::KeyboardKey(VirtualKeyCode::Key2));
    m.insert(Ascii::new("Key3"), Key::KeyboardKey(VirtualKeyCode::Key3));
    m.insert(Ascii::new("Key4"), Key::KeyboardKey(VirtualKeyCode::Key4));
    m.insert(Ascii::new("Key5"), Key::KeyboardKey(VirtualKeyCode::Key5));
    m.insert(Ascii::new("Key6"), Key::KeyboardKey(VirtualKeyCode::Key6));
    m.insert(Ascii::new("Key7"), Key::KeyboardKey(VirtualKeyCode::Key7));
    m.insert(Ascii::new("Key8"), Key::KeyboardKey(VirtualKeyCode::Key8));
    m.insert(Ascii::new("Key9"), Key::KeyboardKey(VirtualKeyCode::Key9));
    m.insert(Ascii::new("Key0"), Key::KeyboardKey(VirtualKeyCode::Key0));
    m.insert(Ascii::new("Escape"), Key::KeyboardKey(VirtualKeyCode::Escape));
    m.insert(Ascii::new("F1"), Key::KeyboardKey(VirtualKeyCode::F1));
    m.insert(Ascii::new("F2"), Key::KeyboardKey(VirtualKeyCode::F2));
    m.insert(Ascii::new("F3"), Key::KeyboardKey(VirtualKeyCode::F3));
    m.insert(Ascii::new("F4"), Key::KeyboardKey(VirtualKeyCode::F4));
    m.insert(Ascii::new("F5"), Key::KeyboardKey(VirtualKeyCode::F5));
    m.insert(Ascii::new("F6"), Key::KeyboardKey(VirtualKeyCode::F6));
    m.insert(Ascii::new("F7"), Key::KeyboardKey(VirtualKeyCode::F7));
    m.insert(Ascii::new("F8"), Key::KeyboardKey(VirtualKeyCode::F8));
    m.insert(Ascii::new("F9"), Key::KeyboardKey(VirtualKeyCode::F9));
    m.insert(Ascii::new("F10"), Key::KeyboardKey(VirtualKeyCode::F10));
    m.insert(Ascii::new("F11"), Key::KeyboardKey(VirtualKeyCode::F11));
    m.insert(Ascii::new("F12"), Key::KeyboardKey(VirtualKeyCode::F12));
    m.insert(Ascii::new("F13"), Key::KeyboardKey(VirtualKeyCode::F13));
    m.insert(Ascii::new("F14"), Key::KeyboardKey(VirtualKeyCode::F14));
    m.insert(Ascii::new("F15"), Key::KeyboardKey(VirtualKeyCode::F15));
    m.insert(Ascii::new("F16"), Key::KeyboardKey(VirtualKeyCode::F16));
    m.insert(Ascii::new("F17"), Key::KeyboardKey(VirtualKeyCode::F17));
    m.insert(Ascii::new("F18"), Key::KeyboardKey(VirtualKeyCode::F18));
    m.insert(Ascii::new("F19"), Key::KeyboardKey(VirtualKeyCode::F19));
    m.insert(Ascii::new("F20"), Key::KeyboardKey(VirtualKeyCode::F20));
    m.insert(Ascii::new("F21"), Key::KeyboardKey(VirtualKeyCode::F21));
    m.insert(Ascii::new("F22"), Key::KeyboardKey(VirtualKeyCode::F22));
    m.insert(Ascii::new("F23"), Key::KeyboardKey(VirtualKeyCode::F23));
    m.insert(Ascii::new("F24"), Key::KeyboardKey(VirtualKeyCode::F24));
    m.insert(Ascii::new("Snapshot"), Key::KeyboardKey(VirtualKeyCode::Snapshot));
    m.insert(Ascii::new("Scroll"), Key::KeyboardKey(VirtualKeyCode::Scroll));
    m.insert(Ascii::new("Pause"), Key::KeyboardKey(VirtualKeyCode::Pause));
    m.insert(Ascii::new("Insert"), Key::KeyboardKey(VirtualKeyCode::Insert));
    m.insert(Ascii::new("Home"), Key::KeyboardKey(VirtualKeyCode::Home));
    m.insert(Ascii::new("Delete"), Key::KeyboardKey(VirtualKeyCode::Delete));
    m.insert(Ascii::new("End"), Key::KeyboardKey(VirtualKeyCode::End));
    m.insert(Ascii::new("PageDown"), Key::KeyboardKey(VirtualKeyCode::PageDown));
    m.insert(Ascii::new("PageUp"), Key::KeyboardKey(VirtualKeyCode::PageUp));
    m.insert(Ascii::new("Left"), Key::KeyboardKey(VirtualKeyCode::Left));
    m.insert(Ascii::new("Up"), Key::KeyboardKey(VirtualKeyCode::Up));
    m.insert(Ascii::new("Right"), Key::KeyboardKey(VirtualKeyCode::Right));
    m.insert(Ascii::new("Down"), Key::KeyboardKey(VirtualKeyCode::Down));
    m.insert(Ascii::new("Back"), Key::KeyboardKey(VirtualKeyCode::Back));
    m.insert(Ascii::new("Return"), Key::KeyboardKey(VirtualKeyCode::Return));
    m.insert(Ascii::new("Space"), Key::KeyboardKey(VirtualKeyCode::Space));
    m.insert(Ascii::new("Compose"), Key::KeyboardKey(VirtualKeyCode::Compose));
    m.insert(Ascii::new("Caret"), Key::KeyboardKey(VirtualKeyCode::Caret));
    m.insert(Ascii::new("Numlock"), Key::KeyboardKey(VirtualKeyCode::Numlock));
    m.insert(Ascii::new("Numpad0"), Key::KeyboardKey(VirtualKeyCode::Numpad0));
    m.insert(Ascii::new("Numpad1"), Key::KeyboardKey(VirtualKeyCode::Numpad1));
    m.insert(Ascii::new("Numpad2"), Key::KeyboardKey(VirtualKeyCode::Numpad2));
    m.insert(Ascii::new("Numpad3"), Key::KeyboardKey(VirtualKeyCode::Numpad3));
    m.insert(Ascii::new("Numpad4"), Key::KeyboardKey(VirtualKeyCode::Numpad4));
    m.insert(Ascii::new("Numpad5"), Key::KeyboardKey(VirtualKeyCode::Numpad5));
    m.insert(Ascii::new("Numpad6"), Key::KeyboardKey(VirtualKeyCode::Numpad6));
    m.insert(Ascii::new("Numpad7"), Key::KeyboardKey(VirtualKeyCode::Numpad7));
    m.insert(Ascii::new("Numpad8"), Key::KeyboardKey(VirtualKeyCode::Numpad8));
    m.insert(Ascii::new("Numpad9"), Key::KeyboardKey(VirtualKeyCode::Numpad9));
    m.insert(Ascii::new("AbntC1"), Key::KeyboardKey(VirtualKeyCode::AbntC1));
    m.insert(Ascii::new("AbntC2"), Key::KeyboardKey(VirtualKeyCode::AbntC2));
    m.insert(Ascii::new("NumpadAdd"), Key::KeyboardKey(VirtualKeyCode::NumpadAdd));
    m.insert(Ascii::new("NumpadComma"), Key::KeyboardKey(VirtualKeyCode::NumpadComma));
    m.insert(Ascii::new("NumpadDecimal"), Key::KeyboardKey(VirtualKeyCode::NumpadDecimal));
    m.insert(Ascii::new("NumpadDivide"), Key::KeyboardKey(VirtualKeyCode::NumpadDivide));
    m.insert(Ascii::new("NumpadEnter"), Key::KeyboardKey(VirtualKeyCode::NumpadEnter));
    m.insert(Ascii::new("NumpadEquals"), Key::KeyboardKey(VirtualKeyCode::NumpadEquals));
    m.insert(Ascii::new("NumpadMultiply"), Key::KeyboardKey(VirtualKeyCode::NumpadMultiply));
    m.insert(Ascii::new("NumpadSubtract"), Key::KeyboardKey(VirtualKeyCode::NumpadSubtract));
    m.insert(Ascii::new("Apostrophe"), Key::KeyboardKey(VirtualKeyCode::Apostrophe));
    m.insert(Ascii::new("Apps"), Key::KeyboardKey(VirtualKeyCode::Apps));
    m.insert(Ascii::new("At"), Key::KeyboardKey(VirtualKeyCode::At));
    m.insert(Ascii::new("Ax"), Key::KeyboardKey(VirtualKeyCode::Ax));
    m.insert(Ascii::new("Backslash"), Key::KeyboardKey(VirtualKeyCode::Backslash));
    m.insert(Ascii::new("Calculator"), Key::KeyboardKey(VirtualKeyCode::Calculator));
    m.insert(Ascii::new("Capital"), Key::KeyboardKey(VirtualKeyCode::Capital));
    m.insert(Ascii::new("Colon"), Key::KeyboardKey(VirtualKeyCode::Colon));
    m.insert(Ascii::new("Comma"), Key::KeyboardKey(VirtualKeyCode::Comma));
    m.insert(Ascii::new("Convert"), Key::KeyboardKey(VirtualKeyCode::Convert));
    m.insert(Ascii::new("Equals"), Key::KeyboardKey(VirtualKeyCode::Equals));
    m.insert(Ascii::new("Grave"), Key::KeyboardKey(VirtualKeyCode::Grave));
    m.insert(Ascii::new("Kana"), Key::KeyboardKey(VirtualKeyCode::Kana));
    m.insert(Ascii::new("Kanji"), Key::KeyboardKey(VirtualKeyCode::Kanji));
    m.insert(Ascii::new("LAlt"), Key::KeyboardKey(VirtualKeyCode::LAlt));
    m.insert(Ascii::new("LBracket"), Key::KeyboardKey(VirtualKeyCode::LBracket));
    m.insert(Ascii::new("LControl"), Key::KeyboardKey(VirtualKeyCode::LControl));
    m.insert(Ascii::new("LShift"), Key::KeyboardKey(VirtualKeyCode::LShift));
    m.insert(Ascii::new("LWin"), Key::KeyboardKey(VirtualKeyCode::LWin));
    m.insert(Ascii::new("Mail"), Key::KeyboardKey(VirtualKeyCode::Mail));
    m.insert(Ascii::new("MediaSelect"), Key::KeyboardKey(VirtualKeyCode::MediaSelect));
    m.insert(Ascii::new("MediaStop"), Key::KeyboardKey(VirtualKeyCode::MediaStop));
    m.insert(Ascii::new("Minus"), Key::KeyboardKey(VirtualKeyCode::Minus));
    m.insert(Ascii::new("Mute"), Key::KeyboardKey(VirtualKeyCode::Mute));
    m.insert(Ascii::new("MyComputer"), Key::KeyboardKey(VirtualKeyCode::MyComputer));
    m.insert(Ascii::new("NavigateForward"), Key::KeyboardKey(VirtualKeyCode::NavigateForward));
    m.insert(Ascii::new("NavigateBackward"), Key::KeyboardKey(VirtualKeyCode::NavigateBackward));
    m.insert(Ascii::new("NextTrack"), Key::KeyboardKey(VirtualKeyCode::NextTrack));
    m.insert(Ascii::new("NoConvert"), Key::KeyboardKey(VirtualKeyCode::NoConvert));
    m.insert(Ascii::new("OEM102"), Key::KeyboardKey(VirtualKeyCode::OEM102));
    m.insert(Ascii::new("Period"), Key::KeyboardKey(VirtualKeyCode::Period));
    m.insert(Ascii::new("PlayPause"), Key::KeyboardKey(VirtualKeyCode::PlayPause));
    m.insert(Ascii::new("Power"), Key::KeyboardKey(VirtualKeyCode::Power));
    m.insert(Ascii::new("PrevTrack"), Key::KeyboardKey(VirtualKeyCode::PrevTrack));
    m.insert(Ascii::new("RAlt"), Key::KeyboardKey(VirtualKeyCode::RAlt));
    m.insert(Ascii::new("RBracket"), Key::KeyboardKey(VirtualKeyCode::RBracket));
    m.insert(Ascii::new("RControl"), Key::KeyboardKey(VirtualKeyCode::RControl));
    m.insert(Ascii::new("RShift"), Key::KeyboardKey(VirtualKeyCode::RShift));
    m.insert(Ascii::new("RWin"), Key::KeyboardKey(VirtualKeyCode::RWin));
    m.insert(Ascii::new("Semicolon"), Key::KeyboardKey(VirtualKeyCode::Semicolon));
    m.insert(Ascii::new("Slash"), Key::KeyboardKey(VirtualKeyCode::Slash));
    m.insert(Ascii::new("Sleep"), Key::KeyboardKey(VirtualKeyCode::Sleep));
    m.insert(Ascii::new("Stop"), Key::KeyboardKey(VirtualKeyCode::Stop));
    m.insert(Ascii::new("Sysrq"), Key::KeyboardKey(VirtualKeyCode::Sysrq));
    m.insert(Ascii::new("Tab"), Key::KeyboardKey(VirtualKeyCode::Tab));
    m.insert(Ascii::new("Underline"), Key::KeyboardKey(VirtualKeyCode::Underline));
    m.insert(Ascii::new("Unlabeled"), Key::KeyboardKey(VirtualKeyCode::Unlabeled));
    m.insert(Ascii::new("VolumeDown"), Key::KeyboardKey(VirtualKeyCode::VolumeDown));
    m.insert(Ascii::new("VolumeUp"), Key::KeyboardKey(VirtualKeyCode::VolumeUp));
    m.insert(Ascii::new("Wake"), Key::KeyboardKey(VirtualKeyCode::Wake));
    m.insert(Ascii::new("WebBack"), Key::KeyboardKey(VirtualKeyCode::WebBack));
    m.insert(Ascii::new("WebFavorites"), Key::KeyboardKey(VirtualKeyCode::WebFavorites));
    m.insert(Ascii::new("WebForward"), Key::KeyboardKey(VirtualKeyCode::WebForward));
    m.insert(Ascii::new("WebHome"), Key::KeyboardKey(VirtualKeyCode::WebHome));
    m.insert(Ascii::new("WebRefresh"), Key::KeyboardKey(VirtualKeyCode::WebRefresh));
    m.insert(Ascii::new("WebSearch"), Key::KeyboardKey(VirtualKeyCode::WebSearch));
    m.insert(Ascii::new("WebStop"), Key::KeyboardKey(VirtualKeyCode::WebStop));
    m.insert(Ascii::new("Yen"), Key::KeyboardKey(VirtualKeyCode::Yen));
    m.insert(Ascii::new("Copy"), Key::KeyboardKey(VirtualKeyCode::Copy));
    m.insert(Ascii::new("Paste"), Key::KeyboardKey(VirtualKeyCode::Paste));
    m.insert(Ascii::new("Cut"), Key::KeyboardKey(VirtualKeyCode::Cut));
    m
});

impl Key {
    pub fn from_virtual(s: &str) -> Result<Self> {
        if let Some(key) = KEYCODES.get(&Ascii::new(s)) {
            return Ok(*key);
        }
        if s.len() > 5 && eq_ascii(&s[0..5], "mouse") {
            let button = s[5..].parse::<u32>()?;
            return Ok(Key::MouseButton(button));
        }
        bail!("unknown virtual keycode")
    }

    pub fn modifier(&self) -> ModifiersState {
        match self {
            Key::KeyboardKey(vkey) => match vkey {
                VirtualKeyCode::LControl => ModifiersState::CTRL,
                VirtualKeyCode::RControl => ModifiersState::CTRL,
                VirtualKeyCode::LShift => ModifiersState::SHIFT,
                VirtualKeyCode::RShift => ModifiersState::SHIFT,
                VirtualKeyCode::LAlt => ModifiersState::ALT,
                VirtualKeyCode::RAlt => ModifiersState::ALT,
                VirtualKeyCode::LWin => ModifiersState::LOGO,
                VirtualKeyCode::RWin => ModifiersState::LOGO,
                _ => ModifiersState::default(),
            },
            _ => ModifiersState::default(),
        }
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct KeySet {
    pub keys: SmallVec<[Key; 2]>,
    pub modifiers: ModifiersState,
}

impl KeySet {
    // Parse keysets of the form a+b+c; e.g. LControl+RControl+Space into
    // a discreet keyset.
    //
    // Note that there is a special case for the 4 modifiers in which we
    // expect to be able to refer to "Control" and not care what key it is.
    // In this case we emit all possible keysets, combinatorially.
    pub fn from_virtual(keyset: &str) -> Result<Vec<Self>> {
        let mut out = vec![SmallVec::<[Key; 2]>::new()];
        for keyname in keyset.split('+') {
            if let Ok(key) = Key::from_virtual(keyname) {
                for tmp in &mut out {
                    tmp.push(key);
                }
            } else if MIRROR_MODIFIERS.contains(&Ascii::new(keyname)) {
                let mut next_out = Vec::new();
                for mut tmp in out.drain(..) {
                    let mut cpy = tmp.clone();
                    tmp.push(Key::from_virtual(&format!("L{}", keyname))?);
                    cpy.push(Key::from_virtual(&format!("R{}", keyname))?);
                    next_out.push(tmp);
                    next_out.push(cpy);
                }
                out = next_out;
            } else {
                println!("attempting to lookup unknown key name: {}", keyname);
                warn!("unknown key name: {}", keyname);
            }
        }
        ensure!(!out.is_empty(), "no key matching {}", keyset);
        Ok(out
            .drain(..)
            .map(|v| Self {
                modifiers: v.iter().fold(ModifiersState::default(), |modifiers, k| {
                    modifiers | k.modifier()
                }),
                keys: v,
            })
            .collect::<Vec<_>>())
    }

    pub fn contains_key(&self, key: &Key) -> bool {
        for own_key in &self.keys {
            if key == own_key {
                return true;
            }
        }
        false
    }

    pub fn is_subset_of(&self, other: &KeySet) -> bool {
        if self.keys.len() >= other.keys.len() {
            return false;
        }
        for key in &other.keys {
            if !other.keys.contains(key) {
                return false;
            }
        }
        true
    }

    pub fn is_pressed(&self, state: &State) -> bool {
        // We want to account for:
        //   * `Ctrl+e` && `Ctrl+o` being activated with a single hold of the Ctrl key.
        //   * Multiple actions can run at once: e.g. if someone is holding `w` to walk
        //     forward, they should still be able to tap `e` to use.
        //   * On the other hand, `Shift+e` is a superset of `e`, but should not trigger
        //     `e`'s action.
        //   * Potential weird case: what if `Ctrl+w` is walk forward and someone taps `e` to use?
        //     Should it depend on whether `Ctrl+e` is bound? I think we should disallow for now.
        //     Most apps treat modifiers as different planes of keys and I think that makes sense
        //     here as well.
        //
        // Simple solution: superset check, with special handling of modifier keys.
        for key in &self.keys {
            if let Some(current_state) = state.key_states.get(key) {
                if *current_state == ElementState::Released {
                    return false;
                }
            } else {
                return false;
            }
        }

        // All matching keys (including modifiers) are pressed.
        // Also make sure we are in the same modifier plan.
        if state.modifiers_state != self.modifiers {
            return false;
        }

        true
    }
}

impl fmt::Display for KeySet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{")?;
        for (i, key) in self.keys.iter().enumerate() {
            if i != 0 {
                write!(f, ",")?;
            }
            write!(f, "{:?}", key)?;
        }
        write!(f, "}}")
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_can_create_keys() -> Result<()> {
        assert_eq!(Key::from_virtual("A")?, Key::KeyboardKey(VirtualKeyCode::A));
        assert_eq!(Key::from_virtual("a")?, Key::KeyboardKey(VirtualKeyCode::A));
        assert_eq!(
            Key::from_virtual("PageUp")?,
            Key::KeyboardKey(VirtualKeyCode::PageUp)
        );
        assert_eq!(
            Key::from_virtual("pageup")?,
            Key::KeyboardKey(VirtualKeyCode::PageUp)
        );
        assert_eq!(
            Key::from_virtual("pAgEuP")?,
            Key::KeyboardKey(VirtualKeyCode::PageUp)
        );
        Ok(())
    }

    #[test]
    fn test_can_create_mouse() -> Result<()> {
        assert_eq!(Key::from_virtual("MoUsE5000")?, Key::MouseButton(5000));
        Ok(())
    }

    #[test]
    fn test_can_create_keysets() -> Result<()> {
        assert_eq!(KeySet::from_virtual("a+b")?.len(), 1);
        assert_eq!(KeySet::from_virtual("Control+Win+a")?.len(), 4);
        assert_eq!(KeySet::from_virtual("Control+b+Shift")?.len(), 4);
        Ok(())
    }
}
