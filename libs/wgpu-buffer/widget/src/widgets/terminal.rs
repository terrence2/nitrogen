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
    font_context::FontId,
    layout::{LayoutMeasurements, LayoutPacking},
    paint_context::PaintContext,
    region::Extent,
    text_run::TextRun,
    WidgetBuffer, WidgetInfo, WidgetRenderStep,
};
use anyhow::{Context, Result};
use bevy_ecs::prelude::*;
use csscolorparser::Color;
use gpu::Gpu;
use input::{ElementState, InputEvent, InputEventVec, InputSystem, InputTarget, VirtualKeyCode};
use nitrous::{
    inject_nitrous_resource,
    ir::{Expr, Stmt, Term},
    method, HeapMut, HeapRef, NitrousAst, NitrousResource, Value,
};
use platform_dirs::AppDirs;
use runtime::{
    report, Extension, Runtime, RuntimeStep, ScriptCompletion, ScriptCompletions, ScriptHerder,
    ScriptResult, ScriptRunKind, ERROR_REPORTS,
};
use std::{
    collections::VecDeque,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};
use window::{
    size::{AbsSize, RelSize, ScreenDir, Size},
    Window,
};

// TODO: expand this once we have scroll bars
const HISTORY_SIZE: usize = 80;

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum TerminalSimStep {
    HandleEvents,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum TerminalRenderStep {
    ReportScriptCompletions,
    Measure,
    Upload,
}

#[derive(Component, Debug)]
pub struct TerminalWidgetTag;

// Items packed from top to bottom.
#[derive(NitrousResource, Debug)]
pub struct Terminal {
    lines: VecDeque<TextRun>,
    edit: TextRun,
    prompt: TextRun,

    font_id: FontId,
    font_size: Size,
    visible: bool,

    history: Vec<String>,
    history_file: Option<File>,
    history_cursor: usize,
}

impl Extension for Terminal {
    fn init(runtime: &mut Runtime) -> Result<()> {
        let font_size = AbsSize::Pts(14.0);
        let font_id = {
            let gpu = runtime.resource::<Gpu>();
            let context = runtime.resource::<PaintContext>();
            let font_id = context.font_context.font_id_for_name("dejavu-mono");
            context
                .font_context
                .cache_ascii_glyphs(font_id, font_size, gpu)?;
            context
                .font_context
                .cache_ascii_glyphs(font_id, font_size, gpu)?;
            font_id
        };

        let terminal = Terminal::new(
            font_id,
            font_size.into(),
            &runtime.resource::<AppDirs>().state_dir,
        )?;
        runtime.insert_named_resource("terminal", terminal);
        let term_packing = LayoutPacking::default()
            .float_start()
            .float_top()
            .set_background("#222a")?
            .set_padding("2px", runtime.heap_mut())?
            .set_border_color("#0061cf")?
            .set_border_bottom("3px", runtime.heap_mut())?
            .to_owned();
        let term_id = runtime
            .spawn_named("terminal")?
            .insert(TerminalWidgetTag)
            .insert_named(term_packing)?
            .insert(LayoutMeasurements::default())
            .id();
        runtime
            .resource_mut::<WidgetBuffer>()
            .root_mut()
            .push_widget(term_id)?;

        runtime.add_input_system(
            Self::sys_handle_terminal_events
                .exclusive_system()
                .label(TerminalSimStep::HandleEvents),
        );

        runtime.add_startup_system(
            Self::sys_report_script_completions.label(TerminalRenderStep::ReportScriptCompletions),
        );
        runtime.add_frame_system(
            Self::sys_report_script_completions
                .label(TerminalRenderStep::ReportScriptCompletions)
                .before(TerminalRenderStep::Measure)
                .before(RuntimeStep::ClearCompletions),
        );

        runtime.add_frame_system(
            Terminal::sys_measure
                .label(TerminalRenderStep::Measure)
                .before(WidgetRenderStep::LayoutWidgets),
        );
        runtime.add_frame_system(
            Terminal::sys_upload
                .label(TerminalRenderStep::Upload)
                .after(WidgetRenderStep::PrepareForFrame)
                .after(WidgetRenderStep::LayoutWidgets)
                .before(WidgetRenderStep::EnsureUploaded),
        );

        Ok(())
    }
}

#[inject_nitrous_resource]
impl Terminal {
    const WIDTH: RelSize = RelSize::from_percent(100.);
    const HEIGHT: RelSize = RelSize::from_percent(40.);

    fn sys_measure(
        mut terminals: Query<(
            &TerminalWidgetTag,
            &mut LayoutPacking,
            &mut LayoutMeasurements,
        )>,
        terminal: Res<Terminal>,
        input_target: Res<InputTarget>,
        win: Res<Window>,
        paint_context: Res<PaintContext>,
    ) {
        let (_, mut packing, mut measure) = terminals.single_mut();
        packing.set_display(input_target.terminal_active());
        for line in &terminal.lines {
            let _ = report!(line.measure(&win, &paint_context.font_context));
        }
        report!(terminal.edit.measure(&win, &paint_context.font_context));
        let metrics = report!(terminal.prompt.measure(&win, &paint_context.font_context));
        measure.set_metrics(metrics);
        measure.set_child_extent(Extent::new(Self::WIDTH, Self::HEIGHT), &packing);
    }

    fn sys_upload(
        terminals: Query<(&TerminalWidgetTag, &LayoutMeasurements)>,
        terminal: Res<Terminal>,
        input_target: Res<InputTarget>,
        win: Res<Window>,
        gpu: Res<Gpu>,
        mut context: ResMut<PaintContext>,
    ) {
        if !input_target.terminal_active() {
            return;
        }

        let (_, measure) = terminals.single();
        let info = context.push_widget(&WidgetInfo::default());

        let mut pos = *measure.child_allocation().position();
        *pos.bottom_mut() -= measure.metrics().descent.as_rel(&win, ScreenDir::Vertical);

        report!(terminal
            .prompt
            .upload(pos.into(), info, &win, &gpu, &mut context));
        *pos.left_mut() += measure.metrics().width.as_rel(&win, ScreenDir::Horizontal);
        report!(terminal
            .edit
            .upload(pos.into(), info, &win, &gpu, &mut context));
        *pos.left_mut() -= measure.metrics().width.as_rel(&win, ScreenDir::Horizontal);

        let h = measure.metrics().height.as_rel(&win, ScreenDir::Vertical);
        for line in &terminal.lines {
            *pos.bottom_mut() += h;
            report!(line.upload(pos.into(), info, &win, &gpu, &mut context));
        }
    }

    fn sys_handle_terminal_events(world: &mut World) {
        if !world.resource::<InputTarget>().terminal_active() {
            return;
        }
        let events = world.resource::<InputEventVec>().to_owned();
        world.resource_scope(|world, mut terminal: Mut<Terminal>| {
            for event in &events {
                report!(terminal.handle_terminal_events(event, HeapMut::wrap(world)));
            }
        });
    }

    fn sys_report_script_completions(
        mut terminal: ResMut<Terminal>,
        completions: Res<ScriptCompletions>,
    ) {
        terminal.report_script_completions(&completions);
    }

    pub fn new(font_id: FontId, font_size: Size, state_dir: &Path) -> Result<Self> {
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

        let mut terminal = Self {
            lines: VecDeque::with_capacity(HISTORY_SIZE * 4),
            edit: TextRun::empty()
                .with_default_font(font_id)
                .with_default_size(font_size)
                .with_default_color(&Color::from([1., 1., 1.]))
                .with_text("help()"),
            prompt: TextRun::empty()
                .with_hidden_selection()
                .with_default_font(font_id)
                .with_default_size(font_size)
                .with_default_color(&Color::from([0.8, 0.8, 1.]))
                .with_text("n2o\u{27a4} "),
            font_id,
            font_size,
            visible: true,
            history,
            history_file,
            history_cursor,
        };

        terminal.println("Nitrogen Terminal starting up...");
        terminal.println("Type `help()` for help.");

        Ok(terminal)
    }

    #[method]
    pub fn set_font_size(&mut self, size_pts: f64) {
        let sz = Size::from_pts(size_pts as f32);
        for line in self.lines.iter_mut() {
            line.set_default_size(sz);
            line.select_all();
            line.change_size(sz);
            line.select_none();
        }
        self.edit.set_default_size(sz);
        self.edit.select_all();
        self.edit.change_size(sz);
        self.edit.select_none();
    }

    fn add_command_to_history(&mut self, command: &str) -> Result<()> {
        // Echo the command into the output buffer as a literal.
        self.println(&("> ".to_owned() + command));

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

    #[method]
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    pub fn println(&mut self, line: &str) {
        println!("{}", line);
        let run = TextRun::empty()
            .with_default_color(&Color::from([0., 1., 0.]))
            .with_default_font(self.font_id)
            .with_default_size(self.font_size)
            .with_hidden_selection()
            .with_text(line);
        self.lines.push_front(run);
        if self.lines.len() > HISTORY_SIZE {
            self.lines.pop_back();
        }
    }

    fn last_line_mut(&mut self) -> &mut TextRun {
        self.lines.front_mut().unwrap()
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
        let incomplete = self.edit.flatten();
        if let Ok(partial) = NitrousAst::parse(&incomplete) {
            if let Some(full) = self.try_completion(partial, heap) {
                self.edit.select_all();
                self.edit.insert(&full);
            }
        }
    }

    fn on_up_pressed(&mut self) {
        if self.history_cursor > 0 {
            self.history_cursor -= 1;
            self.edit.select_all();
            self.edit.insert(&self.history[self.history_cursor]);
        }
    }

    fn on_down_pressed(&mut self) {
        if self.history_cursor < self.history.len() {
            self.history_cursor += 1;
            self.edit.select_all();
            if self.history_cursor < self.history.len() {
                self.edit.insert(&self.history[self.history_cursor]);
            } else {
                self.edit.insert("");
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
        let command = self.edit.flatten();
        self.edit.select_all();
        self.edit.delete();

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

    pub fn report_script_completions(&mut self, completions: &[ScriptCompletion]) {
        for err in ERROR_REPORTS.lock().unwrap().drain(..) {
            self.println(&err);
        }
        for completion in completions {
            if completion.meta.kind() == ScriptRunKind::Interactive {
                match &completion.result {
                    ScriptResult::Ok(v) => match v {
                        Value::String(s) => {
                            for line in s.lines() {
                                self.println(line);
                            }
                        }
                        Value::ResourceMethod(_, _) | Value::ComponentMethod(_, _, _) => {
                            self.println(&format!("{v} is a method, did you mean to call it?",));
                            self.println(
                                "Try using up-arrow to go back and add parentheses to the end.",
                            );
                        }
                        _ => {
                            self.println(&v.to_string());
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

    fn show_script_error(&mut self, completion: &ScriptCompletion, error: &str) {
        self.show_run_error(&completion.meta.context().script().to_string(), error);
    }

    fn show_run_error(&mut self, command: &str, error: &str) {
        let prefix = "Script Failed: ";
        let script = command.to_owned();
        self.println(&format!("{prefix}{script}"));
        self.last_line_mut().select_all();
        self.last_line_mut()
            .change_color(&Color::from([1., 1., 0.]));
        self.last_line_mut()
            .select(prefix.len()..prefix.len() + script.len());
        self.last_line_mut().change_color(&Color::from([0.8; 3]));

        let mut errors = error.lines();
        let error = errors.next().unwrap();
        let prefix = "  Error: ";
        self.println(&format!("{prefix}{error}"));
        self.last_line_mut().select_all();
        self.last_line_mut().change_color(&Color::from([0.8; 3]));
        self.last_line_mut()
            .select(prefix.len()..prefix.len() + error.len());
        self.last_line_mut()
            .change_color(&Color::from([1., 0., 0.]));
        for error in errors {
            self.println(&format!("         {error}"));
            self.last_line_mut().select_all();
            self.last_line_mut()
                .change_color(&Color::from([1., 0., 0.]));
        }
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
                    (false, VirtualKeyCode::Home) => self.edit.move_home(modifiers_state),
                    (false, VirtualKeyCode::Delete) => self.edit.delete(),
                    (false, VirtualKeyCode::Back) => self.edit.backspace(),
                    (false, VirtualKeyCode::End) => self.edit.move_end(modifiers_state),
                    (false, VirtualKeyCode::Left) => self.edit.move_left(modifiers_state),
                    (false, VirtualKeyCode::Right) => self.edit.move_right(modifiers_state),
                    (false, virtual_keycode) => {
                        let (base, shifted) = InputSystem::code_to_char(virtual_keycode);
                        if let Some(mut c) = base {
                            if modifiers_state.shift() {
                                c = shifted.unwrap_or(c);
                            }
                            self.edit.insert(&c.to_string());
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}
