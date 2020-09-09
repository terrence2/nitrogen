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
#include <wgpu-render/shader_shared/include/consts.glsl>
#include <wgpu-render/shader_shared/include/quaternion.glsl>
#include <wgpu-buffer/global_data/include/global_data.glsl>

layout(location = 0) in vec3 v_position; // eye relative
layout(location = 1) in vec3 v_normal;
layout(location = 2) in vec2 v_graticule; // earth centered

layout(location = 0) out vec4 v_color;

layout(set = 2, binding = 0) uniform utexture2D index_texture;
layout(set = 2, binding = 1) uniform sampler index_sampler;
layout(set = 2, binding = 2) uniform itexture2DArray atlas_texture;
layout(set = 2, binding = 3) uniform sampler atlas_sampler;
// layout(set = 2, binding = 4) buffer TileInfo { tile_info[]; }

const float INDEX_WIDTH_AS = 509 * 2560;
const float INDEX_HEIGHT_AS = 509 * 1280;
const float INDEX_BASE_LON_AS = -INDEX_WIDTH_AS / 2.0;
const float INDEX_BASE_LAT_AS = -INDEX_HEIGHT_AS / 2.0;
const float INDEX_BASE_LON_DEG = INDEX_BASE_LON_AS / 60.0 / 60.0;
const float INDEX_BASE_LAT_DEG = INDEX_BASE_LAT_AS / 60.0 / 60.0;
const float INDEX_ANGULAR_EXTENT_LON_DEG = INDEX_WIDTH_AS / 60.0 / 60.0;
const float INDEX_ANGULAR_EXTENT_LAT_DEG = INDEX_HEIGHT_AS / 60.0 / 60.0;

void main() {
    // FIXME: no need for a center indicator on the projection matrix, just scale.
    gl_Position = dbg_geocenter_m_projection() * vec4(v_position, 1);

    // Map latitude in -x -> x to 0 to 1.
    vec2 grat = degrees(v_graticule);
    float index_t = (grat.x - INDEX_BASE_LAT_DEG) / INDEX_ANGULAR_EXTENT_LAT_DEG;
    float index_s = (grat.y - INDEX_BASE_LON_DEG) / INDEX_ANGULAR_EXTENT_LON_DEG;

    uvec4 index_texel = texture(
        usampler2D(index_texture, index_sampler),
        vec2(
            1.0 - index_s,
            index_t
        )
    );
    uint slot = index_texel.r;
    float v = float(slot) / 65535.0 * 128.0;
    //v_color = vec4(v, v, v, 1);

    float tile_t = index_t;
    float tile_s = index_s;
    ivec4 atlas_texel = texture(
        isampler2DArray(atlas_texture, atlas_sampler),
        vec3(
            1.0 - tile_s,
            tile_t,
            float(slot)
        )
    );
    float height = float(atlas_texel.r);
    float clr = height / 8800.0;
    v_color = vec4(clr, clr, v, 1.0);
}
