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
#include <wgpu-buffer/widget/include/widget.glsl>
#include <wgpu-buffer/world/include/world-deferred.glsl>

layout(location = 0) in vec2 v_tex_coord;
layout(location = 1) in vec4 v_color;
layout(location = 2) in vec2 v_screen_tex_coord;
layout(location = 3) flat in uint widget_info_id;

layout(location = 0) out vec4 f_color;

void main() {
    float alpha = texture(sampler2D(glyph_sheet_texture, glyph_sheet_sampler), v_tex_coord).r;

    WidgetInfo info = widget_info[widget_info_id];
    vec3 world_clr = texture(sampler2D(world_deferred_texture, world_deferred_sampler), v_screen_tex_coord).rgb;
    if (widget_has_pre_blended_text(info)) {
        f_color = vec4(world_clr * (1.0 - alpha) + v_color.rgb * alpha, 1);
    } else {
        f_color = vec4(v_color.xyz, alpha);
    }
}
