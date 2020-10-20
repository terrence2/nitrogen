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
#include <wgpu-render/shader_shared/include/consts.glsl>
#include <wgpu-render/shader_shared/include/quaternion.glsl>
#include <wgpu-buffer/global_data/include/global_data.glsl>
#include <wgpu-buffer/terrain_geo/include/terrain_geo.glsl>

layout(location = 0) in vec3 v_position; // eye relative
layout(location = 1) in vec3 v_normal;
layout(location = 2) in vec2 v_graticule; // earth centered

layout(location = 0) out vec4 v_color;

layout(set = 2, binding = 0) uniform utexture2D index_texture;
layout(set = 2, binding = 1) uniform sampler index_sampler;
layout(set = 2, binding = 2) uniform itexture2DArray atlas_texture;
layout(set = 2, binding = 3) uniform sampler atlas_sampler;
layout(set = 2, binding = 4) buffer TileLayout { TileInfo tile_info[]; };


void main() {
    // FIXME: no need for a center indicator on the projection matrix, just scale.
    gl_Position = dbg_geocenter_m_projection() * vec4(v_position, 1);
    v_color = vec4(v_graticule, v_normal.x, v_normal.z);
}
