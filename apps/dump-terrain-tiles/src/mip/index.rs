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
use crate::mip::{tile::Tile, DataSource};
use absolute_unit::ArcSeconds;
use failure::Fallible;
use json::JsonValue;
use parking_lot::RwLock;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use terrain_geo::tile::{ChildIndex, DataSetCoordinates, DataSetDataKind, TerrainLevel};

// Files in Catalogs are flat, so each dataset gets its own unique prefix. All datasets can
// be found by looking for *metadata.json. Each level is laid out flat in the dataset, so the
// full level can be listed as <dataset>-L<lvl>-*.geo. Not that you generally want to do this.
// The dataset gets rebuilt by looking for specific files from the root down and building a
// matching tree. The metadata file only contains meta information about the dataset; the
// type of data it contains, extents, project, etc. The quad tree should get rebuilt from file
// names only.

pub struct Index {
    path: PathBuf,
    data_sets: HashMap<String, Arc<RwLock<IndexDataSet>>>,
}

impl Index {
    // Note: we do not try to discover data sets since they may be incomplete at this point.
    // Discovery of existing resources is left up to the builders.
    pub fn empty(path: &Path) -> Self {
        Self {
            path: path.to_owned(),
            data_sets: HashMap::new(),
        }
    }

    pub fn add_data_set(
        &mut self,
        prefix: &'static str,
        kind: DataSetDataKind,
        coordinates: DataSetCoordinates,
        source: Arc<RwLock<dyn DataSource>>,
    ) -> Fallible<Arc<RwLock<IndexDataSet>>> {
        let mut path = self.path.clone();
        path.push(&prefix);

        let ds = Arc::new(RwLock::new(IndexDataSet::new(
            prefix,
            &path,
            kind,
            coordinates,
            source,
        )?));
        self.data_sets.insert(prefix.to_owned(), ds.clone());
        Ok(ds)
    }

    pub fn data_sets(&self, coordinates: DataSetCoordinates) -> Vec<Arc<RwLock<IndexDataSet>>> {
        let mut dss = self
            .data_sets
            .values()
            .filter(|ds| ds.read().coordinates() == coordinates)
            .cloned()
            .collect::<Vec<_>>();
        dss.sort_by_key(|ds| ds.read().prefix().to_owned());
        dss
    }
}

pub struct IndexDataSet {
    prefix: &'static str,
    path: PathBuf,
    work_path: PathBuf,
    kind: DataSetDataKind,
    coordinates: DataSetCoordinates,
    root: Arc<RwLock<Tile>>,
    source: Arc<RwLock<dyn DataSource>>,
}

impl IndexDataSet {
    fn new(
        prefix: &'static str,
        path: &Path,
        kind: DataSetDataKind,
        coordinates: DataSetCoordinates,
        source: Arc<RwLock<dyn DataSource>>,
    ) -> Fallible<Self> {
        let mut work_dir = path.to_owned();
        work_dir.push("work");
        if !work_dir.exists() {
            fs::create_dir_all(work_dir)?;
        }
        Ok(Self {
            prefix,
            path: path.to_owned(),
            work_path: path.join("work"),
            kind,
            coordinates,
            root: Arc::new(RwLock::new(Tile::new_uninitialized(
                prefix,
                TerrainLevel::new(0),
                ChildIndex::SouthWest,
                (
                    TerrainLevel::base().lat::<ArcSeconds>().round() as i32,
                    TerrainLevel::base().lon::<ArcSeconds>().round() as i32,
                ),
                TerrainLevel::base_angular_extent().round() as i32,
            ))),
            source,
        })
    }

    pub fn coordinates(&self) -> DataSetCoordinates {
        self.coordinates
    }

    pub fn kind(&self) -> DataSetDataKind {
        self.kind
    }

    pub fn source(&self) -> Arc<RwLock<dyn DataSource>> {
        self.source.clone()
    }

    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    pub fn base_path(&self) -> &Path {
        &self.path
    }

    pub fn work_path(&self) -> &Path {
        &self.work_path
    }

    pub fn get_root_tile(&mut self) -> Arc<RwLock<Tile>> {
        self.root.clone()
    }

    pub fn lookup(&mut self, level: usize, base: (i32, i32)) -> Option<Arc<RwLock<Tile>>> {
        self.root.read().lookup(level, base)
    }

    pub fn as_json(&self) -> Fallible<JsonValue> {
        let mut obj = JsonValue::new_object();
        obj.insert::<&str>("prefix", &self.prefix)?;
        obj.insert("kind", self.kind.name())?;
        obj.insert("coordinates", self.coordinates.name())?;
        // let mut index = JsonValue::new_object();
        // let mut absolute_base = JsonValue::new_object();
        // absolute_base.insert("latitude_arcseconds", arcseconds!(self.root.read().base_corner_graticule().latitude()).f64());
        // absolute_base.insert("longitude_arcseconds", arcseconds!(self.root.read().base_corner_graticule().longitude()).f64());
        // index.insert("absolute_base", absolue_base);
        Ok(obj)
    }

    pub fn write(&self) -> Fallible<()> {
        let mut filename = self.path.clone();
        filename.push(&format!("{}-index.json", self.prefix));
        fs::write(filename, self.as_json()?.to_string())?;
        Ok(())
    }
}
