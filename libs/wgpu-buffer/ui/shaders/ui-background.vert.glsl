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

layout(set = 2, binding = 0) uniform texture2D world_deferred_texture;
layout(set = 2, binding = 1) uniform sampler world_deferred_sampler;

layout(location = 0) in vec3 position;
layout(location = 1) in vec2 tex_coord;
layout(location = 2) in uint widget_info_id;

layout(location = 0) out vec2 v_tex_coord;
layout(location = 1) flat out vec4 v_color;

void main() {
    v_tex_coord = tex_coord;

    WidgetInfo info = widget_info[widget_info_id];
    v_color = info.background_color;

    vec4 widget_position = vec4(info.position[0],  info.position[1], 0, 0);
    gl_Position = widget_position + vec4(position.xy, position.z / MAX_WIDGETS, 1);
}
