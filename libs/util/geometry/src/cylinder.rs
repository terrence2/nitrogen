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
use nalgebra::Vector3;
use std::f64;

#[derive(Clone, Debug)]
pub struct Cylinder {
    radius_bottom: f64,
    height: f64,
    radius_top: f64,
}

impl Cylinder {
    pub fn new(height: f64, radius: f64) -> Self {
        Self {
            radius_bottom: radius,
            height,
            radius_top: radius,
        }
    }

    pub fn new_tapered(height: f64, radius_bottom: f64, radius_top: f64) -> Self {
        Self {
            radius_bottom,
            height,
            radius_top,
        }
    }
}

impl RenderPrimitive for Cylinder {
    fn to_primitive(&self, detail: u32) -> Primitive {
        // Number of faces on the sides
        let steps = detail;
        let mut bottom = make_unit_circle(steps, 0., self.radius_bottom);
        let mut top = make_unit_circle(steps, self.height, self.radius_top);
        bottom.append(&mut top);

        let mut faces = Vec::new();

        // Sides
        for i in 0..steps {
            let a = i;
            let b = (i + 1) % steps;
            let c = a + steps;
            let d = b + steps;
            faces.push(Face::new(a, b, c, &bottom));
            faces.push(Face::new(b, d, c, &bottom));
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

        Primitive {
            verts: bottom,
            faces,
        }
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
