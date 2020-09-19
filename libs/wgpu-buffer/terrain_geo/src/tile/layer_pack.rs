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
use catalog::{Catalog, FileId};
use failure::{ensure, err_msg, Fallible};
use packed_struct::packed_struct;
use std::{collections::HashMap, mem};
use zerocopy::LayoutVerified;

// Terrain tile layers typically hold hundreds of thousands or even millions of
// individual tiles. This is enough that even naming them adds significant cost
// to the loading process. Instead of leaving loose files in the catalog, we
// pack them up by layers, leaving our catalog fast and allowing us to reference
// tiles by coordinate and level, rather than by hashing a string.

packed_struct!(LayerPackHeader {
    _0 => magic: [u8; 3],
    _1 => version: u8,
    _2 => angular_extent_as: i32,
    _3 => index_start: u32 as usize,
    _4 => tile_start: u32 as usize
    // Followed immediately by index data at file[index_start..tile_start]
    // Followed by tile data at files[tile_start..] with offsets determined by
});

packed_struct!(LayerPackIndexItem {
    _0 => base_lat_as: i32,
    _1 => base_lon_as: i32,
    _2 => tile_start: usize,
    _3 => tile_end: usize
});

const HEADER_MAGIC: [u8; 3] = [b'L', b'P', b'K'];
const HEADER_VERSION: u8 = 0;

pub struct LayerPack {
    // Map from base lat/lon in arcseconds, to start and end offsets in the file.
    layer_pack_fid: FileId,
    angular_extent_as: i32,
    index: HashMap<(i32, i32), (usize, usize)>,
}

impl LayerPack {
    pub fn new(layer_pack_fid: FileId, catalog: &Catalog) -> Fallible<Self> {
        let header_raw =
            catalog.read_slice_sync(layer_pack_fid, 0..mem::size_of::<LayerPackHeader>())?;
        let header = LayerPackHeader::overlay(&header_raw);
        ensure!(header.magic() == HEADER_MAGIC);
        ensure!(header.version() == HEADER_VERSION);
        let index_buf =
            catalog.read_slice_sync(layer_pack_fid, header.index_start()..header.tile_start())?;
        let raw_index = LayerPackIndexItem::overlay_slice(&index_buf);
        let mut index = HashMap::with_capacity(raw_index.len());
        for item in raw_index {
            index.insert(
                (item.base_lat_as(), item.base_lon_as()),
                (item.tile_start(), item.tile_end()),
            );
        }
        Ok(Self {
            layer_pack_fid,
            angular_extent_as: header.angular_extent_as(),
            index,
        })
    }

    pub async fn load_tile(&self, base: (i32, i32), catalog: &Catalog) -> Fallible<Vec<u8>> {
        let (start, end) = self.index[&base];
        Ok(catalog.read_slice(self.layer_pack_fid, start..end).await?)
    }
}
