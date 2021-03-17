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
use input::{
    ButtonId, ElementState, GenericEvent, GenericSystemEvent, GenericWindowEvent, ModifiersState,
    VirtualKeyCode,
};
use log::warn;
use once_cell::sync::Lazy;
use smallvec::SmallVec;
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    fmt,
};
use unicase::Ascii;

/// This enum is the meeting place between "bindings" on one side -- canonically strings specified
/// by the user -- and "events" on the other side -- which are produced by the input system in
/// response to key presses and such. This enum can be created from either of those sources, thus
/// bridging the gap in a nice, strongly typed way.
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum Input {
    KeyboardKey(VirtualKeyCode),
    MouseButton(ButtonId),
    JoystickButton(ButtonId),
    Axis(AxisInput),
    Window(WindowInput),
    System(SystemInput),
}

impl Input {
    pub fn from_event(event: &GenericEvent) -> Option<Self> {
        Some(match event {
            GenericEvent::KeyboardKey {
                virtual_keycode, ..
            } => Input::KeyboardKey(*virtual_keycode),
            GenericEvent::MouseButton { button, .. } => Input::MouseButton(*button),
            GenericEvent::MouseMotion { .. } => Input::Axis(AxisInput::MouseMotion),
            GenericEvent::MouseWheel { .. } => Input::Axis(AxisInput::MouseWheel),
            GenericEvent::JoystickButton { dummy, .. } => Input::JoystickButton(*dummy),
            GenericEvent::JoystickAxis { id, .. } => Input::Axis(AxisInput::JoystickAxis(*id)),
            GenericEvent::Window(event) => Input::Window(WindowInput::from_event(event)),
            GenericEvent::System(event) => Input::System(SystemInput::from_event(event)),
            GenericEvent::CursorMove { .. } => return None,
        })
    }
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum AxisInput {
    MouseMotion,
    MouseWheel,
    JoystickAxis(u32),
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum WindowInput {
    Resized,
    DpiChanged,
}

impl WindowInput {
    pub fn from_event(event: &GenericWindowEvent) -> Self {
        match event {
            GenericWindowEvent::Resized { .. } => Self::Resized,
            GenericWindowEvent::ScaleFactorChanged { .. } => Self::DpiChanged,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum SystemInput {
    Quit,
    DeviceAdded,
    DeviceRemoved,
}
impl SystemInput {
    pub fn from_event(event: &GenericSystemEvent) -> Self {
        match event {
            GenericSystemEvent::Quit => Self::Quit,
            GenericSystemEvent::DeviceAdded { .. } => Self::DeviceAdded,
            GenericSystemEvent::DeviceRemoved { .. } => Self::DeviceRemoved,
        }
    }
}

static MIRROR_MODIFIERS: Lazy<HashSet<Ascii<&'static str>>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert(Ascii::new("Control"));
    s.insert(Ascii::new("Alt"));
    s.insert(Ascii::new("Win"));
    s.insert(Ascii::new("Shift"));
    s.insert(Ascii::new("Any"));
    s
});

fn ascii(s: &'static str) -> Ascii<Cow<'static, str>> {
    Ascii::new(Cow::from(s))
}

#[rustfmt::skip]
static BIND_MAP: Lazy<HashMap<Ascii<Cow<'static, str>>, Input>> = Lazy::new(|| {
    let mut m = HashMap::new();
    // System
    m.insert(ascii("quit"), Input::System(SystemInput::Quit));
    m.insert(ascii("deviceAdded"), Input::System(SystemInput::DeviceAdded));
    m.insert(ascii("deviceRemoved"), Input::System(SystemInput::DeviceRemoved));
    // Window
    m.insert(ascii("windowResized"), Input::Window(WindowInput::Resized));
    m.insert(ascii("windowDpiChanged"), Input::Window(WindowInput::DpiChanged));
    // Mouse
    m.insert(ascii("mouseMotion"), Input::Axis(AxisInput::MouseMotion));
    m.insert(ascii("mouseWheel"), Input::Axis(AxisInput::MouseWheel));
    for i in 0..99 {
        m.insert(Ascii::new(Cow::from(format!("mouse{}", i))), Input::MouseButton(i));
    }
    // Joystick
    for i in 0..99 {
        m.insert(Ascii::new(Cow::from(format!("axis{}", i))), Input::Axis(AxisInput::JoystickAxis(i)));
    }
    for i in 0..99 {
        m.insert(Ascii::new(Cow::from(format!("joy{}", i))), Input::JoystickButton(i));
    }
    // Key
    m.insert(ascii("A"), Input::KeyboardKey(VirtualKeyCode::A));
    m.insert(ascii("B"), Input::KeyboardKey(VirtualKeyCode::B));
    m.insert(ascii("C"), Input::KeyboardKey(VirtualKeyCode::C));
    m.insert(ascii("D"), Input::KeyboardKey(VirtualKeyCode::D));
    m.insert(ascii("E"), Input::KeyboardKey(VirtualKeyCode::E));
    m.insert(ascii("F"), Input::KeyboardKey(VirtualKeyCode::F));
    m.insert(ascii("G"), Input::KeyboardKey(VirtualKeyCode::G));
    m.insert(ascii("H"), Input::KeyboardKey(VirtualKeyCode::H));
    m.insert(ascii("I"), Input::KeyboardKey(VirtualKeyCode::I));
    m.insert(ascii("J"), Input::KeyboardKey(VirtualKeyCode::J));
    m.insert(ascii("K"), Input::KeyboardKey(VirtualKeyCode::K));
    m.insert(ascii("L"), Input::KeyboardKey(VirtualKeyCode::L));
    m.insert(ascii("M"), Input::KeyboardKey(VirtualKeyCode::M));
    m.insert(ascii("N"), Input::KeyboardKey(VirtualKeyCode::N));
    m.insert(ascii("O"), Input::KeyboardKey(VirtualKeyCode::O));
    m.insert(ascii("P"), Input::KeyboardKey(VirtualKeyCode::P));
    m.insert(ascii("Q"), Input::KeyboardKey(VirtualKeyCode::Q));
    m.insert(ascii("R"), Input::KeyboardKey(VirtualKeyCode::R));
    m.insert(ascii("S"), Input::KeyboardKey(VirtualKeyCode::S));
    m.insert(ascii("T"), Input::KeyboardKey(VirtualKeyCode::T));
    m.insert(ascii("U"), Input::KeyboardKey(VirtualKeyCode::U));
    m.insert(ascii("V"), Input::KeyboardKey(VirtualKeyCode::V));
    m.insert(ascii("W"), Input::KeyboardKey(VirtualKeyCode::W));
    m.insert(ascii("X"), Input::KeyboardKey(VirtualKeyCode::X));
    m.insert(ascii("Y"), Input::KeyboardKey(VirtualKeyCode::Y));
    m.insert(ascii("Z"), Input::KeyboardKey(VirtualKeyCode::Z));
    m.insert(ascii("Key1"), Input::KeyboardKey(VirtualKeyCode::Key1));
    m.insert(ascii("Key2"), Input::KeyboardKey(VirtualKeyCode::Key2));
    m.insert(ascii("Key3"), Input::KeyboardKey(VirtualKeyCode::Key3));
    m.insert(ascii("Key4"), Input::KeyboardKey(VirtualKeyCode::Key4));
    m.insert(ascii("Key5"), Input::KeyboardKey(VirtualKeyCode::Key5));
    m.insert(ascii("Key6"), Input::KeyboardKey(VirtualKeyCode::Key6));
    m.insert(ascii("Key7"), Input::KeyboardKey(VirtualKeyCode::Key7));
    m.insert(ascii("Key8"), Input::KeyboardKey(VirtualKeyCode::Key8));
    m.insert(ascii("Key9"), Input::KeyboardKey(VirtualKeyCode::Key9));
    m.insert(ascii("Key0"), Input::KeyboardKey(VirtualKeyCode::Key0));
    m.insert(ascii("Escape"), Input::KeyboardKey(VirtualKeyCode::Escape));
    m.insert(ascii("F1"), Input::KeyboardKey(VirtualKeyCode::F1));
    m.insert(ascii("F2"), Input::KeyboardKey(VirtualKeyCode::F2));
    m.insert(ascii("F3"), Input::KeyboardKey(VirtualKeyCode::F3));
    m.insert(ascii("F4"), Input::KeyboardKey(VirtualKeyCode::F4));
    m.insert(ascii("F5"), Input::KeyboardKey(VirtualKeyCode::F5));
    m.insert(ascii("F6"), Input::KeyboardKey(VirtualKeyCode::F6));
    m.insert(ascii("F7"), Input::KeyboardKey(VirtualKeyCode::F7));
    m.insert(ascii("F8"), Input::KeyboardKey(VirtualKeyCode::F8));
    m.insert(ascii("F9"), Input::KeyboardKey(VirtualKeyCode::F9));
    m.insert(ascii("F10"), Input::KeyboardKey(VirtualKeyCode::F10));
    m.insert(ascii("F11"), Input::KeyboardKey(VirtualKeyCode::F11));
    m.insert(ascii("F12"), Input::KeyboardKey(VirtualKeyCode::F12));
    m.insert(ascii("F13"), Input::KeyboardKey(VirtualKeyCode::F13));
    m.insert(ascii("F14"), Input::KeyboardKey(VirtualKeyCode::F14));
    m.insert(ascii("F15"), Input::KeyboardKey(VirtualKeyCode::F15));
    m.insert(ascii("F16"), Input::KeyboardKey(VirtualKeyCode::F16));
    m.insert(ascii("F17"), Input::KeyboardKey(VirtualKeyCode::F17));
    m.insert(ascii("F18"), Input::KeyboardKey(VirtualKeyCode::F18));
    m.insert(ascii("F19"), Input::KeyboardKey(VirtualKeyCode::F19));
    m.insert(ascii("F20"), Input::KeyboardKey(VirtualKeyCode::F20));
    m.insert(ascii("F21"), Input::KeyboardKey(VirtualKeyCode::F21));
    m.insert(ascii("F22"), Input::KeyboardKey(VirtualKeyCode::F22));
    m.insert(ascii("F23"), Input::KeyboardKey(VirtualKeyCode::F23));
    m.insert(ascii("F24"), Input::KeyboardKey(VirtualKeyCode::F24));
    m.insert(ascii("Snapshot"), Input::KeyboardKey(VirtualKeyCode::Snapshot));
    m.insert(ascii("Scroll"), Input::KeyboardKey(VirtualKeyCode::Scroll));
    m.insert(ascii("Pause"), Input::KeyboardKey(VirtualKeyCode::Pause));
    m.insert(ascii("Insert"), Input::KeyboardKey(VirtualKeyCode::Insert));
    m.insert(ascii("Home"), Input::KeyboardKey(VirtualKeyCode::Home));
    m.insert(ascii("Delete"), Input::KeyboardKey(VirtualKeyCode::Delete));
    m.insert(ascii("End"), Input::KeyboardKey(VirtualKeyCode::End));
    m.insert(ascii("PageDown"), Input::KeyboardKey(VirtualKeyCode::PageDown));
    m.insert(ascii("PageUp"), Input::KeyboardKey(VirtualKeyCode::PageUp));
    m.insert(ascii("Left"), Input::KeyboardKey(VirtualKeyCode::Left));
    m.insert(ascii("Up"), Input::KeyboardKey(VirtualKeyCode::Up));
    m.insert(ascii("Right"), Input::KeyboardKey(VirtualKeyCode::Right));
    m.insert(ascii("Down"), Input::KeyboardKey(VirtualKeyCode::Down));
    m.insert(ascii("Back"), Input::KeyboardKey(VirtualKeyCode::Back));
    m.insert(ascii("Return"), Input::KeyboardKey(VirtualKeyCode::Return));
    m.insert(ascii("Space"), Input::KeyboardKey(VirtualKeyCode::Space));
    m.insert(ascii("Compose"), Input::KeyboardKey(VirtualKeyCode::Compose));
    m.insert(ascii("Caret"), Input::KeyboardKey(VirtualKeyCode::Caret));
    m.insert(ascii("Numlock"), Input::KeyboardKey(VirtualKeyCode::Numlock));
    m.insert(ascii("Numpad0"), Input::KeyboardKey(VirtualKeyCode::Numpad0));
    m.insert(ascii("Numpad1"), Input::KeyboardKey(VirtualKeyCode::Numpad1));
    m.insert(ascii("Numpad2"), Input::KeyboardKey(VirtualKeyCode::Numpad2));
    m.insert(ascii("Numpad3"), Input::KeyboardKey(VirtualKeyCode::Numpad3));
    m.insert(ascii("Numpad4"), Input::KeyboardKey(VirtualKeyCode::Numpad4));
    m.insert(ascii("Numpad5"), Input::KeyboardKey(VirtualKeyCode::Numpad5));
    m.insert(ascii("Numpad6"), Input::KeyboardKey(VirtualKeyCode::Numpad6));
    m.insert(ascii("Numpad7"), Input::KeyboardKey(VirtualKeyCode::Numpad7));
    m.insert(ascii("Numpad8"), Input::KeyboardKey(VirtualKeyCode::Numpad8));
    m.insert(ascii("Numpad9"), Input::KeyboardKey(VirtualKeyCode::Numpad9));
    m.insert(ascii("AbntC1"), Input::KeyboardKey(VirtualKeyCode::AbntC1));
    m.insert(ascii("AbntC2"), Input::KeyboardKey(VirtualKeyCode::AbntC2));
    m.insert(ascii("NumpadAdd"), Input::KeyboardKey(VirtualKeyCode::NumpadAdd));
    m.insert(ascii("NumpadComma"), Input::KeyboardKey(VirtualKeyCode::NumpadComma));
    m.insert(ascii("NumpadDecimal"), Input::KeyboardKey(VirtualKeyCode::NumpadDecimal));
    m.insert(ascii("NumpadDivide"), Input::KeyboardKey(VirtualKeyCode::NumpadDivide));
    m.insert(ascii("NumpadEnter"), Input::KeyboardKey(VirtualKeyCode::NumpadEnter));
    m.insert(ascii("NumpadEquals"), Input::KeyboardKey(VirtualKeyCode::NumpadEquals));
    m.insert(ascii("NumpadMultiply"), Input::KeyboardKey(VirtualKeyCode::NumpadMultiply));
    m.insert(ascii("NumpadSubtract"), Input::KeyboardKey(VirtualKeyCode::NumpadSubtract));
    m.insert(ascii("Apostrophe"), Input::KeyboardKey(VirtualKeyCode::Apostrophe));
    m.insert(ascii("Apps"), Input::KeyboardKey(VirtualKeyCode::Apps));
    m.insert(ascii("At"), Input::KeyboardKey(VirtualKeyCode::At));
    m.insert(ascii("Ax"), Input::KeyboardKey(VirtualKeyCode::Ax));
    m.insert(ascii("Backslash"), Input::KeyboardKey(VirtualKeyCode::Backslash));
    m.insert(ascii("Calculator"), Input::KeyboardKey(VirtualKeyCode::Calculator));
    m.insert(ascii("Capital"), Input::KeyboardKey(VirtualKeyCode::Capital));
    m.insert(ascii("Colon"), Input::KeyboardKey(VirtualKeyCode::Colon));
    m.insert(ascii("Comma"), Input::KeyboardKey(VirtualKeyCode::Comma));
    m.insert(ascii("Convert"), Input::KeyboardKey(VirtualKeyCode::Convert));
    m.insert(ascii("Equals"), Input::KeyboardKey(VirtualKeyCode::Equals));
    m.insert(ascii("Grave"), Input::KeyboardKey(VirtualKeyCode::Grave));
    m.insert(ascii("Kana"), Input::KeyboardKey(VirtualKeyCode::Kana));
    m.insert(ascii("Kanji"), Input::KeyboardKey(VirtualKeyCode::Kanji));
    m.insert(ascii("LAlt"), Input::KeyboardKey(VirtualKeyCode::LAlt));
    m.insert(ascii("LBracket"), Input::KeyboardKey(VirtualKeyCode::LBracket));
    m.insert(ascii("LControl"), Input::KeyboardKey(VirtualKeyCode::LControl));
    m.insert(ascii("LShift"), Input::KeyboardKey(VirtualKeyCode::LShift));
    m.insert(ascii("LWin"), Input::KeyboardKey(VirtualKeyCode::LWin));
    m.insert(ascii("Mail"), Input::KeyboardKey(VirtualKeyCode::Mail));
    m.insert(ascii("MediaSelect"), Input::KeyboardKey(VirtualKeyCode::MediaSelect));
    m.insert(ascii("MediaStop"), Input::KeyboardKey(VirtualKeyCode::MediaStop));
    m.insert(ascii("Minus"), Input::KeyboardKey(VirtualKeyCode::Minus));
    m.insert(ascii("Mute"), Input::KeyboardKey(VirtualKeyCode::Mute));
    m.insert(ascii("MyComputer"), Input::KeyboardKey(VirtualKeyCode::MyComputer));
    m.insert(ascii("NavigateForward"), Input::KeyboardKey(VirtualKeyCode::NavigateForward));
    m.insert(ascii("NavigateBackward"), Input::KeyboardKey(VirtualKeyCode::NavigateBackward));
    m.insert(ascii("NextTrack"), Input::KeyboardKey(VirtualKeyCode::NextTrack));
    m.insert(ascii("NoConvert"), Input::KeyboardKey(VirtualKeyCode::NoConvert));
    m.insert(ascii("OEM102"), Input::KeyboardKey(VirtualKeyCode::OEM102));
    m.insert(ascii("Period"), Input::KeyboardKey(VirtualKeyCode::Period));
    m.insert(ascii("PlayPause"), Input::KeyboardKey(VirtualKeyCode::PlayPause));
    m.insert(ascii("Power"), Input::KeyboardKey(VirtualKeyCode::Power));
    m.insert(ascii("PrevTrack"), Input::KeyboardKey(VirtualKeyCode::PrevTrack));
    m.insert(ascii("RAlt"), Input::KeyboardKey(VirtualKeyCode::RAlt));
    m.insert(ascii("RBracket"), Input::KeyboardKey(VirtualKeyCode::RBracket));
    m.insert(ascii("RControl"), Input::KeyboardKey(VirtualKeyCode::RControl));
    m.insert(ascii("RShift"), Input::KeyboardKey(VirtualKeyCode::RShift));
    m.insert(ascii("RWin"), Input::KeyboardKey(VirtualKeyCode::RWin));
    m.insert(ascii("Semicolon"), Input::KeyboardKey(VirtualKeyCode::Semicolon));
    m.insert(ascii("Slash"), Input::KeyboardKey(VirtualKeyCode::Slash));
    m.insert(ascii("Sleep"), Input::KeyboardKey(VirtualKeyCode::Sleep));
    m.insert(ascii("Stop"), Input::KeyboardKey(VirtualKeyCode::Stop));
    m.insert(ascii("Sysrq"), Input::KeyboardKey(VirtualKeyCode::Sysrq));
    m.insert(ascii("Tab"), Input::KeyboardKey(VirtualKeyCode::Tab));
    m.insert(ascii("Underline"), Input::KeyboardKey(VirtualKeyCode::Underline));
    m.insert(ascii("Unlabeled"), Input::KeyboardKey(VirtualKeyCode::Unlabeled));
    m.insert(ascii("VolumeDown"), Input::KeyboardKey(VirtualKeyCode::VolumeDown));
    m.insert(ascii("VolumeUp"), Input::KeyboardKey(VirtualKeyCode::VolumeUp));
    m.insert(ascii("Wake"), Input::KeyboardKey(VirtualKeyCode::Wake));
    m.insert(ascii("WebBack"), Input::KeyboardKey(VirtualKeyCode::WebBack));
    m.insert(ascii("WebFavorites"), Input::KeyboardKey(VirtualKeyCode::WebFavorites));
    m.insert(ascii("WebForward"), Input::KeyboardKey(VirtualKeyCode::WebForward));
    m.insert(ascii("WebHome"), Input::KeyboardKey(VirtualKeyCode::WebHome));
    m.insert(ascii("WebRefresh"), Input::KeyboardKey(VirtualKeyCode::WebRefresh));
    m.insert(ascii("WebSearch"), Input::KeyboardKey(VirtualKeyCode::WebSearch));
    m.insert(ascii("WebStop"), Input::KeyboardKey(VirtualKeyCode::WebStop));
    m.insert(ascii("Yen"), Input::KeyboardKey(VirtualKeyCode::Yen));
    m.insert(ascii("Copy"), Input::KeyboardKey(VirtualKeyCode::Copy));
    m.insert(ascii("Paste"), Input::KeyboardKey(VirtualKeyCode::Paste));
    m.insert(ascii("Cut"), Input::KeyboardKey(VirtualKeyCode::Cut));
    m
});

impl Input {
    pub fn from_binding(s: &str) -> Result<Self> {
        Ok(if let Some(key) = BIND_MAP.get(&Ascii::new(Cow::from(s))) {
            *key
        } else {
            let mut similar = BIND_MAP
                .keys()
                .map(|key| (strsim::levenshtein(key, s), key.to_owned()))
                .collect::<Vec<_>>();
            similar.sort();
            bail!(
                "unrecognized event identifier: {}\ndid you mean: {}",
                s,
                similar.first().unwrap().1
            )
        })
    }

    pub fn modifier(&self) -> ModifiersState {
        match self {
            Input::KeyboardKey(vkey) => match vkey {
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
pub struct InputSet {
    pub keys: SmallVec<[Input; 2]>,
    pub modifiers: ModifiersState,
}

impl InputSet {
    // Parse keysets of the form a+b+c; e.g. LControl+RControl+Space into
    // a discreet keyset.
    //
    // Note that there is a special case for the 4 modifiers in which we
    // expect to be able to refer to "Control" and not care what key it is.
    // In this case we emit all possible keysets, combinatorially.
    pub fn from_binding(keyset: &str) -> Result<Vec<Self>> {
        let mut out = vec![SmallVec::<[Input; 2]>::new()];
        for keyname in keyset.split('+') {
            if let Ok(key) = Input::from_binding(keyname) {
                for tmp in &mut out {
                    tmp.push(key);
                }
            } else if MIRROR_MODIFIERS.contains(&Ascii::new(keyname)) {
                let mut next_out = Vec::new();
                for mut tmp in out.drain(..) {
                    let mut cpy = tmp.clone();
                    tmp.push(Input::from_binding(&format!("L{}", keyname))?);
                    cpy.push(Input::from_binding(&format!("R{}", keyname))?);
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

    pub fn contains_key(&self, key: &Input) -> bool {
        for own_key in &self.keys {
            if key == own_key {
                return true;
            }
        }
        false
    }

    pub fn is_subset_of(&self, other: &InputSet) -> bool {
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

    pub fn is_pressed(&self, edge_input: Option<Input>, state: &State) -> bool {
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
            if Some(*key) == edge_input {
                continue;
            }
            if let Some(current_state) = state.input_states.get(key) {
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

impl fmt::Display for InputSet {
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
        assert_eq!(
            Input::from_binding("A")?,
            Input::KeyboardKey(VirtualKeyCode::A)
        );
        assert_eq!(
            Input::from_binding("a")?,
            Input::KeyboardKey(VirtualKeyCode::A)
        );
        assert_eq!(
            Input::from_binding("PageUp")?,
            Input::KeyboardKey(VirtualKeyCode::PageUp)
        );
        assert_eq!(
            Input::from_binding("pageup")?,
            Input::KeyboardKey(VirtualKeyCode::PageUp)
        );
        assert_eq!(
            Input::from_binding("pAgEuP")?,
            Input::KeyboardKey(VirtualKeyCode::PageUp)
        );
        Ok(())
    }

    #[test]
    fn test_can_create_mouse() -> Result<()> {
        assert_eq!(Input::from_binding("MoUsE50")?, Input::MouseButton(50));
        Ok(())
    }

    #[test]
    fn test_can_create_keysets() -> Result<()> {
        assert_eq!(InputSet::from_binding("a+b")?.len(), 1);
        assert_eq!(InputSet::from_binding("Control+Win+a")?.len(), 4);
        assert_eq!(InputSet::from_binding("Control+b+Shift")?.len(), 4);
        Ok(())
    }
}
