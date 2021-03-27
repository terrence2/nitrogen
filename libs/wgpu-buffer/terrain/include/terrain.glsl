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

struct TerrainVertex {
    // Note that we cannot use vec3 here as that packs into vec4 in a struct storage buffer context,
    // unlike in a vertex context where it packs properly.
    float surface_position[3];
    float position[3];
    float normal[3];
    float graticule[2];
};

// Use a more densely packed representation during upload.
struct TerrainUploadVertex {
    float position[3];
    float normal[3];
    float graticule[2];
};

// 3 vertices per patch stride in the upload buffer.
#define PATCH_UPLOAD_STRIDE 3

struct SubdivisionContext {
    uint target_stride;
    uint target_subdivision_level;
};

struct SubdivisionExpandContext {
    uint current_target_subdivision_level;
    uint skip_vertices_in_patch;
    uint compute_vertices_in_patch;
};

struct TileInfo {
    float base_as[2];
    float angular_extent_as;
    float atlas_slot;
};

///////////////////////////////////////////////////////////////////////////////
/// Spherical height lookup
const float INDEX_WIDTH_AS = 509 * 2560;
const float INDEX_HEIGHT_AS = 509 * 1280;
const float INDEX_BASE_LON_AS = -INDEX_WIDTH_AS / 2.0;
const float INDEX_BASE_LAT_AS = -INDEX_HEIGHT_AS / 2.0;
const float INDEX_BASE_LON_DEG = INDEX_BASE_LON_AS / 60.0 / 60.0;
const float INDEX_BASE_LAT_DEG = INDEX_BASE_LAT_AS / 60.0 / 60.0;
const float INDEX_ANGULAR_EXTENT_LON_DEG = INDEX_WIDTH_AS / 60.0 / 60.0;
const float INDEX_ANGULAR_EXTENT_LAT_DEG = INDEX_HEIGHT_AS / 60.0 / 60.0;

uint
terrain_atlas_slot_for_graticule(vec2 graticule_rad, utexture2D index_texture, sampler index_sampler) {
    // Our index is stored in degrees -- close enough with the pad in the tile data, even at full depth.
    vec2 graticule_deg = degrees(graticule_rad);

    // Look up the best available atlas slot by consulting the index.
    float index_t = (graticule_deg.x - INDEX_BASE_LAT_DEG) / INDEX_ANGULAR_EXTENT_LAT_DEG;
    float index_s = (graticule_deg.y - INDEX_BASE_LON_DEG) / INDEX_ANGULAR_EXTENT_LON_DEG;
    uvec4 index_texel = texture(
        usampler2D(index_texture, index_sampler),
        vec2(
            index_s,
            index_t
        )
    );
    return index_texel.r;
}

vec2
terrain_graticule_to_st(vec2 graticule_rad, TileInfo tile) {
    // Tile metadata is stored with arcseconds as maximum precision.
    vec2 graticule_deg = degrees(graticule_rad);
    vec2 graticule_as = graticule_deg * 60.0 * 60.0;

    // MipTiles have size 512x512, but the edge is overlapped by one pixel with adjacent tiles so that
    // we can always do linear filtering locally. The upshot is that angular_extent below is over the
    // middle 510 pixels (509 gaps) and the base_as is located offset 1x1 into the image.

    // Compute s/t in the "tile".
    vec2 tile_st = vec2(
        (graticule_as.y - tile.base_as[1]) / tile.angular_extent_as,
        (graticule_as.x - tile.base_as[0]) / tile.angular_extent_as
    );

    // Project the tile s/t into the image as a whole.
    vec2 img_st = (tile_st * vec2(509.0) / vec2(512.0)) + vec2(1.0 / 512.0);

    return img_st;
}

ivec4
terrain_sample_in_tile(vec2 graticule_rad, TileInfo tile, itexture2DArray atlas_texture, sampler atlas_sampler) {
    return texture(
        isampler2DArray(atlas_texture, atlas_sampler),
        vec3(
            terrain_graticule_to_st(graticule_rad, tile),
            tile.atlas_slot
        )
    );
}

int
terrain_height_in_tile(vec2 graticule_rad, TileInfo tile, itexture2DArray atlas_texture, sampler atlas_sampler) {
    return terrain_sample_in_tile(graticule_rad, tile, atlas_texture, atlas_sampler).r;
}

ivec2
terrain_normal_in_tile(vec2 graticule_rad, TileInfo tile, itexture2DArray atlas_texture, sampler atlas_sampler) {
    return terrain_sample_in_tile(graticule_rad, tile, atlas_texture, atlas_sampler).xy;
}

vec4
terrain_color_in_tile(vec2 graticule_rad, TileInfo tile, texture2DArray atlas_texture, sampler atlas_sampler) {
    return texture(
        sampler2DArray(atlas_texture, atlas_sampler),
        vec3(
            terrain_graticule_to_st(graticule_rad, tile),
            tile.atlas_slot
        )
    );
}
///////////////////////////////////////////////////////////////////////////////
