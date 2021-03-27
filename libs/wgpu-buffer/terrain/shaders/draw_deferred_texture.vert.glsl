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
    // Note: we upload positions in eye space: e.g. pre-multiplied by the view matrix.
    gl_Position = camera_perspective_m * vec4(v_position, 1);
    v_color = vec4(v_graticule, v_normal.x, v_normal.z);
}
