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
layout(location = 0) in vec2 in_texcoord;
layout(location = 0) out vec4 f_color;

layout(set = 0, binding = 0) uniform texture2D upload_texture;
layout(set = 0, binding = 1) uniform sampler upload_sampler;

void
main()
{
    vec4 clr = texture(sampler2D(upload_texture, upload_sampler), in_texcoord);
    f_color = vec4(clr.rgb, 1);
}
