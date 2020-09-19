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
use crate::mip::Region;
use absolute_unit::{meters, Angle, ArcSeconds};
use failure::Fallible;
use geodesy::{GeoCenter, Graticule};
use image::{ImageBuffer, Luma};
use memmap::{Mmap, MmapOptions};
use std::{
    fs,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};
use terrain_geo::tile::{ChildIndex, TerrainLevel, TILE_PHYSICAL_SIZE};
use zerocopy::AsBytes;

enum TileData {
    Absent,                                                       // Not yet generated or loaded.
    Empty,                                                        // Loaded and no content.
    Inline(Box<[[i16; TILE_PHYSICAL_SIZE]; TILE_PHYSICAL_SIZE]>), // Building until we can write it out and mmap it.
    Mapped(Mmap),                                                 // In a file, mapped into memory.
}

impl TileData {
    fn is_absent(&self) -> bool {
        match self {
            Self::Absent => true,
            Self::Empty => false,
            Self::Inline(_) => false,
            Self::Mapped(_) => false,
        }
    }

    fn is_inline(&self) -> bool {
        match self {
            Self::Absent => false,
            Self::Empty => false,
            Self::Inline(_) => true,
            Self::Mapped(_) => false,
        }
    }

    fn is_mapped(&self) -> bool {
        match self {
            Self::Absent => false,
            Self::Empty => false,
            Self::Inline(_) => false,
            Self::Mapped(_) => true,
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            Self::Absent => false,
            Self::Empty => true,
            Self::Inline(_) => false,
            Self::Mapped(_) => false,
        }
    }

    fn state(&self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Empty => "empty",
            Self::Inline(_) => "inline",
            Self::Mapped(_) => "mapped",
        }
    }

    fn as_inline(&self) -> &[[i16; TILE_PHYSICAL_SIZE]; TILE_PHYSICAL_SIZE] {
        match self {
            Self::Inline(ba) => ba,
            _ => panic!("not an inline data"),
        }
    }

    fn as_inline_mut(&mut self) -> &mut [[i16; TILE_PHYSICAL_SIZE]; TILE_PHYSICAL_SIZE] {
        match self {
            Self::Inline(ba) => ba,
            _ => panic!("not an inline data"),
        }
    }

    fn as_mmap(&self) -> &Mmap {
        match self {
            Self::Mapped(mm) => mm,
            _ => panic!("not an inline data"),
        }
    }

    fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Absent => None,
            Self::Empty => None,
            Self::Inline(ba) => Some(ba.as_bytes()),
            Self::Mapped(mmap) => Some(&mmap),
        }
    }
}

pub struct Tile {
    // The location of the tile.
    prefix: String,

    // Number of arcseconds in a sample.
    level: TerrainLevel,

    // The tile's bottom left corner. Note the full extent, not the extent clipped.
    base: Graticule<GeoCenter>,

    // The full angular extent from base to the last of TILE_PHYSICAL_SIZE.
    angular_extent: Angle<ArcSeconds>,

    // Samples. Low indices are more south. This is opposite from SRTM ordering.
    data: TileData,

    // Keep a quad-tree of children. Indices as per ChildIndex.
    children: [Option<Arc<RwLock<Tile>>>; 4],
}

impl Tile {
    pub fn new_uninitialized(
        prefix: &str,
        level: TerrainLevel,
        base: &Graticule<GeoCenter>,
        angular_extent: Angle<ArcSeconds>,
    ) -> Self {
        Self {
            prefix: prefix.to_owned(),
            level,
            base: *base,
            angular_extent,
            data: TileData::Absent,
            children: [None, None, None, None],
        }
    }

    pub fn level(&self) -> TerrainLevel {
        self.level
    }

    pub fn base(&self) -> &Graticule<GeoCenter> {
        &self.base
    }

    pub fn extent(&self) -> Angle<ArcSeconds> {
        self.angular_extent
    }

    pub fn child_base(&self, index: ChildIndex) -> Graticule<GeoCenter> {
        let h = meters!(0);
        let ang = self.child_angular_extent();
        match index {
            ChildIndex::SouthWest => self.base,
            ChildIndex::SouthEast => {
                Graticule::new(self.base.latitude, self.base.longitude + ang, h)
            }
            ChildIndex::NorthWest => {
                Graticule::new(self.base.latitude + ang, self.base.longitude, h)
            }
            ChildIndex::NorthEast => {
                Graticule::new(self.base.latitude + ang, self.base.longitude + ang, h)
            }
        }
    }

    pub fn child_angular_extent(&self) -> Angle<ArcSeconds> {
        self.angular_extent / 2.0
    }

    pub fn child_region(&self, index: ChildIndex) -> Region {
        Region {
            base: self.child_base(index),
            extent: self.child_angular_extent(),
        }
    }

    pub fn add_child(
        &mut self,
        target_level: TerrainLevel,
        index: ChildIndex,
    ) -> Arc<RwLock<Tile>> {
        assert_eq!(self.level.offset() + 1, target_level.offset());
        let base = self.child_base(index);
        let tile = Arc::new(RwLock::new(Tile::new_uninitialized(
            &self.prefix,
            target_level,
            &base,
            self.child_angular_extent(),
        )));
        self.children[index.to_index()] = Some(tile.clone());
        tile
    }

    pub fn has_child(&self, index: ChildIndex) -> bool {
        self.children[index.to_index()].is_some()
    }

    pub fn has_children(&self) -> bool {
        ChildIndex::all_indices().all(|i| self.children[i].is_some())
    }

    pub fn maybe_children(&self) -> &[Option<Arc<RwLock<Tile>>>] {
        &self.children
    }

    pub fn base_corner_graticule(&self) -> &Graticule<GeoCenter> {
        &self.base
    }

    pub fn filename_base(&self) -> String {
        format!(
            "{}-L{}-{}-{}",
            self.prefix,
            self.level.offset(),
            self.base.latitude.format_latitude(),
            self.base.longitude.format_longitude(),
        )
    }

    pub fn filename(&self, directory: &Path) -> PathBuf {
        let mut buf = directory.to_owned();
        buf.push(self.filename_base());
        buf.with_extension("bin")
    }

    pub fn find_sampled_extremes(&self) -> (i16, i16) {
        assert!(self.data.is_inline());
        let mut lo = i16::MAX;
        let mut hi = i16::MIN;
        for row in self.data.as_inline().iter() {
            for &v in row.iter() {
                if v > hi {
                    hi = v;
                }
                if v < lo {
                    lo = v;
                }
            }
        }
        (lo, hi)
    }

    // Fill in a sample by linearly interpolating the data from our child's samples.
    pub fn pull_sample(&mut self, lat_offset: i32, lon_offset: i32) -> i16 {
        assert!(self.data.is_inline());
        assert!(lat_offset >= 0);
        assert!(lon_offset >= 0);
        assert!(lat_offset < 512);
        assert!(lon_offset < 512);

        // 0..511: what do we do about the edges? It seems like we'd need to linear interpolate
        // from adjacent tiles... uhg. Parent link? Or do we assume we only consume to the right.

        // If upper left sample here is multiple samples below.
        //    x  .  x  .  x  .
        //    .  .  .  .  .  .
        //    x  .  x  .  x  .
        //    .  .  .  .  .  .
        // 0..255

        if lat_offset < 256 && lon_offset < 256 {
            if let Some(childref) = &self.children[ChildIndex::SouthWest.to_index()] {
                let child = childref.read().unwrap();
                let child_lat = lat_offset * 2;
                let child_lon = lon_offset * 2;
                return (child.get_sample(child_lat, child_lon)
                    + child.get_sample(child_lat, child_lon + 1)
                    + child.get_sample(child_lat + 1, child_lon)
                    + child.get_sample(child_lat + 1, child_lon + 1))
                    / 4;
            }
        } else if lat_offset < 256 {
            if let Some(childref) = &self.children[ChildIndex::SouthEast.to_index()] {
                let child = childref.read().unwrap();
                let child_lat = lat_offset * 2;
                let child_lon = (lon_offset - 256) * 2;
                return (child.get_sample(child_lat, child_lon)
                    + child.get_sample(child_lat, child_lon + 1)
                    + child.get_sample(child_lat + 1, child_lon)
                    + child.get_sample(child_lat + 1, child_lon + 1))
                    / 4;
            }
        } else if lon_offset < 256 {
            if let Some(childref) = &self.children[ChildIndex::NorthWest.to_index()] {
                let child = childref.read().unwrap();
                let child_lat = (lat_offset - 256) * 2;
                let child_lon = lon_offset * 2;
                return (child.get_sample(child_lat, child_lon)
                    + child.get_sample(child_lat, child_lon + 1)
                    + child.get_sample(child_lat + 1, child_lon)
                    + child.get_sample(child_lat + 1, child_lon + 1))
                    / 4;
            }
        } else {
            if let Some(childref) = &self.children[ChildIndex::NorthEast.to_index()] {
                let child = childref.read().unwrap();
                let child_lat = (lat_offset - 256) * 2;
                let child_lon = (lon_offset - 256) * 2;
                return (child.get_sample(child_lat, child_lon)
                    + child.get_sample(child_lat, child_lon + 1)
                    + child.get_sample(child_lat + 1, child_lon)
                    + child.get_sample(child_lat + 1, child_lon + 1))
                    / 4;
            }
        }
        0
    }

    pub fn is_empty_tile(&self) -> bool {
        let (lo, hi) = self.find_sampled_extremes();
        lo == 0 && hi == 0
    }

    pub fn allocate_scratch_data(&mut self) {
        assert!(self.data.is_absent());
        self.data = TileData::Inline(Box::new([[0i16; TILE_PHYSICAL_SIZE]; TILE_PHYSICAL_SIZE]));
    }

    pub fn maybe_map_data(&mut self, directory: &Path) -> Fallible<bool> {
        assert!(self.data.is_absent());
        let filename = self.filename(directory);
        if filename.exists() {
            let fp = File::open(&filename)?;
            let mmap = unsafe { MmapOptions::new().map(&fp)? };
            self.data = TileData::Mapped(mmap);
            return Ok(true);
        }
        Ok(false)
    }

    pub fn promote_absent_to_empty(&mut self) {
        if self.data.is_absent() {
            self.data = TileData::Empty;
        }
    }

    pub fn data_state(&self) -> &'static str {
        self.data.state()
    }

    // Set a sample, offset in samples from the base corner.
    pub fn set_sample(&mut self, lat_offset: i32, lon_offset: i32, sample: i16) {
        debug_assert!(self.data.is_inline());
        self.data.as_inline_mut()[lat_offset as usize][lon_offset as usize] = sample;
    }

    pub fn get_sample(&self, lat_offset: i32, lon_offset: i32) -> i16 {
        if self.data.is_mapped() {
            let m = self.data.as_mmap();
            let n: &[i16] = unsafe { std::mem::transmute(m.as_bytes()) };
            return n[TILE_PHYSICAL_SIZE * lat_offset as usize + lon_offset as usize];
        }
        assert!(self.data.is_empty());
        return 0;
    }

    pub fn save_equalized_png(&self, directory: &Path) -> Fallible<()> {
        assert!(self.data.is_inline());
        if self.is_empty_tile() {
            return Ok(());
        }
        let path = self.filename(directory);

        let (_, high) = self.find_sampled_extremes();

        let mut pic: ImageBuffer<Luma<u8>, Vec<u8>> = ImageBuffer::new(512, 512);
        for (y, row) in self.data.as_inline().iter().enumerate() {
            for (x, &v) in row.iter().enumerate() {
                // Scale 0..high into 0..255
                let p = v.max(0) as f32;
                let pf = p / (high as f32) * 255f32;
                pic.put_pixel(
                    x as u32,
                    (TILE_PHYSICAL_SIZE - y - 1) as u32,
                    Luma([pf as u8]),
                );
            }
        }
        pic.save(path.with_extension("png"))?;
        Ok(())
    }

    pub fn file_exists(&self, directory: &Path) -> bool {
        self.filename(directory).exists()
    }

    pub fn write(&mut self, directory: &Path) -> Fallible<()> {
        assert!(self.data.is_inline());
        if self.is_empty_tile() {
            self.data = TileData::Empty;
            return Ok(());
        }
        let path = self.filename(directory);
        if !path.parent().expect("subdir").exists() {
            fs::create_dir(path.parent().expect("subdir"))?;
        }
        {
            let mut fp = File::create(&path)?;
            fp.write_all(self.data.as_bytes().unwrap())?;
        }
        let fp = File::open(&path)?;
        let mmap = unsafe { MmapOptions::new().map(&fp)? };
        self.data = TileData::Mapped(mmap);
        Ok(())
    }
}
