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
use itertools::*;
use log::{trace, warn};
use nitrous::{
    ExecutionContext, HeapMut, HeapRef, LocalNamespace, NitrousExecutor, NitrousScript, Value,
    YieldState,
};
use once_cell::sync::Lazy;
use std::sync::{Arc, Mutex};

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
    @player.throttle.set_detent(4)
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

/// Sometimes scripts need to run other scripts. But since we're inside Herder,
/// it's not available to push to directly. Herder will check this resource at
/// the start of it's run phase and start anything that's been queued.
#[derive(Debug, Default)]
pub struct ScriptQueue {
    queue: Vec<String>,
}

impl ScriptQueue {
    pub fn run_interactive<S: Into<String>>(&mut self, script_text: S) {
        self.queue.push(script_text.into());
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
    receipt: ScriptReceipt,
}

impl ExecutionMetadata {
    pub fn kind(&self) -> ScriptRunKind {
        self.kind
    }

    pub fn context(&self) -> &ExecutionContext {
        &self.context
    }

    pub fn maybe_add_builtins(&mut self, heap: HeapRef) {
        if self.context.has_started() {
            // Don't repeat for yielded scripts
            return;
        }

        self.context.locals_mut().put_if_absent(
            "exit",
            Value::RustMethod(Arc::new(|_args, mut heap| {
                heap.resource_mut::<ExitRequest>().request_exit();
                Ok(Value::True())
            })),
        );
        if self.kind == ScriptRunKind::Interactive {
            #[allow(unstable_name_collisions)]
            let item_list: Value = (String::new()
                + "Resources:\n  "
                + &heap
                    .resource_names()
                    .intersperse("\n  ")
                    .collect::<String>()
                + "\nEntities:\n  @"
                + &heap.entity_names().intersperse("\n  @").collect::<String>())
                .into();
            self.context.locals_mut().put_if_absent(
                "list",
                Value::RustMethod(Arc::new(move |_, _| Ok(item_list.clone()))),
            );
            self.context.locals_mut().put_if_absent(
                "help",
                Value::RustMethod(Arc::new(move |_, _| Ok(GUIDE.to_owned().into()))),
            );
        }
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

    pub fn unwrap(self) -> Value {
        match self {
            Self::Ok(v) => v,
            Self::Err(e) => panic!("ScriptResult::unwrap: {}", e),
        }
    }
}

/// Returned by run_script so that script results can be correlated.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct ScriptReceipt(usize);

/// Report on script execution result.
#[derive(Clone, Debug)]
pub struct ScriptCompletion {
    pub receipt: ScriptReceipt,
    pub result: ScriptResult,
    pub phase: ScriptRunPhase,
    pub meta: ExecutionMetadata,
}

impl ScriptCompletion {
    pub fn unwrap(&self) -> Value {
        self.result.clone().unwrap()
    }
}

/// A set of script execution results, indented for use as a resource for other systems.
pub type ScriptCompletions = Vec<ScriptCompletion>;

/// Additional reporting to make globally.
pub static ERROR_REPORTS: Lazy<Mutex<Vec<String>>> = Lazy::new(|| Mutex::new(Vec::new()));

#[macro_export]
macro_rules! report {
    ($expr:expr $(,)?) => {
        match $expr {
            $crate::reexport::result::Result::Ok(val) => val,
            $crate::reexport::result::Result::Err(err) => {
                $crate::reexport::log::error!("{}\n{}", err, err.backtrace());
                $crate::ERROR_REPORTS.lock().unwrap().push(err.to_string());
                return;
                //return $crate::result::Result::Err($crate::convert::From::from(err));
            }
        }
    };
}

/// Manage script execution state.
#[derive(Default)]
pub struct ScriptHerder {
    receipt_offset: usize,
    gthread: Vec<ExecutionMetadata>,
}

impl ScriptHerder {
    #[inline]
    pub fn run_interactive(&mut self, script_text: &str) -> Result<ScriptReceipt> {
        Ok(self.run(
            NitrousScript::compile(script_text)?,
            ScriptRunKind::Interactive,
        ))
    }

    #[inline]
    pub fn run_string(&mut self, script_text: &str) -> Result<ScriptReceipt> {
        trace!("run_string: {}", script_text);
        Ok(self.run(NitrousScript::compile(script_text)?, ScriptRunKind::String))
    }

    #[inline]
    pub fn run<N: Into<NitrousScript>>(&mut self, script: N, kind: ScriptRunKind) -> ScriptReceipt {
        self.run_with_locals(LocalNamespace::empty(), script, kind)
    }

    #[inline]
    pub fn run_binding<N: Into<NitrousScript>>(
        &mut self,
        locals: LocalNamespace,
        script: N,
    ) -> ScriptReceipt {
        self.run_with_locals(locals, script, ScriptRunKind::Binding)
    }

    #[inline]
    pub fn run_with_locals<N: Into<NitrousScript>>(
        &mut self,
        locals: LocalNamespace,
        script: N,
        kind: ScriptRunKind,
    ) -> ScriptReceipt {
        self.receipt_offset += 1;
        let receipt = ScriptReceipt(self.receipt_offset);
        self.gthread.push(ExecutionMetadata {
            context: ExecutionContext::new(locals, script.into()),
            kind,
            receipt,
        });
        receipt
    }

    #[inline]
    pub(crate) fn sys_run_startup_scripts(world: &mut World) {
        world.resource_scope(|world, mut herder: Mut<ScriptHerder>| {
            herder._run_scripts(HeapMut::wrap(world), ScriptRunPhase::Startup);
        });
        trace!("clearing startup script completions");
        world
            .get_resource_mut::<ScriptCompletions>()
            .unwrap()
            .clear();
    }

    #[inline]
    pub(crate) fn sys_run_sim_scripts(world: &mut World) {
        world.resource_scope(|world, mut herder: Mut<ScriptHerder>| {
            herder._run_scripts(HeapMut::wrap(world), ScriptRunPhase::Sim);
        });
    }

    pub(crate) fn sys_clear_completions(mut completions: ResMut<ScriptCompletions>) {
        // This runs at frame schedule, whereas scripts may run each sim step.
        // trace!("clearing script completions");
        completions.clear();
    }

    // Exposed for testing the internals
    pub fn _run_scripts(&mut self, mut heap: HeapMut, phase: ScriptRunPhase) {
        // If there are any scripts queued to run, start them up.
        for script in &heap.resource::<ScriptQueue>().queue {
            if let Err(err) = self.run_interactive(script) {
                warn!("script failed: {}", err);
                #[cfg(debug_assertions)]
                {
                    println!("script failed: {}", err);
                }
            }
        }
        heap.resource_mut::<ScriptQueue>().queue.clear();

        let mut next_gthreads = Vec::with_capacity(self.gthread.capacity());
        for mut meta in self.gthread.drain(..) {
            meta.maybe_add_builtins(heap.as_ref());
            let executor = NitrousExecutor::new(&mut meta.context, heap.as_mut());
            match executor.run_until_yield() {
                Ok(yield_state) => match yield_state {
                    YieldState::Yielded => next_gthreads.push(meta),
                    YieldState::Finished(result) => {
                        trace!("{:?}: {} <- {}", phase, result, meta.context.script());
                        heap.resource_mut::<ScriptCompletions>()
                            .push(ScriptCompletion {
                                receipt: meta.receipt,
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
                            receipt: meta.receipt,
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
