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
#include <wgpu-buffer/global_data/include/global_data.glsl>
#include <wgpu-buffer/terrain_geo/include/terrain_geo.glsl>

layout(location = 0) in vec2 position;
layout(location = 0) out vec2 v_tc;
layout(location = 1) out vec3 v_ray_world;
layout(location = 2) out vec2 v_ndc;

void main() {
    gl_Position = vec4(position, 0.0, 1.0);
    v_ray_world = raymarching_view_ray(position);
    v_ndc = position;

    // map -1->1 screen coord to 0->1 u/v
    vec2 tc = position;
    tc.y = -tc.y;
    tc = (tc + vec2(1.0, 1.0)) / 2.0;
    v_tc = tc;
}
