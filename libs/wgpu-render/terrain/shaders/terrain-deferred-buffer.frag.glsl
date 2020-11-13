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
#include <wgpu-buffer/global_data/include/global_data.glsl>
#include <wgpu-buffer/terrain_geo/include/layout_composite.glsl>
#include <wgpu-buffer/atmosphere/include/global.glsl>
#include <wgpu-buffer/atmosphere/include/descriptorset.glsl>
#include <wgpu-buffer/atmosphere/include/library.glsl>

layout(location = 0) out vec4 f_color;
layout(location = 1) in vec2 v_tc;
layout(location = 2) in vec3 v_ray;

const float EXPOSURE = MAX_LUMINOUS_EFFICACY * 0.0001;

void
main()
{
    vec3 view_direction = normalize(v_ray);
    vec3 world_camera_km = camera_position_km.xyz;
    vec3 sun_direction = sun_direction.xyz;

    float depth_sample = texture(sampler2D(terrain_deferred_depth, terrain_linear_sampler), v_tc).x;
    float terrain_depth_m = 1.0 / (depth_sample * 0.5);
    if (depth_sample != -1) {
        vec3 eye_intersect_km = view_direction * (terrain_depth_m / 1000.0);
        //vec3 world_intersect_km = eye_intersect_km + world_camera_km;
        vec3 world_intersect_km = (camera_inverse_view_km * vec4(eye_intersect_km, 1)).xyz;
        //vec3 world_intersect_km = (vec4(eye_intersect_km, 1) * m4_geocenter_inverse_view()).xyz;
        //vec3 world_intersect_km = vec4(eye_intersect_km, 1).xyz;
        float height = length(world_intersect_km) - EARTH_RADIUS_KM;

        float v = height / 8000.0;
        f_color = vec4(v, v, v, 1);

        /*
        vec4 _local_normal = texture(sampler2D(terrain_normal_acc_texture, terrain_linear_sampler), v_tc);
        //vec4 color = texture(sampler2D(terrain_color_acc_texture, terrain_linear_sampler), v_tc);

        vec3 eye_intersect_km = view_direction * (terrain_depth_m / 1000.0);
        vec3 world_intersect_km = (m4_geocenter_inverse_view() * vec4(eye_intersect_km, 1)).xyz;
        //vec3 world_intersect_km = (vec4(eye_intersect_km, 1) * m4_geocenter_inverse_view()).xyz;
        vec3 normal = normalize(world_intersect_km);

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
        vec3 ground_radiance = vec3(atmosphere.ground_albedo) * (1.0 / PI) * (
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
            world_camera_km,
            world_intersect_km,
            sun_direction,
            transmittance,
            in_scatter
        );
        ground_radiance = ground_radiance * transmittance + in_scatter;

        vec3 radiance = ground_radiance;
        vec3 color = pow(
            vec3(1.0) - exp(-radiance / vec3(atmosphere.whitepoint) * EXPOSURE),
            vec3(1.0 / 2.2)
        );

        f_color = vec4(sky_irradiance, 1);
        //f_color = vec4(ground_radiance, 1);
        //f_color = vec4(color, 1);
        //f_color = vec4(0, 1, 0, 1);
        //f_color = vec4(local_normal.xyz, 1);
        */
    } else {
        // Sky and stars
        f_color = vec4(0, 0, 0, 1);
    }
}
