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
#include <wgpu-buffer/terrain_geo/include/layout_composite.glsl>

layout(location = 0) out vec4 f_color;
layout(location = 0) in vec2 v_tc;
layout(location = 1) in vec3 v_ray;

void
main()
{
    vec4 texel = texture(sampler2D(terrain_deferred_depth, terrain_linear_sampler), v_tc);

    float zp = texel.x;

    // Invert the depth projection to get to the real eye space depth.
    float z = 1 / (zp * 0.5);

    // Cast back into 0->1 on an earth scale.
    z = z / EARTH_RADIUS_M;

    // Enhance middle distance depths so we can clearly see what's going on.
    z = sqrt(sqrt(z));

    f_color = vec4(z, z, z, 1);
}