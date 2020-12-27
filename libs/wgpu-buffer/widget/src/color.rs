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

#[derive(Copy, Clone, Debug)]
pub enum Color {
    Transparent,
    Black,
    Gray,
    Brown,
    White,
    Pink,
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
    Magenta,
    Custom([f32; 4]),
}

impl Color {
    pub fn to_u8_array(&self) -> [u8; 4] {
        let a = self.to_f32_array();
        [
            (a[0] * 255.0) as u8,
            (a[1] * 255.0) as u8,
            (a[2] * 255.0) as u8,
            (a[3] * 255.0) as u8,
        ]
    }

    pub fn to_f32_array(&self) -> [f32; 4] {
        match self {
            Self::Transparent => [0.; 4],
            Self::Black => [0., 0., 0., 1.],
            Self::Gray => [0.5, 0.5, 0.5, 1.],
            Self::Brown => [0.65, 0.165, 0.165, 1.],
            Self::White => [1.; 4],
            Self::Pink => [1., 0.5, 0.5, 1.],
            Self::Red => [1., 0., 0., 1.],
            Self::Orange => [1., 0.64, 0., 1.],
            Self::Yellow => [1., 1., 0., 1.],
            Self::Green => [0., 1., 0., 1.],
            Self::Blue => [0., 0., 1., 1.],
            Self::Purple => [0.5, 0., 0.5, 1.],
            Self::Magenta => [1., 0., 1., 1.],
            Self::Custom(f) => *f,
        }
    }
}
