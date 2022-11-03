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
use crate::{ConsumeResult, Engine, FuelSystem, ThrottleInceptor};
use absolute_unit::{kilograms, newtons, Force, Meters, Newtons, Seconds, Velocity};
use animate::TimeStep;
use anyhow::Result;
use bevy_ecs::prelude::*;
use nitrous::{inject_nitrous_component, method, NitrousComponent};
use physical_constants::StandardAtmosphere;
use runtime::{Extension, Runtime};

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum PowerSystemStep {
    ThrottleEngines,
    ConsumeFuel,
}

#[derive(Component, NitrousComponent, Default)]
#[Name = "power"]
pub struct PowerSystem {
    engines: Vec<Box<dyn Engine>>,
}

impl Extension for PowerSystem {
    fn init(runtime: &mut Runtime) -> Result<()> {
        runtime.add_sim_system(Self::sys_throttle_engines.label(PowerSystemStep::ThrottleEngines));
        runtime.add_sim_system(Self::sys_consume_fuel.label(PowerSystemStep::ConsumeFuel));
        Ok(())
    }
}

#[inject_nitrous_component]
impl PowerSystem {
    pub fn with_engine<T: Engine>(mut self, engine: T) -> Self {
        self.engines.push(Box::new(engine));
        self
    }

    #[method]
    pub fn is_afterburner(&self) -> bool {
        for engine in &self.engines {
            if engine.current_power().is_afterburner() {
                return true;
            }
        }
        false
    }

    pub fn engine(&self, number: usize) -> &(dyn Engine + 'static) {
        self.engines[number].as_ref()
    }

    pub fn current_thrust(
        &self,
        atmosphere: &StandardAtmosphere,
        velocity: Velocity<Meters, Seconds>,
    ) -> Force<Newtons> {
        let mut total = newtons!(0f64);
        for engine in &self.engines {
            total += engine.compute_thrust(atmosphere, velocity);
        }
        total
    }

    fn sys_consume_fuel(
        timestep: Res<TimeStep>,
        mut query: Query<(&mut PowerSystem, &mut FuelSystem)>,
    ) {
        for (mut power, mut fuel) in query.iter_mut() {
            // Compute fuel use up front so that we can flame out all engines,
            // rather than staggering them out.
            let mut required_fuel = kilograms!(0f64);
            for engine in &power.engines {
                required_fuel += engine.compute_fuel_use(timestep.step());
            }
            let result = fuel.consume_fuel(required_fuel);
            if result == ConsumeResult::OutOfFuel {
                for engine in &mut power.engines {
                    engine.set_out_of_fuel();
                }
            }
        }
    }

    fn sys_throttle_engines(
        timestep: Res<TimeStep>,
        mut query: Query<(&ThrottleInceptor, &mut PowerSystem)>,
    ) {
        for (throttle, mut power) in query.iter_mut() {
            // FIXME: do not assume that throttles are ganged
            for engine in &mut power.engines {
                // FIXME: need to find operational ceiling
                engine.adjust_power(throttle.position(), timestep.step());
            }
        }
    }
}
