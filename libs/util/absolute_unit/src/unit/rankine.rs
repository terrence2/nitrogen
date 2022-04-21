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
use crate::temperature::TemperatureUnit;

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Rankine;
impl TemperatureUnit for Rankine {
    fn unit_name() -> &'static str {
        "rankine"
    }
    fn suffix() -> &'static str {
        "°R"
    }
    fn convert_to_kelvin(degrees_in: f64) -> f64 {
        degrees_in * 5. / 9.
    }
    fn convert_from_kelvin(degrees_k: f64) -> f64 {
        degrees_k * 9. / 5.
    }
}

#[macro_export]
macro_rules! rankine {
    ($num:expr) => {
        $crate::Temperature::<$crate::Rankine>::from(&$num)
    };
}
