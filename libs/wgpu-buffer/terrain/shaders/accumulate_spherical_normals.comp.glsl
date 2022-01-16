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
#include <wgpu-buffer/terrain/include/terrain.glsl>
#include <wgpu-buffer/terrain/include/layout_accumulate.glsl>

layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;

layout(set = 2, binding = 0) uniform utexture2D index_texture;
layout(set = 2, binding = 1) uniform sampler index_sampler;
layout(set = 2, binding = 2) uniform itexture2DArray atlas_texture;
layout(set = 2, binding = 3) uniform sampler atlas_sampler;
layout(set = 2, binding = 4) readonly buffer TileLayout { TileInfo tile_info[]; };

void
main()
{
    ivec2 coord = ivec2(gl_GlobalInvocationID.xy);

    // Do a depth check to see if we're even looking at terrain.
    float depth = texelFetch(sampler2D(terrain_deferred_depth, terrain_linear_sampler), coord, 0).x;
    if (depth > -1) {
        // Load the relevant normal sample.
        vec2 grat = texelFetch(sampler2D(terrain_deferred_texture, terrain_linear_sampler), coord, 0).xy;
        uint atlas_slot = terrain_atlas_slot_for_graticule(grat, index_texture, index_sampler);
        ivec2 raw_normal = terrain_normal_in_tile(grat, tile_info[atlas_slot], atlas_texture, atlas_sampler);

        // FIXME: blend normal with existing buffer.

        // Write back blended normal.
        imageStore(
            terrain_normal_acc,
            coord,
            ivec4(raw_normal, 0, 0)
        );
    }
}
