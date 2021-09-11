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

// NOTE: must be included last and must include bind groups for atmosphere & globals.

vec3
radiance_at_point(
    vec3 point_w_km,
    vec3 normal_w,
    vec3 solid_diffuse_color,
    vec3 sun_direction_w,
    vec3 camera_position_w_km,
    vec3 camera_direction_w
) {
    // Get sun and sky irradiance at the ground point and modulate
    // by the ground albedo.
    vec3 sky_irradiance;
    vec3 sun_irradiance;
    get_sun_and_sky_irradiance(
        atmosphere,
        transmittance_texture,
        transmittance_sampler,
        irradiance_texture,
        irradiance_sampler,
        point_w_km,
        normal_w,
        sun_direction_w,
        sun_irradiance,
        sky_irradiance
    );

    // FIXME: this ground albedo scaling factor is arbitrary and dependent on our source material
    vec3 ground_radiance = solid_diffuse_color * 2 * (
        // Todo: properer shadow maps so we can get sun visibility
        sun_irradiance * get_sun_visibility(point_w_km, sun_direction_w) +
        sky_irradiance * get_sky_visibility(point_w_km)
    );

    // Fade the radiance on the ground by the amount of atmosphere
    // between us and that point and brighten by ambient in-scatter
    // to the camera on that path.
    vec3 transmittance;
    vec3 in_scatter;
    get_sky_radiance_to_point(
        atmosphere,
        transmittance_texture,
        transmittance_sampler,
        scattering_texture,
        scattering_sampler,
        single_mie_scattering_texture,
        single_mie_scattering_sampler,
        camera_position_w_km,
        point_w_km,
        camera_direction_w,
        sun_direction_w,
        transmittance,
        in_scatter
    );
    ground_radiance = ground_radiance * transmittance + in_scatter;

    return ground_radiance;
}

vec3 tone_mapping(vec3 radiance) {
    return pow(
        vec3(1.0) - exp(-radiance / vec3(atmosphere.whitepoint) * MAX_LUMINOUS_EFFICACY * camera_exposure),
        vec3(1.0 / tone_gamma)
    );
}
