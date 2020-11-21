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

vec4 quat_from_axis_angle(vec3 axis, float angle)
{
    vec4 qr;
    float half_angle = (angle * 0.5); // * 3.14159 / 180.0;
    qr.x = axis.x * sin(half_angle);
    qr.y = axis.y * sin(half_angle);
    qr.z = axis.z * sin(half_angle);
    qr.w = cos(half_angle);
    return qr;
}

vec4 quat_conj(vec4 q)
{
    return vec4(-q.x, -q.y, -q.z, q.w);
}

vec4 quat_mult(vec4 q1, vec4 q2)
{
    vec4 qr;
    qr.x = (q1.w * q2.x) + (q1.x * q2.w) + (q1.y * q2.z) - (q1.z * q2.y);
    qr.y = (q1.w * q2.y) - (q1.x * q2.z) + (q1.y * q2.w) + (q1.z * q2.x);
    qr.z = (q1.w * q2.z) + (q1.x * q2.y) - (q1.y * q2.x) + (q1.z * q2.w);
    qr.w = (q1.w * q2.w) - (q1.x * q2.x) - (q1.y * q2.y) - (q1.z * q2.z);
    return qr;
}

vec4 quat_rotate(vec4 qr, vec3 position)
{
    vec4 qr_conj = quat_conj(qr);
    vec4 q_pos = vec4(position.xyz, 0);
    vec4 q_tmp = quat_mult(qr, q_pos);
    qr = quat_mult(q_tmp, qr_conj);
    return vec4(qr.xyz, 1);
}

#define bivec3 vec3

bivec3 bivec3_unit_xy() {
    return bivec3(1, 0, 0);
}

bivec3 bivec3_unit_xz() {
    return bivec3(0, 1, 0);
}

bivec3 bivec3_unit_yz() {
    return bivec3(0, 0, 1);
}

float bivec3_xy(bivec3 bv) {
    return bv.x;
}

float bivec3_xz(bivec3 bv) {
    return bv.y;
}

float bivec3_yz(bivec3 bv) {
    return bv.z;
}

struct Rotor3 {
    float s;
    bivec3 bv;
};

Rotor3 rotor3_from_angle_plane(float angle_r, bivec3 plane) {
    float half_angle = angle_r * 0.5;
    float sin = sin(half_angle);
    float cos = cos(half_angle);
    return Rotor3(cos, plane * -sin);
}

// Rotation about the y axis.
Rotor3 rotor3_from_rotation_xz(float angle_r) {
    return rotor3_from_angle_plane(angle_r, bivec3_unit_xz());
}

// Rotation about the x axis.
Rotor3 rotor3_from_rotation_yz(float angle_r) {
    return rotor3_from_angle_plane(angle_r, bivec3_unit_yz());
}

vec3 rotor3_rotate_vec(Rotor3 rotor, vec3 vec) {
    float xy = bivec3_xy(rotor.bv);
    float xz = bivec3_xz(rotor.bv);
    float yz = bivec3_yz(rotor.bv);

    float fx = rotor.s * vec.x + xy * vec.y + xz * vec.z;
    float fy = rotor.s * vec.y - xy * vec.x + yz * vec.z;
    float fz = rotor.s * vec.z - xz * vec.x - yz * vec.y;
    float fw = xy * vec.z - xz * vec.y + yz * vec.x;

    return vec3(
        rotor.s * fx + xy * fy + xz * fz + yz * fw,
        rotor.s * fy - xy * fx - xz * fw + yz * fz,
        rotor.s * fz + xy * fw - xz * fx - yz * fy
    );
}

Rotor3 rotor3_multiply(Rotor3 self, Rotor3 q) {
    float self_bv_xy = bivec3_xy(self.bv);
    float self_bv_xz = bivec3_xz(self.bv);
    float self_bv_yz = bivec3_yz(self.bv);
    float q_bv_xy = bivec3_xy(q.bv);
    float q_bv_xz = bivec3_xz(q.bv);
    float q_bv_yz = bivec3_yz(q.bv);
    return Rotor3(
        self.s * q.s - self_bv_xy * q_bv_xy - self_bv_xz * q_bv_xz - self_bv_yz * q_bv_yz,
        bivec3(
            self_bv_xy * q.s + self.s * q_bv_xy + self_bv_yz * q_bv_xz - self_bv_xz * q_bv_yz,
            self_bv_xz * q.s + self.s * q_bv_xz - self_bv_yz * q_bv_xy + self_bv_xy * q_bv_yz,
            self_bv_yz * q.s + self.s * q_bv_yz + self_bv_xz * q_bv_xy - self_bv_xy * q_bv_xz
        )
    );
}
