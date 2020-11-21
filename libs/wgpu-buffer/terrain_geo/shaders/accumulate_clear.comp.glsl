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
#include <wgpu-buffer/terrain_geo/include/layout_accumulate.glsl>

layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;

void
main()
{
    ivec2 coord = ivec2(gl_GlobalInvocationID.xy);
    imageStore(terrain_color_acc, coord, vec4(0));
    imageStore(terrain_normal_acc, coord, ivec4(0));
}
