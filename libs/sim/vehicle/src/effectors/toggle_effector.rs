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
use crate::{
    effectors::effector_chase, AirbrakeControl, BayControl, FlapsControl, GearControl, HookControl,
};
use animate::TimeStep;
use anyhow::Result;
use bevy_ecs::prelude::*;
use nitrous::{inject_nitrous_component, NitrousComponent};
use runtime::{Extension, Runtime};
use std::time::Duration;

macro_rules! make_toggle_chase {
    ($cls:ident, $name:expr, $control:ident) => {
        /// $cls are a simple effector that chases the control position with some velocity.
        #[derive(Component, NitrousComponent, Debug, Copy, Clone)]
        #[Name = $name]
        pub struct $cls {
            position: f64,

            /// The extension time is how long in seconds it takes to go from full up to full down.
            /// This is assumed to be symmetrical for up and down actuation.
            // TODO: allow for up and down at different rates
            #[property]
            extension_time: f64,
        }

        impl Extension for $cls {
            fn init(runtime: &mut Runtime) -> Result<()> {
                runtime.add_sim_system(Self::sys_tick);
                Ok(())
            }
        }

        #[inject_nitrous_component]
        impl $cls {
            #[inline]
            pub fn new(position: f64, duration: Duration) -> Self {
                Self {
                    position: position.max(0.).min(1.),
                    extension_time: duration.as_secs_f64(),
                }
            }

            #[inline]
            pub fn position(&self) -> f64 {
                self.position
            }

            fn sys_tick(timestep: Res<TimeStep>, mut query: Query<(&$control, &mut $cls)>) {
                for (control, mut state) in query.iter_mut() {
                    state.position = effector_chase(
                        control.position(),
                        timestep.step().as_secs_f64() / state.extension_time,
                        state.position,
                    );
                }
            }
        }
    };
}

make_toggle_chase!(AirbrakeEffector, "airbrake_effector", AirbrakeControl);
make_toggle_chase!(FlapsEffector, "flaps_effector", FlapsControl);
make_toggle_chase!(HookEffector, "hook_effector", HookControl);
make_toggle_chase!(GearEffector, "gear_effector", GearControl);
make_toggle_chase!(BayEffector, "bay_effector", BayControl);
