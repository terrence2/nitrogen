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
use absolute_unit::{Kilograms, Mass};
use bevy_ecs::prelude::*;
use nitrous::{inject_nitrous_component, NitrousComponent};

#[derive(Component, NitrousComponent, Debug, Clone)]
#[Name = "airframe"]
pub struct Airframe {
    dry_mass: Mass<Kilograms>,
}

#[inject_nitrous_component]
impl Airframe {
    pub fn new(dry_mass: Mass<Kilograms>) -> Self {
        Self { dry_mass }
    }

    pub fn dry_mass(&self) -> Mass<Kilograms> {
        self.dry_mass
    }
}
