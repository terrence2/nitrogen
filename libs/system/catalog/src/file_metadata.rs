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
use crate::{DrawerFileMetadata, FileId};

// Information about a "file" in the catalog.
#[derive(Debug)]
pub struct FileMetadata {
    id: FileId,
    name: String,
    compression: Option<&'static str>,
    packed_size: u64,
    unpacked_size: u64,
    path: String,
}

impl FileMetadata {
    pub(crate) fn from_drawer(id: FileId, drawer_meta: DrawerFileMetadata) -> FileMetadata {
        Self {
            id,
            name: drawer_meta.name,
            compression: drawer_meta.compression,
            packed_size: drawer_meta.packed_size,
            unpacked_size: drawer_meta.unpacked_size,
            path: drawer_meta.path,
        }
    }

    pub fn id(&self) -> FileId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn compression(&self) -> Option<&'static str> {
        self.compression
    }

    pub fn packed_size(&self) -> u64 {
        self.packed_size
    }

    pub fn unpacked_size(&self) -> u64 {
        self.unpacked_size
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}
