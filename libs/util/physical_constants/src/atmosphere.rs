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
use absolute_unit::{
    feet, psf, rankine, scalar, slugs_per_foot3, Density, Feet, Length, LengthUnit,
    PoundsSquareFoot, Pressure, Rankine, Slugs, Temperature,
};

pub struct UsStandardAtmosphere;

impl UsStandardAtmosphere {
    pub fn at_altitude<Unit: LengthUnit>(
        altitude: Length<Unit>,
    ) -> (
        Temperature<Rankine>,
        Density<Slugs, Feet>,
        Pressure<PoundsSquareFoot>,
    ) {
        // Temperature, density, and pressure per foot, respectively.
        let altitude_ft = feet!(altitude).f64();
        let (theta, sigma, delta) = if altitude_ft < 36_089. {
            (
                1.0 - altitude_ft / 145_442.,
                (1.0 - altitude_ft / 145_442.).powf(4.255_876),
                (1.0 - altitude_ft / 145_442.).powf(5.255_876),
            )
        } else if altitude_ft < 65_617. {
            (
                0.751_865,
                0.297_076 * ((36_089. - altitude_ft) / 20_806.).exp(),
                0.223_361 * ((36_089. - altitude_ft) / 20_806.).exp(),
            )
        } else if altitude_ft < 104_987. {
            (
                0.682_457 + altitude_ft / 945_374.,
                (0.978_261 + altitude_ft / 659_515.).powf(-35.163_19),
                (0.988_626 + altitude_ft / 652_600.).powf(-34.163_19),
            )
        } else if altitude_ft < 154_199. {
            (
                0.482_561 + altitude_ft / 337_634.,
                (0.857_003 + altitude_ft / 190_115.).powf(-13.201_14),
                (0.898_309 + altitude_ft / 181_373.).powf(-12.201_14),
            )
        } else if altitude_ft < 167_323. {
            (
                0.939_268,
                0.001_165_33 * ((altitude_ft - 154_200.) / -25_992.).exp(),
                0.001_094_56 * ((altitude_ft - 154_200.) / -25_992.).exp(),
            )
        } else if altitude_ft < 232_940. {
            (
                1.434_843 - altitude_ft / 337_634.,
                (0.798_99 - altitude_ft / 606_330.).powf(11.201_14),
                (0.838_263 - altitude_ft / 577_922.).powf(12.201_14),
            )
        } else {
            (0.745, 0.000_052, 0.000_039)
        };

        let sea_level_temperature_rankine = rankine!(518.67);
        let sea_level_density_slug_ft3 = slugs_per_foot3!(0.002_376_89);
        let sea_level_pressure_lbf_ft2 = psf!(2_116.22);

        let temperature_rankine = scalar!(theta) * sea_level_temperature_rankine;
        let density = scalar!(sigma) * sea_level_density_slug_ft3;
        let pressure = scalar!(delta) * sea_level_pressure_lbf_ft2;

        let _viscosity = 0.022_696_8 * temperature_rankine.f64().powf(1.5)
            / (temperature_rankine.f64() + 198.72)
            / 1_000_000.0;
        let _speed_of_sound = (1.4 * 1716.56 * temperature_rankine.f64()).sqrt();

        (temperature_rankine, density, pressure)
    }
}
