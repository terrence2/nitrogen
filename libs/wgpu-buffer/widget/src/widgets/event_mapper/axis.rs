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
use failure::{bail, Fallible};
use unicase::eq_ascii;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum AxisKind {
    MouseMotion,
    MouseWheel,
    // JoystickAxis(i64),
}

impl AxisKind {
    pub fn from_virtual(v: &str) -> Fallible<Self> {
        Ok(if eq_ascii(v, "mousemotion") {
            AxisKind::MouseMotion
        } else if eq_ascii(v, "mousewheel") {
            AxisKind::MouseWheel
        } else {
            bail!("unrecognized axis identifier: {}", v)
        })
    }
}
