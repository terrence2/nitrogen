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
    float screen_physical_width;
    float screen_physical_height;
    float screen_logical_width;
    float screen_logical_height;

    // Orrery
    vec4 orrery_sun_direction;

    // Camera
    float camera_fov_y;
    float camera_aspect_ratio;
    float camera_z_near_m;
    float camera_z_near_km;
    vec4 camera_forward;
    vec4 camera_up;
    vec4 camera_right;
    vec4 camera_position_m;
    vec4 camera_position_km;
    mat4 camera_perspective_m;
    mat4 camera_perspective_km;
    mat4 camera_inverse_perspective_m;
    mat4 camera_inverse_perspective_km;
    mat4 camera_view_m;
    mat4 camera_view_km;
    mat4 camera_inverse_view_m;
    mat4 camera_inverse_view_km;
    mat4 camera_look_at_rhs_m;
    float camera_exposure;

    // Tone mapping
    float tone_gamma;

    // Padding
    float pad1[2];
};

mat4 screen_letterbox_projection() { return globals_screen_letterbox_projection; }

vec3
raymarching_view_ray(vec2 position)
{
    // https://www.derschmale.com/2014/01/26/reconstructing-positions-from-the-depth-buffer/

    // Position is a corner of ~ndc~ clip at z=0.
    vec4 corner = vec4(position, 0, 1);

    // Reverse ~ndc~ clip into eye space vector.
    vec4 eye = camera_inverse_perspective_km * corner;
    eye.w = 0;

    // Reverse eye space into world space direction.
    vec4 wrld = camera_inverse_view_km * eye;

    return wrld.xyz;
}
