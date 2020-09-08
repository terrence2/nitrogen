// This file is part of OpenFA.
//
// OpenFA is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// OpenFA is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with OpenFA.  If not, see <http://www.gnu.org/licenses/>.
#version 450
#include <wgpu-buffer/terrain_geo/include/terrain_geo.glsl>

layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;

layout(set = 0, binding = 0) buffer Vertices { TerrainVertex vertices[]; };
layout(set = 1, binding = 0) uniform utexture2D index_texture;
layout(set = 1, binding = 1) uniform sampler index_sampler;
layout(set = 1, binding = 2) uniform itexture2DArray atlas_texture;
layout(set = 1, binding = 3) uniform sampler atlas_sampler;

const float BASE = -1042432.0 / 60.0 / 60.0; // degrees
const float ANGULAR_EXTENT = 2084864.0 / 60.0 / 60.0; // degrees

void
main()
{
    // One invocation per vertex.
    uint i = gl_GlobalInvocationID.x;

    // Map latitude in -x -> x to 0 to 1.
    float latitude_deg = degrees(vertices[i].graticule[0]);
    float longitude_deg = degrees(vertices[i].graticule[1]);
    float t = (latitude_deg - BASE) / ANGULAR_EXTENT;
    float s = (longitude_deg - BASE) / ANGULAR_EXTENT;

    // Look up atlas slot in the index.
    uvec4 index_texel = texture(
        usampler2D(index_texture, index_sampler),
        vec2(
            1.0 - s,
            t
        )
    );
    uint slot = index_texel.r;

    ivec4 atlas_texel = texture(
        isampler2DArray(atlas_texture, atlas_sampler),
        vec3(
            1.0 - s,
            t,
            float(slot)
        )
    );
    float height = float(atlas_texel.r);

    vec3 planet_norm = vec3(vertices[i].normal[0], vertices[i].normal[1], vertices[i].normal[2]);
    vec3 surface_pos = vec3(vertices[i].position[0], vertices[i].position[1], vertices[i].position[2]);

    vec3 displaced_pos = surface_pos + (height * 100 * planet_norm);

    vertices[i].position[0] = displaced_pos.x;
    vertices[i].position[1] = displaced_pos.y;
    vertices[i].position[2] = displaced_pos.z;
}