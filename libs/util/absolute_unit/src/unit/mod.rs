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

/// Must be implemented by all unit types.
pub trait Unit {
    const UNIT_NAME: &'static str;
    const UNIT_SHORT_NAME: &'static str;
    const UNIT_SUFFIX: &'static str;
}

// Unitless
pub(crate) mod scalar;

// Angular
pub(crate) mod arcminutes;
pub(crate) mod arcseconds;
pub(crate) mod degrees;
pub(crate) mod radians;

// Distance
pub(crate) mod feet;
pub(crate) mod kilometers;
pub(crate) mod meters;
pub(crate) mod miles;
pub(crate) mod nautical_miles;

// Temperature
pub(crate) mod celsius;
pub(crate) mod fahrenheit;
pub(crate) mod kelvin;
pub(crate) mod rankine;

// Mass
pub(crate) mod kilograms;
pub(crate) mod pounds_mass;
pub(crate) mod slugs;

// Time
pub(crate) mod hours;
pub(crate) mod seconds;

// Force
pub(crate) mod newtons;
pub(crate) mod pounds_force;

// Pressure
pub(crate) mod pascals;
pub(crate) mod pounds_square_foot;
