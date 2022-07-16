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
use approx::relative_eq;
use nalgebra::{Point3, Vector3};
use std::fmt::Debug;

#[derive(Clone, Copy, Debug)]
pub struct Plane {
    normal: Vector3<f64>,
    distance: f64,
}

impl Plane {
    pub fn xy() -> Self {
        Self {
            normal: Vector3::new(0_f64, 0_f64, 1_f64),
            distance: 0_f64,
        }
    }

    pub fn yz() -> Self {
        Self {
            normal: Vector3::new(1_f64, 0_f64, 0_f64),
            distance: 0_f64,
        }
    }

    pub fn xz() -> Self {
        Self {
            normal: Vector3::new(0_f64, 1_f64, 0_f64),
            distance: 0_f64,
        }
    }

    pub fn from_point_and_normal(p: &Point3<f64>, n: &Vector3<f64>) -> Self {
        Self {
            normal: n.to_owned(),
            distance: p.coords.dot(n),
        }
    }

    pub fn from_normal_and_distance(normal: Vector3<f64>, distance: f64) -> Self {
        Self { normal, distance }
    }

    pub fn point_on_plane(&self, p: &Point3<f64>) -> bool {
        relative_eq!(self.normal.dot(&p.coords) - self.distance(), 0_f64)
    }

    pub fn distance_to_point(&self, p: &Point3<f64>) -> f64 {
        self.normal.dot(&p.coords) - self.distance()
    }

    pub fn closest_point_on_plane(&self, p: &Point3<f64>) -> Point3<f64> {
        p - (self.normal * self.distance_to_point(p))
    }

    pub fn point_is_in_front(&self, p: &Point3<f64>) -> bool {
        self.normal.dot(&p.coords) - self.distance() >= 0_f64
    }

    pub fn point_is_in_front_with_offset(&self, p: &Point3<f64>, offset: f64) -> bool {
        self.normal.dot(&p.coords) - self.distance() >= offset
    }

    pub fn normal(&self) -> &Vector3<f64> {
        &self.normal
    }

    pub fn distance(&self) -> f64 {
        self.distance
    }

    pub fn d(&self) -> f64 {
        -self.distance
    }

    pub fn vec4<To>(&self) -> [f64; 4] {
        [self.normal.x, self.normal.y, self.normal.z, self.distance]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_point_on_plane() {
        let plane = Plane::from_point_and_normal(
            &Point3::new(0f64, 0f64, 0f64),
            &Vector3::new(0f64, 0f64, 1f64),
        );
        assert!(plane.point_on_plane(&Point3::new(10f64, 10f64, 0f64)));
        assert!(!plane.point_on_plane(&Point3::new(10f64, 10f64, 0.1f64)));
        assert!(!plane.point_on_plane(&Point3::new(10f64, 10f64, -0.1f64)));
    }

    #[test]
    fn test_point_distance() {
        let plane = Plane::from_point_and_normal(
            &Point3::new(0f64, 0f64, 0f64),
            &Vector3::new(0f64, 0f64, 1f64),
        );

        assert_relative_eq!(
            -1f64,
            plane.distance_to_point(&Point3::new(1f64, 1f64, -1f64))
        );
        assert_relative_eq!(
            1f64,
            plane.distance_to_point(&Point3::new(-1f64, -1f64, 1f64))
        );
    }

    #[test]
    fn test_closest_point_on_plane() {
        let plane = Plane::from_point_and_normal(
            &Point3::new(0f64, 0f64, 0f64),
            &Vector3::new(0f64, 0f64, 1f64),
        );

        assert_relative_eq!(
            Point3::new(1f64, 1f64, 0f64),
            plane.closest_point_on_plane(&Point3::new(1f64, 1f64, -1f64))
        );
        assert_relative_eq!(
            Point3::new(-1f64, -1f64, 0f64),
            plane.closest_point_on_plane(&Point3::new(-1f64, -1f64, 1f64))
        );
    }
}
