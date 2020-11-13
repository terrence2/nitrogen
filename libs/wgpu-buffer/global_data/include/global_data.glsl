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

layout(set = 0, binding = 0) buffer CameraParameters {
    mat4 globals_screen_letterbox_projection;

    // Camera
    float camera_fov_y;
    float camera_aspect_ratio;
    float camera_z_near;
    float pad0;
    vec4 camera_forward;
    vec4 camera_up;
    vec4 camera_right;
    vec4 camera_position_m;
    vec4 camera_position_km;
    mat4 camera_projection_m;
    mat4 camera_projection_km;
    mat4 camera_inverse_projection_m;
    mat4 camera_inverse_projection_km;
    mat4 camera_view_m;
    mat4 camera_view_km;
    mat4 camera_inverse_view_m;
    mat4 camera_inverse_view_km;
};

mat4 screen_letterbox_projection() { return globals_screen_letterbox_projection; }

vec3
raymarching_view_ray(vec2 position)
{
    vec4 reverse_vec;

    // inverse perspective projection
    reverse_vec = vec4(position, 0.0, 1.0);
    reverse_vec = camera_inverse_projection_km * reverse_vec;

    // inverse modelview, without translation
    reverse_vec.w = 0.0;
    reverse_vec = camera_inverse_view_km * reverse_vec;

    return vec3(reverse_vec);
}
