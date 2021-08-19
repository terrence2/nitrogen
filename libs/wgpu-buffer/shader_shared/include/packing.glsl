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

// Packing work-alikes, since naga lacks support.

vec4
unpackUnorm4x8(uint v)
{
    return vec4(
        ((v >> 0) & 0xFFu) / 255.0,
        ((v >> 8) & 0xFFu) / 255.0,
        ((v >> 16) & 0xFFu) / 255.0,
        ((v >> 24) & 0xFFu) / 255.0
    );
}
