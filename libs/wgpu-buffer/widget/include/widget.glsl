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
#define MAX_WIDGETS 512

struct WidgetInfo {
    vec4 position;
    uvec4 flags;
};

// Flags
#define GLASS_BACKGROUND 0x00000001

layout(set = 1, binding = 0) uniform WidgetBlock {
    WidgetInfo widget_info[MAX_WIDGETS];
};
layout(set = 1, binding = 1) uniform texture2D glyph_sheet_texture;
layout(set = 1, binding = 2) uniform sampler glyph_sheet_sampler;

bool
widget_has_glass_background(WidgetInfo info) {
    return (info.flags.x & GLASS_BACKGROUND) > 0;
}

float
glyph_alpha_uv(vec2 tex_coord)
{
    return texture(sampler2D(glyph_sheet_texture, glyph_sheet_sampler), tex_coord).r;
}
