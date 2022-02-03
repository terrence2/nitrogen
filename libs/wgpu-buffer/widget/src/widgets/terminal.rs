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
    region::{Extent, Position, Region},
    widget::{Widget, WidgetFocus},
    LineEdit, TextEdit, VerticalBox,
};
use anyhow::{Context, Result};
use gpu::Gpu;
use input::{ElementState, InputEvent, VirtualKeyCode};
use nitrous::{
    ir::{Expr, Stmt, Term},
    NitrousAst, NitrousScript, Value,
};
use parking_lot::RwLock;
use runtime::ScriptHerder;
use std::io::Read;
use std::{
    fs::{File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::Path,
    sync::Arc,
    time::Instant,
};
use window::{
    size::{AbsSize, Size},
    Window,
};

// Items packed from top to bottom.
#[derive(Debug)]
pub struct Terminal {
    edit: Arc<RwLock<LineEdit>>,
    output: Arc<RwLock<TextEdit>>,
    container: Arc<RwLock<VerticalBox>>,
    visible: bool,
    history: Vec<String>,
    history_file: Option<File>,
    history_cursor: usize,
}

impl Terminal {
    const WIDTH: Size = Size::from_percent(100.);
    const HEIGHT: Size = Size::from_percent(40.);

    pub fn new(font_context: &FontContext, state_dir: &Path) -> Result<Self> {
        let output = TextEdit::new("")
            .with_default_font(font_context.font_id_for_name("dejavu-mono"))
            .with_default_color(Color::Green)
            .with_text("Nitrogen Terminal\nType `help()` for help.")
            .wrapped();
        let edit = LineEdit::empty()
            .with_default_font(font_context.font_id_for_name("mono"))
            .with_default_color(Color::White)
            .with_default_size(Size::from_pts(12.0))
            .with_text("help()")
            .wrapped();
        edit.write().line_mut().select_all();
        let container = VerticalBox::new_with_children(&[output.clone(), edit.clone()])
            .with_background_color(Color::Gray.darken(3.).opacity(0.8))
            .with_glass_background()
            .with_overridden_extent(Extent::new(Self::WIDTH, Self::HEIGHT))
            .with_fill(0)
            .wrapped();

        // Load command history from state dir
        let mut history_path = state_dir.to_owned();
        history_path.push("command_history.txt");
        let mut history_file = OpenOptions::new()
            .read(true)
            .append(true)
            .truncate(false)
            .create(true)
            .open(&history_path)
            .ok();

        let history = if let Some(fp) = history_file.as_mut() {
            fp.seek(SeekFrom::Start(0))?;
            let mut content = String::new();
            fp.read_to_string(&mut content)
                .with_context(|| "corrupted history")?;
            content.lines().map(|s| s.to_owned()).collect()
        } else {
            vec![]
        };
        let history_cursor = history.len();

        Ok(Self {
            edit,
            output,
            container,
            visible: true,
            history,
            history_file,
            history_cursor,
        })
    }

    fn add_command_to_history(&mut self, command: &str) -> Result<()> {
        // Echo the command into the output buffer as a literal.
        self.output
            .write()
            .append_line(&("> ".to_owned() + command));

        // And print to the console in case we need to copy the transaction.
        println!("{}", command);

        // And save it in our local history so we don't have to re-type it
        self.history.push(command.to_owned());

        // And stream it to our history file
        if let Some(fp) = self.history_file.as_mut() {
            fp.write(format!("{}\n", command).as_bytes())
                .with_context(|| "recording history")?;
            fp.sync_data()?;
        }

        // Reset the history cursor
        self.history_cursor = self.history.len();

        Ok(())
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

    fn try_completion(&self, mut partial: NitrousAst, herder: &mut ScriptHerder) -> Option<String> {
        if partial.statements().len() != 1 {
            return None;
        }
        if let Stmt::Expr(ref mut e) = partial.statements_mut()[0].as_mut() {
            // FIXME: we need access to world for global access to do matching
            if let Expr::Term(Term::Symbol(sym)) = e.as_mut() {
                let sim = herder
                    .resource_names()
                    .filter(|&s| s.starts_with(sym.as_str()))
                    .collect::<Vec<&String>>();
                if sim.len() == 1 {
                    return Some(sim[0].to_owned());
                }
            } else if let Expr::Attr(mod_name_term, Term::Symbol(sym)) = e.as_mut() {
                if let Expr::Term(Term::Symbol(mod_name)) = mod_name_term.as_ref() {
                    if let Some(value) = herder.lookup_resource(mod_name) {
                        /*
                        if let Ok(attr_names) = herder.attrs(value) {
                            let sim = attr_names
                                .iter()
                                .filter(|&s| s.starts_with(sym.as_str()))
                                .cloned()
                                .collect::<Vec<&str>>();
                            if sim.len() == 1 {
                                return Some(format!("{}.{}", mod_name, sim[0]));
                            }
                        }
                         */
                    }
                }
            }
        }
        None
    }

    fn on_tab_pressed(&mut self, herder: &mut ScriptHerder) {
        let incomplete = self.edit.read().line().flatten();
        if let Ok(partial) = NitrousAst::parse(&incomplete) {
            if let Some(full) = self.try_completion(partial, herder) {
                self.edit.write().line_mut().select_all();
                self.edit.write().line_mut().insert(&full);
            }
        }
    }

    fn on_up_pressed(&mut self) {
        if self.history_cursor > 0 {
            self.history_cursor -= 1;
            self.edit.write().line_mut().select_all();
            self.edit
                .write()
                .line_mut()
                .insert(&self.history[self.history_cursor]);
        }
    }

    fn on_down_pressed(&mut self) {
        if self.history_cursor < self.history.len() {
            self.history_cursor += 1;
            self.edit.write().line_mut().select_all();
            if self.history_cursor < self.history.len() {
                self.edit
                    .write()
                    .line_mut()
                    .insert(&self.history[self.history_cursor]);
            } else {
                self.edit.write().line_mut().insert("");
            }
        }
    }

    fn on_enter_pressed(&mut self, herder: &mut ScriptHerder) -> Result<()> {
        let command = self.edit.read().line().flatten();
        self.edit.write().line_mut().select_all();
        self.edit.write().line_mut().delete();

        self.add_command_to_history(&command)?;

        herder.run(NitrousScript::compile(&command)?);

        // FIXME: make sure we have logging of errors... event system? entity system?

        /*
        let output = self.output.clone();
        let mut interpreter = interpreter.to_owned();
        rayon::spawn(move || match interpreter.interpret_once(&command) {
            Ok(value) => {
                let s = match value {
                    Value::String(s) => s,
                    v => format!("{}", v),
                };
                for line in s.lines() {
                    output.write().append_line(line);
                    println!("{}", line);
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
         */

        Ok(())
    }
}

impl Widget for Terminal {
    fn measure(&mut self, win: &Window, font_context: &mut FontContext) -> Result<Extent<Size>> {
        if !self.visible {
            return Ok(Extent::zero());
        }

        self.container.write().measure(win, font_context)
    }

    fn layout(
        &mut self,
        now: Instant,
        region: Region<Size>,
        win: &Window,
        font_context: &mut FontContext,
    ) -> Result<()> {
        if !self.visible {
            return Ok(());
        }

        self.container
            .write()
            .layout(now, region, win, font_context)
    }

    fn upload(
        &self,
        now: Instant,
        win: &Window,
        gpu: &Gpu,
        context: &mut PaintContext,
    ) -> Result<()> {
        if !self.visible {
            return Ok(());
        }
        self.container.read().upload(now, win, gpu, context)
    }

    fn handle_event(
        &mut self,
        event: &InputEvent,
        focus: WidgetFocus,
        cursor_position: Position<AbsSize>,
        herder: &mut ScriptHerder,
    ) -> Result<()> {
        // FIXME: don't hard-code the name
        if focus != WidgetFocus::Terminal {
            return Ok(());
        }

        // FIXME: set focus parameter here equal to whatever we call the line_edit child
        self.edit
            .write()
            .handle_event(event, focus, cursor_position, herder)?;

        // Intercept the enter key and process the command in edit into the terminal.
        if let InputEvent::KeyboardKey {
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
                match (modifiers_state.ctrl(), virtual_keycode) {
                    (false, VirtualKeyCode::Tab) => {
                        self.on_tab_pressed(herder);
                    }
                    (false, VirtualKeyCode::Up) => {
                        self.on_up_pressed();
                    }
                    (false, VirtualKeyCode::Down) => {
                        self.on_down_pressed();
                    }
                    (false, VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter) => {
                        self.on_enter_pressed(herder)?;
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }
}
