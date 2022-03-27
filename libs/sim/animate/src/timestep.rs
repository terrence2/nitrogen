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
use nitrous::{inject_nitrous_resource, NitrousResource};
use runtime::{Extension, Runtime};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum TimeStepStep {
    Tick,
}

#[derive(Debug, NitrousResource)]
pub struct TimeStep {
    start: Instant,
    now: Instant,
    delta: Duration,
}

impl Extension for TimeStep {
    fn init(runtime: &mut Runtime) -> Result<()> {
        runtime.insert_named_resource("time", TimeStep::new_60fps());
        runtime.add_sim_system(Self::sys_tick_time.label(TimeStepStep::Tick));
        Ok(())
    }
}

#[inject_nitrous_resource]
impl TimeStep {
    pub fn new_60fps() -> Self {
        let delta = Duration::from_micros(1_000_000 / 60);
        let start = Instant::now();
        Self {
            start,
            // Note: start one tick behind now so that the sim schedule will always
            //       run at least once before the frame scheduler.
            now: start - delta,
            delta,
        }
    }

    pub fn sys_tick_time(mut timestep: ResMut<TimeStep>) {
        let dt = timestep.delta;
        timestep.now += dt;
    }

    pub fn start(&self) -> &Instant {
        &self.start
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
