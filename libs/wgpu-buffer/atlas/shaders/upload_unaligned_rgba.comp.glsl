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
#include <wgpu-buffer/shader_shared/include/packing.glsl>

struct CopyInfo {
    uint x;
    uint y;
    uint w;
    uint h;
    uint padding_px;
    uint border_color;
};

layout(local_size_x = 16, local_size_y = 16, local_size_z = 1) in;
layout(set = 0, binding = 0) uniform Meta { CopyInfo info; };
layout(set = 0, binding = 1) readonly buffer Data {
    uint data[];
};
layout(set = 0, binding = 2, rgba8) uniform writeonly image2D atlas_texture;

void
main() {
    ivec2 coord = ivec2(gl_GlobalInvocationID.xy);
    ivec2 src_coord = coord - ivec2(info.padding_px);

    vec4 clr = unpackUnorm4x8(info.border_color);
    if (src_coord.x >= 0 && src_coord.x < info.w && src_coord.y >= 0 && src_coord.y < info.h) {
        uint block_offset = src_coord.y * info.w + src_coord.x;
        clr = unpackUnorm4x8(data[block_offset]);
    }

    imageStore(atlas_texture, ivec2(info.x, info.y) + coord, clr);
}