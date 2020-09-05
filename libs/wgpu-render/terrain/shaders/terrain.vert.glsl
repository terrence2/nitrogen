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

layout(set = 2, binding = 0) uniform utexture2D srtm_index_texture;
layout(set = 2, binding = 1) uniform sampler srtm_index_sampler;
layout(set = 2, binding = 2) uniform itexture2DArray srtm_atlas_texture;
layout(set = 2, binding = 3) uniform sampler srtm_atlas_sampler;

const float BASE = -1042432.0 / 60 / 60; // degrees
const float ANGULAR_EXTENT = 2084864.0 / 60 / 60; // degrees

void main() {
    // FIXME: no need for a center indicator on the projection matrix, just scale.
    gl_Position = dbg_geocenter_m_projection() * vec4(v_position, 1);

    // Map latitude in -x -> x to 0 to 1.
    vec2 grat = degrees(v_graticule);
    float t = (grat.x - BASE) / ANGULAR_EXTENT;
    float s = (grat.y - BASE) / ANGULAR_EXTENT;

/*
    float t = (grat.x + 90.0) / 180.0;
    float s = (grat.y + 180.0) / 360.0;

    // Map s, t onto the actual subsection of the atlas that is used.
    // Each pixel of the 4096 square atlas is 1 tile at max resolution.
    // The atlas is therefore 509 * 4096 arcseconds across, but centered.
    // The fraction of the atlas taken by earth bits longitudinally is:
    //   >>> (360 * 60 * 60) / (509 * 4096)
    //   0.6216232809430255
    // Which means we to map [-0.31,0.31) on each side of the center to [0,1].

    float tile_extent = 512.0 * 4096.0;
    float fract_lon = (360.0 * 60.0 * 60.0) / tile_extent;
    float s0 = s * fract_lon - (1.0 - fract_lon / 2.0);


//    float fract_lat = (180.0 * 60.0 * 60.0) / tile_extent;
    float fract_lat = 1.0;
    //float fract_lon = 1.0;

    //v_color = vec4(s, t, 0, 1);
    */

    uvec4 index_texel = texture(
        usampler2D(srtm_index_texture, srtm_index_sampler),
        vec2(
            1.0 - s,
            t
        )
    );
    float v = float(index_texel.r) / 65535.0 * 128.0;
    v_color = vec4(v, v, v, 1);

    /*
    ivec4 height_texel = texture(
        isampler2DArray(srtm_atlas_texture, srtm_atlas_sampler),
        vec3(
            1.0 - s * fract_lon,
            t * fract_lat,
            0
        )
    );
    float height = height_texel.r / 255.0;
    v_color = vec4(height, height, height, 1);
    */

    //v_color = vec4(1, 0, 1, 1);
}
