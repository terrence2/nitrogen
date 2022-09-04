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
pub(crate) mod absolute_control;
pub(crate) mod symmetric_inceptor;
pub(crate) mod throttle_inceptor;
pub(crate) mod toggle_control;

// Takes position and returns the modified value. This is only relevant
// for keyboard input as we do not want an abrupt transition between 0
// and 1/-1. For joystick or mouse control, the inceptor matches the
// physical device position exactly.
pub(crate) fn inceptor_position_tick(target: f64, dt: f64, mut position: f64) -> f64 {
    if target > position {
        position += dt;
        if target < position {
            position = target;
        }
    } else if target < position {
        position -= dt;
        if target > position {
            position = target;
        }
    }
    position.max(-1.).min(1.)
}
