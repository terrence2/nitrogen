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
use crate::mip::{DataSource, Region};
use absolute_unit::{degrees, ArcSeconds, Degrees};
use failure::Fallible;
use geodesy::{GeoSurface, Graticule};
use image::{open, EncodableLayout, ImageBuffer, Rgb};
use memmap::{Mmap, MmapOptions};
use parking_lot::RwLock;
use std::{fs::File, io::Write, ops::Range, path::Path, sync::Arc};
use terrain_geo::tile::TerrainLevel;

type MmapRgbImage = ImageBuffer<Rgb<u8>, Mmap>;

pub struct Index {
    raw: Vec<Vec<MmapRgbImage>>,
}

impl Index {
    pub const TILE_SIZE: u32 = 21_600;

    pub fn from_directory(directory: &Path) -> Fallible<Arc<RwLock<Self>>> {
        let mut raw = Vec::new();
        for (_lon, lon_name) in "ABCD".chars().enumerate() {
            let mut inner = Vec::new();
            for (_lat, lat_name) in "12".chars().enumerate() {
                let raw_filename = format!(
                    "world.topo.200405.3x21600x21600.{}{}.raw",
                    lon_name, lat_name
                );
                let mut raw_path = directory.to_owned();
                raw_path.push(raw_filename);

                if !raw_path.exists() {
                    let png_path = raw_path.with_extension("png");
                    println!("copying {:?} to {:?}...", png_path, raw_path);
                    let img = open(&png_path)?;
                    {
                        let mut raw_fp = File::create(&raw_path)?;
                        raw_fp.write_all(img.as_rgb8().unwrap().as_bytes())?;
                    }
                }

                let raw_fp = File::open(&raw_path)?;
                let raw_mmap = unsafe { MmapOptions::new().map(&raw_fp)? };
                let buf =
                    ImageBuffer::from_raw(Self::TILE_SIZE, Self::TILE_SIZE, raw_mmap).unwrap();
                inner.push(buf);
            }
            raw.push(inner.into());
        }
        Ok(Arc::new(RwLock::new(Self { raw })))
    }
}

impl DataSource for Index {
    fn contains_region(&self, region: &Region) -> bool {
        // FIXME: do we need to filter our regions outside of standard coordinates?
        if region.base.lon::<Degrees>() > degrees!(180) {
            return false;
        }
        if region.base.lon::<Degrees>() + degrees!(region.extent) < degrees!(-180) {
            return false;
        }
        if region.base.lat::<Degrees>() > degrees!(90) {
            return false;
        }
        if region.base.lat::<Degrees>() + degrees!(region.extent) < degrees!(-90) {
            return false;
        }
        true
    }

    fn root_level(&self) -> TerrainLevel {
        // 12 -> 1 as
        // 11 -> 2 as
        // 10 -> 4 as
        // 9 -> 8 as
        // 8 -> 16 as
        TerrainLevel::new(8)
    }

    // Number of tiles per layer not filtered out above.
    fn expect_intersecting_tiles(&self, layer: usize) -> usize {
        const EXPECT_LAYER_COUNTS: [usize; 13] =
            [1, 4, 8, 24, 60, 200, 800, 3_200, 12_800, 0, 0, 0, 0];
        EXPECT_LAYER_COUNTS[layer]
    }

    fn expect_present_tiles(&self, layer: usize) -> Range<usize> {
        let a = self.expect_intersecting_tiles(layer);
        a..a
    }

    fn sample_nearest_height(&self, _grat: &Graticule<GeoSurface>) -> i16 {
        0
    }

    fn compute_local_normal(&self, _grat: &Graticule<GeoSurface>) -> [i16; 2] {
        [0; 2]
    }

    fn sample_color(&self, grat: &Graticule<GeoSurface>) -> Rgb<u8> {
        // FIXME: assert that our grat is over a reasonable range
        assert!(grat.lon::<Degrees>() < degrees!(190));
        assert!(grat.lon::<Degrees>() > degrees!(-190));
        assert!(grat.lat::<Degrees>() < degrees!(100));
        assert!(grat.lat::<Degrees>() > degrees!(-100));
        let lat_px = ((grat.lat::<ArcSeconds>().f64() + 324_000f64) / 15f64).round();
        let lon_px = ((grat.lon::<ArcSeconds>().f64() + 648_000f64) / 15f64).round();
        let lon_img = ((lon_px / 21_600f64).floor() as i64) % 4;
        let lat_img = ((lat_px / 21_600f64).floor() as i64) % 2;
        // assert!(lon_img >= 0);
        // assert!(lon_img <= 3);
        // assert!(lat_img >= 0);
        // assert!(lat_img <= 1);
        let lon_off = lon_px as i64 % 21_600;
        let lat_off = lat_px as i64 % 21_600;
        *self.raw[lon_img as usize][lat_img as usize].get_pixel(lon_off as u32, lat_off as u32)
    }
}
