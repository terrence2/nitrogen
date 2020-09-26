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
use crate::mip::tile::Tile;
use failure::Fallible;
use json::JsonValue;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
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
        name: &str,
        kind: DataSetDataKind,
        coordinates: DataSetCoordinates,
    ) -> Fallible<Arc<RwLock<IndexDataSet>>> {
        let mut path = self.path.clone();
        path.push(name);

        let ds = Arc::new(RwLock::new(IndexDataSet::new(
            name,
            &path,
            kind,
            coordinates,
        )?));
        self.data_sets.insert(name.to_owned(), ds.clone());
        Ok(ds)
    }
}

pub struct IndexDataSet {
    prefix: String,
    path: PathBuf,
    work_path: PathBuf,
    kind: DataSetDataKind,
    coordinates: DataSetCoordinates,
    root: Arc<RwLock<Tile>>,
}

impl IndexDataSet {
    fn new(
        prefix: &str,
        path: &Path,
        kind: DataSetDataKind,
        coordinates: DataSetCoordinates,
    ) -> Fallible<Self> {
        if !path.exists() {
            fs::create_dir(path)?;
        }
        Ok(Self {
            prefix: prefix.to_owned(),
            path: path.to_owned(),
            work_path: path.join("work"),
            kind,
            coordinates,
            root: Arc::new(RwLock::new(Tile::new_uninitialized(
                prefix,
                TerrainLevel::new(0),
                ChildIndex::SouthWest,
                &TerrainLevel::base(),
                TerrainLevel::base_angular_extent(),
            ))),
        })
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

    pub fn as_json(&self) -> Fallible<JsonValue> {
        let mut obj = JsonValue::new_object();
        obj.insert::<&str>("prefix", &self.prefix)?;
        obj.insert("kind", self.kind.name())?;
        obj.insert("coordinates", self.coordinates.name())?;
        // let mut index = JsonValue::new_object();
        // let mut absolute_base = JsonValue::new_object();
        // absolute_base.insert("latitude_arcseconds", arcseconds!(self.root.read().unwrap().base_corner_graticule().latitude()).f64());
        // absolute_base.insert("longitude_arcseconds", arcseconds!(self.root.read().unwrap().base_corner_graticule().longitude()).f64());
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
