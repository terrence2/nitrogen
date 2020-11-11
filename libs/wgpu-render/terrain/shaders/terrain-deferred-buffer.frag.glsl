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

layout(location = 0) out vec4 f_color;
layout(location = 1) in vec2 v_tc;
layout(location = 2) in vec3 v_ray;

void
main()
{
    vec3 view = normalize(v_ray);
    vec3 camera = camera_position_earth_km().xyz;
    //vec3 sun_direction = sun_direction.xyz;

    float terrain_depth = texture(sampler2D(terrain_deferred_depth, terrain_linear_sampler), v_tc).x;
    if (terrain_depth > -1) {
        vec4 local_normal = texture(sampler2D(terrain_normal_acc_texture, terrain_linear_sampler), v_tc);
        //vec4 color = texture(sampler2D(terrain_color_acc_texture, terrain_linear_sampler), v_tc);

        f_color = vec4(local_normal.xyz, 1);
    } else {
        // Sky and stars
        f_color = vec4(0, 0, 0, 1);
    }
}
