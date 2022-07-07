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
use nalgebra::{Point3, UnitQuaternion, Vector3};
use std::{f64, f64::consts::PI};

#[derive(Clone, Debug)]
pub struct Cylinder<Unit: LengthUnit> {
    origin: Point3<Length<Unit>>,
    axis: Vector3<Length<Unit>>,
    radius_bottom: Length<Unit>,
    radius_top: Length<Unit>,
}

impl<Unit: LengthUnit> Cylinder<Unit> {
    pub fn new(
        origin: Point3<Length<Unit>>,
        axis: Vector3<Length<Unit>>,
        radius: Length<Unit>,
    ) -> Self {
        Self {
            origin,
            axis,
            radius_bottom: radius,
            radius_top: radius,
        }
    }

    pub fn new_tapered(
        origin: Point3<Length<Unit>>,
        axis: Vector3<Length<Unit>>,
        radius_bottom: Length<Unit>,
        radius_top: Length<Unit>,
    ) -> Self {
        Self {
            origin,
            axis,
            radius_bottom,
            radius_top,
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

impl<Unit: LengthUnit> RenderPrimitive for Cylinder<Unit> {
    fn to_primitive(&self, detail: u32) -> Primitive {
        // Number of faces on the sides
        let steps = detail;
        let origin = self.origin.map(|v| v.f64());
        let axis = self.axis.map(|v| v.f64());

        // Build all vertices by subdividing up two circles on +y.
        let mut verts = make_unit_circle(steps, 0_f64, self.radius_bottom.f64());
        let mut top = make_unit_circle(steps, axis.magnitude(), self.radius_top.f64());
        verts.append(&mut top);

        // Transform the vertices from +y into the axis basis.
        let facing = if let Some(q) = UnitQuaternion::rotation_between(&Vector3::y(), &axis) {
            q
        } else {
            UnitQuaternion::from_axis_angle(&Vector3::x_axis(), PI)
        };
        for vert in &mut verts {
            vert.position = (origin + facing * vert.position).coords;
            vert.normal = facing * vert.normal;
        }

        // Build faces
        // Sides
        let mut faces = Vec::new();
        for i in 0..steps {
            let a = i;
            let b = (i + 1) % steps;
            let c = a + steps;
            let d = b + steps;
            faces.push(Face::new(a, b, c, &verts));
            faces.push(Face::new(b, d, c, &verts));
        }
        // bottom cap
        let normal = Vector3::new(0., -1., 0.);
        for i in 1..steps {
            faces.push(Face::new_with_normal(0, (i + 1) % steps, i, &normal));
        }
        // top cap
        let normal = Vector3::new(0., 1., 0.);
        for i in 1..steps {
            faces.push(Face::new_with_normal(
                steps,
                i + steps,
                (i + 1) % steps + steps,
                &normal,
            ));
        }

        Primitive { verts, faces }
    }
}

fn make_unit_circle(steps: u32, offset: f64, radius: f64) -> Vec<Vertex> {
    let mut out = Vec::new();
    let dr = 2. * f64::consts::PI / steps as f64;
    for i in 0..steps {
        let alpha = dr * i as f64;
        out.push(Vertex {
            position: Vector3::new(alpha.cos() * radius, offset, alpha.sin() * radius),
            normal: Vector3::new(alpha.cos(), 0., alpha.sin()),
        });
    }
    out
}
