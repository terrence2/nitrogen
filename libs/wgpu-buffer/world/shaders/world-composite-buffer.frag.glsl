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
#include <wgpu-buffer/shader_shared/include/consts.glsl>
#include <wgpu-buffer/shader_shared/include/quaternion.glsl>
#include <wgpu-buffer/global_data/include/global_data.glsl>
#include <wgpu-buffer/terrain/include/layout_composite.glsl>
#include <wgpu-buffer/atmosphere/include/global.glsl>
#include <wgpu-buffer/atmosphere/include/descriptorset.glsl>
#include <wgpu-buffer/atmosphere/include/library.glsl>
#include <wgpu-buffer/stars/include/stars.glsl>

layout(location = 0) out vec4 f_color;
layout(location = 0) in vec2 v_tc;
layout(location = 1) in vec3 v_ray_world;
layout(location = 2) in vec2 v_fullscreen;

vec4
fullscreen_to_world_km(vec2 v_fullscreen, float z_ndc)
{
    // Use our depth coordinate in ndc to find w. We know the z component of clip is always the near depth
    // because of how we constructed our perspective matrix. We multiplied by perspective to get clip and
    // assigned to gl_Position. Internally this will have produced a fragment in ndc by dividing by w, which
    // is more or less -z, again because of how we constructed our perspective matrix.
    // z{ndc} = z{clip} / w
    // w = z{clip} / z{ndc}
    float w = camera_z_near_m / z_ndc;

    // Now that we have w, we can reconstruct the clip space we passed into gl_Position. The fullscreen position
    // we pass through the vertex shader is technically in clip space, but because we also set w to 1 on position
    // that is equal to the ndc that would have been present for the z ndc coordinate we looked up in the depth
    // buffer at the equivalent texture coordinate. Thus we can reverse the fullscreen clip as if it were the
    // terrain's ndc.
    // x{ndc} = x{clip} / w
    // x{clip} = x{ndc} * w
    // z{clip} = z{ndc} * w
    vec4 clip = vec4(
        v_fullscreen.x * w /* camera_aspect_ratio */,
        v_fullscreen.y * w,
        camera_z_near_m,
        w
    );

    // Now that we have the clip that was passed to gl_Position in draw_deferred, we can
    // invert the relevant transforms to get back to world space.
    // V{clip} = M{proj}*M{scale}*M{view}*V{wrld}
    // M{-view}*M{-scale}*M{-proj}*V{clip} = V{wrld}
    float s = 1. / 1000.;
    mat4 inverse_scale = mat4(
        s, 0, 0, 0,
        0, s, 0, 0,
        0, 0, s, 0,
        0, 0, 0, 1
    );
    return camera_inverse_view_km * inverse_scale * camera_inverse_perspective_m * clip;
}

void
main()
{
    vec3 camera_view_direction_w = normalize(v_ray_world);
    vec3 camera_position_w_km = camera_position_km.xyz;
    vec3 sun_direction = sun_direction.xyz;

    // Compute ground alpha and radiance.
    float ground_alpha = 0.0;
    vec3 ground_radiance = vec3(0);

    float depth_sample = texture(sampler2D(terrain_deferred_depth, terrain_linear_sampler), v_tc).x;
    if (depth_sample > -1) {
        ground_alpha = 1.0;

        vec3 ground_intersect_w_km = fullscreen_to_world_km(v_fullscreen, depth_sample).xyz;

        // These can be subtracted into the output to check if the inverse transform is working correctly.
        // vec3 fake_view_direction_w = normalize(ground_intersect_w_km - camera_position_w_km);
        // assert(fake_view_direction_w == camera_view_direction_w);

        // Load the accumulated color at the current screen position.
        vec3 ground_albedo = texture(sampler2D(terrain_color_acc_texture, terrain_linear_sampler), v_tc).xyz;

        // Load the accumulated normal at the current screen position and translate
        // that from the storage format into a local vector assuming up is the y axis.
        ivec2 ground_normal_raw = texture(isampler2D(terrain_normal_acc_texture, terrain_nearest_sampler), v_tc).xy;
        vec2 ground_normal_flat = ground_normal_raw / 32768.0;
        vec3 ground_normal_local = vec3(
            ground_normal_flat.x,
            sqrt(1.0 - (ground_normal_flat.x * ground_normal_flat.x + ground_normal_flat.y * ground_normal_flat.y)),
            ground_normal_flat.y
        );

        // Translate the normal's local coordinates into global coordinates by using the lat/lon.
        vec2 latlon = texture(sampler2D(terrain_deferred_texture, terrain_linear_sampler), v_tc).xy;
        vec4 r_lon = quat_from_axis_angle(vec3(0, 1, 0), latlon.y);
        vec3 lat_axis = quat_rotate(r_lon, vec3(1, 0, 0)).xyz;
        vec4 r_lat = quat_from_axis_angle(lat_axis, PI / 2.0 - latlon.x);
        vec3 ground_normal_w = quat_rotate(r_lat, quat_rotate(r_lon, ground_normal_local).xyz).xyz;

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
            ground_intersect_w_km,
            ground_normal_w,
            sun_direction,
            sun_irradiance,
            sky_irradiance
        );

        // FIXME: this ground albedo scaling factor is arbitrary and dependent on our source material
        ground_radiance = ground_albedo * 2 * (
            // Todo: properer shadow maps so we can get sun visibility
            sun_irradiance * get_sun_visibility(ground_intersect_w_km, sun_direction) +
            sky_irradiance * get_sky_visibility(ground_intersect_w_km)
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
            ground_intersect_w_km,
            camera_view_direction_w,
            sun_direction,
            transmittance,
            in_scatter
        );
        ground_radiance = ground_radiance * transmittance + in_scatter;
    }

    // Sky and stars
    vec3 sky_radiance = vec3(0);
    vec3 transmittance;
    get_sky_radiance(
        atmosphere,
        transmittance_texture,
        transmittance_sampler,
        scattering_texture,
        scattering_sampler,
        single_mie_scattering_texture,
        single_mie_scattering_sampler,
        camera_position_w_km,
        camera_view_direction_w,
        sun_direction,
        transmittance,
        sky_radiance);

    if (depth_sample == -1 &&
        dot(camera_view_direction_w, sun_direction) > cos(atmosphere.sun_angular_radius))
    {
        vec3 sun_lums = get_solar_luminance(
            vec3(atmosphere.sun_irradiance),
            atmosphere.sun_angular_radius,
            atmosphere.sun_spectral_radiance_to_luminance
        );
        sky_radiance = transmittance * sun_lums;
    }

    vec3 star_radiance;
    float star_alpha = 0.5;
    show_stars(camera_view_direction_w, star_radiance, star_alpha);

    vec3 radiance = sky_radiance + star_radiance * star_alpha;
    radiance = mix(radiance, ground_radiance, ground_alpha);

    vec3 color = pow(
        vec3(1.0) - exp(-radiance / vec3(atmosphere.whitepoint) * MAX_LUMINOUS_EFFICACY * camera_exposure),
        vec3(1.0 / tone_gamma)
    );

    f_color = vec4(color, 1);
}
