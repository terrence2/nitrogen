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
use crate::tile::{TerrainLevel, TileCompression};
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
    _4 => tile_level: u16,
    _5 => tile_compression: u16,
    _6 => index_start: u32 as usize,
    _7 => tile_start: u32 as usize
    // Followed immediately by index data at file[index_start..tile_start]
    // Followed by tile data at files[tile_start..] with offsets determined by
});

packed_struct!(LayerPackIndexItem {
    _0 => base_lat_as: i32,
    _1 => base_lon_as: i32,
    _3 => index_in_parent: u32,

    // Relative to file start, no offset needed.
    _4 => tile_start: u64,
    _5 => tile_end: u64
});

const HEADER_MAGIC: [u8; 3] = [b'L', b'P', b'K'];
const HEADER_VERSION: u8 = 1;

pub struct LayerPack {
    // Map from base lat/lon in arcseconds, to start and end offsets in the file.
    layer_pack_fid: FileId,
    terrain_level: TerrainLevel,
    angular_extent_as: i32,
    index_extent: Range<usize>,
    tile_count: usize,
    tile_compression: TileCompression,
}

impl LayerPack {
    pub fn new(layer_pack_fid: FileId, catalog: &Catalog) -> Fallible<Self> {
        let header_raw =
            catalog.read_slice_sync(layer_pack_fid, 0..mem::size_of::<LayerPackHeader>())?;
        let header = LayerPackHeader::overlay(&header_raw)?;
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
            tile_compression: TileCompression::from_u16(header.tile_compression()),
        })
    }

    pub(crate) fn index_bytes<'a>(&self, catalog: &'a Catalog) -> Fallible<Cow<'a, [u8]>> {
        catalog.read_slice_sync(self.layer_pack_fid, self.index_extent.clone())
    }

    pub fn tile_compression(&self) -> TileCompression {
        self.tile_compression
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

pub struct LayerPackBuilder {
    // Relative to file start, no offset needed.
    index_cursor: u64,

    // Relative to file start, no offset needed.
    tile_cursor: u64,

    // The output file.
    stream: File,
}

impl LayerPackBuilder {
    pub fn new(
        path: &Path,
        tile_count: usize,
        tile_level: u32,
        tile_compression: TileCompression,
        angular_extent_as: i32,
    ) -> Fallible<Self> {
        let mut stream = File::create(path)?;

        // Emit the header
        let index_start = mem::size_of::<LayerPackHeader>();
        let tile_start = index_start + mem::size_of::<LayerPackIndexItem>() * tile_count;
        let header = LayerPackHeader::build(
            HEADER_MAGIC,
            HEADER_VERSION,
            angular_extent_as,
            tile_count as u32,
            tile_level as u16,
            tile_compression as u16,
            index_start as u32,
            tile_start as u32,
        )?;
        stream.write_all(header.as_bytes())?;

        Ok(Self {
            // reservations: Vec::new(),
            // reserve_cursor: 0,
            index_cursor: index_start as u64,
            tile_cursor: tile_start as u64,
            stream,
        })
    }

    pub fn push_tile(
        &mut self,
        base: (i32, i32),
        index_in_parent: u32,
        data: &[u8],
    ) -> Fallible<()> {
        if data.is_empty() {
            return Ok(());
        }

        self.stream.seek(SeekFrom::Start(self.index_cursor))?;
        let index_item = LayerPackIndexItem::build(
            base.0,
            base.1,
            index_in_parent,
            self.tile_cursor,
            self.tile_cursor + data.len() as u64,
        )?;
        self.stream.write_all(index_item.as_bytes())?;
        self.index_cursor += mem::size_of::<LayerPackIndexItem>() as u64;

        self.stream.seek(SeekFrom::Start(self.tile_cursor))?;
        self.stream.write_all(data)?;
        self.tile_cursor += data.len() as u64;

        Ok(())
    }
}
