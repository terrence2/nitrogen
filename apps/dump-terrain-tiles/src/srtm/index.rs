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
use crate::{mip::Region, srtm::tile::Tile};
use absolute_unit::{degrees, Degrees};
use failure::Fallible;
use geodesy::{GeoCenter, Graticule};
use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

pub struct Index {
    tiles: Vec<Tile>,
    // by latitude, then longitude
    by_graticule: HashMap<i16, HashMap<i16, usize>>,
}

impl Index {
    pub fn from_directory(directory: &Path) -> Fallible<Arc<RwLock<Self>>> {
        let mut index_filename = PathBuf::from(directory);
        index_filename.push("srtm30m_bounding_boxes.json");

        let mut index_file = File::open(index_filename.as_path())?;
        let mut index_content = String::new();
        index_file.read_to_string(&mut index_content)?;

        let index_json = json::parse(&index_content)?;
        assert_eq!(index_json["type"], "FeatureCollection");
        let features = &index_json["features"];
        let mut tiles = Vec::new();
        for feature in features.members() {
            let tile = Tile::from_feature(&feature, directory)?;
            tiles.push(tile);
        }

        let mut by_graticule = HashMap::new();
        for (i, tile) in tiles.iter().enumerate() {
            let lon = tile.longitude();
            by_graticule
                .entry(tile.latitude())
                .or_insert_with(HashMap::new)
                .insert(lon, i);
        }

        println!("Mapped {} SRTM tiles", tiles.len());
        Ok(Arc::new(RwLock::new(Self {
            tiles,
            by_graticule,
        })))
    }

    /// Check if this tile-set has any tiles that overlap with the given region.
    pub fn contains_region(&self, region: Region) -> bool {
        // Figure out what integer latitudes lie on or in the region.
        let lo_lat = region.base.lat::<Degrees>().floor() as i16;
        let hi_lat = degrees!(region.base.latitude + region.extent).floor() as i16;
        let lo_lon = region.base.lon::<Degrees>().floor() as i16;
        let hi_lon = degrees!(region.base.longitude + region.extent).floor() as i16;
        for lat in lo_lat..=hi_lat {
            if let Some(by_lon) = self.by_graticule.get(&lat) {
                for lon in lo_lon..=hi_lon {
                    if by_lon.get(&lon).is_some() {
                        return true;
                    }
                }
            }
        }
        false
    }

    #[allow(unused)]
    pub fn sample_linear(&self, grat: &Graticule<GeoCenter>) -> f32 {
        let lat = Tile::index(grat.lat());
        let lon = Tile::index(grat.lon());
        if let Some(row) = self.by_graticule.get(&lat) {
            if let Some(&tile_id) = row.get(&lon) {
                return self.tiles[tile_id].sample_linear(grat);
            }
        }
        0f32
    }

    pub fn sample_nearest(&self, grat: &Graticule<GeoCenter>) -> i16 {
        let lat = Tile::index(grat.lat());
        let lon = Tile::index(grat.lon());
        if let Some(row) = self.by_graticule.get(&lat) {
            if let Some(&tile_id) = row.get(&lon) {
                return self.tiles[tile_id].sample_nearest(grat);
            }
        }
        0
    }
}
