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
    HeapMut, HeapRef, NitrousAst, Value,
};
use parking_lot::RwLock;
use runtime::{ScriptCompletion, ScriptHerder, ScriptResult, ScriptRunKind};
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

    pub fn new(font_context: &mut FontContext, state_dir: &Path, gpu: &Gpu) -> Result<Self> {
        let output = TextEdit::new("")
            .with_default_font(font_context.font_id_for_name("dejavu-mono"))
            .with_default_color(Color::Green)
            .with_text("Nitrogen Terminal\nType `help()` for help.")
            .wrapped();
        let edit = LineEdit::empty()
            .with_default_font(font_context.font_id_for_name("mono"))
            .with_default_color(Color::White)
            .with_text("help()")
            .wrapped();
        edit.write().line_mut().select_all();
        let container = VerticalBox::new_with_children(&[output.clone(), edit.clone()])
            .with_background_color(Color::Gray.darken(3.).opacity(0.8))
            .with_glass_background()
            .with_overridden_extent(Extent::new(Self::WIDTH, Self::HEIGHT))
            .with_fill(0)
            .wrapped();

        font_context.cache_ascii_glyphs(output.read().default_font(), AbsSize::Pts(12.0), gpu)?;
        font_context.cache_ascii_glyphs(
            edit.read().line().default_font(),
            AbsSize::Pts(12.0),
            gpu,
        )?;

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

    pub fn set_font_size(&mut self, size: AbsSize) {
        self.output.write().set_font_size(size.into());
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

    fn try_complete_resource(partial: &NitrousAst, heap: HeapRef) -> Option<String> {
        if let Stmt::Expr(ref e) = partial.statements()[0].as_ref() {
            if let Expr::Term(Term::Symbol(sym)) = e.as_ref() {
                let matching_resources = heap
                    .resource_names()
                    .filter(|&s| s.starts_with(sym.as_str()))
                    .collect::<Vec<&str>>();
                if matching_resources.len() == 1 {
                    return Some(matching_resources[0].to_owned());
                } else {
                    // TODO: show top N potential completions?
                }
            }
        }
        None
    }

    fn try_complete_resource_attrs(partial: &NitrousAst, heap: HeapRef) -> Option<String> {
        if let Stmt::Expr(ref e) = partial.statements()[0].as_ref() {
            if let Expr::Attr(lhs_name_term, Term::Symbol(sym)) = e.as_ref() {
                if let Expr::Term(Term::Symbol(res_name)) = lhs_name_term.as_ref() {
                    if let Some(resource) = heap.maybe_resource_by_name(res_name) {
                        let matching_attrs = resource
                            .names()
                            .iter()
                            .filter(|&s| s.starts_with(sym.as_str()))
                            .map(|s| s.to_owned().to_owned())
                            .collect::<Vec<String>>();
                        if matching_attrs.len() == 1 {
                            return Some(format!("{}.{}", res_name, matching_attrs[0]));
                        } else {
                            // TODO: show top N potential completions
                        }
                    }
                }
            }
        }
        None
    }

    fn try_complete_entity(partial: &NitrousAst, heap: HeapRef) -> Option<String> {
        if let Stmt::Expr(ref e) = partial.statements()[0].as_ref() {
            if let Expr::Term(Term::AtSymbol(sym)) = e.as_ref() {
                let matching_entities = heap
                    .entity_names()
                    .filter(|&s| s.starts_with(sym.as_str()))
                    .collect::<Vec<&str>>();
                if matching_entities.len() == 1 {
                    return Some("@".to_owned() + matching_entities[0]);
                } else {
                    // TODO: show top N potential completions?
                }
            }
        }
        None
    }

    fn try_complete_entity_component(partial: &NitrousAst, heap: HeapRef) -> Option<String> {
        if let Stmt::Expr(ref e) = partial.statements()[0].as_ref() {
            if let Expr::Attr(lhs_name_term, Term::Symbol(sym)) = e.as_ref() {
                if let Expr::Term(Term::AtSymbol(ent_name)) = lhs_name_term.as_ref() {
                    if let Some(entity) = heap.maybe_entity_by_name(ent_name) {
                        if let Some(attrs) = heap.entity_component_names(entity) {
                            let matching_components = attrs
                                .filter(|&s| s.starts_with(sym.as_str()))
                                .collect::<Vec<_>>();
                            if matching_components.len() == 1 {
                                return Some(format!("@{}.{}", ent_name, matching_components[0]));
                            } else {
                                // TODO: show top N potential completions
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn try_complete_entity_component_attrs(partial: &NitrousAst, heap: HeapRef) -> Option<String> {
        if let Stmt::Expr(ref e) = partial.statements()[0].as_ref() {
            if let Expr::Attr(attr_term, Term::Symbol(attr_sym)) = e.as_ref() {
                if let Expr::Attr(ent_term, Term::Symbol(comp_sym)) = attr_term.as_ref() {
                    if let Expr::Term(Term::AtSymbol(ent_sym)) = ent_term.as_ref() {
                        if let Some(entity) = heap.maybe_entity_by_name(ent_sym) {
                            if let Some(attrs) = heap.entity_component_names(entity) {
                                let matching_attrs = attrs
                                    .filter(|&s| s.starts_with(attr_sym.as_str()))
                                    .collect::<Vec<_>>();
                                if matching_attrs.len() == 1 {
                                    return Some(format!(
                                        "@{}.{}.{}",
                                        ent_sym, comp_sym, matching_attrs[0]
                                    ));
                                } else {
                                    // TODO: show top N potential completions
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn try_completion(&self, partial: NitrousAst, heap: HeapRef) -> Option<String> {
        if partial.statements().len() != 1 {
            return None;
        }
        if let Some(s) = Self::try_complete_resource(&partial, heap) {
            return Some(s);
        }
        if let Some(s) = Self::try_complete_resource_attrs(&partial, heap) {
            return Some(s);
        }
        if let Some(s) = Self::try_complete_entity_component_attrs(&partial, heap) {
            return Some(s);
        }
        if let Some(s) = Self::try_complete_entity(&partial, heap) {
            return Some(s);
        }
        if let Some(s) = Self::try_complete_entity_component(&partial, heap) {
            return Some(s);
        }
        None
    }

    fn on_tab_pressed(&mut self, heap: HeapRef) {
        let incomplete = self.edit.read().line().flatten();
        if let Ok(partial) = NitrousAst::parse(&incomplete) {
            if let Some(full) = self.try_completion(partial, heap) {
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

    fn is_help_command(command: &str) -> bool {
        let cmd = command.trim().to_lowercase();
        cmd.starts_with("help") || cmd.starts_with('?') || cmd.ends_with('?')
    }

    fn is_quit_command(command: &str) -> bool {
        let cmd = command.trim().to_lowercase();
        cmd == "quit" || cmd == "exit" || cmd == "q"
    }

    fn on_enter_pressed(&mut self, herder: &mut ScriptHerder) -> Result<()> {
        let command = self.edit.read().line().flatten();
        self.edit.write().line_mut().select_all();
        self.edit.write().line_mut().delete();

        self.add_command_to_history(&command)?;

        match herder.run_interactive(&command) {
            Ok(_) => {}
            Err(e) => {
                // Help with some common cases before we show a scary error.
                if Self::is_help_command(&command) {
                    herder.run_interactive("help()")?;
                } else if Self::is_quit_command(&command) {
                    herder.run_interactive("quit()")?;
                } else {
                    self.show_run_error(&command, &e.to_string());
                }
            }
        }

        Ok(())
    }

    fn show_line(&self, line: &str) {
        let screen = &mut self.output.write();
        println!("{}", line);
        screen.append_line(line);
        if screen.line_count() > 80 {
            screen.remove_first_line();
        }
    }

    pub fn report_script_completions(&self, completions: &[ScriptCompletion]) {
        for completion in completions {
            if completion.meta.kind() == ScriptRunKind::Interactive {
                match &completion.result {
                    ScriptResult::Ok(v) => match v {
                        Value::String(s) => {
                            for line in s.lines() {
                                self.show_line(line);
                            }
                        }
                        Value::ResourceMethod(_, _) | Value::ComponentMethod(_, _, _) => {
                            self.show_line(&format!("{v} is a method, did you mean to call it?",));
                            self.show_line(
                                "Try using up-arrow to go back and add parentheses to the end.",
                            );
                        }
                        _ => {
                            self.show_line(&v.to_string());
                        }
                    },
                    ScriptResult::Err(error) => {
                        self.show_script_error(completion, error);
                    }
                };
            } else if completion.result.is_error() {
                self.show_script_error(completion, completion.result.error().unwrap());
            }
        }
    }

    fn show_script_error(&self, completion: &ScriptCompletion, error: &str) {
        self.show_run_error(&completion.meta.context().script().to_string(), error);
    }

    fn show_run_error(&self, command: &str, error: &str) {
        let screen = &mut self.output.write();

        let prefix = "Script Failed: ";
        let script = command.to_owned();
        let line = format!("{prefix}{script}");
        println!("{}", line);
        screen.append_line(&line);
        screen.last_line_mut().unwrap().select_all();
        screen.last_line_mut().unwrap().change_color(Color::Yellow);
        screen
            .last_line_mut()
            .unwrap()
            .select(prefix.len()..prefix.len() + script.len());
        screen.last_line_mut().unwrap().change_color(Color::Gray);

        let prefix = "  Error: ";
        let line = format!("{prefix}{error}");
        println!("{}", line);
        screen.append_line(&line);
        screen.last_line_mut().unwrap().select_all();
        screen.last_line_mut().unwrap().change_color(Color::Gray);
        screen
            .last_line_mut()
            .unwrap()
            .select(prefix.len()..prefix.len() + error.len());
        screen.last_line_mut().unwrap().change_color(Color::Red);
    }

    // We need the world in order to do completions.
    pub fn handle_terminal_events(&mut self, event: &InputEvent, mut heap: HeapMut) -> Result<()> {
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
                        self.on_tab_pressed(heap.as_ref());
                    }
                    (false, VirtualKeyCode::Up) => {
                        self.on_up_pressed();
                    }
                    (false, VirtualKeyCode::Down) => {
                        self.on_down_pressed();
                    }
                    (false, VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter) => {
                        let herder = &mut heap.resource_mut::<ScriptHerder>();
                        self.on_enter_pressed(herder)?;
                    }
                    _ => {}
                }
            }
        }
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

        // FIXME: handle keyboard focus properly; force it to the line_edit child
        self.edit
            .write()
            .handle_event(event, focus, cursor_position, herder)?;

        // Note: terminal-specific input is handled later in handle_terminal_input

        Ok(())
    }
}
