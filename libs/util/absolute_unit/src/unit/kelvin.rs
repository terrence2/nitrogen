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
pub struct Kelvin;
impl TemperatureUnit for Kelvin {
    fn unit_name() -> &'static str {
        "kelvin"
    }
    fn suffix() -> &'static str {
        "°K"
    }
    fn convert_to_kelvin(degrees_in: f64) -> f64 {
        degrees_in
    }
    fn convert_from_kelvin(degrees_k: f64) -> f64 {
        degrees_k
    }
}

#[macro_export]
macro_rules! kelvin {
    ($num:expr) => {
        $crate::Temperature::<$crate::Kelvin>::from(&$num)
    };
}
