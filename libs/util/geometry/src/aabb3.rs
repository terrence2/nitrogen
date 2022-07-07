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
use crate::{Face, Primitive, RenderPrimitive, Vertex};
use absolute_unit::{Length, LengthUnit};
use nalgebra::{Point3, Vector3};
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

impl<Unit> RenderPrimitive for Aabb3<Unit>
where
    Unit: LengthUnit + PartialOrd,
{
    fn to_primitive(&self, _detail: u32) -> Primitive {
        let lo = self.lo.map(|v| v.f64());
        let hi = self.hi.map(|v| v.f64());
        let verts = vec![
            Vertex::new(&Vector3::new(lo.x, hi.y, lo.z)),
            Vertex::new(&Vector3::new(hi.x, hi.y, lo.z)),
            Vertex::new(&Vector3::new(hi.x, lo.y, lo.z)),
            Vertex::new(&Vector3::new(hi.x, lo.y, hi.z)),
            Vertex::new(&Vector3::new(lo.x, hi.y, hi.z)),
            Vertex::new(&Vector3::new(lo.x, lo.y, hi.z)),
            Vertex::new(&Vector3::new(lo.x, lo.y, lo.z)),
            Vertex::new(&Vector3::new(hi.x, hi.y, hi.z)),
        ];
        let [a, b, c, d, e, f, lo, hi] = [0u32, 1, 2, 3, 4, 5, 6, 7];
        let quads = [
            ([lo, a, b, c], -Vector3::z_axis()),
            ([c, b, hi, d], Vector3::x_axis()),
            ([d, hi, e, f], Vector3::z_axis()),
            ([f, e, a, lo], -Vector3::x_axis()),
            ([a, e, hi, b], Vector3::y_axis()),
            ([f, lo, c, d], -Vector3::y_axis()),
        ];
        let mut faces = Vec::new();
        for ([a, b, c, d], norm) in quads {
            faces.push(Face::new_with_normal(a, b, c, &norm.xyz()));
            faces.push(Face::new_with_normal(a, c, d, &norm.xyz()));
        }

        Primitive { verts, faces }
    }
}
