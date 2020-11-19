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

layout(location = 0) out vec4 f_color;
layout(location = 0) in vec2 v_tc;
layout(location = 1) in vec3 v_ray_world;
layout(location = 2) in vec2 v_ndc;

const float EXPOSURE = MAX_LUMINOUS_EFFICACY * 0.0001;

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

    float depth_sample = texture(sampler2D(terrain_deferred_depth, terrain_linear_sampler), v_tc).x;
    if (depth_sample > -1) {
        /* FULLY WORKING (in meters)
        float z = -camera_z_near_m / depth_sample;
        float w = -z;
        float x = v_ndc.x * w / camera_aspect_ratio;
        float y = v_ndc.y * w;
        vec3 eyep_m = vec3(x, y, z);
        vec3 wrldp_m = (camera_inverse_view_m * vec4(eyep_m, 1)).xyz;
        float height = length(wrldp_m);
        float v = (height - EARTH_RADIUS_M) / EVEREST_HEIGHT_M;
        f_color = vec4(v, v, v, 1);
        */

        /* abstract */
        vec3 world_intersect_km = ndc_to_world_km(vec3(v_ndc, depth_sample)).xyz;

        /* (in km)
        float z = -camera_z_near_km / depth_sample;
        float w = -z;
        float x = v_ndc.x * w / camera_aspect_ratio;
        float y = v_ndc.y * w;
        vec3 eyep_km = vec3(x, y, z);
        vec3 wrldp_km = (camera_inverse_view_km * vec4(eyep_km, 1)).xyz;
        float height = length(wrldp_km);
        float v = (height - EARTH_RADIUS_KM) / EVEREST_HEIGHT_KM;
        f_color = vec4(v, v, v, 1);
        */

        vec2 raw_normal = texture(sampler2D(terrain_normal_acc_texture, terrain_linear_sampler), v_tc).xy;
        vec3 local_normal = vec3(
            raw_normal.x,
            sqrt(1.0 - (raw_normal.x * raw_normal.x + raw_normal.y * raw_normal.y)),
            raw_normal.y
        );
        //vec4 color = texture(sampler2D(terrain_color_acc_texture, terrain_linear_sampler), v_tc);

        vec2 latlon = texture(sampler2D(terrain_deferred_texture, terrain_linear_sampler), v_tc).xy;
        vec4 r_lon = quat_from_axis_angle(vec3(0, 1, 0), latlon.y);
        vec3 lat_axis = quat_mult(r_lon, vec4(1, 0, 0, 1)).xyz;
        vec4 r_lat = quat_from_axis_angle(lat_axis, PI / 2.0 - latlon.x);
        /*
        let r_lon = UnitQuaternion::from_axis_angle(
            &NUnit::new_unchecked(Vector3::new(0f64, 1f64, 0f64)),
            -f64::from(self.target.longitude),
        );
        let r_lat = UnitQuaternion::from_axis_angle(
            &NUnit::new_normalize(r_lon * Vector3::new(1f64, 0f64, 0f64)),
            PI / 2.0 - f64::from(self.target.latitude),
        );
        */
        vec3 normal = quat_mult(r_lat, quat_mult(r_lon, vec4(local_normal, 1))).xyz;

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
            world_position_km,
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

        //f_color = vec4(sky_irradiance, 1);
        //f_color = vec4(ground_radiance, 1);
        f_color = vec4(color, 1);
        //f_color = vec4(0, 1, 0, 1);
        //f_color = vec4(local_normal.xyz, 1);
    } else {
        // Sky and stars
        f_color = vec4(0, 0, 0, 1);
    }
}
