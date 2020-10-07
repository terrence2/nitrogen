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
use crate::mip::{MipIndexDataSet, Region};
use absolute_unit::{arcseconds, meters, Angle, ArcSeconds};
use failure::Fallible;
use geodesy::{GeoCenter, Graticule};
use image::{ImageBuffer, Luma};
use memmap::{Mmap, MmapOptions};
use parking_lot::RwLock;
use std::{
    fs,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};
use terrain_geo::tile::{ChildIndex, DataSetDataKind, TerrainLevel, TILE_PHYSICAL_SIZE};
use zerocopy::{AsBytes, LayoutVerified};

enum TileData {
    Absent, // Not yet generated or loaded.
    Empty,  // Loaded and no content.
    InlineHeights(Box<[[i16; TILE_PHYSICAL_SIZE]; TILE_PHYSICAL_SIZE]>),
    InlineNormals(Box<[[[i16; 2]; TILE_PHYSICAL_SIZE]; TILE_PHYSICAL_SIZE]>),
    MappedHeights(Mmap),
    MappedNormals(Mmap),
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
pub enum NeighborIndex {
    West,
    SouthWest,
    South,
    SouthEast,
    East,
    NorthEast,
    North,
    NorthWest,
}

impl NeighborIndex {
    pub const fn len() -> usize {
        8
    }

    pub fn from_index(i: usize) -> Self {
        match i {
            0 => Self::West,
            1 => Self::SouthWest,
            2 => Self::South,
            3 => Self::SouthEast,
            4 => Self::East,
            5 => Self::NorthEast,
            6 => Self::North,
            7 => Self::NorthWest,
            _ => panic!("not a valid neighbor index"),
        }
    }

    pub fn index(&self) -> usize {
        match self {
            Self::West => 0,
            Self::SouthWest => 1,
            Self::South => 2,
            Self::SouthEast => 3,
            Self::East => 4,
            Self::NorthEast => 5,
            Self::North => 6,
            Self::NorthWest => 7,
        }
    }
}

impl TileData {
    fn is_absent(&self) -> bool {
        match self {
            Self::Absent => true,
            _ => false,
        }
    }

    fn is_inline(&self) -> bool {
        match self {
            Self::InlineHeights { .. } => true,
            Self::InlineNormals { .. } => true,
            _ => false,
        }
    }

    fn is_mapped(&self) -> bool {
        match self {
            Self::MappedHeights { .. } => true,
            Self::MappedNormals { .. } => true,
            _ => false,
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            Self::Empty => true,
            _ => false,
        }
    }

    fn state(&self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Empty => "empty",
            Self::InlineHeights { .. } => "inline_heights",
            Self::InlineNormals { .. } => "inline_normals",
            Self::MappedHeights { .. } => "mapped_heights",
            Self::MappedNormals { .. } => "mapped_normals",
        }
    }

    fn raw_data(&self) -> &[u8] {
        match self {
            Self::Absent => &[],
            Self::Empty => &[],
            Self::InlineHeights(ba) => ba.as_bytes(),
            Self::InlineNormals(ba) => ba.as_bytes(),
            Self::MappedHeights(mm) => mm.as_bytes(),
            Self::MappedNormals(mm) => mm.as_bytes(),
        }
    }

    fn as_inline_heights(&self) -> &[[i16; TILE_PHYSICAL_SIZE]; TILE_PHYSICAL_SIZE] {
        match self {
            Self::InlineHeights(ba) => ba,
            _ => panic!("not an inline data"),
        }
    }

    fn as_inline_heights_mut(&mut self) -> &mut [[i16; TILE_PHYSICAL_SIZE]; TILE_PHYSICAL_SIZE] {
        match self {
            Self::InlineHeights(ba) => ba,
            _ => panic!("not an inline data"),
        }
    }

    fn as_inline_normals_mut(
        &mut self,
    ) -> &mut [[[i16; 2]; TILE_PHYSICAL_SIZE]; TILE_PHYSICAL_SIZE] {
        match self {
            Self::InlineNormals(ba) => ba,
            _ => panic!("not an inline data"),
        }
    }

    fn as_mmap_heights(&self) -> &Mmap {
        match self {
            Self::MappedHeights(ba) => ba,
            _ => panic!("not an inline data"),
        }
    }
}

pub struct Tile {
    // The location of the tile.
    prefix: &'static str,

    // Number of arcseconds in a sample.
    level: TerrainLevel,

    // Record which child of the parent this is. Given Angular extent and base, we can
    // work out our parents base; this is important when rebuilding efficiently from
    // layers.
    index_in_parent: ChildIndex,

    // The tile's bottom left corner. Note the full extent, not the extent clipped.
    base: (i32, i32),

    // The full angular extent from base to the last of TILE_PHYSICAL_SIZE.
    angular_extent: i32,

    // Samples. Low indices are more south. This is opposite from SRTM ordering.
    data: TileData,

    // Keep a quad-tree of children. Indices as per ChildIndex.
    children: [Option<Arc<RwLock<Tile>>>; 4],

    neighbors: [Option<Arc<RwLock<Tile>>>; NeighborIndex::len()],
}

impl Tile {
    pub fn new_uninitialized(
        prefix: &'static str,
        level: TerrainLevel,
        index_in_parent: ChildIndex,
        base: (i32, i32),
        angular_extent: i32,
    ) -> Self {
        Self {
            prefix,
            level,
            index_in_parent,
            base,
            angular_extent,
            data: TileData::Absent,
            children: [None, None, None, None],
            neighbors: [None, None, None, None, None, None, None, None],
        }
    }

    pub fn level(&self) -> TerrainLevel {
        self.level
    }

    pub fn index_in_parent(&self) -> ChildIndex {
        self.index_in_parent
    }

    pub fn base(&self) -> (i32, i32) {
        self.base
    }

    pub fn base_graticule(&self) -> Graticule<GeoCenter> {
        Graticule::<GeoCenter>::new(self.base_latitude(), self.base_longitude(), meters!(0))
    }

    pub fn base_latitude(&self) -> Angle<ArcSeconds> {
        arcseconds!(self.base.0)
    }

    pub fn base_longitude(&self) -> Angle<ArcSeconds> {
        arcseconds!(self.base.1)
    }

    pub fn child_angular_extent(&self) -> Angle<ArcSeconds> {
        arcseconds!(self.child_angular_extent_as())
    }

    pub fn child_angular_extent_as(&self) -> i32 {
        self.angular_extent / 2
    }

    pub fn set_neighbors(&mut self, neighbors: [Option<Arc<RwLock<Tile>>>; NeighborIndex::len()]) {
        self.neighbors = neighbors;
    }

    pub fn extent(&self) -> i32 {
        self.angular_extent
    }

    pub fn child_base(&self, index: ChildIndex) -> (i32, i32) {
        let ang = self.child_angular_extent_as();
        match index {
            ChildIndex::SouthWest => self.base,
            ChildIndex::SouthEast => (self.base.0, self.base.1 + ang),
            ChildIndex::NorthWest => (self.base.0 + ang, self.base.1),
            ChildIndex::NorthEast => (self.base.0 + ang, self.base.1 + ang),
        }
    }

    pub fn child_base_graticule(&self, index: ChildIndex) -> Graticule<GeoCenter> {
        let (lat, lon) = self.child_base(index);
        Graticule::<GeoCenter>::new(arcseconds!(lat), arcseconds!(lon), meters!(0))
    }

    pub fn child_region(&self, index: ChildIndex) -> Region {
        Region {
            base: self.child_base_graticule(index),
            extent: self.child_angular_extent(),
        }
    }

    pub fn add_child(
        &mut self,
        target_level: TerrainLevel,
        index: ChildIndex,
    ) -> Arc<RwLock<Tile>> {
        assert_eq!(self.level.offset() + 1, target_level.offset());
        let tile = Arc::new(RwLock::new(Tile::new_uninitialized(
            &self.prefix,
            target_level,
            index,
            self.child_base(index),
            self.child_angular_extent_as(),
        )));
        self.children[index.to_index()] = Some(tile.clone());
        tile
    }

    pub fn has_child(&self, index: ChildIndex) -> bool {
        self.children[index.to_index()].is_some()
    }

    pub fn get_child(&self, index: ChildIndex) -> Option<Arc<RwLock<Tile>>> {
        self.children[index.to_index()].clone()
    }

    pub fn has_children(&self) -> bool {
        ChildIndex::all_indices().all(|i| self.children[i].is_some())
    }

    pub fn maybe_children(&self) -> &[Option<Arc<RwLock<Tile>>>] {
        &self.children
    }

    pub fn filename_base(&self) -> String {
        format!(
            "{}-L{}-{}-{}",
            self.prefix,
            self.level.offset(),
            self.base_latitude().format_latitude(),
            self.base_longitude().format_longitude(),
        )
    }

    pub fn mip_filename(&self, directory: &Path) -> PathBuf {
        let mut buf = directory.to_owned();
        buf.push(self.filename_base());
        buf.with_extension("bin")
    }

    pub fn find_sampled_extremes(&self) -> (i16, i16) {
        assert!(self.data.is_inline());
        let mut lo = i16::MAX;
        let mut hi = i16::MIN;
        for row in self.data.as_inline_heights().iter() {
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

    pub fn lookup(&self, level: usize, base: (i32, i32)) -> Option<Arc<RwLock<Tile>>> {
        // Look in our child, rather that descending: we need to return the child ref, which we
        // don't have from the context of the unwrapped node itself.
        for maybe_child in &self.children {
            if let Some(childref) = maybe_child {
                let child = childref.read();
                if child.base.0 <= base.0
                    && child.base.0 + child.angular_extent > base.0
                    && child.base.1 <= base.1
                    && child.base.1 + child.angular_extent > base.1
                {
                    if child.level.offset() == level {
                        return maybe_child.clone();
                    } else {
                        child.lookup(level, base);
                    }
                }
            }
        }
        None
    }

    fn sum_region(&self, lat: i32, lon: i32) -> i32 {
        let mut total = 0;
        for a in -1..=1 {
            for b in -1..=1 {
                total += self.get_height_sample(lat + a, lon + b) as i32;
            }
        }
        total
    }

    // Fill in a sample by linearly interpolating the data from our child's samples.
    pub fn pull_height_sample(
        &mut self,
        dataset: Arc<RwLock<MipIndexDataSet>>,
        lat_offset: i32,
        lon_offset: i32,
    ) -> i16 {
        assert!(self.data.is_inline());
        assert!(lat_offset >= 0);
        assert!(lon_offset >= 0);
        assert!(lat_offset < TILE_PHYSICAL_SIZE as i32);
        assert!(lon_offset < TILE_PHYSICAL_SIZE as i32);

        //    .  .  .  .  .  .
        //    x  .  x  .  x  .
        //    .  .  .  .  .  .
        //    x  .  x  .  x  .
        // We can reach right and up in our children without issue. Left and/or down is problematic.
        // If we are at the top or left, we need to reach to our siblings.

        let s = if lat_offset < 256 && lon_offset < 256 {
            self.children[ChildIndex::SouthWest.to_index()]
                .as_ref()
                .map(|child| {
                    let lat = lat_offset * 2;
                    let lon = lon_offset * 2;
                    child.read().sum_region(lat, lon)
                })
        } else if lat_offset < 256 {
            self.children[ChildIndex::SouthEast.to_index()]
                .as_ref()
                .map(|child| {
                    let lat = lat_offset * 2;
                    let lon = (lon_offset - 256) * 2;
                    child.read().sum_region(lat, lon)
                })
        } else if lon_offset < 256 {
            self.children[ChildIndex::NorthWest.to_index()]
                .as_ref()
                .map(|child| {
                    let lat = (lat_offset - 256) * 2;
                    let lon = lon_offset * 2;
                    child.read().sum_region(lat, lon)
                })
        } else if let Some(childref) = &self.children[ChildIndex::NorthEast.to_index()] {
            self.children[ChildIndex::NorthEast.to_index()]
                .as_ref()
                .map(|child| {
                    let lat = (lat_offset - 256) * 2;
                    let lon = (lon_offset - 256) * 2;
                    child.read().sum_region(lat, lon)
                })
        } else {
            None
        };

        if let Some(sum_height) = s {
            let height = ((sum_height as f64) / 9f64).round() as i16;
            height
        } else {
            0
        }
        // if let Some((childref, child_lat, child_lon)) = s {
        //     let sum_height = childref.read().get_height_sample(child_lat, child_lon) as i32
        //         + childref.read().get_height_sample(child_lat, child_lon + 1) as i32
        //         + childref.read().get_height_sample(child_lat + 1, child_lon) as i32
        //         + childref
        //             .read()
        //             .get_height_sample(child_lat + 1, child_lon + 1) as i32;
        //     let height = ((sum_height as f64) / 4f64).round() as i16;
        //     // FIXME: how do we compute an average normal?
        //     height
        // } else {
        //     0
        // }
    }

    pub fn is_empty_tile(&self) -> bool {
        let (lo, hi) = self.find_sampled_extremes();
        lo == 0 && hi == 0
    }

    pub fn allocate_scratch_data(&mut self, kind: DataSetDataKind) {
        assert!(self.data.is_absent());
        self.data = match kind {
            DataSetDataKind::Height => {
                TileData::InlineHeights(Box::new([[0i16; TILE_PHYSICAL_SIZE]; TILE_PHYSICAL_SIZE]))
            }
            DataSetDataKind::Normal => TileData::InlineNormals(Box::new(
                [[[0i16; 2]; TILE_PHYSICAL_SIZE]; TILE_PHYSICAL_SIZE],
            )),
            DataSetDataKind::Color => panic!("unsupported kind: Color"),
        };
    }

    pub fn maybe_map_data(&mut self, kind: DataSetDataKind, directory: &Path) -> Fallible<bool> {
        assert!(self.data.is_absent());
        let mip_filename = self.mip_filename(directory);
        if mip_filename.exists() {
            let mip_fp = File::open(&mip_filename)?;
            let mip_mmap = unsafe { MmapOptions::new().map(&mip_fp)? };
            self.data = match kind {
                DataSetDataKind::Height => TileData::MappedHeights(mip_mmap),
                DataSetDataKind::Normal => TileData::MappedNormals(mip_mmap),
                DataSetDataKind::Color => panic!("unsupported kind: color"),
            };
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

    pub fn raw_data(&self) -> &[u8] {
        self.data.raw_data()
    }

    pub fn get_height_sample(&self, lat_offset: i32, lon_offset: i32) -> i16 {
        // FIXME: debug asserts once we've validated our indexing
        const END: i32 = TILE_PHYSICAL_SIZE as i32;
        const LAST: i32 = END - 1;
        assert!(lat_offset >= -1);
        assert!(lat_offset <= END);
        assert!(lon_offset >= -1);
        assert!(lon_offset <= END);
        // Note: first with matching pattern matches, so we have to put the exact matches (corners)
        //       at the top and only put the edges (with placeholder match) later.
        match (lat_offset, lon_offset) {
            (-1, -1) => self.neighbors[NeighborIndex::SouthWest.index()]
                .clone()
                .map(|t| t.read().get_own_height_sample(LAST, LAST))
                .unwrap_or(0),
            (-1, END) => self.neighbors[NeighborIndex::SouthEast.index()]
                .clone()
                .map(|t| t.read().get_own_height_sample(LAST, 0))
                .unwrap_or(0),
            (END, -1) => self.neighbors[NeighborIndex::NorthWest.index()]
                .clone()
                .map(|t| t.read().get_own_height_sample(0, LAST))
                .unwrap_or(0),
            (END, END) => self.neighbors[NeighborIndex::NorthEast.index()]
                .clone()
                .map(|t| t.read().get_own_height_sample(0, 0))
                .unwrap_or(0),
            (-1, lon) => self.neighbors[NeighborIndex::South.index()]
                .clone()
                .map(|t| t.read().get_own_height_sample(LAST, lon))
                .unwrap_or(0),
            (END, lon) => self.neighbors[NeighborIndex::North.index()]
                .clone()
                .map(|t| t.read().get_own_height_sample(0, lon))
                .unwrap_or(0),
            (lat, -1) => self.neighbors[NeighborIndex::West.index()]
                .clone()
                .map(|t| t.read().get_own_height_sample(lat, LAST))
                .unwrap_or(0),
            (lat, END) => self.neighbors[NeighborIndex::East.index()]
                .clone()
                .map(|t| t.read().get_own_height_sample(lat, 0))
                .unwrap_or(0),
            _ => self.get_own_height_sample(lat_offset, lon_offset),
        }
    }

    pub fn get_own_height_sample(&self, lat_offset: i32, lon_offset: i32) -> i16 {
        // FIXME: debug asserts once we've validated our indexing
        assert!(lat_offset > -1);
        assert!(lat_offset < 512);
        assert!(lon_offset > -1);
        assert!(lon_offset < 512);
        if self.data.is_mapped() {
            let m = self.data.as_mmap_heights();
            let n = LayoutVerified::<&[u8], [i16]>::new_slice(m.as_bytes()).unwrap();
            return n[TILE_PHYSICAL_SIZE * lat_offset as usize + lon_offset as usize];
        }
        assert!(self.data.is_empty());
        0
    }

    pub fn set_height_sample(&mut self, lat_offset: i32, lon_offset: i32, height: i16) {
        debug_assert!(self.data.is_inline());
        self.data.as_inline_heights_mut()[lat_offset as usize][lon_offset as usize] = height;
    }

    pub fn save_equalized_png(&self, kind: DataSetDataKind, directory: &Path) -> Fallible<()> {
        assert!(self.data.is_inline());
        if self.is_empty_tile() {
            return Ok(());
        }
        let path = self.mip_filename(directory);

        match kind {
            DataSetDataKind::Height => {
                let (_, high) = self.find_sampled_extremes();
                let mut pic: ImageBuffer<Luma<u8>, Vec<u8>> = ImageBuffer::new(512, 512);
                for (y, row) in self.data.as_inline_heights().iter().enumerate() {
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
            }
            _ => panic!("unsupported png output type"),
        }

        Ok(())
    }

    pub fn file_exists(&self, directory: &Path) -> bool {
        self.mip_filename(directory).exists()
    }

    pub fn write(&mut self, directory: &Path, allow_empty: bool) -> Fallible<()> {
        assert!(self.data.is_inline());
        if !allow_empty && self.is_empty_tile() {
            self.data = TileData::Empty;
            return Ok(());
        }
        let mip_path = self.mip_filename(directory);
        if !mip_path.parent().expect("subdir").exists() {
            println!("Creating directory: {:?}", mip_path.parent());
            fs::create_dir(mip_path.parent().expect("subdir")).ok();
            assert!(mip_path.parent().expect("subdir").exists());
        }
        {
            let mut mip_fp = File::create(&mip_path)?;
            mip_fp.write_all(self.data.raw_data())?;
        }
        let mip_fp = File::open(&mip_path)?;
        let mip_mmap = unsafe { MmapOptions::new().map(&mip_fp)? };
        self.data = match self.data {
            TileData::InlineHeights(_) => TileData::MappedHeights(mip_mmap),
            TileData::InlineNormals(_) => TileData::MappedNormals(mip_mmap),
            _ => panic!("unsupported inlien type"),
        };
        Ok(())
    }
}
