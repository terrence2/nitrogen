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
use nitrous::{Interpreter, LocalNamespace, Module, ModuleTraitObject, Script};
use std::{any::TypeId, collections::HashMap, mem::transmute, sync::atomic::AtomicPtr};

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
    modules: HashMap<String, ModuleTraitObject>,
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
    pub(crate) fn insert_module<T: Resource + Module>(&mut self, name: String, resource: &T) {
        // Safety:
        // The resource of type T is stored as the first value in a unique_component Column,
        // represented as a BlobVec, where it is the first and only allocation. The allocation
        // was made with std::alloc::alloc, and will only be reallocated if the BlobVec Grows.
        // It will not grow, since this is a unique_component.

        // As such, we can cast it to the &dyn Module here, then transmute to and from TraitObject
        // safely, as long as the underlying allocation never changes. Since modules are permanent
        // and tied to the world and runtime, we will stop running scripts (via the runtime's
        // scheduler) before deallocating the Runtime's World, and thus the storage.
        // let module_trait_obj = resource as &dyn Module;
        // // let module_ptr: *const dyn Module = unsafe { transmute(module) };
        // let module_ptr = AtomicPtr::new(module_trait_obj);
        self.modules.insert(
            name,
            ModuleTraitObject::from_module(resource as &dyn Module),
        );
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
        println!("RUN SCRIPTS: {:#?}", self.modules.keys());
        let mut next_gthreads = Vec::with_capacity(self.gthread.capacity());
        for mut source in self.gthread.drain(..) {
            let mut interpreter = Interpreter::default();
            interpreter.set_modules(self.modules.clone());
            interpreter.interpret2(&source.script, world).unwrap();
            // let output_state = Interpreter::run_until_blocked(&source.locals, &source.script, 0);
            // if let Some(position) = output_state {
            //     source.position = position;
            //     next_gthreads.push(source);
            // }
        }
        self.gthread = next_gthreads;
    }
}
