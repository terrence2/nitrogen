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
    font_context::FontContext,
    paint_context::PaintContext,
    widget::{Size, UploadMetrics, Widget},
    LineEdit, TextEdit, VerticalBox,
};
use anyhow::Result;
use gpu::Gpu;
use input::{ElementState, GenericEvent, VirtualKeyCode};
use nitrous::{
    ir::{Expr, Stmt, Term},
    Interpreter, Module, Script, Value,
};
use parking_lot::RwLock;
use std::sync::Arc;

// Items packed from top to bottom.
#[derive(Debug)]
pub struct Terminal {
    edit: Arc<RwLock<LineEdit>>,
    output: Arc<RwLock<TextEdit>>,
    container: Arc<RwLock<VerticalBox>>,
    visible: bool,
}

impl Terminal {
    pub fn new(font_context: &FontContext) -> Self {
        let output = TextEdit::new("")
            .with_default_font(font_context.font_id_for_name("dejavu-mono"))
            .with_default_color(Color::Green)
            .with_text("Nitrogen Terminal\nType `help()` for help.")
            .wrapped();
        let edit = LineEdit::empty()
            .with_default_font(font_context.font_id_for_name("mono"))
            .with_default_color(Color::White)
            .with_default_size(Size::Pts(12.0))
            .with_text("help()")
            .wrapped();
        edit.write().line_mut().select_all();
        let container = VerticalBox::new_with_children(&[output.clone(), edit.clone()])
            .with_background_color(Color::Gray.darken(3.).opacity(0.8))
            .with_width(Size::Percent(100.))
            .with_height(Size::Percent(40.))
            .wrapped();
        container.write().info_mut().set_glass_background(true);
        Self {
            edit,
            output,
            container,
            visible: true,
        }
    }

    pub fn with_visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    pub fn wrapped(self) -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(self))
    }

    pub fn println(&self, line: &str) {
        self.output.write().append_line(line);
    }

    pub fn try_completion(
        &self,
        mut partial: Script,
        interpreter: Arc<RwLock<Interpreter>>,
    ) -> Option<String> {
        if partial.statements().len() != 1 {
            return None;
        }
        if let Stmt::Expr(e) = partial.statements_mut()[0].as_mut() {
            if let Expr::Term(Term::Symbol(sym)) = e.as_mut() {
                let pin = interpreter.read().globals();
                let globals = pin.read();
                let sim = globals
                    .names()
                    .iter()
                    .filter(|&s| s.starts_with(sym.as_str()))
                    .cloned()
                    .collect::<Vec<&str>>();
                if sim.len() == 1 {
                    return Some(sim[0].to_string());
                }
            } else if let Expr::Attr(mod_name_term, Term::Symbol(sym)) = e.as_mut() {
                if let Expr::Term(Term::Symbol(mod_name)) = mod_name_term.as_ref() {
                    if let Some(Value::Module(pin)) = interpreter.read().get_global(&mod_name) {
                        let ns = pin.read();
                        let sim = ns
                            .names()
                            .iter()
                            .filter(|&s| s.starts_with(sym.as_str()))
                            .cloned()
                            .collect::<Vec<&str>>();
                        if sim.len() == 1 {
                            return Some(format!("{}.{}", mod_name, sim[0]));
                        }
                    }
                }
            }
        }
        None
    }
}

impl Widget for Terminal {
    fn upload(&self, gpu: &Gpu, context: &mut PaintContext) -> Result<UploadMetrics> {
        if self.visible {
            self.container.read().upload(gpu, context)
        } else {
            Ok(Default::default())
        }
    }

    fn handle_event(
        &mut self,
        event: &GenericEvent,
        focus: &str,
        interpreter: Arc<RwLock<Interpreter>>,
    ) -> Result<()> {
        // FIXME: don't hard-code the name
        if focus != "terminal" {
            return Ok(());
        }

        // FIXME: set focus parameter here equal to whatever we call the line_edit child
        self.edit
            .write()
            .handle_event(event, focus, interpreter.clone())?;

        // Intercept the enter key and process the command in edit into the terminal.
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
                match virtual_keycode {
                    VirtualKeyCode::Tab => {
                        let incomplete = self.edit.read().line().flatten();
                        if let Ok(partial) = Script::compile(&incomplete) {
                            if let Some(full) = self.try_completion(partial, interpreter) {
                                self.edit.write().line_mut().select_all();
                                self.edit.write().line_mut().insert(&full);
                            }
                        }
                    }
                    VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter => {
                        let command = self.edit.read().line().flatten();
                        self.edit.write().line_mut().select_all();
                        self.edit.write().line_mut().delete();

                        // Echo the command into the output buffer as a literal.
                        self.output
                            .write()
                            .append_line(&("> ".to_owned() + &command));

                        let output = self.output.clone();
                        rayon::spawn(move || match interpreter.write().interpret_once(&command) {
                            Ok(value) => {
                                let s = match value {
                                    Value::String(s) => s,
                                    v => format!("{}", v),
                                };
                                for line in s.lines() {
                                    output.write().append_line(line);
                                }
                            }
                            Err(err) => {
                                println!("failed to execute '{}'", command);
                                println!("  Error: {:?}", err);
                                output
                                    .write()
                                    .append_line(&format!("failed to execute '{}'", command));
                                output.write().append_line(&format!("  Error: {:?}", err));
                            }
                        });
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }
}
