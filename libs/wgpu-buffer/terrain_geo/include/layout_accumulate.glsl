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

// Read for the the deferred coordinate and depth buffers with appropriate sampler.
// Write for the color and normal accumulation buffer.
// Used by:
//    clear accumulators
//    accumulate spherical normals
//    accumulate spherical color
//    accumulate cartesian normals
//    accumulate cartesian color
layout(set = 1, binding = 0) uniform texture2D terrain_deferred_texture;
layout(set = 1, binding = 1) uniform texture2D terrain_deferred_depth;
layout(set = 1, binding = 2, rgba8) uniform image2D terrain_color_acc;
layout(set = 1, binding = 3, rg16i) uniform iimage2D terrain_normal_acc;
layout(set = 1, binding = 4) uniform sampler terrain_linear_sampler;

/*
layout(set = 2, binding = 0) uniform texture2D terrain_deferred_texture;
layout(set = 2, binding = 1) uniform texture2D terrain_deferred_depth;
layout(set = 2, binding = 2) uniform utexture2D terrain_color_acc_texture;
layout(set = 2, binding = 3) uniform texture2D terrain_normal_acc_texture;
layout(set = 2, binding = 4) uniform sampler terrain_linear_sampler;
*/
