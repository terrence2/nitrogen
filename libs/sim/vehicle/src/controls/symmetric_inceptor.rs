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
use crate::controls::inceptor_position_tick;
use animate::TimeStep;
use anyhow::Result;
use bevy_ecs::prelude::*;
use nitrous::{inject_nitrous_component, method, NitrousComponent};
use runtime::{Extension, Runtime};

// Self-centering, 0 centered, symmetrical controls.
macro_rules! make_sym {
    ($cls:ident, $name:expr, $up:ident, $down:ident) => {
        #[derive(Component, NitrousComponent, Debug, Copy, Clone)]
        #[Name = $name]
        pub struct $cls {
            position: f64,        // [-1, 1]
            key_move_target: f64, // target of move, depending on what key is held
            #[property]
            key_sensitivity: f64,
        }

        impl Extension for $cls {
            fn init(runtime: &mut Runtime) -> Result<()> {
                runtime.add_sim_system(Self::sys_tick);
                Ok(())
            }
        }

        impl Default for $cls {
            fn default() -> Self {
                Self {
                    position: 0_f64,
                    key_move_target: 0_f64,
                    key_sensitivity: 2_f64,
                }
            }
        }

        #[inject_nitrous_component]
        impl $cls {
            #[method]
            pub fn $up(&mut self, pressed: bool) {
                self.key_move_target = if pressed { 1. } else { 0. };
            }

            #[method]
            pub fn $down(&mut self, pressed: bool) {
                self.key_move_target = if pressed { -1. } else { 0. };
            }

            #[method]
            pub fn position(&self) -> f64 {
                self.position as f64
            }

            fn sys_tick(timestep: Res<TimeStep>, mut query: Query<&mut $cls>) {
                for mut inceptor in query.iter_mut() {
                    inceptor.position = inceptor_position_tick(
                        inceptor.key_move_target,
                        inceptor.key_sensitivity * timestep.step().as_secs_f64(),
                        inceptor.position,
                    );
                }
            }
        }
    };
}

make_sym!(PitchInceptor, "stick_pitch", key_move_back, key_move_front);
make_sym!(RollInceptor, "stick_roll", key_move_right, key_move_left);
make_sym!(YawInceptor, "pedals_yaw", key_move_right, key_move_left);
