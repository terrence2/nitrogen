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
    font_context::{FontContext, FontId, TextSpanMetrics},
    paint_context::PaintContext,
    region::{Extent, Position, Region},
    text_run::TextRun,
    widget::Widget,
    widget_info::WidgetInfo,
};
use anyhow::Result;
use gpu::Gpu;
use input::{ElementState, GenericEvent, ModifiersState, VirtualKeyCode};
use nitrous::Interpreter;
use parking_lot::RwLock;
use std::{ops::Range, sync::Arc, time::Instant};
use window::{
    size::{AbsSize, AspectMath, ScreenDir, Size},
    Window,
};

#[derive(Debug)]
pub struct LineEdit {
    line: TextRun,
    metrics: TextSpanMetrics,

    position: Position<Size>,
    extent: Extent<Size>,
}

impl LineEdit {
    pub fn empty() -> Self {
        Self {
            line: TextRun::empty(),
            metrics: TextSpanMetrics::default(),

            position: Position::origin(),
            extent: Extent::zero(),
        }
    }

    pub fn with_default_color(mut self, color: Color) -> Self {
        self.line.set_default_color(color);
        self
    }

    pub fn with_default_font(mut self, font_id: FontId) -> Self {
        self.line.set_default_font(font_id);
        self
    }

    pub fn with_default_size(mut self, size: Size) -> Self {
        self.line.set_default_size(size);
        self
    }

    pub fn with_text(mut self, text: &str) -> Self {
        self.line.insert(text);
        self
    }

    pub fn line(&self) -> &TextRun {
        &self.line
    }

    pub fn line_mut(&mut self) -> &mut TextRun {
        &mut self.line
    }

    pub fn select(&mut self, range: Range<usize>) {
        self.line.select(range);
    }

    pub fn change_size(&mut self, size: Size) {
        self.line.change_size(size);
    }

    pub fn take_action(
        &mut self,
        virtual_keycode: &VirtualKeyCode,
        modifiers: &ModifiersState,
    ) -> Result<()> {
        match virtual_keycode {
            // Move to actions.
            VirtualKeyCode::Home => self.line.move_home(modifiers),
            VirtualKeyCode::Delete => self.line.delete(),
            VirtualKeyCode::Back => self.line.backspace(),
            VirtualKeyCode::End => self.line.move_end(modifiers),
            VirtualKeyCode::Left => self.line.move_left(modifiers),
            VirtualKeyCode::Right => self.line.move_right(modifiers),
            _ => {}
        }
        Ok(())
    }

    pub fn wrapped(self) -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(self))
    }
}

impl Widget for LineEdit {
    fn measure(&mut self, win: &Window, font_context: &mut FontContext) -> Result<Extent<Size>> {
        self.metrics = self.line.measure(win, font_context)?;
        Ok(Extent::<Size>::new(
            self.metrics.width.into(),
            (self.metrics.height - self.metrics.descent).into(),
        ))
    }

    fn layout(
        &mut self,
        _now: Instant,
        region: Region<Size>,
        win: &Window,
        _font_context: &mut FontContext,
    ) -> Result<()> {
        let mut position = *region.position();
        *position.bottom_mut() =
            position
                .bottom()
                .sub(&self.metrics.descent.into(), win, ScreenDir::Vertical);
        self.position = position;
        self.extent = *region.extent();
        Ok(())
    }

    fn upload(
        &self,
        _now: Instant,
        win: &Window,
        gpu: &Gpu,
        context: &mut PaintContext,
    ) -> Result<()> {
        let info = WidgetInfo::default();
        let widget_info_index = context.push_widget(&info);

        self.line
            .upload(self.position, widget_info_index, win, gpu, context)?;

        Ok(())
    }

    fn handle_event(
        &mut self,
        _now: Instant,
        event: &GenericEvent,
        _focus: &str,
        _cursor_position: Position<AbsSize>,
        _interpreter: Interpreter,
    ) -> Result<()> {
        // FIXME: add name to widget and obey focus
        if let GenericEvent::KeyboardKey {
            virtual_keycode,
            press_state,
            modifiers_state,
            window_focused,
            ..
        } = event
        {
            if !window_focused {
                return Ok(());
            }

            // Reserved for window manager.
            if modifiers_state.alt() || modifiers_state.logo() {
                return Ok(());
            }

            if *press_state == ElementState::Pressed {
                let (base, shifted) = code_to_char(virtual_keycode);
                if let Some(mut c) = base {
                    if modifiers_state.shift() {
                        c = shifted.unwrap_or(c);
                    }
                    self.line.insert(&c.to_string());
                } else {
                    self.take_action(virtual_keycode, modifiers_state)?;
                }
            }
        }
        Ok(())
    }
}

fn code_to_char(virtual_keycode: &VirtualKeyCode) -> (Option<char>, Option<char>) {
    match virtual_keycode {
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
}
