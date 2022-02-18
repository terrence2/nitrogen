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
    herder::{ExitRequest, ScriptCompletions, ScriptHerder, ScriptRunKind},
};
use anyhow::Result;
use bevy_ecs::{prelude::*, system::Resource, world::EntityMut};
use nitrous::{Heap, LocalNamespace, NamedEntityMut, NitrousScript, ScriptResource};
use std::path::PathBuf;

/// Interface for extending the Runtime.
pub trait Extension {
    fn init(runtime: &mut Runtime) -> Result<()>;
}

/// Systems may be scheduled to run at startup.
/// The startup scripts run in RunScript.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, StageLabel)]
pub enum StartupStage {
    PreScript,
    RunScript,
    PostScript,
}

/// The simulation schedule should be used for "pure" entity to entity work and update of
/// a handful of game related resources, rather than communicating with the GPU.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, StageLabel)]
pub enum SimStage {
    /// Apply delta-t from TimeStep to whatever systems want to track time.
    TimeStep,
    /// Consume any input that has accumulated.
    ReadInput,
    /// Run anything that should run with last frame's inputs. TODO: necessary?
    PreInput,
    /// Use the input event vec in any systems that need to.
    HandleInput,
    /// Runs after input is processed, with new values.
    PostInput,
    /// Runs before the serial scripting phase.
    PreScript,
    /// Fully serial phase where scripts run.
    RunScript,
    /// Runs after scripts have processed.
    PostScript,
}

// Copy from entities into buffers more suitable for upload to the GPU. Also, do heavier
// CPU-side graphics work that can be parallelized efficiently, like updating the terrain
// from the current cameras. Not generally for actually writing to the GPU.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, StageLabel)]
pub enum FrameStage {
    /// Run anything that should run with last frame system state.
    PreInput,
    /// Read values from the system event queue.
    ReadSystem,
    /// Do anything needed with system events this frame.
    HandleSystem,
    /// Respond to system events, like display config changes.
    HandleDisplayChange,
    /// Do anything needed that uses the after-change settings.
    PostSystem,
    /// Transfer entity state into CPU-side GPU transfer buffers.
    TrackStateChanges,
    /// Push non-render uploads into the frame update queue.
    EnsureGpuUpdated,
    /// Create the target surface.
    CreateTargetSurface,
    /// Create the frame's encoder.
    CreateCommandEncoder,
    /// Encode any uploads we queued up.
    DispatchUploads,
    /// Everything that needs to use the command encoder and target surface.
    Render,
    /// Finish and submit commands to the GPU.
    SubmitCommands,
    /// Present our target surface.
    PresentTargetSurface,
    /// Recreate display if out-of-date.
    HandleOutOfDateRenderer,
    /// Right before frame end.
    FrameEnd,
}

pub struct Runtime {
    heap: Heap,
    startup_schedule: Schedule,
    sim_schedule: Schedule,
    frame_schedule: Schedule,
    dump_schedules: bool,
}

impl Default for Runtime {
    fn default() -> Self {
        let startup_schedule = Schedule::default()
            .with_stage(StartupStage::PreScript, SystemStage::parallel())
            .with_stage(
                StartupStage::RunScript,
                SystemStage::single_threaded()
                    .with_system(ScriptHerder::sys_run_startup_scripts.exclusive_system()),
            )
            .with_stage(StartupStage::PostScript, SystemStage::parallel());

        let sim_schedule = Schedule::default()
            .with_stage(SimStage::TimeStep, SystemStage::parallel())
            .with_stage(SimStage::PreInput, SystemStage::parallel())
            .with_stage(SimStage::ReadInput, SystemStage::parallel())
            .with_stage(SimStage::HandleInput, SystemStage::parallel())
            .with_stage(SimStage::PostInput, SystemStage::parallel())
            .with_stage(SimStage::PreScript, SystemStage::parallel())
            .with_stage(
                SimStage::RunScript,
                SystemStage::single_threaded()
                    .with_system(ScriptHerder::sys_run_sim_scripts.exclusive_system()),
            )
            .with_stage(SimStage::PostScript, SystemStage::parallel());

        use SystemStage as SS;
        let frame_schedule = Schedule::default()
            .with_stage(FrameStage::PreInput, SS::parallel())
            .with_stage(FrameStage::ReadSystem, SS::parallel())
            .with_stage(FrameStage::HandleSystem, SS::parallel())
            .with_stage(FrameStage::HandleDisplayChange, SS::parallel())
            .with_stage(FrameStage::PostSystem, SS::parallel())
            .with_stage(FrameStage::TrackStateChanges, SS::parallel())
            .with_stage(FrameStage::EnsureGpuUpdated, SS::parallel())
            .with_stage(FrameStage::CreateTargetSurface, SS::single_threaded())
            .with_stage(FrameStage::HandleOutOfDateRenderer, SS::single_threaded())
            .with_stage(FrameStage::CreateCommandEncoder, SS::single_threaded())
            .with_stage(FrameStage::DispatchUploads, SS::single_threaded())
            .with_stage(FrameStage::Render, SS::parallel())
            .with_stage(FrameStage::SubmitCommands, SS::single_threaded())
            .with_stage(FrameStage::PresentTargetSurface, SS::single_threaded())
            .with_stage(FrameStage::FrameEnd, SS::parallel());

        let mut runtime = Self {
            heap: Heap::default(),
            startup_schedule,
            sim_schedule,
            frame_schedule,
            dump_schedules: false,
        };

        runtime
            .insert_resource(ExitRequest::Continue)
            .insert_resource(ScriptHerder::default())
            .insert_resource(ScriptCompletions::new());

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
        }
    }

    #[inline]
    pub fn set_dump_schedules_on_startup(&mut self) {
        self.dump_schedules = true;
    }

    #[inline]
    pub fn dump_startup_schedule(&self) {
        dump_schedule(
            &self.heap.world(),
            &self.startup_schedule,
            &PathBuf::from("startup_schedule.dot"),
        );
    }

    #[inline]
    pub fn dump_sim_schedule(&self) {
        dump_schedule(
            &self.heap.world(),
            &self.sim_schedule,
            &PathBuf::from("sim_schedule.dot"),
        );
    }

    #[inline]
    pub fn dump_frame_schedule(&self) {
        dump_schedule(
            &self.heap.world(),
            &self.frame_schedule,
            &PathBuf::from("frame_schedule.dot"),
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

    #[inline]
    pub fn frame_stage_mut(&mut self, frame_stage: FrameStage) -> &mut SystemStage {
        self.frame_schedule.get_stage_mut(&frame_stage).unwrap()
    }

    #[inline]
    pub fn startup_stage_mut(&mut self, startup_stage: StartupStage) -> &mut SystemStage {
        self.startup_schedule.get_stage_mut(&startup_stage).unwrap()
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
    pub fn resource_scope<T: Resource, U>(&mut self, f: impl FnOnce(&mut World, Mut<T>) -> U) -> U {
        self.heap.resource_scope(f)
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
