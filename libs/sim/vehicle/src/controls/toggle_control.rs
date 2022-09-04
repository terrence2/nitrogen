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
macro_rules! make_toggle {
    ($cls:ident, $name:expr, $enabled:ident) => {
        #[derive(Component, NitrousComponent, Debug, Default, Copy, Clone)]
        #[Name = $name]
        pub struct $cls {
            enabled: bool, // [0, 1]
        }

        #[inject_nitrous_component]
        impl $cls {
            pub fn new(enabled: bool) -> Self {
                Self { enabled }
            }

            #[method]
            pub fn toggle(&mut self) {
                self.enabled = !self.enabled;
            }

            #[method]
            pub fn enabled(&self) -> bool {
                self.enabled
            }

            #[method]
            pub fn $enabled(&self) -> bool {
                self.enabled
            }

            // Expose as a float to be compatible with absolute controls.
            #[method]
            pub fn position(&self) -> f64 {
                if self.enabled {
                    1.
                } else {
                    0.
                }
            }
        }
    };
}

make_toggle!(GearControl, "gear", is_down);
make_toggle!(HookControl, "hook", is_down);
make_toggle!(BayControl, "bay", is_open);
