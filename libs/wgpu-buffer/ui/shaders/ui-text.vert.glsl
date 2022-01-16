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
#include <wgpu-buffer/widget/include/widget.glsl>

layout(location = 0) in vec3 position;
layout(location = 1) in vec2 tex_coord;
layout(location = 2) in vec4 color;
layout(location = 3) in uint widget_info_id;

layout(location = 0) out vec2 v_tex_coord;
layout(location = 1) out vec4 v_color;
layout(location = 2) out vec2 v_screen_tex_coord;
layout(location = 3) flat out uint widget_info_id_frag;

void main() {
    WidgetInfo info = widget_info[widget_info_id];
    vec4 widget_position = vec4(
        position.x,
        position.y,
        position.z / MAX_WIDGETS,
        1
    );

    v_screen_tex_coord = (widget_position.xy + 1) / 2;
    v_screen_tex_coord.y = 1 - v_screen_tex_coord.y;

    widget_info_id_frag = widget_info_id;
    v_tex_coord = tex_coord;
    v_color = color;
    gl_Position = widget_position;
}
