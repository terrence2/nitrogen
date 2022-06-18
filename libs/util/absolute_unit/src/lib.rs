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

pub(crate) mod acceleration;
pub(crate) mod angle;
pub(crate) mod angular_acceleration;
pub(crate) mod angular_velocity;
pub(crate) mod area;
pub(crate) mod density;
pub(crate) mod dynamic_unit;
pub(crate) mod force;
pub(crate) mod generic;
pub(crate) mod length;
pub(crate) mod mass;
pub(crate) mod pressure;
pub(crate) mod rotational_inertia;
pub(crate) mod temperature;
pub(crate) mod time;
pub(crate) mod torque;
pub(crate) mod unit;
pub(crate) mod velocity;
pub(crate) mod velocity_squared;
pub(crate) mod volume;
pub(crate) mod weight;

/// Must be implemented by all quantity types.
pub trait Quantity {}

pub use crate::{
    acceleration::Acceleration,
    angle::{Angle, AngleUnit},
    angular_acceleration::AngularAcceleration,
    angular_velocity::AngularVelocity,
    area::Area,
    density::Density,
    dynamic_unit::DynamicUnits,
    force::{Force, ForceUnit},
    length::{Length, LengthUnit},
    mass::{Mass, MassUnit},
    pressure::{Pressure, PressureUnit},
    rotational_inertia::RotationalInertia,
    temperature::{Temperature, TemperatureUnit},
    time::{Time, TimeUnit},
    torque::Torque,
    unit::{
        arcminutes::ArcMinutes, arcseconds::ArcSeconds, celsius::Celsius, degrees::Degrees,
        fahrenheit::Fahrenheit, feet::Feet, hours::Hours, kelvin::Kelvin, kilograms::Kilograms,
        kilometers::Kilometers, meters::Meters, miles::Miles, nautical_miles::NauticalMiles,
        newtons::Newtons, pascals::Pascals, pounds_force::PoundsForce, pounds_mass::PoundsMass,
        pounds_square_foot::PoundsSquareFoot, pounds_weight::PoundsWeight, radians::Radians,
        rankine::Rankine, scalar::Scalar, seconds::Seconds, slugs::Slugs, Unit,
    },
    velocity::Velocity,
    velocity_squared::VelocitySquared,
    volume::Volume,
    weight::{Weight, WeightUnit},
};

// For use from macros
pub use approx;
pub use ordered_float;
