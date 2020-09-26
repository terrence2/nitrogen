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
use crate::tile::TerrainLevel;
use catalog::{Catalog, FileId};
use failure::{ensure, Fallible};
use packed_struct::packed_struct;
use std::{
    borrow::Cow,
    fs::File,
    io::{Seek, SeekFrom, Write},
    mem,
    ops::Range,
    path::Path,
};
use zerocopy::AsBytes;

// Terrain tile layers typically hold hundreds of thousands or even millions of
// individual tiles. This is enough that even naming them adds significant cost
// to the loading process. Instead of leaving loose files in the catalog, we
// pack them up by layers, leaving our catalog fast and allowing us to reference
// tiles by coordinate and level, rather than by hashing a string.

packed_struct!(LayerPackHeader {
    _0 => magic: [u8; 3],
    _1 => version: u8,
    _2 => angular_extent_as: i32,
    _3 => tile_count: u32,
    _4 => tile_level: u32,
    _5 => index_start: u32 as usize,
    _6 => tile_start: u32 as usize
    // Followed immediately by index data at file[index_start..tile_start]
    // Followed by tile data at files[tile_start..] with offsets determined by
});

packed_struct!(LayerPackIndexItem {
    _0 => base_lat_as: i32,
    _1 => base_lon_as: i32,
    _2 => tile_kind: u32,
    _3 => index_in_parent: u32,
    _4 => tile_start: usize,
    _5 => tile_end: usize
});

const HEADER_MAGIC: [u8; 3] = [b'L', b'P', b'K'];
const HEADER_VERSION: u8 = 0;

pub struct LayerPack {
    // Map from base lat/lon in arcseconds, to start and end offsets in the file.
    layer_pack_fid: FileId,
    terrain_level: TerrainLevel,
    angular_extent_as: i32,
    index_extent: Range<usize>,
    tile_count: usize,
}

impl LayerPack {
    pub fn new(layer_pack_fid: FileId, catalog: &Catalog) -> Fallible<Self> {
        let header_raw =
            catalog.read_slice_sync(layer_pack_fid, 0..mem::size_of::<LayerPackHeader>())?;
        let header = LayerPackHeader::overlay(&header_raw);
        ensure!(header.magic() == HEADER_MAGIC);
        ensure!(header.version() == HEADER_VERSION);
        ensure!(
            (header.tile_start() - header.index_start()) % mem::size_of::<LayerPackIndexItem>()
                == 0
        );
        Ok(Self {
            layer_pack_fid,
            angular_extent_as: header.angular_extent_as(),
            terrain_level: TerrainLevel::new(header.tile_level() as usize),
            index_extent: header.index_start()..header.tile_start(),
            tile_count: (header.tile_start() - header.index_start())
                / mem::size_of::<LayerPackIndexItem>(),
        })
    }

    pub(crate) fn index_bytes<'a>(&self, catalog: &'a Catalog) -> Fallible<Cow<'a, [u8]>> {
        catalog.read_slice_sync(self.layer_pack_fid, self.index_extent.clone())
    }

    pub fn angular_extent_as(&self) -> i32 {
        self.angular_extent_as
    }

    pub fn terrain_level(&self) -> &TerrainLevel {
        &self.terrain_level
    }

    pub fn tile_count(&self) -> usize {
        self.tile_count
    }

    pub fn file_id(&self) -> FileId {
        self.layer_pack_fid
    }
}

struct Reservation {
    base: (i32, i32),
    index_in_parent: u32,
    data_location: usize,
    data_length: u32,
}

pub struct LayerPackBuilder {
    // Contains start relative to tile_start and the length.
    reservations: Vec<Reservation>,
    reserve_cursor: usize,
    stream: File,
}

impl LayerPackBuilder {
    pub fn new(path: &Path) -> Fallible<Self> {
        Ok(Self {
            reservations: Vec::new(),
            reserve_cursor: 0,
            stream: File::create(path)?,
        })
    }

    pub fn reserve(&mut self, base: (i32, i32), index_in_parent: u32, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        self.reservations.push(Reservation {
            base,
            index_in_parent,
            data_location: self.reserve_cursor,
            data_length: data.len() as u32,
        });
        self.reserve_cursor += data.len();
    }

    pub fn write_header(&mut self, tile_level: u32, angular_extent_as: i32) -> Fallible<()> {
        assert_eq!(self.stream.seek(SeekFrom::Current(0))?, 0u64);

        // Write out the header
        let index_start = mem::size_of::<LayerPackHeader>();
        let tile_start =
            index_start + mem::size_of::<LayerPackIndexItem>() * self.reservations.len();
        let header = LayerPackHeader::build(
            HEADER_MAGIC,
            HEADER_VERSION,
            angular_extent_as,
            self.reservations.len() as u32,
            tile_level,
            index_start as u32,
            tile_start as u32,
        )?;
        self.stream.write_all(header.as_bytes())?;

        // Write out the index
        for reservation in &self.reservations {
            let index_item = LayerPackIndexItem::build(
                reservation.base.0,
                reservation.base.1,
                0,
                reservation.index_in_parent,
                tile_start + reservation.data_location,
                tile_start + reservation.data_location + reservation.data_length as usize,
            )?;
            self.stream.write_all(index_item.as_bytes())?;
        }

        // Assuming our math is right:
        assert_eq!(self.stream.seek(SeekFrom::Current(0))?, tile_start as u64);
        Ok(())
    }

    pub fn push_tile(&mut self, data: &[u8]) -> Fallible<()> {
        if data.is_empty() {
            return Ok(());
        }
        self.stream.write_all(data)?;
        Ok(())
    }
}
