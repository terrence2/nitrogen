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
#include <wgpu-buffer/shader_shared/include/buffer_helpers.glsl>

const uint WORKGROUP_WIDTH = 65536;

layout(local_size_x = 64, local_size_y = 2, local_size_z = 1) in;

layout(binding = 0) uniform SubdivisionCtx { SubdivisionContext context; };
layout(binding = 1) uniform ExpansionCtx { SubdivisionExpandContext expand; };
layout(binding = 2) coherent buffer TargetVertices { TerrainVertex target_vertices[]; };
layout(binding = 3) readonly buffer IndexDependencyLut { uint index_dependency_lut[]; };

void
main()
{
    // The iteration vector is over expand.compute_vertices_in_patch * num_patches.
    uint i = gl_GlobalInvocationID.x + gl_GlobalInvocationID.y * WORKGROUP_WIDTH;

    // Find our patch offset and our offset within the current work set.
    uint patch_id = i / expand.compute_vertices_in_patch;
    uint relative_offset = i % expand.compute_vertices_in_patch;

    // To get the buffer offset we find our base patch offset, skip the prior computed vertices, then offset.
    uint patch_base = context.target_stride * patch_id;
    uint patch_offset = expand.skip_vertices_in_patch + relative_offset;
    uint offset = patch_base + patch_offset;

    // There are two dependencies per input, uploaded sequentially. Note that the deps are per-patch.
    uint dep_a = patch_base + index_dependency_lut[patch_offset * 2 + 0];
    uint dep_b = patch_base + index_dependency_lut[patch_offset * 2 + 1];

    // Load vertices.
    TerrainVertex tva = target_vertices[dep_a];
    TerrainVertex tvb = target_vertices[dep_b];

    // Note: patch edges that show up as A->B->C on one patch may be wound as C->B->A on an adjacent patch. There is no
    // way to fix this at the patch level (for the same reason we need to note the adjacent patch's edge in our peers).
    // While it should not actually matter if we process these as A-B vs B-A, in practice numerical precision issues
    // crop up in tall and steep slopes, leading to obvious gaps between patches, even with correctly wound strips.
    // HACK: process all pairs in the same order by sorting on x; edges lying exactly in the y-z
    //       plane are unlikely enough in combination that we can get away with a single compare.
    if (tva.surface_position[0] > tvb.surface_position[0]) {
        TerrainVertex tmp = tva;
        tva = tvb;
        tvb = tmp;
    }

    // Do normal interpolation the normal way.
    vec3 na = arr_to_vec3(tva.normal);
    vec3 nb = arr_to_vec3(tvb.normal);
    vec3 tmp = na + nb;
    vec3 nt = tmp / length(tmp);
    // Note clamp to 1 to avoid NaN from acos.
    float w = acos(min(1, dot(na, nt)));

    // Use the haversine geodesic midpoint method to compute graticule.
    // j/k => a/b
    float phi_a = tva.graticule[0];
    float theta_a = tva.graticule[1];
    float phi_b = tvb.graticule[0];
    float theta_b = tvb.graticule[1];
    // bx = cos(φk) · cos(θk−θj)
    float beta_x = cos(phi_b) * cos(theta_b - theta_a);
    // by = cos(φk) · sin(θk−θj)
    float beta_y = cos(phi_b) * sin(theta_b - theta_a);
    // φi = atan2(sin(φj) + sin(φk), √((cos(φj) + bx)^2 + by^2))
    float cpa_beta_x = cos(phi_a) + beta_x;
    float phi_t = atan(
        sin(phi_a) + sin(phi_b),
        sqrt(cpa_beta_x * cpa_beta_x + beta_y * beta_y)
    );
    // θi = θj + atan2(by, cos(φj) + bx)
    float theta_t = theta_a + atan(beta_y, cos(phi_a) + beta_x);

    // Use the clever tan method from figure 35.
    vec3 pa = arr_to_vec3(tva.surface_position);
    vec3 pb = arr_to_vec3(tvb.surface_position);
    float x = length(pb - pa) / 2.0;
    // Note that the angle we get is not the same as the opposite-over-adjacent angle we want.
    // It seems to be related to that angle though, by being 2x that angle; thus, divide by 2.
    float y = x * tan(w / 2);
    vec3 midpoint = (pa + pb) / 2.0;
    vec3 pt = midpoint + y * nt;

    target_vertices[offset].surface_position[0] = pt.x;
    target_vertices[offset].surface_position[1] = pt.y;
    target_vertices[offset].surface_position[2] = pt.z;
    target_vertices[offset].normal[0] = nt.x;
    target_vertices[offset].normal[1] = nt.y;
    target_vertices[offset].normal[2] = nt.z;
    target_vertices[offset].graticule[0] = phi_t;
    target_vertices[offset].graticule[1] = theta_t;
}
