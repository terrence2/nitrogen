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
    dump_schedule::dump_schedule,
    herder::{ExitRequest, ScriptCompletions, ScriptHerder, ScriptQueue, ScriptRunKind},
};
use anyhow::Result;
use bevy_ecs::{
    prelude::*, query::WorldQuery, schedule::IntoSystemDescriptor, system::Resource,
    world::EntityMut,
};
use bevy_tasks::TaskPool;
use nitrous::{
    inject_nitrous_resource, method, Heap, HeapMut, LocalNamespace, NamedEntityMut,
    NitrousResource, NitrousScript, ScriptResource,
};
use std::{fs, path::PathBuf};

/// Interface for extending the Runtime.
pub trait Extension {
    fn init(runtime: &mut Runtime) -> Result<()>;
}

/// Systems may be scheduled to run at startup.
/// The startup scripts run in RunScript.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, StageLabel)]
pub enum StartupStage {
    Main,
}

/// Systems may be scheduled to run after the mainloop, for cleanup.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, StageLabel)]
pub enum ShutdownStage {
    Cleanup,
}

/// The simulation schedule should be used for "pure" entity to entity work and update of
/// a handful of game related resources, rather than communicating with the GPU.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, StageLabel)]
pub enum SimStage {
    /// Pre-script parallel phase.
    Main,
    /// Fully serial phase where scripts run.
    RunScript,
}

// Copy from entities into buffers more suitable for upload to the GPU. Also, do heavier
// CPU-side graphics work that can be parallelized efficiently, like updating the terrain
// from the current cameras. Not generally for actually writing to the GPU.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, StageLabel)]
pub enum FrameStage {
    Main,
}

#[derive(Debug, Default, NitrousResource)]
pub struct RuntimeResource;

#[inject_nitrous_resource]
impl RuntimeResource {
    #[method]
    fn exec(&self, filename: &str, mut heap: HeapMut) -> Result<()> {
        let script_text = fs::read_to_string(&PathBuf::from(filename))?;
        heap.resource_mut::<ScriptQueue>()
            .run_interactive(&script_text);
        Ok(())
    }
}

pub struct Runtime {
    heap: Heap,
    startup_schedule: Schedule,
    sim_schedule: Schedule,
    frame_schedule: Schedule,
    shutdown_schedule: Schedule,
    dump_schedules: bool,
}

impl Default for Runtime {
    fn default() -> Self {
        let startup_schedule = Schedule::default().with_stage(
            StartupStage::Main,
            SystemStage::single_threaded()
                .with_system(ScriptHerder::sys_run_startup_scripts.exclusive_system()),
        );

        let sim_schedule = Schedule::default()
            .with_stage(SimStage::Main, SystemStage::parallel())
            .with_stage(
                SimStage::RunScript,
                SystemStage::single_threaded()
                    .with_system(ScriptHerder::sys_run_sim_scripts.exclusive_system()),
            );

        let frame_schedule =
            Schedule::default().with_stage(FrameStage::Main, SystemStage::parallel());

        let shutdown_schedule =
            Schedule::default().with_stage(ShutdownStage::Cleanup, SystemStage::single_threaded());

        let mut runtime = Self {
            heap: Heap::default(),
            startup_schedule,
            sim_schedule,
            frame_schedule,
            shutdown_schedule,
            dump_schedules: false,
        };

        runtime
            .insert_resource(ExitRequest::Continue)
            .insert_resource(ScriptHerder::default())
            .insert_resource(ScriptCompletions::new())
            .insert_resource(ScriptQueue::default())
            .insert_resource(TaskPool::default())
            .insert_named_resource("runtime", RuntimeResource::default());

        runtime
    }
}

impl Runtime {
    #[inline]
    pub fn run_sim_once(&mut self) {
        self.sim_schedule.run_once(self.heap.world_mut());
    }

    #[inline]
    pub fn run_frame_once(&mut self) {
        self.frame_schedule.run_once(self.heap.world_mut());
    }

    #[inline]
    pub fn run_startup(&mut self) {
        self.startup_schedule.run_once(self.heap.world_mut());

        if self.dump_schedules {
            self.dump_schedules = false;
            self.dump_startup_schedule();
            self.dump_sim_schedule();
            self.dump_frame_schedule();
            self.dump_shutdown_schedule();
        }
    }

    #[inline]
    pub fn run_shutdown(&mut self) {
        self.shutdown_schedule.run_once(self.heap.world_mut());
    }

    #[inline]
    pub fn set_dump_schedules_on_startup(&mut self) {
        self.dump_schedules = true;
    }

    #[inline]
    pub fn dump_startup_schedule(&self) {
        dump_schedule(
            self.heap.world(),
            &self.startup_schedule,
            &PathBuf::from("startup_schedule.dot"),
        );
    }

    #[inline]
    pub fn dump_sim_schedule(&self) {
        dump_schedule(
            self.heap.world(),
            &self.sim_schedule,
            &PathBuf::from("sim_schedule.dot"),
        );
    }

    #[inline]
    pub fn dump_frame_schedule(&self) {
        dump_schedule(
            self.heap.world(),
            &self.frame_schedule,
            &PathBuf::from("frame_schedule.dot"),
        );
    }

    #[inline]
    pub fn dump_shutdown_schedule(&self) {
        dump_schedule(
            self.heap.world(),
            &self.shutdown_schedule,
            &PathBuf::from("shutdown_schedule.dot"),
        );
    }

    #[inline]
    pub fn load_extension<T: Extension>(&mut self) -> Result<&mut Self> {
        T::init(self)?;
        Ok(self)
    }

    #[inline]
    pub fn with_extension<T: Extension>(mut self) -> Result<Self> {
        T::init(&mut self)?;
        Ok(self)
    }

    #[inline]
    pub fn sim_stage_mut(&mut self, sim_stage: SimStage) -> &mut SystemStage {
        self.sim_schedule.get_stage_mut(&sim_stage).unwrap()
    }

    pub fn add_sim_system<Params>(
        &mut self,
        system: impl IntoSystemDescriptor<Params>,
    ) -> &mut Self {
        self.sim_schedule
            .get_stage_mut::<SystemStage>(&SimStage::Main)
            .unwrap()
            .add_system(system);
        self
    }

    #[inline]
    pub fn frame_stage_mut(&mut self, frame_stage: FrameStage) -> &mut SystemStage {
        self.frame_schedule.get_stage_mut(&frame_stage).unwrap()
    }

    pub fn add_frame_system<Params>(
        &mut self,
        system: impl IntoSystemDescriptor<Params>,
    ) -> &mut Self {
        self.frame_schedule
            .get_stage_mut::<SystemStage>(&FrameStage::Main)
            .unwrap()
            .add_system(system);
        self
    }

    #[inline]
    pub fn startup_stage_mut(&mut self, startup_stage: StartupStage) -> &mut SystemStage {
        self.startup_schedule.get_stage_mut(&startup_stage).unwrap()
    }

    pub fn add_startup_system<Params>(
        &mut self,
        system: impl IntoSystemDescriptor<Params>,
    ) -> &mut Self {
        self.startup_schedule
            .get_stage_mut::<SystemStage>(&StartupStage::Main)
            .unwrap()
            .add_system(system);
        self
    }

    #[inline]
    pub fn shutdown_stage_mut(&mut self, shutdown_stage: ShutdownStage) -> &mut SystemStage {
        self.shutdown_schedule
            .get_stage_mut(&shutdown_stage)
            .unwrap()
    }

    // Script passthrough

    #[inline]
    pub fn run_interactive(&mut self, script_text: &str) -> Result<()> {
        self.resource_mut::<ScriptHerder>()
            .run_interactive(script_text)
    }

    #[inline]
    pub fn run_string(&mut self, script_text: &str) -> Result<()> {
        self.resource_mut::<ScriptHerder>().run_string(script_text)
    }

    #[inline]
    pub fn run<N: Into<NitrousScript>>(&mut self, script: N) {
        self.resource_mut::<ScriptHerder>()
            .run(script, ScriptRunKind::Precompiled)
    }

    #[inline]
    pub fn run_with_locals<N: Into<NitrousScript>>(&mut self, locals: LocalNamespace, script: N) {
        self.resource_mut::<ScriptHerder>().run_with_locals(
            locals,
            script,
            ScriptRunKind::Precompiled,
        )
    }

    // Heap passthrough

    #[inline]
    pub fn spawn(&mut self) -> EntityMut {
        self.heap.spawn()
    }

    #[inline]
    pub fn spawn_named<S>(&mut self, name: S) -> Result<NamedEntityMut>
    where
        S: Into<String>,
    {
        self.heap.spawn_named(name)
    }

    #[inline]
    pub fn get<T: Component + 'static>(&self, entity: Entity) -> &T {
        self.heap.get::<T>(entity)
    }

    #[inline]
    pub fn get_mut<T: Component + 'static>(&mut self, entity: Entity) -> Mut<T> {
        self.heap.get_mut::<T>(entity)
    }

    #[inline]
    pub fn insert_named_resource<S, T>(&mut self, name: S, value: T) -> &mut Self
    where
        S: Into<String>,
        T: Resource + ScriptResource + 'static,
    {
        self.heap.insert_named_resource(name, value);
        self
    }

    #[inline]
    pub fn insert_resource<T: Resource>(&mut self, value: T) -> &mut Self {
        self.heap.insert_resource(value);
        self
    }

    #[inline]
    pub fn insert_non_send<T: 'static>(&mut self, value: T) -> &mut Self {
        self.heap.insert_non_send(value);
        self
    }

    #[inline]
    pub fn maybe_resource<T: Resource>(&self) -> Option<&T> {
        self.heap.maybe_resource()
    }

    #[inline]
    pub fn resource<T: Resource>(&self) -> &T {
        self.heap.resource::<T>()
    }

    #[inline]
    pub fn resource_mut<T: Resource>(&mut self) -> Mut<T> {
        self.heap.resource_mut::<T>()
    }

    #[inline]
    pub fn resource_by_name(&mut self, name: &str) -> &dyn ScriptResource {
        self.heap.resource_by_name(name)
    }

    #[inline]
    pub fn remove_resource<T: Resource>(&mut self) -> Option<T> {
        self.heap.remove_resource::<T>()
    }

    #[inline]
    pub fn resource_names(&self) -> impl Iterator<Item = &str> {
        self.heap.resource_names()
    }

    #[inline]
    pub fn resource_scope<T: Resource, U>(&mut self, f: impl FnOnce(HeapMut, Mut<T>) -> U) -> U {
        self.heap.resource_scope(f)
    }

    #[inline]
    pub fn query<Q>(&mut self) -> QueryState<Q, ()>
    where
        Q: WorldQuery,
    {
        self.heap.query::<Q>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let _ = Runtime::default();
    }
}
