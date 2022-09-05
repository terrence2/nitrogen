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
use bevy_ecs::prelude::*;
use nitrous::{inject_nitrous_component, method, NitrousComponent};

#[derive(Debug, Copy, Clone)]
pub enum ThrottlePosition {
    Military(f64),
    Afterburner(Option<i64>),
}

impl ThrottlePosition {
    pub fn military(&self) -> f64 {
        match self {
            Self::Military(m) => *m,
            Self::Afterburner(_) => 101.,
        }
    }

    pub fn is_afterburner(&self) -> bool {
        matches!(self, Self::Afterburner(_))
    }
}

impl ToString for ThrottlePosition {
    fn to_string(&self) -> String {
        match self {
            Self::Afterburner(None) => "AFT".to_owned(),
            Self::Afterburner(Some(i)) => format!("AFT{}", i),
            Self::Military(m) => format!("{:0.0}%", m),
        }
    }
}

// Moving the lever to a new position is assumed to take zero time.
#[derive(Component, NitrousComponent, Debug, Copy, Clone)]
#[Name = "throttle"]
pub struct ThrottleInceptor {
    position: ThrottlePosition,
}

#[inject_nitrous_component]
impl ThrottleInceptor {
    pub fn new_min_power() -> Self {
        Self {
            position: ThrottlePosition::Military(0.),
        }
    }

    #[method]
    pub fn throttle_display(&self) -> String {
        self.position.to_string()
    }

    pub fn position(&self) -> &ThrottlePosition {
        &self.position
    }

    #[method]
    fn set_military(&mut self, percent: f64) {
        self.position = ThrottlePosition::Military(percent);
    }

    #[method]
    fn set_afterburner(&mut self) {
        self.position = ThrottlePosition::Afterburner(None);
    }

    #[method]
    fn set_afterburner_level(&mut self, level: i64) {
        self.position = ThrottlePosition::Afterburner(Some(level));
    }
}
