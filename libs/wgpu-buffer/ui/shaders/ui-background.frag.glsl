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
#include <wgpu-buffer/world/include/world-deferred.glsl>

layout(location = 0) flat in vec4 v_color;
layout(location = 1) in vec2 v_screen_tex_coord;
layout(location = 2) flat in uint widget_info_id;

layout(location = 0) out vec4 f_color;

void main() {
    WidgetInfo info = widget_info[widget_info_id];

    vec3 world_clr = vec3(0);
    if (widget_has_glass_background(info)) {
        float x_step = 1.0 / screen_logical_width * 4.0;
        float y_step = 1.0 / screen_logical_height * 4.0;
        float weights[7 * 7] = {
            0.000, 0.000, 0.001, 0.001, 0.001, 0.000, 0.000,
            0.000, 0.002, 0.012, 0.020, 0.012, 0.002, 0.000,
            0.001, 0.012, 0.068, 0.109, 0.068, 0.012, 0.001,
            0.001, 0.020, 0.109, 0.172, 0.109, 0.020, 0.001,
            0.001, 0.012, 0.068, 0.109, 0.068, 0.012, 0.001,
            0.000, 0.002, 0.012, 0.020, 0.012, 0.002, 0.000,
            0.000, 0.000, 0.001, 0.001, 0.001, 0.000, 0.000
        };

        for(int y = 0; y < 7; ++y) {
            float dy = (float(y) - 3.0) * y_step;
            for(int x = 0; x < 7; ++x) {
                float weight = weights[x + y * 7];
                float dx = (float(x) - 3.0) * x_step;
                vec4 world = texture(sampler2D(world_deferred_texture, world_deferred_sampler), v_screen_tex_coord + vec2(dx, dy));
                world_clr += world.rgb * weight;
            }
        }
        float alpha = v_color.a;
        world_clr = world_clr * (1.0 - alpha) + v_color.rgb * alpha;
    } else {
        world_clr = v_color.rgb;
    }

    f_color = vec4(world_clr, 1.0);
}
