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

layout(location = 0) flat in vec4 v_color;
layout(location = 1) smooth in vec3 v_normal;
layout(location = 0) out vec4 f_color;

void main() {
    vec4 ambient = 0.3 * v_color;
    vec4 diffuse = 0.7 * (v_color * dot(orrery_sun_direction.xyz, v_normal));
    vec4 specular = 1.0 * (vec4(1)
        * clamp(
           pow(dot(camera_forward.xyz,
                reflect(orrery_sun_direction.xyz, v_normal)),
           6),
          0, 1));

    f_color = vec4((ambient + diffuse + specular).xyz, v_color.w);
}
