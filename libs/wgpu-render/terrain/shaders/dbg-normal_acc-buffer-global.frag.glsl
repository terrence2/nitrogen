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

layout(location = 0) out vec4 f_color;
layout(location = 0) in vec2 v_tc;
layout(location = 1) in vec3 v_ray;

void
main()
{
    float depth = texture(sampler2D(terrain_deferred_depth, terrain_linear_sampler), v_tc).x;
    if (depth > -1) {
        ivec2 raw_normal = texture(isampler2D(terrain_normal_acc_texture, terrain_linear_sampler), v_tc).xy;
        vec2 flat_normal = raw_normal / 32768.0;
        vec3 local_normal = vec3(
            flat_normal.x,
            sqrt(1.0 - (flat_normal.x * flat_normal.x + flat_normal.y * flat_normal.y)),
            flat_normal.y
        );

        vec4 samp = texture(sampler2D(terrain_deferred_texture, terrain_linear_sampler), v_tc);
        vec2 latlon = samp.xy;
        vec4 r_lon = quat_from_axis_angle(vec3(0, 1, 0), latlon.y);
        vec3 lat_axis = quat_rotate(r_lon, vec3(1, 0, 0)).xyz;
        vec4 r_lat = quat_from_axis_angle(lat_axis, -latlon.x);
        vec3 global_normal = quat_rotate(r_lat, quat_rotate(r_lon, local_normal).xyz).xyz;
        f_color = vec4((global_normal + 1) / 2, 1);
    } else {
        f_color = vec4(0, 0, 0, 1);
    }
}
