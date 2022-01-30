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
use nitrous_injector::{inject_nitrous_resource, NitrousResource};
use runtime::{Extension, Runtime, SimStage};
use std::time::{Duration, Instant};

#[derive(Debug, NitrousResource)]
pub struct TimeStep {
    now: Instant,
    delta: Duration,
}

impl Extension for TimeStep {
    fn init(runtime: &mut Runtime) -> Result<()> {
        runtime.insert_named_resource("time", TimeStep::new_60fps());
        runtime
            .sim_stage_mut(SimStage::TimeStep)
            .add_system(Self::sys_tick_time);
        Ok(())
    }
}

#[inject_nitrous_resource]
impl TimeStep {
    pub fn new_60fps() -> Self {
        Self {
            now: Instant::now(),
            delta: Duration::from_micros(16_666),
        }
    }

    pub fn sys_tick_time(mut timestep: ResMut<TimeStep>) {
        let dt = timestep.delta;
        timestep.now += dt;
    }

    pub fn now(&self) -> &Instant {
        &self.now
    }

    pub fn step(&self) -> &Duration {
        &self.delta
    }

    pub fn next_now(&self) -> Instant {
        self.now + self.delta
    }
}
