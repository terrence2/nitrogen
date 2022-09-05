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
use crate::{ThrottleInceptor, ThrottlePosition};
use absolute_unit::{scalar, Force, Newtons};
use animate::TimeStep;
use anyhow::Result;
use bevy_ecs::prelude::*;
use nitrous::{inject_nitrous_component, NitrousComponent};
use runtime::{Extension, Runtime};
use std::fmt::Formatter;
use std::{fmt, fmt::Display, time::Duration};

#[derive(Debug, Copy, Clone)]
pub enum EnginePower {
    Military(f64),
    Afterburner(Option<i64>),
}

impl EnginePower {
    pub fn military(&self) -> f64 {
        match self {
            Self::Military(m) => *m,
            Self::Afterburner(_) => 101.,
        }
    }

    pub fn is_afterburner(&self) -> bool {
        matches!(self, Self::Afterburner(_))
    }

    fn increase(&mut self, delta: f64, max: &ThrottlePosition) {
        match self {
            Self::Military(current) => {
                let next = (*current + delta).min(max.military());
                *self = if next >= 100. && max.is_afterburner() {
                    // TODO: return a new afterburner state so we can play sound?
                    Self::Afterburner(None)
                } else {
                    Self::Military(next)
                };
            }
            Self::Afterburner(_) => {}
        }
    }

    fn decrease(&mut self, delta: f64, min: &ThrottlePosition) {
        if self.is_afterburner() {
            *self = Self::Military(100.);
        }
        if let Self::Military(current) = self {
            let next = (*current - delta).max(min.military());
            *self = Self::Military(next);
        }
    }
}

impl Display for EnginePower {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Afterburner(None) => "AFT".to_owned(),
            Self::Afterburner(Some(i)) => format!("AFT{}", i),
            Self::Military(m) => format!("{:0.0}%", m),
        };
        write!(f, "{}", s)
    }
}

/// A jet engine modeled on a base thrust and various decay and limit factors.
#[derive(Component, NitrousComponent, Debug, Copy, Clone)]
#[Name = "engine"]
pub struct SimpleJetEngine {
    // Current engine setting as a percentage.
    power: EnginePower,

    /// Base thrust at sea level and zero speed at 100% military power.
    base_thrust: Force<Newtons>,

    /// Base thrust at sea level and zero speed at afterburner power.
    base_ab_thrust: Force<Newtons>,

    /// Rate at which the engine should respond to throttle increases as percentage per second.
    throttle_up_rate: f64,

    /// Rate at which the engine should respond to throttle decreases as percentage per second.
    throttle_down_rate: f64,
}

impl Extension for SimpleJetEngine {
    fn init(runtime: &mut Runtime) -> Result<()> {
        runtime.add_sim_system(Self::sys_tick);
        Ok(())
    }
}

#[inject_nitrous_component]
impl SimpleJetEngine {
    pub fn new_min_power(
        base_thrust: Force<Newtons>,
        base_ab_thrust: Force<Newtons>,
        throttle_up_rate: f64,
        throttle_down_rate: f64,
    ) -> Self {
        Self {
            power: EnginePower::Military(0.),
            base_thrust,
            base_ab_thrust,
            throttle_up_rate,
            throttle_down_rate,
        }
    }

    fn sys_tick(
        timestep: Res<TimeStep>,
        mut query: Query<(&ThrottleInceptor, &mut SimpleJetEngine)>,
    ) {
        for (throttle, mut engine) in query.iter_mut() {
            engine.throttle_chase(throttle.position(), timestep.step());
        }
    }

    fn throttle_chase(&mut self, throttle: &ThrottlePosition, dt: &Duration) {
        if self.power.military() < throttle.military() {
            self.power
                .increase(self.throttle_up_rate * dt.as_secs_f64(), throttle);
        }
        if self.power.military() > throttle.military() {
            self.power
                .decrease(self.throttle_down_rate * dt.as_secs_f64(), throttle);
        }
    }

    pub fn power(&self) -> &EnginePower {
        &self.power
    }

    pub fn current_thrust(&self) -> Force<Newtons> {
        if self.power.is_afterburner() {
            self.base_ab_thrust
        } else {
            self.base_thrust * scalar!(self.power.military())
        }
    }
}
