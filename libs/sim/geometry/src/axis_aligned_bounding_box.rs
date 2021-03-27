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

#[derive(Debug)]
pub struct Aabb2<T: PartialOrd> {
    lo: [T; 2],
    hi: [T; 2],
}

impl<T: PartialOrd> Aabb2<T> {
    pub fn new(lo: [T; 2], hi: [T; 2]) -> Self {
        Self { lo, hi }
    }

    pub fn contains(&self, p: [T; 2]) -> bool {
        p[0] >= self.lo[0] && p[1] >= self.lo[1] && p[0] <= self.hi[0] && p[1] <= self.hi[1]
    }

    pub fn overlaps(&self, other: &Self) -> bool {
        (self.lo[0] <= other.hi[0] && self.hi[0] >= other.lo[0])
            && (self.lo[1] <= other.hi[1] && self.hi[1] >= other.lo[1])
    }
}
