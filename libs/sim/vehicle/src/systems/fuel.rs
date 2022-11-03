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
use absolute_unit::{kilograms, scalar, Kilograms, Mass};
use anyhow::{ensure, Result};
use bevy_ecs::prelude::*;
use nitrous::{inject_nitrous_component, method, NitrousComponent};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ConsumeResult {
    Satisfied,
    OutOfFuel,
}

#[derive(Debug, Copy, Clone)]
pub enum FuelTankKind {
    LeftWing,
    RightWing,
    LeftBelly,
    RightBelly,
    Center,
    LeftDrop,
    RightDrop,
}

impl FuelTankKind {
    pub fn is_drop_tank(&self) -> bool {
        matches!(self, FuelTankKind::LeftDrop | FuelTankKind::RightDrop)
    }
}

#[derive(Debug, Copy, Clone)]
pub struct FuelTank {
    kind: FuelTankKind,
    // TODO: implement a mass model for each of these
    // distribution: MassModel,
    full_mass: Mass<Kilograms>,
    current_mass: Mass<Kilograms>,
}

impl FuelTank {
    pub fn new(kind: FuelTankKind, full_mass: Mass<Kilograms>) -> Self {
        Self {
            kind,
            full_mass,
            current_mass: full_mass,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.current_mass <= kilograms!(0f64)
    }

    pub fn refill(&mut self) {
        self.current_mass = self.full_mass;
    }

    pub fn consume_fuel(&mut self, amount: Mass<Kilograms>) -> Mass<Kilograms> {
        if amount <= self.current_mass {
            self.current_mass -= amount;
            kilograms!(0f64)
        } else {
            let remainder = amount - self.current_mass;
            self.current_mass = kilograms!(0f64);
            remainder
        }
    }
}

#[derive(Component, NitrousComponent, Debug, Default, Clone)]
#[Name = "fuel"]
pub struct FuelSystem {
    internal: Vec<FuelTank>,
    drop: Vec<FuelTank>,
}

#[inject_nitrous_component]
impl FuelSystem {
    pub fn with_internal_tank(mut self, tank: FuelTank) -> Result<Self> {
        ensure!(!tank.kind.is_drop_tank());
        self.internal.push(tank);
        Ok(self)
    }

    pub fn add_drop_tank(&mut self, tank: FuelTank) -> Result<()> {
        ensure!(tank.kind.is_drop_tank());
        self.drop.push(tank);
        Ok(())
    }

    #[method]
    pub fn has_drop_tanks(&self) -> bool {
        !self.drop.is_empty()
    }

    pub fn consume_fuel(&mut self, mut amount: Mass<Kilograms>) -> ConsumeResult {
        // Consume from drop tanks first.
        if !self.drop.is_empty() {
            // Try to consume as evenly as possible from non-empty tanks.
            let non_empty_drop = self.drop.iter().filter(|&tank| !tank.is_empty()).count();
            let amt_per_drop = amount / scalar!(non_empty_drop);
            for tank in &mut self.drop {
                if !tank.is_empty() {
                    let unfulfilled = tank.consume_fuel(amt_per_drop);
                    amount -= amt_per_drop - unfulfilled;
                }
            }
        }
        // If we run out of fuel in our drop tanks, there may be more left needed.
        assert!(amount >= kilograms!(0f64));

        // Try to consume as evenly as possible from all tanks.
        let non_empty_internal = self
            .internal
            .iter()
            .filter(|&tank| !tank.is_empty())
            .count();
        let amt_per_internal = amount / scalar!(non_empty_internal);
        for tank in &mut self.internal {
            if !tank.is_empty() {
                let unfulfilled = tank.consume_fuel(amt_per_internal);
                amount -= amt_per_internal - unfulfilled;
            }
        }

        if amount > kilograms!(0f64) {
            return ConsumeResult::OutOfFuel;
        }
        ConsumeResult::Satisfied
    }

    pub fn fuel_mass(&self) -> Mass<Kilograms> {
        let mut total = kilograms!(0);
        for tank in &self.drop {
            total += tank.current_mass;
        }
        for tank in &self.internal {
            total += tank.current_mass;
        }
        total
    }
}
