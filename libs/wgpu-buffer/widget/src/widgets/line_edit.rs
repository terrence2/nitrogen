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
    color::Color,
    font_context::FontId,
    paint_context::PaintContext,
    text_run::TextRun,
    widget::{UploadMetrics, Widget},
    widget_info::WidgetInfo,
};
use failure::Fallible;
use gpu::GPU;
use parking_lot::RwLock;
use std::sync::Arc;
use winit::event::{ElementState, KeyboardInput, ModifiersState, VirtualKeyCode};

pub struct LineEdit {
    line: TextRun,
    width: f32,
}

impl LineEdit {
    pub fn new(markup: &str) -> Self {
        let mut obj = Self {
            line: TextRun::empty(),
            width: 1.,
        };
        obj.line.insert(markup);
        obj
    }

    pub fn with_default_color(mut self, color: Color) -> Self {
        self.line = self.line.with_default_color(color);
        self
    }

    pub fn with_default_font(mut self, font_id: FontId) -> Self {
        self.line = self.line.with_default_font_id(font_id);
        self
    }

    pub fn with_default_size_pts(mut self, size_pts: f32) -> Self {
        self.line = self.line.with_default_size_pts(size_pts);
        self
    }

    pub fn line_mut(&mut self) -> &mut TextRun {
        &mut self.line
    }

    pub fn take_action(
        &mut self,
        virtual_keycode: Option<VirtualKeyCode>,
        modifiers: &ModifiersState,
    ) {
        if let Some(code) = virtual_keycode {
            match code {
                // Move to actions.
                VirtualKeyCode::Home => self.line.move_home(modifiers),
                VirtualKeyCode::Delete => self.line.delete(),
                VirtualKeyCode::Back => self.line.backspace(),
                VirtualKeyCode::End => self.line.move_end(modifiers),
                VirtualKeyCode::Left => self.line.move_left(modifiers),
                VirtualKeyCode::Right => self.line.move_right(modifiers),
                VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter => {
                    let raw_cmd = self.line.flatten();
                    println!("would run command: {}", raw_cmd);
                    //self.run_command();
                }
                _ => {}
            }
        }
    }

    pub fn wrapped(self) -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(self))
    }
}

impl Widget for LineEdit {
    fn upload(&self, gpu: &GPU, context: &mut PaintContext) -> UploadMetrics {
        let info = WidgetInfo::default()
            .with_background_color(Color::White)
            .with_foreground_color(Color::White);
        let widget_info_index = context.push_widget(&info);

        let line_metrics = self.line.upload(0f32, widget_info_index, gpu, context);
        UploadMetrics {
            widget_info_indexes: vec![widget_info_index],
            width: self.width,
            baseline_height: line_metrics.baseline_height,
            height: line_metrics.height,
        }
    }

    fn handle_keyboard(&mut self, events: &[(KeyboardInput, ModifiersState)]) -> Fallible<()> {
        for (event, modifiers) in events {
            // Reserved for window manager.
            if modifiers.alt() || modifiers.logo() {
                continue;
            }

            if event.state == ElementState::Pressed {
                let (base, shifted) = code_to_char(event.virtual_keycode);
                if let Some(mut c) = base {
                    if modifiers.shift() {
                        c = shifted.unwrap_or(c);
                    }
                    self.line.insert(&c.to_string());
                } else {
                    self.take_action(event.virtual_keycode, modifiers);
                }
            }
        }
        Ok(())
    }
}

fn code_to_char(virtual_keycode: Option<VirtualKeyCode>) -> (Option<char>, Option<char>) {
    if let Some(code) = virtual_keycode {
        match code {
            VirtualKeyCode::Numpad0 => (Some('0'), None),
            VirtualKeyCode::Numpad1 => (Some('1'), None),
            VirtualKeyCode::Numpad2 => (Some('2'), None),
            VirtualKeyCode::Numpad3 => (Some('3'), None),
            VirtualKeyCode::Numpad4 => (Some('4'), None),
            VirtualKeyCode::Numpad5 => (Some('5'), None),
            VirtualKeyCode::Numpad6 => (Some('6'), None),
            VirtualKeyCode::Numpad7 => (Some('7'), None),
            VirtualKeyCode::Numpad8 => (Some('8'), None),
            VirtualKeyCode::Numpad9 => (Some('9'), None),
            VirtualKeyCode::Key0 => (Some('0'), Some(')')),
            VirtualKeyCode::Key1 => (Some('1'), Some('!')),
            VirtualKeyCode::Key2 => (Some('2'), Some('@')),
            VirtualKeyCode::Key3 => (Some('3'), Some('#')),
            VirtualKeyCode::Key4 => (Some('4'), Some('$')),
            VirtualKeyCode::Key5 => (Some('5'), Some('%')),
            VirtualKeyCode::Key6 => (Some('6'), Some('^')),
            VirtualKeyCode::Key7 => (Some('7'), Some('&')),
            VirtualKeyCode::Key8 => (Some('8'), Some('*')),
            VirtualKeyCode::Key9 => (Some('9'), Some('(')),
            VirtualKeyCode::A => (Some('a'), Some('A')),
            VirtualKeyCode::B => (Some('b'), Some('B')),
            VirtualKeyCode::C => (Some('c'), Some('C')),
            VirtualKeyCode::D => (Some('d'), Some('D')),
            VirtualKeyCode::E => (Some('e'), Some('E')),
            VirtualKeyCode::F => (Some('f'), Some('F')),
            VirtualKeyCode::G => (Some('g'), Some('G')),
            VirtualKeyCode::H => (Some('h'), Some('H')),
            VirtualKeyCode::I => (Some('i'), Some('I')),
            VirtualKeyCode::J => (Some('j'), Some('J')),
            VirtualKeyCode::K => (Some('k'), Some('K')),
            VirtualKeyCode::L => (Some('l'), Some('L')),
            VirtualKeyCode::M => (Some('m'), Some('M')),
            VirtualKeyCode::N => (Some('n'), Some('N')),
            VirtualKeyCode::O => (Some('o'), Some('O')),
            VirtualKeyCode::P => (Some('p'), Some('P')),
            VirtualKeyCode::Q => (Some('q'), Some('Q')),
            VirtualKeyCode::R => (Some('r'), Some('R')),
            VirtualKeyCode::S => (Some('s'), Some('S')),
            VirtualKeyCode::T => (Some('t'), Some('T')),
            VirtualKeyCode::U => (Some('u'), Some('U')),
            VirtualKeyCode::V => (Some('v'), Some('V')),
            VirtualKeyCode::W => (Some('w'), Some('W')),
            VirtualKeyCode::X => (Some('x'), Some('X')),
            VirtualKeyCode::Y => (Some('y'), Some('Y')),
            VirtualKeyCode::Z => (Some('z'), Some('Z')),
            VirtualKeyCode::Space => (Some(' '), None),
            VirtualKeyCode::Caret => (Some('^'), None),
            VirtualKeyCode::NumpadAdd => (Some('+'), None),
            VirtualKeyCode::NumpadDivide => (Some('/'), None),
            VirtualKeyCode::NumpadDecimal => (Some('.'), None),
            VirtualKeyCode::NumpadComma => (Some(','), None),
            VirtualKeyCode::NumpadEquals => (Some('='), None),
            VirtualKeyCode::NumpadMultiply => (Some('*'), None),
            VirtualKeyCode::NumpadSubtract => (Some('-'), None),
            VirtualKeyCode::Apostrophe => (Some('\''), Some('"')),
            VirtualKeyCode::Asterisk => (Some('*'), None),
            VirtualKeyCode::At => (Some('@'), None),
            VirtualKeyCode::Backslash => (Some('\\'), Some('|')),
            VirtualKeyCode::Colon => (Some(':'), None),
            VirtualKeyCode::Comma => (Some(','), Some('<')),
            VirtualKeyCode::Equals => (Some('='), Some('+')),
            VirtualKeyCode::Grave => (Some('`'), Some('~')),
            VirtualKeyCode::LBracket => (Some('['), Some('{')),
            VirtualKeyCode::Minus => (Some('-'), Some('_')),
            VirtualKeyCode::Period => (Some('.'), Some('>')),
            VirtualKeyCode::Plus => (Some('+'), None),
            VirtualKeyCode::RBracket => (Some(']'), Some('}')),
            VirtualKeyCode::Semicolon => (Some(';'), Some(':')),
            VirtualKeyCode::Slash => (Some('/'), Some('?')),

            // Move to top level?
            // Tab,
            // Copy,
            // Paste,
            // Cut,
            _ => (None, None),
        }
    } else {
        (None, None)
    }
}
