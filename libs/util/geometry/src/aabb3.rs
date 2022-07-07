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
use absolute_unit::{Length, LengthUnit};
use nalgebra::Point3;
use std::{cmp::PartialOrd, fmt::Debug};

#[derive(Clone, Copy, Debug)]
pub struct Aabb3<Unit>
where
    Unit: LengthUnit + PartialOrd,
{
    hi: Point3<Length<Unit>>,
    lo: Point3<Length<Unit>>,
}

impl<Unit> Aabb3<Unit>
where
    Unit: LengthUnit + PartialOrd,
{
    pub fn from_bounds(lo: Point3<Length<Unit>>, hi: Point3<Length<Unit>>) -> Self {
        debug_assert!(lo.x <= hi.x);
        debug_assert!(lo.y <= hi.y);
        debug_assert!(lo.z <= hi.z);
        Self { hi, lo }
    }

    pub fn hi(&self) -> &Point3<Length<Unit>> {
        &self.hi
    }

    pub fn lo(&self) -> &Point3<Length<Unit>> {
        &self.lo
    }

    pub fn low(&self, index: usize) -> &Length<Unit> {
        &self.lo[index]
    }

    pub fn span(&self, index: usize) -> Length<Unit> {
        self.hi[index] - self.lo[index]
    }
}
