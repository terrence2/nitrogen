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

// Controls that move instantly to some position and are left where positioned.
// Range [0,1]
macro_rules! make_abs {
    ($cls:ident, $name:expr) => {
        #[derive(Component, NitrousComponent, Debug, Default, Copy, Clone)]
        #[Name = $name]
        pub struct $cls {
            position: f64, // [0, 1]
        }

        #[inject_nitrous_component]
        impl $cls {
            #[method]
            pub fn toggle(&mut self) {
                self.position = if self.position > 0. { 0. } else { 1. };
            }

            #[method]
            pub fn position(&self) -> f64 {
                self.position
            }

            #[method]
            pub fn set_position(&mut self, v: f64) {
                self.position = v;
            }
        }
    };
}

make_abs!(AirbrakeControl, "airbrake");
make_abs!(FlapsControl, "flaps");
