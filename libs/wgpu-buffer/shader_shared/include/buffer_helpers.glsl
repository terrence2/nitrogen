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

vec2
arr_to_vec2(float[2] arr) {
    return vec2(arr[0], arr[1]);
}

vec3
arr_to_vec3(float[3] arr) {
    return vec3(arr[0], arr[1], arr[2]);
}

float[3]
vec3_to_arr(vec3 v) {
    return float[3](v.x, v.y, v.z);
}

