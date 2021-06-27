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

layout(location = 0) in vec3 v_surface_position; // eye relative
layout(location = 1) in vec3 v_position; // eye relative
layout(location = 2) in vec3 v_normal;
layout(location = 3) in vec2 v_graticule; // earth centered

layout(location = 0) out vec4 v_color;

void main() {
    // Note: we upload positions in eye space instead of world space for precision reasons.
    gl_Position = camera_perspective_m * vec4(v_position, 1);

    // Normals are uploaded in eye space so that they can displace the eye-space verticies as we
    // build the vertices. We want to invert the normal to world space for storage in the deferred
    // texture.
    vec3 normal_w = (vec4(v_normal, 1) * camera_inverse_view_km).xyz;
    v_color = vec4(v_graticule, normal_w.x, normal_w.z);
}
