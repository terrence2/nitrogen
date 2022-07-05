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
use crate::{algorithm::bisect_edge_verts, Face, Primitive, RenderPrimitive, Vertex};
use nalgebra::{Point3, Vector3};
use std::fmt::Debug;

#[derive(Clone, Copy, Debug)]
pub struct Sphere {
    center: Point3<f64>,
    radius: f64,
}

impl Default for Sphere {
    fn default() -> Self {
        Self {
            center: Point3::origin(),
            radius: 1_f64,
        }
    }
}

impl Sphere {
    pub fn from_center_and_radius(center: &Point3<f64>, radius: f64) -> Self {
        Self {
            center: *center,
            radius,
        }
    }

    pub fn center(&self) -> &Point3<f64> {
        &self.center
    }

    pub fn radius(&self) -> f64 {
        self.radius
    }
}

impl RenderPrimitive for Sphere {
    // Detail here is level of splitting
    fn to_primitive(&self, detail: u32) -> Primitive {
        // The bones of the d12 are 3 orthogonal quads at the origin.
        let t = (1f64 + 5f64.sqrt()) / 2f64;
        let mut init = vec![
            Vector3::new(-1f64, t, 0f64).normalize(),
            Vector3::new(1f64, t, 0f64).normalize(),
            Vector3::new(-1f64, -t, 0f64).normalize(),
            Vector3::new(1f64, -t, 0f64).normalize(),
            Vector3::new(0f64, -1f64, t).normalize(),
            Vector3::new(0f64, 1f64, t).normalize(),
            Vector3::new(0f64, -1f64, -t).normalize(),
            Vector3::new(0f64, 1f64, -t).normalize(),
            Vector3::new(t, 0f64, -1f64).normalize(),
            Vector3::new(t, 0f64, 1f64).normalize(),
            Vector3::new(-t, 0f64, -1f64).normalize(),
            Vector3::new(-t, 0f64, 1f64).normalize(),
        ];
        let mut verts = init
            .drain(..)
            .map(|ref position| Vertex::new(position, position))
            .collect::<Vec<Vertex>>();

        let mut faces = vec![
            // 5 faces around point 0
            Face::new(0, 11, 5, &verts),
            Face::new(0, 5, 1, &verts),
            Face::new(0, 1, 7, &verts),
            Face::new(0, 7, 10, &verts),
            Face::new(0, 10, 11, &verts),
            // 5 adjacent faces
            Face::new(1, 5, 9, &verts),
            Face::new(5, 11, 4, &verts),
            Face::new(11, 10, 2, &verts),
            Face::new(10, 7, 6, &verts),
            Face::new(7, 1, 8, &verts),
            // 5 faces around point 3
            Face::new(3, 9, 4, &verts),
            Face::new(3, 4, 2, &verts),
            Face::new(3, 2, 6, &verts),
            Face::new(3, 6, 8, &verts),
            Face::new(3, 8, 9, &verts),
            // 5 adjacent faces
            Face::new(4, 9, 5, &verts),
            Face::new(2, 4, 11, &verts),
            Face::new(6, 2, 10, &verts),
            Face::new(8, 6, 7, &verts),
            Face::new(9, 8, 1, &verts),
        ];

        // Subdivide repeatedly to get a spherical object.
        for _ in 0..detail {
            let mut next_faces = Vec::new();
            for face in &faces {
                let a = bisect_edge_verts(&verts[face.i0()], &verts[face.i1()]).normalize();
                let b = bisect_edge_verts(&verts[face.i1()], &verts[face.i2()]).normalize();
                let c = bisect_edge_verts(&verts[face.i2()], &verts[face.i0()]).normalize();

                let ia = verts.len() as u32;
                verts.push(Vertex::new(&(self.center.coords + (a * self.radius)), &a));
                let ib = verts.len() as u32;
                verts.push(Vertex::new(&(self.center.coords + (b * self.radius)), &b));
                let ic = verts.len() as u32;
                verts.push(Vertex::new(&(self.center.coords + (c * self.radius)), &c));

                next_faces.push(Face::new(face.index0, ia, ic, &verts));
                next_faces.push(Face::new(face.index1, ib, ia, &verts));
                next_faces.push(Face::new(face.index2, ic, ib, &verts));
                next_faces.push(Face::new(ia, ib, ic, &verts));
            }
            faces = next_faces;
        }

        Primitive { verts, faces }
    }
}
