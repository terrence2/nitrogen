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
use bevy_ecs::{prelude::*, system::Resource};
use log::{trace, warn};
use nitrous::{
    ComponentLookupMutFunc, ExecutionContext, LocalNamespace, NitrousExecutor, NitrousScript,
    ScriptResource, Value, WorldIndex, YieldState,
};
use std::sync::Arc;

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
    index: WorldIndex,
}

impl Default for ScriptHerder {
    fn default() -> Self {
        Self {
            gthread: Vec::new(),
            index: WorldIndex::empty(),
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
        locals: LocalNamespace,
        script: N,
        kind: ScriptRunKind,
    ) {
        self.gthread.push(ExecutionMetadata {
            context: ExecutionContext::new(locals, script.into()),
            kind,
        });
    }

    #[inline]
    pub fn resource_names(&self) -> impl Iterator<Item = &String> {
        self.index.resource_names()
    }

    #[inline]
    pub fn lookup_resource(&self, name: &str) -> Option<Value> {
        self.index.lookup_resource(name)
    }

    #[inline]
    pub fn attrs<'a>(&'a self, value: Value, world: &'a mut World) -> Result<Vec<&'a str>> {
        value.attrs(&self.index, world)
    }

    #[inline]
    pub(crate) fn insert_named_resource<T>(&mut self, name: String)
    where
        T: Resource + ScriptResource + 'static,
    {
        self.index.insert_named_resource::<T>(name);
    }

    #[inline]
    pub(crate) fn upsert_named_component(
        &mut self,
        entity_name: &str,
        entity: Entity,
        component_name: &str,
        lookup: Arc<ComponentLookupMutFunc>,
    ) -> Result<()> {
        self.index
            .upsert_named_component(entity_name, entity, component_name, lookup)
    }

    #[inline]
    pub(crate) fn sys_run_startup_scripts(world: &mut World) {
        world.resource_scope(|world, mut herder: Mut<ScriptHerder>| {
            herder.run_scripts(world, ScriptRunPhase::Startup);
        });
    }

    #[inline]
    pub(crate) fn sys_run_sim_scripts(world: &mut World) {
        world.resource_scope(|world, mut herder: Mut<ScriptHerder>| {
            herder.run_scripts(world, ScriptRunPhase::Sim);
        });
    }

    fn run_scripts(&mut self, world: &mut World, phase: ScriptRunPhase) {
        world
            .get_resource_mut::<ScriptCompletions>()
            .unwrap()
            .clear();
        let mut next_gthreads = Vec::with_capacity(self.gthread.capacity());
        for mut meta in self.gthread.drain(..) {
            let mut executor = NitrousExecutor::new(&mut meta.context, &mut self.index, world);
            match executor.run_until_yield() {
                Ok(yield_state) => match yield_state {
                    YieldState::Yielded => next_gthreads.push(meta),
                    YieldState::Finished(result) => {
                        trace!("{:?}: {} <- {}", phase, result, meta.context.script());
                        world.get_resource_mut::<ScriptCompletions>().unwrap().push(
                            ScriptCompletion {
                                result: ScriptResult::Ok(result),
                                phase,
                                meta,
                            },
                        );
                    }
                },
                Err(err) => {
                    warn!("script failed: {}", err);
                    world
                        .get_resource_mut::<ScriptCompletions>()
                        .unwrap()
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
