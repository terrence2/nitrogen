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
#include <wgpu-buffer/terrain/include/terrain.glsl>

const uint WORKGROUP_WIDTH = 65536;

layout(local_size_x = 64, local_size_y = 2, local_size_z = 1) in;

layout(set = 0, binding = 0) buffer Vertices { TerrainVertex vertices[]; };

void
main()
{
    // One invocation per vertex.
    uint i = gl_GlobalInvocationID.x + gl_GlobalInvocationID.y * WORKGROUP_WIDTH;

    // Direct copy of surface position
    vertices[i].position = vertices[i].surface_position;
}
