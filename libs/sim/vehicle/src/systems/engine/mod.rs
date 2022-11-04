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
pub(crate) mod glider;

use crate::ThrottlePosition;
use absolute_unit::{Force, Kilograms, Mass, Meters, Newtons, Seconds, Velocity};
use physical_constants::StandardAtmosphere;
use std::{fmt, fmt::Display, time::Duration};

#[derive(Debug, Copy, Clone)]
pub enum EnginePower {
    Military(f64),
    Afterburner(Option<i64>),
    FlameOut,
    OutOfFuel,
}

impl EnginePower {
    pub fn military(&self) -> f64 {
        match self {
            Self::Military(m) => *m,
            Self::Afterburner(_) => 101.,
            _ => 0.,
        }
    }

    pub fn is_afterburner(&self) -> bool {
        matches!(self, Self::Afterburner(_))
    }

    pub fn increase(&mut self, delta: f64, max: &ThrottlePosition) {
        if let Self::Military(current) = *self {
            let next = (current + delta).min(max.military());
            *self = if next >= 100. && max.is_afterburner() {
                // TODO: return a new afterburner state so we can play sound?
                Self::Afterburner(None)
            } else {
                Self::Military(next)
            };
        }
    }

    pub fn decrease(&mut self, delta: f64, min: &ThrottlePosition) {
        if self.is_afterburner() {
            *self = Self::Military(100.);
        }
        if let Self::Military(current) = self {
            let next = (*current - delta).max(min.military());
            *self = Self::Military(next);
        }
    }
}

impl Default for EnginePower {
    fn default() -> Self {
        EnginePower::Military(0.)
    }
}

impl Display for EnginePower {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Afterburner(None) => "AFT".to_owned(),
            Self::Afterburner(Some(i)) => format!("AFT{}", i),
            Self::Military(m) => format!("{:0.0}%", m),
            Self::OutOfFuel => "OOF".to_owned(),
            Self::FlameOut => "FO".to_owned(),
        };
        write!(f, "{}", s)
    }
}

pub trait Engine: Send + Sync + 'static {
    fn adjust_power(&mut self, throttle: &ThrottlePosition, dt: &Duration);
    fn current_power(&self) -> &EnginePower;
    fn compute_thrust(
        &self,
        atmosphere: &StandardAtmosphere,
        velocity: Velocity<Meters, Seconds>,
    ) -> Force<Newtons>;
    fn compute_fuel_use(&self, dt: &Duration) -> Mass<Kilograms>;
    fn set_out_of_fuel(&mut self);
}
