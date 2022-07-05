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
use nalgebra::{Point3, Vector3};

pub fn sphere_vs_ray(sphere: &Sphere, ray: &Ray) -> Option<Point3<f64>> {
    let ray2sphere: Vector3<f64> = ray.origin() - sphere.center();
    let a = ray.direction().dot(ray.direction());
    let b = 2_f64 * ray.direction().dot(&ray2sphere);
    let c = ray2sphere.dot(&ray2sphere) - sphere.radius() * sphere.radius();

    let x0;
    let x1;
    let discriminant = b * b - 4_f64 * a * c;
    if discriminant < 0_f64 {
        return None;
    }
    if discriminant == 0_f64 {
        x0 = -0.5_f64 * b / a;
        x1 = x0;
    } else {
        let q = if b > 0_f64 {
            -0.5_f64 * (b + discriminant.sqrt())
        } else {
            -0.5_f64 * (b - discriminant.sqrt())
        };
        x0 = q / a;
        x1 = c / q;
    }
    let mut t = x0.min(x1);
    // One negative: maybe inside sphere or behind.
    if t < 0_f64 {
        t = x0.max(x1);
    }
    // Both negative: sphere is behind us
    if t < 0_f64 {
        return None;
    }

    Some(ray.origin() + (ray.direction() * t))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_ray_sphere_basic() {
        let sphere = Sphere::from_center_and_radius(&Point3::new(0f64, 0f64, 10f64), 1f64);
        let ray = Ray::new(Point3::origin(), Vector3::z_axis().into_inner());
        let intersect = sphere_vs_ray(&sphere, &ray);
        assert!(intersect.is_some());
        assert_eq!(intersect.unwrap().x, 0f64);
        assert_eq!(intersect.unwrap().y, 0f64);
        assert_eq!(intersect.unwrap().z, 9f64);
    }
}
