// This file is part of OpenFA.
//
// OpenFA is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// OpenFA is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with OpenFA.  If not, see <http://www.gnu.org/licenses/>.
use absolute_unit::{arcseconds, degrees, meters, Angle, AngleUnit, ArcSeconds};
use failure::Fallible;
use geodesy::{GeoCenter, Graticule};
use image::{ImageBuffer, Luma};
use std::{
    fs,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};
use terrain_geo::tile::{ChildIndex, TerrainLevel, TILE_EXTENT, TILE_PHYSICAL_SIZE, TILE_SAMPLES};
use zerocopy::AsBytes;

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
    data: [[i16; TILE_PHYSICAL_SIZE]; TILE_PHYSICAL_SIZE],

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
            data: [[0i16; TILE_PHYSICAL_SIZE]; TILE_PHYSICAL_SIZE],
            children: [None, None, None, None],
        }
    }

    pub fn add_child(
        &mut self,
        target_level: TerrainLevel,
        index: ChildIndex,
    ) -> Arc<RwLock<Tile>> {
        // FIXME: check extent and make sure we're inside?
        assert_eq!(self.level.offset() + 1, target_level.offset());
        let h = meters!(0);
        let ang = self.angular_extent / 2.0;
        let base = match index {
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
        };
        let tile = Arc::new(RwLock::new(Tile::new_uninitialized(
            &self.prefix,
            target_level,
            &base,
            ang,
        )));
        self.children[index.to_index()] = Some(tile.clone());
        tile
    }

    pub fn has_children(&self) -> bool {
        self.children[0].is_some()
            || self.children[1].is_some()
            || self.children[2].is_some()
            || self.children[3].is_some()
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
        let mut lo = i16::MAX;
        let mut hi = i16::MIN;
        for row in self.data.iter() {
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

    pub fn is_empty_tile(&self) -> bool {
        let (lo, hi) = self.find_sampled_extremes();
        lo == 0 && hi == 0
    }

    // Set a sample, offset in samples from the base corner.
    pub fn set_sample(&mut self, lat_offset: i32, lon_offset: i32, sample: i16) {
        self.data[lat_offset as usize][lon_offset as usize] = sample;
    }

    pub fn save_equalized_png(&self, directory: &Path) -> Fallible<()> {
        if self.is_empty_tile() {
            return Ok(());
        }
        let path = self.filename(directory);

        let (_, high) = self.find_sampled_extremes();

        let mut pic: ImageBuffer<Luma<u8>, Vec<u8>> = ImageBuffer::new(512, 512);
        for (y, row) in self.data.iter().enumerate() {
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

    pub fn write(&self, directory: &Path) -> Fallible<()> {
        if self.is_empty_tile() {
            return Ok(());
        }
        let path = self.filename(directory);
        if !path.parent().expect("subdir").exists() {
            fs::create_dir(path.parent().expect("subdir"))?;
        }
        let mut fp = File::create(&path)?;
        fp.write_all(self.data.as_bytes())?;
        Ok(())
    }
}
