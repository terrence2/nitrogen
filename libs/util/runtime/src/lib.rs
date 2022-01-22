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
use nitrous::Module;
use std::{any::TypeId, collections::HashMap};

pub trait Extension {
    fn init(runtime: &mut Runtime) -> Result<()>;
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, StageLabel)]
pub enum SimStage {
    TimeStep,
    PreInput,
    ReadInput,
    HandleInput,
    PostInput,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, StageLabel)]
pub enum FrameStage {
    PreInput,
    ReadSystem,
    HandleSystem,
    HandleDisplayChange,
    PostSystem,
    SimStateChange,
    EnsureUpdate,
    Render,
}

pub struct Runtime {
    modules: HashMap<String, TypeId>,
    pub world: World,
    sim_schedule: Schedule,
    frame_schedule: Schedule,
}

impl Default for Runtime {
    fn default() -> Self {
        let sim_schedule = Schedule::default()
            .with_stage(SimStage::TimeStep, SystemStage::parallel())
            .with_stage(SimStage::PreInput, SystemStage::parallel())
            .with_stage(SimStage::ReadInput, SystemStage::parallel())
            .with_stage(SimStage::HandleInput, SystemStage::parallel())
            .with_stage(SimStage::PostInput, SystemStage::parallel());

        let frame_schedule = Schedule::default()
            .with_stage(FrameStage::PreInput, SystemStage::parallel())
            .with_stage(FrameStage::ReadSystem, SystemStage::parallel())
            .with_stage(FrameStage::HandleSystem, SystemStage::parallel())
            .with_stage(FrameStage::HandleDisplayChange, SystemStage::parallel())
            .with_stage(FrameStage::PostSystem, SystemStage::parallel())
            .with_stage(FrameStage::SimStateChange, SystemStage::parallel())
            .with_stage(FrameStage::EnsureUpdate, SystemStage::parallel())
            .with_stage(FrameStage::Render, SystemStage::parallel());

        Self {
            modules: HashMap::new(),
            world: World::default(),
            sim_schedule,
            frame_schedule,
        }
    }
}

impl Runtime {
    pub fn sim_stage_mut(&mut self, sim_stage: SimStage) -> &mut SystemStage {
        self.sim_schedule.get_stage_mut(&sim_stage).unwrap()
    }

    pub fn frame_stage_mut(&mut self, frame_stage: FrameStage) -> &mut SystemStage {
        self.frame_schedule.get_stage_mut(&frame_stage).unwrap()
    }

    pub fn load_extension<T: Extension>(&mut self) -> Result<&mut Self> {
        T::init(self)?;
        Ok(self)
    }

    pub fn insert_module<S: Into<String>, T: Resource + Module>(&mut self, name: S, value: T) {
        self.modules.insert(name.into(), TypeId::of::<T>());
        self.world.insert_resource(value);
    }

    #[inline]
    pub fn insert_resource<T: Resource>(&mut self, value: T) -> &mut Self {
        self.world.insert_resource(value);
        self
    }

    #[inline]
    pub fn get_resource<T: Resource>(&self) -> Option<&T> {
        self.world.get_resource()
    }

    #[inline]
    pub fn resource<T: Resource>(&self) -> &T {
        self.world.get_resource().expect("unset resource")
    }

    #[inline]
    pub fn resource_mut<T: Resource>(&mut self) -> Mut<T> {
        self.world.get_resource_mut().expect("unset resource")
    }

    #[inline]
    pub fn remove_resource<T: Resource>(&mut self) -> Option<T> {
        self.world.remove_resource()
    }

    pub fn run_sim_once(&mut self) {
        self.sim_schedule.run_once(&mut self.world);
    }

    pub fn run_frame_once(&mut self) {
        self.frame_schedule.run_once(&mut self.world);
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
