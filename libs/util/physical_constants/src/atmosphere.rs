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
use crate::{EARTH_RADIUS, STANDARD_GRAVITY};
use absolute_unit::{
    kelvin, kilograms_per_meter3, meters, pascals, scalar, Acceleration, Density, Kelvin, Length,
    LengthUnit, MassUnit, Meters, Pascals, Pressure, PressureUnit, Scalar, Seconds, Temperature,
    TemperatureUnit,
};
use approx::abs_diff_eq;
use once_cell::sync::Lazy;

pub struct LayerInfo {
    geopotential_base_altitude: Length<Meters>,
    geopotential_top_altitude: Length<Meters>,
    base_temperature: Temperature<Kelvin>,
    base_pressure: Pressure<Pascals>,
    gradient: Scalar,
    #[allow(unused)]
    layer_name: &'static str,
}
static LAYERS: Lazy<[LayerInfo; 9]> = Lazy::new(|| {
    [
        LayerInfo {
            geopotential_base_altitude: meters!(-5_000f64),
            geopotential_top_altitude: meters!(0f64),
            base_temperature: kelvin!(320.65f64),
            gradient: scalar!(-6.5e-3f64),
            base_pressure: pascals!(1.776_97e5f64),
            layer_name: "troposphere",
        },
        LayerInfo {
            geopotential_base_altitude: meters!(0f64),
            geopotential_top_altitude: meters!(11_000f64),
            base_temperature: kelvin!(288.15f64),
            gradient: scalar!(-6.5e-3f64),
            base_pressure: pascals!(1.013_25e5f64),
            layer_name: "troposphere",
        },
        LayerInfo {
            geopotential_base_altitude: meters!(11_000f64),
            geopotential_top_altitude: meters!(20_000f64),
            base_temperature: kelvin!(216.65f64),
            gradient: scalar!(0.0f64),
            base_pressure: pascals!(2.263_20e4f64),
            layer_name: "tropopause",
        },
        LayerInfo {
            geopotential_base_altitude: meters!(20_000f64),
            geopotential_top_altitude: meters!(32_000f64),
            base_temperature: kelvin!(216.65f64),
            gradient: scalar!(1.0e-3f64),
            base_pressure: pascals!(5.474_87e3f64),
            layer_name: "stratosphere",
        },
        LayerInfo {
            geopotential_base_altitude: meters!(32_000f64),
            geopotential_top_altitude: meters!(47_000f64),
            base_temperature: kelvin!(228.65f64),
            gradient: scalar!(2.8e-3f64),
            base_pressure: pascals!(8.680_14e2f64),
            layer_name: "stratosphere",
        },
        LayerInfo {
            geopotential_base_altitude: meters!(47_000f64),
            geopotential_top_altitude: meters!(51_000f64),
            base_temperature: kelvin!(270.65f64),
            gradient: scalar!(0f64),
            base_pressure: pascals!(1.109_06e2f64),
            layer_name: "stratopause",
        },
        LayerInfo {
            geopotential_base_altitude: meters!(51_000f64),
            geopotential_top_altitude: meters!(71_000f64),
            base_temperature: kelvin!(270.65f64),
            gradient: scalar!(-2.8e-3f64),
            base_pressure: pascals!(6.693_84e1f64),
            layer_name: "mesosphere",
        },
        LayerInfo {
            geopotential_base_altitude: meters!(71_000f64),
            geopotential_top_altitude: meters!(80_000f64),
            base_temperature: kelvin!(214.65f64),
            gradient: scalar!(-2.0e-3f64),
            base_pressure: pascals!(3.956_39f64),
            layer_name: "mesosphere",
        },
        LayerInfo {
            geopotential_base_altitude: meters!(80_000f64),
            geopotential_top_altitude: meters!(100_000f64),
            base_temperature: kelvin!(196.65f64),
            gradient: scalar!(-2.0e-3f64),
            base_pressure: pascals!(8.862_72e-1f64),
            layer_name: "mesosphere",
        },
    ]
});

// Specific gas constant for "atmosphere".
const R: f64 = 287.052_87;

pub struct StandardAtmosphere {
    geopotential_altitude: Length<Meters>,
}

impl StandardAtmosphere {
    pub fn at_altitude<Unit: LengthUnit>(geometric_altitude: Length<Unit>) -> Self {
        let geometric_altitude = meters!(geometric_altitude);
        Self {
            geopotential_altitude: Self::compute_geopotential_altitude(geometric_altitude),
        }
    }

    fn compute_geopotential_altitude(geometric_altitude: Length<Meters>) -> Length<Meters> {
        let r = EARTH_RADIUS.f64();
        let h = geometric_altitude.f64();
        meters!(r * h / (r + h))
    }

    fn get_layer(&self) -> &LayerInfo {
        for layer in LAYERS.iter() {
            if self.geopotential_altitude >= layer.geopotential_base_altitude
                && self.geopotential_altitude < layer.geopotential_top_altitude
            {
                return layer;
            }
        }
        if self.geopotential_altitude < LAYERS[0].geopotential_base_altitude {
            return &LAYERS[0];
        }
        assert!(self.geopotential_altitude >= LAYERS[LAYERS.len() - 1].geopotential_top_altitude);
        &LAYERS[LAYERS.len() - 1]
    }

    pub fn temperature<Unit: TemperatureUnit>(&self) -> Temperature<Unit> {
        let layer = self.get_layer();
        let k = layer.base_temperature
            + kelvin!(
                layer.gradient.into_inner()
                    * (self.geopotential_altitude - layer.geopotential_base_altitude).f64()
            );
        Temperature::<Unit>::from(&k)
    }

    pub fn pressure<Unit: PressureUnit>(&self) -> Pressure<Unit> {
        let layer = self.get_layer();

        let temperature = self.temperature::<Kelvin>();
        let delta_altitude = self.geopotential_altitude - layer.geopotential_base_altitude;
        let g0: Acceleration<Meters, Seconds> = *STANDARD_GRAVITY;

        // We need a different computation when the temperature change is fixed.
        Pressure::<Unit>::from(&if abs_diff_eq!(layer.gradient.into_inner(), 0.0) {
            layer.base_pressure
                * scalar!((-g0.f64() / (R * temperature.f64()) * delta_altitude.f64()).exp())
        } else {
            let exponent = 1.0 / layer.gradient.into_inner();
            layer.base_pressure
                * scalar!((1f64
                    + (layer.gradient.into_inner() / layer.base_temperature.f64())
                        * delta_altitude.f64())
                .powf(exponent * (-g0.f64() / R)))
        })
    }

    pub fn density<UnitMass: MassUnit, UnitLength: LengthUnit>(
        &self,
    ) -> Density<UnitMass, UnitLength> {
        Density::<UnitMass, UnitLength>::from(&kilograms_per_meter3!(
            self.pressure::<Pascals>().f64() / (R * self.temperature::<Kelvin>().f64())
        ))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use absolute_unit::{
        celsius, kelvin, meters, meters_per_second, meters_per_second2, Acceleration, Celsius,
        Density, Kilograms, Length, Meters, Pascals, Pressure, Seconds, Temperature, Velocity,
    };
    use approx::assert_abs_diff_eq;

    #[allow(unused)]
    struct TestCase {
        geometric_height: Length<Meters>,
        geopotential_height: Length<Meters>,
        temperature: Temperature<Kelvin>,
        temperature_in_celsius: Temperature<Celsius>,
        pressure: Pressure<Pascals>,
        density: Density<Kilograms, Meters>,
        grav_accel: Acceleration<Meters, Seconds>,
        speed_of_sound: Velocity<Meters, Seconds>,
        dynamic_viscosity: Scalar,    // TODO
        kinematic_viscosity: Scalar,  // TODO
        thermal_conductivity: Scalar, // TODO
        pressure_scale_height: Scalar,
        specific_weight: Scalar, // TODO
        number_density: Scalar,  // TODO
        mean_particle_speed: Velocity<Meters, Seconds>,
        collision_frequency: Scalar,
        mean_free_path: Length<Meters>,
    }

    static TEST_CASES: Lazy<[TestCase; 21]> = Lazy::new(|| {
        [
            TestCase {
                geometric_height: meters!(-5000f64),
                geopotential_height: meters!(-5004f64),
                temperature: kelvin!(320.676f64),
                temperature_in_celsius: celsius!(47.526f64),
                pressure: pascals!(1.77762e5f64),
                density: kilograms_per_meter3!(1.93113f64),
                grav_accel: meters_per_second2!(9.8221f64),
                speed_of_sound: meters_per_second!(358.986f64),
                dynamic_viscosity: scalar!(1.9422e-5f64),
                kinematic_viscosity: scalar!(1.0058e-5f64),
                thermal_conductivity: scalar!(2.7861e-2f64),
                pressure_scale_height: scalar!(9371.8f64),
                specific_weight: scalar!(1.8968e1f64),
                number_density: scalar!(4.0154e25f64),
                mean_particle_speed: meters_per_second!(484.15f64),
                collision_frequency: scalar!(1.1507e10f64),
                mean_free_path: meters!(4.2075e-8f64),
            },
            TestCase {
                geometric_height: meters!(-4996f64),
                geopotential_height: meters!(-5000f64),
                temperature: kelvin!(320.650f64),
                temperature_in_celsius: celsius!(47.500f64),
                pressure: pascals!(1.77687e5f64),
                density: kilograms_per_meter3!(1.93047f64),
                grav_accel: meters_per_second2!(9.8221f64),
                speed_of_sound: meters_per_second!(358.972f64),
                dynamic_viscosity: scalar!(1.9421e-5f64),
                kinematic_viscosity: scalar!(1.0060e-5f64),
                thermal_conductivity: scalar!(2.7859e-2f64),
                pressure_scale_height: scalar!(9371.1f64),
                specific_weight: scalar!(1.8961e1f64),
                number_density: scalar!(4.0140e25f64),
                mean_particle_speed: meters_per_second!(484.14f64),
                collision_frequency: scalar!(1.1503e10f64),
                mean_free_path: meters!(4.2089e-8f64),
            },
            TestCase {
                geometric_height: meters!(-2500f64),
                geopotential_height: meters!(-2501f64),
                temperature: kelvin!(304.406f64),
                temperature_in_celsius: celsius!(31.265f64),
                pressure: pascals!(1.35205e5f64),
                density: kilograms_per_meter3!(1.54731f64),
                grav_accel: meters_per_second2!(9.8144f64),
                speed_of_sound: meters_per_second!(349.761f64),
                dynamic_viscosity: scalar!(1.8668e-5f64),
                kinematic_viscosity: scalar!(1.2065e-5f64),
                thermal_conductivity: scalar!(2.6611e-2f64),
                pressure_scale_height: scalar!(8903.3f64),
                specific_weight: scalar!(1.5186e1f64),
                number_density: scalar!(3.2173e25f64),
                mean_particle_speed: meters_per_second!(471.71f64),
                collision_frequency: scalar!(8.9830e9f64),
                mean_free_path: meters!(5.2512e-8f64),
            },
            TestCase {
                geometric_height: meters!(0f64),
                geopotential_height: meters!(0f64),
                temperature: kelvin!(288.15f64),
                temperature_in_celsius: celsius!(15f64),
                pressure: pascals!(1.01325e5f64),
                density: kilograms_per_meter3!(1.225f64),
                grav_accel: meters_per_second2!(9.80665f64),
                speed_of_sound: meters_per_second!(340.294f64),
                dynamic_viscosity: scalar!(1.7894e-5f64),
                kinematic_viscosity: scalar!(1.4607e-5f64),
                thermal_conductivity: scalar!(2.5343e-2f64),
                pressure_scale_height: scalar!(8434.5f64),
                specific_weight: scalar!(1.2013e1f64),
                number_density: scalar!(2.5471e25f64),
                mean_particle_speed: meters_per_second!(458.94f64),
                collision_frequency: scalar!(6.9193e9f64),
                mean_free_path: meters!(6.6328e-8f64),
            },
            TestCase {
                geometric_height: meters!(1000f64),
                geopotential_height: meters!(1000f64),
                temperature: kelvin!(281.651f64),
                temperature_in_celsius: celsius!(8.501f64),
                pressure: pascals!(8.98763e4f64),
                density: kilograms_per_meter3!(1.11166f64),
                grav_accel: meters_per_second2!(9.8036f64),
                speed_of_sound: meters_per_second!(336.435f64),
                dynamic_viscosity: scalar!(1.7579e-5f64),
                kinematic_viscosity: scalar!(1.5813e-5f64),
                thermal_conductivity: scalar!(2.4830e-2f64),
                pressure_scale_height: scalar!(8246.9f64),
                specific_weight: scalar!(1.0898e1f64),
                number_density: scalar!(2.3115e25f64),
                mean_particle_speed: meters_per_second!(453.74f64),
                collision_frequency: scalar!(6.2079e9f64),
                mean_free_path: meters!(7.3090e-8f64),
            },
            TestCase {
                geometric_height: meters!(2000f64),
                geopotential_height: meters!(1999f64),
                temperature: kelvin!(275.154f64),
                temperature_in_celsius: celsius!(2.004f64),
                pressure: pascals!(7.95014e4f64),
                density: kilograms_per_meter3!(1.00655f64),
                grav_accel: meters_per_second2!(9.8005f64),
                speed_of_sound: meters_per_second!(332.532f64),
                dynamic_viscosity: scalar!(1.7260e-5f64),
                kinematic_viscosity: scalar!(1.7147e-5f64),
                thermal_conductivity: scalar!(2.4314e-2f64),
                pressure_scale_height: scalar!(8059.2f64),
                specific_weight: scalar!(9.8647f64),
                number_density: scalar!(2.0929e25f64),
                mean_particle_speed: meters_per_second!(448.48f64),
                collision_frequency: scalar!(5.5558e9f64),
                mean_free_path: meters!(8.0723e-8f64),
            },
            TestCase {
                geometric_height: meters!(11_000f64),
                geopotential_height: meters!(10981f64),
                temperature: kelvin!(216.774f64),
                temperature_in_celsius: celsius!(-56.376f64),
                pressure: pascals!(2.26999e4f64),
                density: kilograms_per_meter3!(3.64801e-1f64),
                grav_accel: meters_per_second2!(9.7728f64),
                speed_of_sound: meters_per_second!(295.154f64),
                dynamic_viscosity: scalar!(1.4223e-5f64),
                kinematic_viscosity: scalar!(3.8988e-5f64),
                thermal_conductivity: scalar!(1.9528e-2f64),
                pressure_scale_height: scalar!(6367.2f64),
                specific_weight: scalar!(3.5651f64),
                number_density: scalar!(7.5853e24f64),
                mean_particle_speed: meters_per_second!(398.07f64),
                collision_frequency: scalar!(1.7872e9f64),
                mean_free_path: meters!(2.2273e-7f64),
            },
            TestCase {
                geometric_height: meters!(11_019f64),
                geopotential_height: meters!(11000f64),
                temperature: kelvin!(216.650f64),
                temperature_in_celsius: celsius!(-56.500f64),
                pressure: pascals!(2.26320e4f64),
                density: kilograms_per_meter3!(3.63918e-1f64),
                grav_accel: meters_per_second2!(9.7727f64),
                speed_of_sound: meters_per_second!(295.069f64),
                dynamic_viscosity: scalar!(1.4216e-5f64),
                kinematic_viscosity: scalar!(3.9064e-5f64),
                thermal_conductivity: scalar!(1.9518e-2f64),
                pressure_scale_height: scalar!(6363.6f64),
                specific_weight: scalar!(3.5565f64),
                number_density: scalar!(7.5669e24f64),
                mean_particle_speed: meters_per_second!(397.95f64),
                collision_frequency: scalar!(1.7824e9f64),
                mean_free_path: meters!(2.2327e-7f64),
            },
            TestCase {
                geometric_height: meters!(15_000f64),
                geopotential_height: meters!(14965f64),
                temperature: kelvin!(216.650f64),
                temperature_in_celsius: celsius!(-56.500f64),
                pressure: pascals!(1.21118e4f64),
                density: kilograms_per_meter3!(1.94755e-1f64),
                grav_accel: meters_per_second2!(9.7605f64),
                speed_of_sound: meters_per_second!(295.069f64),
                dynamic_viscosity: scalar!(1.4216e-5f64),
                kinematic_viscosity: scalar!(7.2995e-5f64),
                thermal_conductivity: scalar!(1.9518e-2f64),
                pressure_scale_height: scalar!(6371.6f64),
                specific_weight: scalar!(1.9009f64),
                number_density: scalar!(4.0495e24f64),
                mean_particle_speed: meters_per_second!(397.95f64),
                collision_frequency: scalar!(9.5386e8f64),
                mean_free_path: meters!(4.1720e-7f64),
            },
            TestCase {
                geometric_height: meters!(20_000f64),
                geopotential_height: meters!(19937f64),
                temperature: kelvin!(216.650f64),
                temperature_in_celsius: celsius!(-56.500f64),
                pressure: pascals!(5.52929e3f64),
                density: kilograms_per_meter3!(8.89097e-2f64),
                grav_accel: meters_per_second2!(9.7452f64),
                speed_of_sound: meters_per_second!(295.069f64),
                dynamic_viscosity: scalar!(1.4216e-5f64),
                kinematic_viscosity: scalar!(1.5989e-4f64),
                thermal_conductivity: scalar!(1.9518e-2f64),
                pressure_scale_height: scalar!(6381.6f64),
                specific_weight: scalar!(8.6645e-1f64),
                number_density: scalar!(1.8487e24f64),
                mean_particle_speed: meters_per_second!(397.95f64),
                collision_frequency: scalar!(4.3546e8f64),
                mean_free_path: meters!(9.1387e-7f64),
            },
            TestCase {
                geometric_height: meters!(20_063f64),
                geopotential_height: meters!(20000f64),
                temperature: kelvin!(216.650f64),
                temperature_in_celsius: celsius!(-56.500f64),
                pressure: pascals!(5.47487e3f64),
                density: kilograms_per_meter3!(8.80345e-2f64),
                grav_accel: meters_per_second2!(9.7450f64),
                speed_of_sound: meters_per_second!(295.069f64),
                dynamic_viscosity: scalar!(1.4216e-5f64),
                kinematic_viscosity: scalar!(1.6148e-4f64),
                thermal_conductivity: scalar!(1.9518e-2f64),
                pressure_scale_height: scalar!(6381.7f64),
                specific_weight: scalar!(8.5790e-1f64),
                number_density: scalar!(1.8305e24f64),
                mean_particle_speed: meters_per_second!(397.95f64),
                collision_frequency: scalar!(4.3117e8f64),
                mean_free_path: meters!(9.2295e-7f64),
            },
            TestCase {
                geometric_height: meters!(25_000f64),
                geopotential_height: meters!(24902f64),
                temperature: kelvin!(221.552f64),
                temperature_in_celsius: celsius!(-51.598f64),
                pressure: pascals!(2.54921e3f64),
                density: kilograms_per_meter3!(4.00837e-2f64),
                grav_accel: meters_per_second2!(9.7300f64),
                speed_of_sound: meters_per_second!(298.389f64),
                dynamic_viscosity: scalar!(1.4484e-5f64),
                kinematic_viscosity: scalar!(3.6135e-4f64),
                thermal_conductivity: scalar!(1.9930e-2f64),
                pressure_scale_height: scalar!(6536.2f64),
                specific_weight: scalar!(3.9001e-1f64),
                number_density: scalar!(8.3346e23f64),
                mean_particle_speed: meters_per_second!(402.43f64),
                collision_frequency: scalar!(1.9853e8f64),
                mean_free_path: meters!(2.0270e-6f64),
            },
            TestCase {
                geometric_height: meters!(32_162f64),
                geopotential_height: meters!(32000f64),
                temperature: kelvin!(228.650f64),
                temperature_in_celsius: celsius!(-44.500f64),
                pressure: pascals!(8.68014e2f64),
                density: kilograms_per_meter3!(1.32249e-2f64),
                grav_accel: meters_per_second2!(9.7082f64),
                speed_of_sound: meters_per_second!(303.131f64),
                dynamic_viscosity: scalar!(1.4868e-5f64),
                kinematic_viscosity: scalar!(1.1242e-3f64),
                thermal_conductivity: scalar!(2.0523e-2f64),
                pressure_scale_height: scalar!(6760.8f64),
                specific_weight: scalar!(1.2839e-1f64),
                number_density: scalar!(2.7499e23f64),
                mean_particle_speed: meters_per_second!(408.82f64),
                collision_frequency: scalar!(6.6542e7f64),
                mean_free_path: meters!(6.1438e-6f64),
            },
            TestCase {
                geometric_height: meters!(41_266f64),
                geopotential_height: meters!(41000f64),
                temperature: kelvin!(253.850f64),
                temperature_in_celsius: celsius!(-19.300f64),
                pressure: pascals!(2.42394e2f64),
                density: kilograms_per_meter3!(3.32646e-3f64),
                grav_accel: meters_per_second2!(9.6806f64),
                speed_of_sound: meters_per_second!(319.399f64),
                dynamic_viscosity: scalar!(1.6189e-5f64),
                kinematic_viscosity: scalar!(4.8668e-3f64),
                thermal_conductivity: scalar!(2.2599e-2f64),
                pressure_scale_height: scalar!(7527.3f64),
                specific_weight: scalar!(3.2202e-2f64),
                number_density: scalar!(6.9167e22f64),
                mean_particle_speed: meters_per_second!(430.76f64),
                collision_frequency: scalar!(1.7636e7f64),
                mean_free_path: meters!(2.4426e-5f64),
            },
            TestCase {
                geometric_height: meters!(47_350f64),
                geopotential_height: meters!(47000f64),
                temperature: kelvin!(270.650f64),
                temperature_in_celsius: celsius!(-2.500f64),
                pressure: pascals!(1.10906e2f64),
                density: kilograms_per_meter3!(1.42752e-3f64),
                grav_accel: meters_per_second2!(9.6622f64),
                speed_of_sound: meters_per_second!(329.799f64),
                dynamic_viscosity: scalar!(1.7037e-5f64),
                kinematic_viscosity: scalar!(1.1934e-2f64),
                thermal_conductivity: scalar!(2.3954e-2f64),
                pressure_scale_height: scalar!(8040.7f64),
                specific_weight: scalar!(1.3793e-2f64),
                number_density: scalar!(2.9683e22f64),
                mean_particle_speed: meters_per_second!(444.79f64),
                collision_frequency: scalar!(7.8146e6f64),
                mean_free_path: meters!(5.6918e-5f64),
            },
            TestCase {
                geometric_height: meters!(50_396f64),
                geopotential_height: meters!(50000f64),
                temperature: kelvin!(270.650f64),
                temperature_in_celsius: celsius!(-2.500f64),
                pressure: pascals!(7.59443e1f64),
                density: kilograms_per_meter3!(9.77519e-4f64),
                grav_accel: meters_per_second2!(9.6530f64),
                speed_of_sound: meters_per_second!(329.799f64),
                dynamic_viscosity: scalar!(1.7037e-5f64),
                kinematic_viscosity: scalar!(1.7429e-2f64),
                thermal_conductivity: scalar!(2.3954e-2f64),
                pressure_scale_height: scalar!(8048.4f64),
                specific_weight: scalar!(9.4360e-3f64),
                number_density: scalar!(2.0326e22f64),
                mean_particle_speed: meters_per_second!(444.79f64),
                collision_frequency: scalar!(5.3512e6f64),
                mean_free_path: meters!(8.3120e-5f64),
            },
            TestCase {
                geometric_height: meters!(51_412f64),
                geopotential_height: meters!(51000f64),
                temperature: kelvin!(270.650f64),
                temperature_in_celsius: celsius!(-2.500f64),
                pressure: pascals!(6.69384e1f64),
                density: kilograms_per_meter3!(8.61600e-4f64),
                grav_accel: meters_per_second2!(9.6499f64),
                speed_of_sound: meters_per_second!(329.799f64),
                dynamic_viscosity: scalar!(1.7037e-5f64),
                kinematic_viscosity: scalar!(1.9773e-2f64),
                thermal_conductivity: scalar!(2.3954e-2f64),
                pressure_scale_height: scalar!(8050.9f64),
                specific_weight: scalar!(8.3144e-3f64),
                number_density: scalar!(1.7915e22f64),
                mean_particle_speed: meters_per_second!(444.79f64),
                collision_frequency: scalar!(4.7166e6f64),
                mean_free_path: meters!(9.4303e-5f64),
            },
            TestCase {
                geometric_height: meters!(61_591f64),
                geopotential_height: meters!(61000f64),
                temperature: kelvin!(242.650f64),
                temperature_in_celsius: celsius!(-30.500f64),
                pressure: pascals!(1.76605e1f64),
                density: kilograms_per_meter3!(2.53548e-4f64),
                grav_accel: meters_per_second2!(9.6193f64),
                speed_of_sound: meters_per_second!(312.274f64),
                dynamic_viscosity: scalar!(1.5610e-5f64),
                kinematic_viscosity: scalar!(6.1565e-2f64),
                thermal_conductivity: scalar!(2.1683e-2f64),
                pressure_scale_height: scalar!(7241.0f64),
                specific_weight: scalar!(2.4390e-3f64),
                number_density: scalar!(5.2720e21f64),
                mean_particle_speed: meters_per_second!(421.15f64),
                collision_frequency: scalar!(1.3142e6f64),
                mean_free_path: meters!(3.2046e-4f64),
            },
            TestCase {
                geometric_height: meters!(71_802f64),
                geopotential_height: meters!(71000f64),
                temperature: kelvin!(214.650f64),
                temperature_in_celsius: celsius!(-58.500f64),
                pressure: pascals!(3.95639e0f64),
                density: kilograms_per_meter3!(6.42105e-5f64),
                grav_accel: meters_per_second2!(9.5888f64),
                speed_of_sound: meters_per_second!(293.704f64),
                dynamic_viscosity: scalar!(1.4106e-5f64),
                kinematic_viscosity: scalar!(2.1968e-1f64),
                thermal_conductivity: scalar!(1.9349e-2f64),
                pressure_scale_height: scalar!(6425.8f64),
                specific_weight: scalar!(6.1570e-4f64),
                number_density: scalar!(1.3351e21f64),
                mean_particle_speed: meters_per_second!(396.11f64),
                collision_frequency: scalar!(3.1303e5f64),
                mean_free_path: meters!(1.2654e-3f64),
            },
            TestCase {
                geometric_height: meters!(75_895f64),
                geopotential_height: meters!(75000f64),
                temperature: kelvin!(206.650f64),
                temperature_in_celsius: celsius!(-66.500f64),
                pressure: pascals!(2.06790e0f64),
                density: kilograms_per_meter3!(3.48604e-5f64),
                grav_accel: meters_per_second2!(9.5766f64),
                speed_of_sound: meters_per_second!(288.179f64),
                dynamic_viscosity: scalar!(1.3661e-5f64),
                kinematic_viscosity: scalar!(3.9188e-1f64),
                thermal_conductivity: scalar!(1.8671e-2f64),
                pressure_scale_height: scalar!(6194.2f64),
                specific_weight: scalar!(3.3384e-4f64),
                number_density: scalar!(7.2485e20f64),
                mean_particle_speed: meters_per_second!(388.66f64),
                collision_frequency: scalar!(1.6675e5f64),
                mean_free_path: meters!(2.3308e-3f64),
            },
            TestCase {
                geometric_height: meters!(81_020f64),
                geopotential_height: meters!(80000f64),
                temperature: kelvin!(196.650f64),
                temperature_in_celsius: celsius!(-76.500f64),
                pressure: pascals!(8.86272e-1f64),
                density: kilograms_per_meter3!(1.57004e-5f64),
                grav_accel: meters_per_second2!(9.5614f64),
                speed_of_sound: meters_per_second!(281.120f64),
                dynamic_viscosity: scalar!(1.3095e-5f64),
                kinematic_viscosity: scalar!(8.3402e-1f64),
                thermal_conductivity: scalar!(1.7817e-2f64),
                pressure_scale_height: scalar!(5903.9f64),
                specific_weight: scalar!(1.5012e-4f64),
                number_density: scalar!(3.2646e20f64),
                mean_particle_speed: meters_per_second!(379.14f64),
                collision_frequency: scalar!(7.3262e4f64),
                mean_free_path: meters!(5.1751e-3f64),
            },
        ]
    });

    #[test]
    fn test_atmosphere_model() {
        for case in TEST_CASES.iter() {
            let atmos = StandardAtmosphere::at_altitude(case.geometric_height);
            assert_abs_diff_eq!(
                case.temperature,
                atmos.temperature::<Kelvin>(),
                epsilon = 0.001
            );
            assert_abs_diff_eq!(case.pressure, atmos.pressure::<Pascals>(), epsilon = 10.0);
            assert_abs_diff_eq!(
                case.density,
                atmos.density::<Kilograms, Meters>(),
                epsilon = 0.001
            );
        }
    }

    #[test]
    fn test_atmosphere_feet() {
        use absolute_unit::feet;
        for i in 0..9 {
            let altitude = feet!(i * 5000);
            let atmos = StandardAtmosphere::at_altitude(altitude);
            println!(
                "{}\t{:0.6}",
                altitude.f64(),
                atmos.density::<Kilograms, Meters>().f64()
            );
        }
    }
}
