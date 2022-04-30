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
use crate::{TemperatureUnit, Unit};

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Fahrenheit;
impl Unit for Fahrenheit {
    const UNIT_NAME: &'static str = "fahrenheit";
    const UNIT_SHORT_NAME: &'static str = "°F";
    const UNIT_SUFFIX: &'static str = "°F";
}
impl TemperatureUnit for Fahrenheit {
    fn convert_to_kelvin(degrees_in: f64) -> f64 {
        (degrees_in - 32.) * 5. / 9. + 273.15
    }
    fn convert_from_kelvin(degrees_k: f64) -> f64 {
        (degrees_k - 273.15) * 9. / 5. + 32.
    }
}

#[macro_export]
macro_rules! fahrenheit {
    ($num:expr) => {
        $crate::Temperature::<$crate::Fahrenheit>::from(&$num)
    };
}
