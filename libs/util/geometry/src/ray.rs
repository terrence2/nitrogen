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
use nalgebra::{Point3, RealField, Vector3};
use num_traits::cast::FromPrimitive;
use std::fmt::{Debug, Display};

pub struct Ray<T>
where
    T: Copy + Clone + Debug + Display + PartialEq + FromPrimitive + RealField + 'static,
{
    origin: Point3<T>,
    direction: Vector3<T>,
}

impl<T> Ray<T>
where
    T: Copy + Clone + Debug + Display + PartialEq + FromPrimitive + RealField + 'static,
{
    pub fn new(origin: Point3<T>, direction: Vector3<T>) -> Self {
        Self { origin, direction }
    }

    pub fn origin(&self) -> &Point3<T> {
        &self.origin
    }

    pub fn direction(&self) -> &Vector3<T> {
        &self.direction
    }
}
