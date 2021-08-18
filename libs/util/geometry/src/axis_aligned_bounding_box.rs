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
use std::ops::Sub;

#[derive(Clone, Debug)]
pub struct Aabb<T, const N: usize> {
    lo: [T; N],
    hi: [T; N],
}

impl<T: Copy + PartialOrd + Sub<Output = T>, const N: usize> Aabb<T, N> {
    pub fn new(lo: [T; N], hi: [T; N]) -> Self {
        assert!((0..N).all(|i| lo[i] <= hi[i]));
        Self { lo, hi }
    }

    pub fn contains(&self, p: [T; N]) -> bool {
        (0..N).all(|i| p[i] >= self.lo[i] && p[i] <= self.hi[i])
    }

    pub fn overlaps(&self, other: &Self) -> bool {
        (0..N).all(|i| self.lo[i] <= other.hi[i] && self.hi[i] >= other.lo[i])
    }

    pub fn span(&self, i: usize) -> T {
        self.hi[i] - self.lo[i]
    }

    pub fn low(&self, i: usize) -> T {
        self.lo[i]
    }

    pub fn high(&self, i: usize) -> T {
        self.hi[i]
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_aabb2_contains() {
        let b = Aabb::new([0f32; 2], [1f32; 2]);
        assert!(b.contains([0.5f32, 0.5f32]));
        assert!(!b.contains([0f32, -1f32]));
        assert!(!b.contains([-1f32, 0f32]));
        assert!(!b.contains([2f32, 0f32]));
        assert!(!b.contains([0f32, 2f32]));
    }

    #[test]
    fn test_aabb3_overlaps() {
        let a = Aabb::new([0f32; 3], [1f32; 3]);
        let b = Aabb::new([0.5f32; 3], [3f32; 3]);
        assert!(a.overlaps(&b));
        let c = Aabb::new([2f32; 3], [3f32; 3]);
        assert!(!a.overlaps(&c));
    }
}
