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
use log::{error, info, trace};
use nitrous::{
    ComponentLookupMutFunc, ExecutionContext, LocalNamespace, NitrousExecutor, NitrousScript,
    ScriptResource, Value, WorldIndex, YieldState,
};
use std::sync::Arc;

/// Manage script execution state.
pub struct ScriptHerder {
    gthread: Vec<ExecutionContext>,
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
    pub fn run_string(&mut self, script_text: &str) -> Result<()> {
        trace!("run_string: {}", script_text);
        self.run(NitrousScript::compile(script_text)?);
        Ok(())
    }

    #[inline]
    pub fn run<N: Into<NitrousScript>>(&mut self, script: N) {
        self.run_with_locals(LocalNamespace::empty(), script)
    }

    #[inline]
    pub fn run_with_locals<N: Into<NitrousScript>>(&mut self, locals: LocalNamespace, script: N) {
        self.gthread
            .push(ExecutionContext::new(locals, script.into()));
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
    pub(crate) fn sys_run_scripts(world: &mut World) {
        world.resource_scope(|world, mut herder: Mut<ScriptHerder>| {
            herder.run_scripts(world);
        });
    }

    fn run_scripts(&mut self, world: &mut World) {
        let mut next_gthreads = Vec::with_capacity(self.gthread.capacity());
        for mut script_context in self.gthread.drain(..) {
            let mut executor = NitrousExecutor::new(&mut script_context, &mut self.index, world);
            match executor.run_until_yield() {
                Ok(yield_state) => match yield_state {
                    YieldState::Yielded => next_gthreads.push(script_context),
                    YieldState::Finished(v) => {
                        info!("script finish: {}", v);
                    }
                },
                Err(err) => {
                    error!("script failed with: {}", err);
                }
            }
        }
        self.gthread = next_gthreads;
    }
}
