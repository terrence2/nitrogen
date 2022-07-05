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
use absolute_unit::{Length, Meters};
use nalgebra::Point3;
use std::fmt::Debug;

#[derive(Clone, Copy, Debug)]
pub struct Aabb3 {
    hi: Point3<Length<Meters>>,
    lo: Point3<Length<Meters>>,
}

impl Aabb3 {
    pub fn from_bounds(hi: Point3<Length<Meters>>, lo: Point3<Length<Meters>>) -> Self {
        Self { hi, lo }
    }

    pub fn hi(&self) -> &Point3<Length<Meters>> {
        &self.hi
    }

    pub fn lo(&self) -> &Point3<Length<Meters>> {
        &self.lo
    }
}
