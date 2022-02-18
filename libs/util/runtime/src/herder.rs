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
use anyhow::Result;
use bevy_ecs::prelude::*;
use log::{trace, warn};
use nitrous::{
    ExecutionContext, HeapMut, LocalNamespace, NitrousExecutor, NitrousScript, Value, YieldState,
};
use std::sync::Arc;

pub const GUIDE: &str = r#"
Welcome to the Nitrogen Terminal
--------------------------------
From here, you can tweak and investigate every aspect of the game.

The command `list()` may be used at the top level, or on any item, to get a list
of all items that can be accessed there. Use `help()` to show this message again.

Engine "resources" are accessed with the name of the resource followed by a dot,
followed by the name of a property or method on the resource. Methods may be called
by adding a pair of parentheses after.

Examples:
   terrain.toggle_pin_camera(true)

Named game "entities" are accessed with an @ symbol, followed by the name of the
entity, followed by a dot, followed by the name of a "component" on the entity,
followed by another dot, followed by the name of a property or method on that
component. As with resources, methods are called by appending parentheses.

Examples:
    @player.camera.exposure()
"#;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ExitRequest {
    Exit,
    Continue,
}

impl ExitRequest {
    pub fn request_exit(&mut self) {
        *self = ExitRequest::Exit;
    }

    pub fn still_running(&self) -> bool {
        *self == ExitRequest::Continue
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ScriptRunKind {
    Interactive,
    String,
    Precompiled,
    Binding,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ScriptRunPhase {
    Startup,
    Sim,
}

#[derive(Clone, Debug)]
pub struct ExecutionMetadata {
    context: ExecutionContext,
    kind: ScriptRunKind,
}

impl ExecutionMetadata {
    pub fn kind(&self) -> ScriptRunKind {
        self.kind
    }

    pub fn context(&self) -> &ExecutionContext {
        &self.context
    }
}

#[derive(Clone, Debug)]
pub enum ScriptResult {
    Ok(Value),
    Err(String),
}

impl ScriptResult {
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Err(_))
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Ok(_) => None,
            Self::Err(s) => Some(s.as_str()),
        }
    }
}

/// Report on script execution result.
#[derive(Clone, Debug)]
pub struct ScriptCompletion {
    pub result: ScriptResult,
    pub phase: ScriptRunPhase,
    pub meta: ExecutionMetadata,
}

/// A set of script execution results, indented for use as a resource for other systems.
pub type ScriptCompletions = Vec<ScriptCompletion>;

/// Manage script execution state.
pub struct ScriptHerder {
    gthread: Vec<ExecutionMetadata>,
}

impl Default for ScriptHerder {
    fn default() -> Self {
        Self {
            gthread: Vec::new(),
        }
    }
}

impl ScriptHerder {
    #[inline]
    pub fn run_interactive(&mut self, script_text: &str) -> Result<()> {
        self.run(
            NitrousScript::compile(script_text)?,
            ScriptRunKind::Interactive,
        );
        Ok(())
    }

    #[inline]
    pub fn run_string(&mut self, script_text: &str) -> Result<()> {
        trace!("run_string: {}", script_text);
        self.run(NitrousScript::compile(script_text)?, ScriptRunKind::String);
        Ok(())
    }

    #[inline]
    pub fn run<N: Into<NitrousScript>>(&mut self, script: N, kind: ScriptRunKind) {
        self.run_with_locals(LocalNamespace::empty(), script, kind)
    }

    #[inline]
    pub fn run_binding<N: Into<NitrousScript>>(&mut self, locals: LocalNamespace, script: N) {
        self.run_with_locals(locals, script, ScriptRunKind::Binding)
    }

    #[inline]
    pub fn run_with_locals<N: Into<NitrousScript>>(
        &mut self,
        mut locals: LocalNamespace,
        script: N,
        kind: ScriptRunKind,
    ) {
        Self::add_builtins(kind, /*&self.index,*/ &mut locals);
        self.gthread.push(ExecutionMetadata {
            context: ExecutionContext::new(locals, script.into()),
            kind,
        });
    }

    pub fn add_builtins(
        kind: ScriptRunKind, /*, index: &WorldIndex*/
        locals: &mut LocalNamespace,
    ) {
        locals.put_if_absent(
            "exit",
            Value::RustMethod(Arc::new(|_args, world| {
                if let Some(mut exit) = world.get_resource_mut::<ExitRequest>() {
                    exit.request_exit();
                }
                Value::True()
            })),
        );
        if kind == ScriptRunKind::Interactive {
            // FIXME: we still need a way to do this
            /*
            #[allow(unstable_name_collisions)]
            let resource_list: Value = index
                .resource_names()
                .intersperse("\n")
                .collect::<String>()
                .into();
            locals.put_if_absent(
                "list",
                Value::RustMethod(Arc::new(move |_, _| resource_list.clone())),
            );
             */
            locals.put_if_absent(
                "help",
                Value::RustMethod(Arc::new(move |_, _| GUIDE.to_owned().into())),
            );
        }
    }

    #[inline]
    pub(crate) fn sys_run_startup_scripts(world: &mut World) {
        world.resource_scope(|world, mut herder: Mut<ScriptHerder>| {
            herder.run_scripts(HeapMut::wrap(world), ScriptRunPhase::Startup);
        });
    }

    #[inline]
    pub(crate) fn sys_run_sim_scripts(world: &mut World) {
        world.resource_scope(|world, mut herder: Mut<ScriptHerder>| {
            herder.run_scripts(HeapMut::wrap(world), ScriptRunPhase::Sim);
        });
    }

    fn run_scripts(&mut self, mut heap: HeapMut, phase: ScriptRunPhase) {
        // If there are any script completions left, dump them.
        heap.resource_mut::<ScriptCompletions>().clear();

        let mut next_gthreads = Vec::with_capacity(self.gthread.capacity());
        for mut meta in self.gthread.drain(..) {
            let executor = NitrousExecutor::new(&mut meta.context, heap.as_mut());
            match executor.run_until_yield() {
                Ok(yield_state) => match yield_state {
                    YieldState::Yielded => next_gthreads.push(meta),
                    YieldState::Finished(result) => {
                        trace!("{:?}: {} <- {}", phase, result, meta.context.script());
                        heap.resource_mut::<ScriptCompletions>()
                            .push(ScriptCompletion {
                                result: ScriptResult::Ok(result),
                                phase,
                                meta,
                            });
                    }
                },
                Err(err) => {
                    warn!("script failed: {}", err);
                    heap.resource_mut::<ScriptCompletions>()
                        .push(ScriptCompletion {
                            result: ScriptResult::Err(format!("{}", err)),
                            phase,
                            meta,
                        });
                }
            }
        }
        self.gthread = next_gthreads;
    }
}
