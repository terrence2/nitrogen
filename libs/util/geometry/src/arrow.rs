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
use crate::{Cylinder, Primitive, RenderPrimitive};
use absolute_unit::{scalar, Length, LengthUnit};
use nalgebra::{Point3, Vector3};

#[derive(Clone, Debug)]
pub struct Arrow<Unit>
where
    Unit: LengthUnit,
{
    origin: Point3<Length<Unit>>,
    axis: Vector3<Length<Unit>>,
    radius: Length<Unit>,
}

impl<Unit> Arrow<Unit>
where
    Unit: LengthUnit,
{
    pub fn new(
        origin: Point3<Length<Unit>>,
        axis: Vector3<Length<Unit>>,
        radius: Length<Unit>,
    ) -> Self {
        Self {
            origin,
            axis,
            radius,
        }
    }

    pub fn axis(&self) -> &Vector3<Length<Unit>> {
        &self.axis
    }

    pub fn origin(&self) -> &Point3<Length<Unit>> {
        &self.origin
    }

    pub fn set_axis(&mut self, axis: Vector3<Length<Unit>>) {
        self.axis = axis;
    }

    pub fn set_origin(&mut self, origin: Point3<Length<Unit>>) {
        self.origin = origin;
    }
}

impl<Unit> RenderPrimitive for Arrow<Unit>
where
    Unit: LengthUnit,
{
    fn to_primitive(&self, detail: u32) -> Primitive {
        let to_head = self.axis.map(|v| v * scalar!(0.9));
        let shaft = Cylinder::new(self.origin, to_head, self.radius);
        let head = Cylinder::new_tapered(
            self.origin + to_head,
            self.axis.map(|v| v * scalar!(0.1_f64)),
            self.radius * scalar!(1.4_f64),
            Length::<Unit>::from(0_f64),
        );

        let mut prim = shaft.to_primitive(detail);
        prim.extend(&mut head.to_primitive(detail));
        prim
    }
}
