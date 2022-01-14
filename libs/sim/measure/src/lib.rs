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
use absolute_unit::Meters;
use bevy_ecs::prelude::*;
use geodesy::{Cartesian, GeoCenter};
use nalgebra::Vector3;

#[derive(Component, Debug, Default)]
pub struct WorldSpaceFrame {
    position: Cartesian<GeoCenter, Meters>,
    forward: Vector3<f64>,
    right: Vector3<f64>,
    up: Vector3<f64>,
}

impl WorldSpaceFrame {
    pub fn new(position: Cartesian<GeoCenter, Meters>, forward: Vector3<f64>) -> Self {
        let right = position.vec64().cross(&forward);
        let up = right.cross(&forward);
        Self {
            position,
            forward: forward.normalize(),
            right: right.normalize(),
            up: up.normalize(),
        }
    }

    pub fn position(&self) -> &Cartesian<GeoCenter, Meters> {
        &self.position
    }

    pub fn forward(&self) -> &Vector3<f64> {
        &self.forward
    }

    pub fn right(&self) -> &Vector3<f64> {
        &self.right
    }

    pub fn up(&self) -> &Vector3<f64> {
        &self.up
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {}
}
