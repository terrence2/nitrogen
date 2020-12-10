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
#version 450
#include <wgpu-render/shader_shared/include/consts.glsl>
#include <wgpu-render/shader_shared/include/quaternion.glsl>
#include <wgpu-buffer/global_data/include/global_data.glsl>
#include <wgpu-buffer/terrain_geo/include/layout_composite.glsl>
#include <wgpu-buffer/atmosphere/include/global.glsl>
#include <wgpu-buffer/atmosphere/include/descriptorset.glsl>
#include <wgpu-buffer/atmosphere/include/library.glsl>
#include <wgpu-buffer/stars/include/stars.glsl>

layout(location = 0) out vec4 f_color;
layout(location = 0) in vec2 v_tc;
layout(location = 1) in vec3 v_ray_world;
layout(location = 2) in vec2 v_ndc;

// FIXME: upload exposure on globals and let us tweak it under a brightness setting.
//const float EXPOSURE = MAX_LUMINOUS_EFFICACY * 0.0001;

vec4
ndc_to_world_km(vec3 ndc)
{
    float w = camera_z_near_km / ndc.z;
    vec4 eye_km = vec4(
        ndc.x * w / camera_aspect_ratio,
        ndc.y * w,
        -w,
        1
    );
    return camera_inverse_view_km * eye_km;
}

void
main()
{
    vec3 world_view_direction = normalize(v_ray_world);
    vec3 world_position_km = camera_position_km.xyz;
    vec3 sun_direction = sun_direction.xyz;

    // Compute ground alpha and radiance.
    float ground_alpha = 0.0;
    vec3 ground_radiance = vec3(0);
    float depth_sample = texture(sampler2D(terrain_deferred_depth, terrain_linear_sampler), v_tc).x;
    if (depth_sample > -1) {
        ground_alpha = 1.0;

        vec3 world_intersect_km = ndc_to_world_km(vec3(v_ndc, depth_sample)).xyz;

        ivec2 raw_normal = texture(isampler2D(terrain_normal_acc_texture, terrain_linear_sampler), v_tc).xy;
        vec2 flat_normal = raw_normal / 32768.0;
        vec3 local_normal = vec3(
            flat_normal.x,
            sqrt(1.0 - (flat_normal.x * flat_normal.x + flat_normal.y * flat_normal.y)),
            flat_normal.y
        );

        vec3 ground_albedo = texture(sampler2D(terrain_color_acc_texture, terrain_linear_sampler), v_tc).xyz;

        vec2 latlon = texture(sampler2D(terrain_deferred_texture, terrain_linear_sampler), v_tc).xy;
        vec4 r_lon = quat_from_axis_angle(vec3(0, 1, 0), latlon.y);
        vec3 lat_axis = quat_rotate(r_lon, vec3(1, 0, 0)).xyz;
        vec4 r_lat = quat_from_axis_angle(lat_axis, PI / 2.0 - latlon.x);
        vec3 normal = quat_rotate(r_lat, quat_rotate(r_lon, local_normal).xyz).xyz;

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
            world_intersect_km,
            normal,
            sun_direction,
            sun_irradiance,
            sky_irradiance
        );
        // FIXME: this ground albedo scaling factor is arbitrary and dependent on our source material
        ground_radiance = ground_albedo * 2 * (
            // Todo: properer shadow maps so we can get sun visibility
            sun_irradiance * get_sun_visibility(world_intersect_km, sun_direction) +
            sky_irradiance * get_sky_visibility(world_intersect_km)
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
            world_position_km,
            world_intersect_km,
            sun_direction,
            transmittance,
            in_scatter
        );
        ground_radiance = ground_radiance * transmittance + in_scatter;
    }

    // Sky and stars
    vec3 transmittance;
    vec3 sky_radiance = vec3(0);
    get_sky_radiance(
        atmosphere,
        transmittance_texture,
        transmittance_sampler,
        scattering_texture,
        scattering_sampler,
        single_mie_scattering_texture,
        single_mie_scattering_sampler,
        world_position_km,
        world_view_direction,
        sun_direction,
        transmittance,
        sky_radiance);

    if (dot(world_view_direction, sun_direction) > cos(atmosphere.sun_angular_radius)) {
        vec3 sun_lums = get_solar_luminance(
            vec3(atmosphere.sun_irradiance),
            atmosphere.sun_angular_radius,
            atmosphere.sun_spectral_radiance_to_luminance
        );
        sky_radiance = transmittance * sun_lums;
    }

    vec3 star_radiance;
    float star_alpha = 0.5;
    show_stars(world_view_direction, star_radiance, star_alpha);

    vec3 radiance = sky_radiance + star_radiance * star_alpha;
    radiance = mix(radiance, ground_radiance, ground_alpha);

    vec3 color = pow(
        vec3(1.0) - exp(-radiance / vec3(atmosphere.whitepoint) * MAX_LUMINOUS_EFFICACY * camera_exposure),
        vec3(1.0 / tone_gamma)
    );

    f_color = vec4(color, 1);
}
