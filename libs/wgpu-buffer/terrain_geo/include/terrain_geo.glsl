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

struct TerrainVertex {
    // Note that we cannot use vec3 here as that packs into vec4 in a struct storage buffer context, unlike in a
    // vertex context where it packs properly. :shrug:
    float position[3];
    float normal[3];
    float graticule[2];
};

// 3 vertices per patch stride in the upload buffer.
#define PATCH_UPLOAD_STRIDE 3

struct SubdivisionContext {
    uint target_stride;
    uint target_subdivision_level;
    uint pad[2];
};

struct SubdivisionExpandContext {
    uint current_target_subdivision_level;
    uint skip_vertices_in_patch;
    uint compute_vertices_in_patch;
    uint pad[1];
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
terrain_geo_atlas_slot_for_graticule(vec2 graticule_rad, utexture2D index_texture, sampler index_sampler) {
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

int
terrain_geo_height_in_tile(vec2 graticule_rad, TileInfo tile, itexture2DArray atlas_texture, sampler atlas_sampler) {
    // Tile metadata is stored in arcseconds for maximum precision.
    vec2 graticule_deg = degrees(graticule_rad);
    vec2 graticule_as = graticule_deg * 60.0 * 60.0;

    // Look up the height information in the tile.
    float tile_t = (graticule_as.x - tile.base_as[0]) / tile.angular_extent_as;
    float tile_s = (graticule_as.y - tile.base_as[1]) / tile.angular_extent_as;
    ivec4 atlas_texel = texture(
        isampler2DArray(atlas_texture, atlas_sampler),
        vec3(
            tile_s,
            tile_t,
            tile.atlas_slot
        )
    );
    return atlas_texel.r;
}
///////////////////////////////////////////////////////////////////////////////
