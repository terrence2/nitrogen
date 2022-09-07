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
layout(location = 0) in vec3 v_ray_world;
layout(location = 1) in vec2 v_fullscreen;
layout(location = 2) in vec2 v_tc_idx;

void
main()
{
    if (v_ray_world.x > 1000.0) {
        f_color = vec4(v_ray_world, 0);
    }
    if (v_fullscreen.x > 1000.0) {
        f_color = vec4(v_fullscreen, 0, 0);
    }

    vec4 texel = texelFetch(sampler2D(terrain_deferred_texture, terrain_linear_sampler), ivec2(v_tc_idx), 0);
    float depth = texelFetch(sampler2D(terrain_deferred_depth, terrain_linear_sampler), ivec2(v_tc_idx), 0).x;
    if (depth > -1) {
        vec2 normal_wrld_xz = normalize(texel.zw);
        vec3 normal_wrld = vec3(
            normal_wrld_xz.x,
            sqrt(1.0 - (normal_wrld_xz.x * normal_wrld_xz.x + normal_wrld_xz.y * normal_wrld_xz.y)),
            normal_wrld_xz.y
        );
        f_color = vec4(normalize(normal_wrld), 1);
    } else {
        f_color = vec4(0, 0, 0, 1);
    }
}
