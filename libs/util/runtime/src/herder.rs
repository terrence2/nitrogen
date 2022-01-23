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
use bevy_ecs::{prelude::*, system::Resource, world::WorldCell};
use nitrous::{Interpreter, LocalNamespace, Module, Script};
use std::{any::TypeId, collections::HashMap};

struct ScriptState {
    script: Script,
    locals: LocalNamespace,
    position: usize,
}

impl ScriptState {
    pub fn new(script: Script) -> Self {
        Self {
            script,
            locals: LocalNamespace::empty(),
            position: 0,
        }
    }
}

/// Manage script execution state.
pub struct ScriptHerder {
    gthread: Vec<ScriptState>,
    modules: HashMap<String, TypeId>,
}

impl Default for ScriptHerder {
    fn default() -> Self {
        Self {
            gthread: Vec::new(),
            modules: HashMap::new(),
        }
    }
}

impl ScriptHerder {
    pub fn run(&mut self, script: Script) {
        self.gthread.push(ScriptState::new(script));
    }

    pub fn run_string(&mut self, script_text: &str) -> Result<()> {
        self.run(Script::compile(script_text)?);
        Ok(())
    }

    pub fn run_with_locals(&mut self, locals: LocalNamespace, script: &Script) {
        // TODO
    }

    #[inline]
    pub(crate) fn insert_module<T: Resource + Module>(&mut self, name: String) {
        self.modules.insert(name, TypeId::of::<T>());
    }

    // pub fn global_names(&self) -> impl Iterator<Item = &str> {
    //     unimplemented!()
    // }

    // pub fn get_global(&self) -> Option<&Value> {
    //
    // }

    pub(crate) fn sys_run_scripts(world: &mut World) {
        let mut herder = world.remove_resource::<ScriptHerder>().unwrap();
        herder.run_scripts(world);
        world.insert_resource(herder);
    }

    fn run_scripts(&mut self, world: &mut World) {
        let mut next_gthreads = Vec::with_capacity(self.gthread.capacity());
        for mut source in self.gthread.drain(..) {
            println!("RUNNING SCRIPT: {}", source.script);
            // let output_state = Interpreter::run_until_blocked(&source.locals, &source.script, 0);
            // if let Some(position) = output_state {
            //     source.position = position;
            //     next_gthreads.push(source);
            // }
        }
        self.gthread = next_gthreads;
    }
}
