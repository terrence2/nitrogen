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
use nitrous::{inject_nitrous_resource, method, NitrousResource};
use runtime::{Extension, Runtime};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum TimeStepStep {
    Tick,
}

#[derive(Debug, NitrousResource)]
pub struct TimeStep {
    sim_start_time: Instant,
    real_time: Instant,
    sim_time: Instant,
    next_sim_time: Instant,
    sim_step: Duration,

    time_compression: u32,
}

impl Extension for TimeStep {
    fn init(runtime: &mut Runtime) -> Result<()> {
        runtime.insert_named_resource("time", TimeStep::new_60fps());
        runtime.add_input_system(Self::sys_tick_time.label(TimeStepStep::Tick));
        Ok(())
    }
}

#[inject_nitrous_resource]
impl TimeStep {
    pub fn new_60fps() -> Self {
        let delta = Duration::from_micros(1_000_000 / 60);
        let start = Instant::now();
        Self {
            sim_start_time: start,
            // Note: start one tick behind now so that the sim schedule will always
            //       run at least once before the frame scheduler.
            sim_time: start,
            next_sim_time: start + delta,
            real_time: start,
            sim_step: delta,
            time_compression: 1,
        }
    }

    #[method]
    pub fn time_compression(&self) -> i64 {
        self.time_compression as i64
    }

    #[method]
    pub fn set_time_compression(&mut self, time_compression: i64) -> Result<()> {
        self.time_compression = u32::try_from(time_compression)?;
        Ok(())
    }

    #[method]
    pub fn next_time_compression(&mut self) {
        if self.time_compression >= 8 {
            self.time_compression = 1;
        } else if self.time_compression >= 4 {
            self.time_compression = 8;
        } else if self.time_compression >= 2 {
            self.time_compression = 4;
        } else if self.time_compression >= 1 {
            self.time_compression = 2;
        } else {
            self.time_compression = 1;
        }
    }

    /// Call once per frame to keep the sim updated.
    pub fn run_sim_loop(runtime: &mut Runtime) {
        // Find the amount of time elapsed in the sim, as the amount of real time elapsed
        // since the last call, times the compression.
        let now = Instant::now();
        {
            let mut ts = runtime.resource_mut::<TimeStep>();
            let real_elapsed = now - ts.real_time;
            let time_compression = ts.time_compression;
            ts.next_sim_time += real_elapsed * time_compression;
            ts.real_time = now;
        }
        while runtime.resource::<TimeStep>().need_step() {
            runtime.run_sim_once();
        }
    }

    pub fn need_step(&self) -> bool {
        self.sim_time + self.sim_step < self.next_sim_time
    }

    pub fn sys_tick_time(mut timestep: ResMut<TimeStep>) {
        let dt = timestep.sim_step;
        timestep.sim_time += dt;
    }

    pub fn sim_start_time(&self) -> &Instant {
        &self.sim_start_time
    }

    pub fn sim_time(&self) -> &Instant {
        &self.sim_time
    }

    pub fn step(&self) -> &Duration {
        &self.sim_step
    }
}
