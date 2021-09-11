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
#include <wgpu-buffer/world/include/world.glsl>

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

vec4 ground_diffuse_color() {
    // Load the accumulated color at the current screen position.
    return texture(sampler2D(terrain_color_acc_texture, terrain_linear_sampler), v_tc);
}

vec3 ground_normal() {
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
    return quat_rotate(r_lat, quat_rotate(r_lon, ground_normal_local).xyz).xyz;
}

void
main()
{
    vec3 camera_direction_w = normalize(v_ray_world);
    vec3 camera_position_w_km = camera_position_km.xyz;
    vec3 sun_direction_w = sun_direction.xyz;

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
        camera_direction_w,
        sun_direction_w,
        transmittance,
        sky_radiance
    );

    // Compute ground alpha and radiance.
    vec4 ground_albedo = vec4(0);
    vec3 ground_radiance = vec3(0);
    float depth_sample = texture(sampler2D(terrain_deferred_depth, terrain_linear_sampler), v_tc).x;
    if (depth_sample > -1) {
        vec3 ground_intersect_w_km = fullscreen_to_world_km(v_fullscreen, depth_sample).xyz;

        // These can be subtracted into the output to check if the inverse transform is working correctly.
        // vec3 fake_view_direction_w = normalize(ground_intersect_w_km - camera_position_w_km);
        // assert(fake_view_direction_w == camera_direction_w);

        ground_albedo = ground_diffuse_color();
        vec3 ground_normal_w = ground_normal();

        ground_radiance = radiance_at_point(
            ground_intersect_w_km,
            ground_normal_w,
            ground_albedo.rgb,
            sun_direction_w,
            camera_position_w_km,
            camera_direction_w
        );
    } else if (dot(camera_direction_w, sun_direction_w) > cos(atmosphere.sun_angular_radius)) {
        vec3 sun_lums = get_solar_luminance(
            vec3(atmosphere.sun_irradiance),
            atmosphere.sun_angular_radius,
            atmosphere.sun_spectral_radiance_to_luminance
        );
        sky_radiance = transmittance * sun_lums;
    }

    vec3 star_radiance;
    float star_alpha = 0.5;
    show_stars(camera_direction_w, star_radiance, star_alpha);

    vec3 radiance = sky_radiance + star_radiance * star_alpha;
    radiance = mix(radiance, ground_radiance, ground_albedo.w);

    vec3 color = tone_mapping(radiance);

    f_color = vec4(color, 1);
}
