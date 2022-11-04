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
use crate::systems::engine::{Engine, EnginePower};
use crate::ThrottlePosition;
use absolute_unit::{
    kilograms, newtons, Force, Kilograms, Mass, Meters, Newtons, Seconds, Velocity,
};
use physical_constants::StandardAtmosphere;
use std::time::Duration;

// A glider engine. Produces zero thrust, consumes zero fuel, implements
// the Engine trait so that all planes can have a power-plant, simplifying
// code elsewhere.
#[derive(Default)]
pub struct GliderEngine {
    power: EnginePower,
}

impl Engine for GliderEngine {
    fn adjust_power(&mut self, throttle: &ThrottlePosition, _dt: &Duration) {
        self.power = match throttle {
            ThrottlePosition::Military(v) => EnginePower::Military(*v),
            ThrottlePosition::Afterburner(v) => EnginePower::Afterburner(*v),
        };
    }

    fn current_power(&self) -> &EnginePower {
        &self.power
    }

    fn compute_thrust(
        &self,
        _atmosphere: &StandardAtmosphere,
        _velocity: Velocity<Meters, Seconds>,
    ) -> Force<Newtons> {
        newtons!(0)
    }

    fn compute_fuel_use(&self, _dt: &Duration) -> Mass<Kilograms> {
        kilograms!(0)
    }

    fn set_out_of_fuel(&mut self) {}
}
