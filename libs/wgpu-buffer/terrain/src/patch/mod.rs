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
mod icosahedron;
mod patch_info;
mod patch_manager;
mod patch_tree;
mod queue;

pub mod patch_winding;
pub mod terrain_upload_vertex;
pub mod terrain_vertex;

pub(crate) use crate::patch::{
    patch_manager::PatchManager,
    patch_tree::{PatchIndex, PatchTree},
    terrain_upload_vertex::TerrainUploadVertex,
};
pub use crate::patch::{patch_winding::PatchWinding, terrain_vertex::TerrainVertex};
