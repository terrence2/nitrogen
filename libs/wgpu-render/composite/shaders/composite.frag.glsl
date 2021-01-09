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
#include <wgpu-buffer/world/include/world-deferred.glsl>
#include <wgpu-buffer/ui/include/ui-deferred.glsl>

layout(location = 0) in vec2 v_tc;
layout(location = 0) out vec4 f_color;

void main() {
    vec4 world = texture(sampler2D(world_deferred_texture, world_deferred_sampler), v_tc);
    vec4 ui = texture(sampler2D(ui_deferred_texture, ui_deferred_sampler), v_tc);
    float alpha = ui.a;
    f_color = vec4(world.rgb * alpha + ui.rgb * (1 - alpha), 1.0);
}
