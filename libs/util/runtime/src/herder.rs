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
use log::error;
use nitrous::{
    ExecutionContext, LocalNamespace, NitrousExecutor, NitrousScript, ResourceNamespace,
    ScriptResource, YieldState,
};

/// Manage script execution state.
pub struct ScriptHerder {
    gthread: Vec<ExecutionContext>,
    resource_namespace: ResourceNamespace,
}

impl Default for ScriptHerder {
    fn default() -> Self {
        Self {
            gthread: Vec::new(),
            resource_namespace: ResourceNamespace::empty(),
        }
    }
}

impl ScriptHerder {
    pub fn run_string(&mut self, script_text: &str) -> Result<()> {
        println!("RUN STRING: {}", script_text);
        self.run(NitrousScript::compile(script_text)?);
        Ok(())
    }

    pub fn run<N: Into<NitrousScript>>(&mut self, script: N) {
        self.run_with_locals(LocalNamespace::empty(), script)
    }

    pub fn run_with_locals<N: Into<NitrousScript>>(&mut self, locals: LocalNamespace, script: N) {
        self.gthread
            .push(ExecutionContext::new(locals, script.into()));
    }

    #[inline]
    pub(crate) fn insert_module<T: Resource + ScriptResource>(
        &mut self,
        name: String,
        resource: &T,
    ) {
        self.resource_namespace
            .insert_named_resource(name, resource);
    }

    pub(crate) fn sys_run_scripts(world: &mut World) {
        world.resource_scope(|world, mut herder: Mut<ScriptHerder>| {
            herder.run_scripts(world);
        });
    }

    fn run_scripts(&mut self, world: &mut World) {
        let mut next_gthreads = Vec::with_capacity(self.gthread.capacity());
        for mut script_context in self.gthread.drain(..) {
            let mut executor =
                NitrousExecutor::new(&mut script_context, &mut self.resource_namespace, world);
            match executor.run_until_yield() {
                Ok(yield_state) => match yield_state {
                    YieldState::Yielded => next_gthreads.push(script_context),
                    YieldState::Finished => {}
                },
                Err(err) => {
                    error!("script failed with: {}", err);
                }
            }
        }
        self.gthread = next_gthreads;
    }
}
