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
use crate::{Ray, Sphere};
use nalgebra::{Point3, RealField, Vector3};
use num_traits::cast::FromPrimitive;
use std::fmt::{Debug, Display};

pub fn sphere_vs_ray<T>(sphere: &Sphere<T>, ray: &Ray<T>) -> Option<Point3<T>>
where
    T: Copy + Clone + Debug + Display + PartialEq + FromPrimitive + RealField + 'static,
{
    let two = T::one() + T::one();
    let half = T::one() / two;
    let four = two + two;

    let ray2sphere: Vector3<T> = ray.origin() - sphere.center();
    let a = ray.direction().dot(ray.direction());
    let b = two * ray.direction().dot(&ray2sphere);
    let c = ray2sphere.dot(&ray2sphere) - sphere.radius() * sphere.radius();

    let x0: T;
    let x1: T;
    let discriminant = b * b - four * a * c;
    if discriminant < T::zero() {
        return None;
    }
    if discriminant == T::zero() {
        x0 = -half * b / a;
        x1 = x0;
    } else {
        let q = if b > T::zero() {
            -half * (b + discriminant.sqrt())
        } else {
            -half * (b - discriminant.sqrt())
        };
        x0 = q / a;
        x1 = c / q;
    }
    let mut t = x0.min(x1);
    // One negative: maybe inside sphere or behind.
    if t < T::zero() {
        t = x0.max(x1);
    }
    // Both negative: sphere is behind us
    if t < T::zero() {
        return None;
    }

    Some(ray.origin() + (ray.direction() * t))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_ray_sphere_basic() {
        let sphere = Sphere::from_center_and_radius(&Point3::new(0f32, 0f32, 10f32), 1f32);
        let ray = Ray::new(Point3::origin(), Vector3::z_axis().into_inner());
        let intersect = sphere_vs_ray(&sphere, &ray);
        assert!(intersect.is_some());
        assert!(intersect.unwrap().x == 0f32);
        assert!(intersect.unwrap().y == 0f32);
        assert!(intersect.unwrap().z == 9f32);
    }
}
