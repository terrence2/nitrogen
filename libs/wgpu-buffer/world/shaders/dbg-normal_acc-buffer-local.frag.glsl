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
#include <wgpu-buffer/global_data/include/global_data.glsl>
#include <wgpu-buffer/terrain/include/layout_composite.glsl>

layout(location = 0) out vec4 f_color;
layout(location = 0) in vec2 v_tc;
layout(location = 1) in vec3 v_ray_world;
layout(location = 2) in vec2 v_fullscreen;
layout(location = 3) in vec2 v_tc_idx;

void
main()
{
    float depth = texelFetch(sampler2D(terrain_deferred_depth, terrain_linear_sampler), ivec2(v_tc_idx), 0).x;
    ivec2 raw_normal = texelFetch(isampler2D(terrain_normal_acc_texture, terrain_nearest_sampler), ivec2(v_tc_idx), 0).xy;
    if (depth > -1) {
        vec2 flat_normal = raw_normal / 32768.0;
        vec3 local_normal = normalize(vec3(
            flat_normal.x,
            sqrt(1.0 - (flat_normal.x * flat_normal.x + flat_normal.y * flat_normal.y)),
            flat_normal.y
        ));
        f_color = vec4((local_normal + 1) / 2, 1);
    } else {
        f_color = vec4(0, 0, 0, 1);
    }
}
