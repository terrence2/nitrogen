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
pub(crate) mod toggle_effector;

pub(crate) fn effector_chase(target: f64, dt: f64, mut position: f64) -> f64 {
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
    position.clamp(-1., 1.)
}
