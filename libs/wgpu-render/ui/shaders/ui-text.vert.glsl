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
//#include <wgpu-buffer/widget/include/widget.glsl>

layout(location = 0) in vec3 position;
layout(location = 1) in vec3 tex_coord;
layout(location = 2) in uint widget_info_id;

layout(location = 0) out vec3 v_tex_coord;
layout(location = 1) flat out vec4 v_color;

void main() {
    vec4 text_layout_position = vec4(-3.5804195, 0.0, 0.0, 0.0);
    gl_Position = screen_letterbox_projection() * (vec4(position, 1.0) + text_layout_position);
    v_tex_coord = tex_coord;
    v_color = vec4(1,0,1,1);//text_layout_color;
}
