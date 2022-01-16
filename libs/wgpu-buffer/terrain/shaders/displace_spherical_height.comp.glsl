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
#include <wgpu-buffer/shader_shared/include/buffer_helpers.glsl>
#include <wgpu-buffer/terrain/include/terrain.glsl>

layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;

layout(set = 0, binding = 0) buffer Vertices { TerrainVertex vertices[]; };
layout(set = 1, binding = 0) uniform utexture2D index_texture;
layout(set = 1, binding = 1) uniform sampler index_sampler;
layout(set = 1, binding = 2) uniform itexture2DArray atlas_texture;
layout(set = 1, binding = 3) uniform sampler atlas_sampler;
layout(set = 1, binding = 4) readonly buffer TileLayout { TileInfo tile_info[]; };

void
main()
{
    // One invocation per vertex.
    uint i = gl_GlobalInvocationID.x;

    vec2 v_graticule = arr_to_vec2(vertices[i].graticule);
    uint atlas_slot = terrain_atlas_slot_for_graticule(v_graticule, index_texture, index_sampler);
    int height = terrain_height_in_tile(v_graticule, tile_info[atlas_slot], atlas_texture, atlas_sampler);

    vec3 v_normal = arr_to_vec3(vertices[i].normal);
    vec3 v_position = arr_to_vec3(vertices[i].surface_position);
    vertices[i].position = vec3_to_arr(v_position + (float(height) * v_normal));
}