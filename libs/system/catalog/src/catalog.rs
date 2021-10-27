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
use crate::{DrawerFileId, DrawerInterface, FileMetadata};
use anyhow::{ensure, Result};
use glob::{MatchOptions, Pattern};
use log::debug;
use smallvec::SmallVec;
use std::{borrow::Cow, collections::HashMap, ops::Range};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
struct DrawerId(u16);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
struct ShelfId(u16);

pub fn from_utf8_string(input: Cow<[u8]>) -> Result<Cow<str>> {
    Ok(match input {
        Cow::Borrowed(r) => Cow::from(std::str::from_utf8(r)?),
        Cow::Owned(o) => Cow::from(String::from_utf8(o)?),
    })
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct FileId {
    drawer_file_id: DrawerFileId,
    drawer_id: DrawerId,
}

/// A catalog is a uniform, indexed interface to a collection of Drawers. This
/// allows a game engine to expose several sources of data through a single interface.
/// Common uses are allowing loose files when developing, while shipping compacted
/// asset packs in production and combining data from multiple packs, e.g. when assets
/// are shipped on multiple disks, or where assets may get extended and overridden
/// with mod content.
pub struct Catalog {
    label: String,
    last_drawer: u16,
    drawer_index: HashMap<(i64, String), DrawerId>,
    drawers: HashMap<DrawerId, Box<dyn DrawerInterface>>,
    index: HashMap<String, FileId>,
}

impl Catalog {
    /// Create and return a new empty catalog.
    pub fn empty<S: ToString>(label: S) -> Self {
        Self {
            label: label.to_string(),
            last_drawer: 0,
            drawer_index: HashMap::new(),
            drawers: HashMap::new(),
            index: HashMap::new(),
        }
    }

    /// Create a new catalog with the given drawers.
    pub fn with_drawers<S: ToString>(
        label: S,
        mut drawers: Vec<Box<dyn DrawerInterface>>,
    ) -> Result<Self> {
        let mut catalog = Self::empty(label);
        for drawer in drawers.drain(..) {
            catalog.add_drawer(drawer)?;
        }
        Ok(catalog)
    }

    /// Return the catalog's label; useful if the app is juggling multiple catalogs.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Add a drawer full of files to the catalog.
    pub fn add_drawer(&mut self, drawer: Box<dyn DrawerInterface>) -> Result<()> {
        debug!("add_drawer: {}:{}", self.label, drawer.name());
        let next_priority = drawer.priority();
        let index = drawer.index()?;
        let drawer_key = (drawer.priority(), drawer.name().to_owned());
        ensure!(
            !self.drawer_index.contains_key(&drawer_key),
            "duplicate drawer added"
        );
        let drawer_id = DrawerId(self.last_drawer);
        self.last_drawer += 1;
        self.drawer_index.insert(drawer_key, drawer_id);
        self.drawers.insert(drawer_id, drawer);
        for (&drawer_file_id, name) in index.iter() {
            if self.index.contains_key(name) {
                let prior_drawer = self.index[name].drawer_id;
                let prior_priority = self.drawers[&prior_drawer].priority();
                // If there is already a higher priority entry, skip indexing the new version.
                if next_priority < prior_priority {
                    continue;
                }
            }
            self.index.insert(
                name.to_owned(),
                FileId {
                    drawer_file_id,
                    drawer_id,
                },
            );
        }
        Ok(())
    }

    /// Check if the given name exists in the catalog.
    pub fn exists(&self, name: &str) -> bool {
        let exists = self.index.contains_key(name);
        debug!("exists {}:{} => {}", self.label, name, exists);
        exists
    }

    /// Get the file id of the given name.
    pub fn lookup(&self, name: &str) -> Option<FileId> {
        self.index.get(name).cloned()
    }

    /// Search for and return the FileID of all items matching the given glob.
    pub fn find_glob(&self, glob: &str) -> Result<Vec<FileId>> {
        debug!("find_matching_names({})", glob);
        let mut matching = Vec::new();
        let opts = MatchOptions {
            case_sensitive: false,
            require_literal_leading_dot: false,
            require_literal_separator: true,
        };
        let pattern = Pattern::new(glob)?;
        for (key, &fid) in self.index.iter() {
            if pattern.matches_with(key, opts) {
                matching.push(fid);
            }
        }
        Ok(matching)
    }

    /// Return all fids that have the given extension, insensitive.
    pub fn find_with_extension(&self, ext: &str) -> Result<Vec<FileId>> {
        debug!("{}:find_with_extension({})", self.label, ext);
        let mut matching = vec![];
        let pattern = ".".to_owned() + ext;
        let pattern_upper = pattern.to_uppercase();
        let pattern_lower = pattern.to_lowercase();
        for (key, fid) in self.index.iter() {
            if key.ends_with(&pattern_upper) || key.ends_with(&pattern_lower) {
                matching.push(*fid);
            }
        }
        Ok(matching)
    }

    /// Find all FileID that match the given glob and have an extension, case sensitive.
    /// This can be significantly faster than find_glob, if the extension is constant.
    pub fn find_glob_with_extension(
        &self,
        glob: &str,
        with_extension: Option<&str>,
    ) -> Result<SmallVec<[FileId; 4]>> {
        debug!("find_matching({}, {:?})", glob, with_extension);
        let mut matching = SmallVec::new();
        let opts = MatchOptions {
            case_sensitive: false,
            require_literal_leading_dot: false,
            require_literal_separator: true,
        };
        let pattern = Pattern::new(glob)?;
        if let Some(ext) = with_extension {
            for (key, fid) in self.index.iter() {
                if key.ends_with(ext) && pattern.matches_with(key, opts) {
                    matching.push(*fid);
                }
            }
        } else {
            for (key, fid) in self.index.iter() {
                if pattern.matches_with(key, opts) {
                    matching.push(*fid);
                }
            }
        }
        Ok(matching)
    }

    /// Get metadata about the given file by id.
    pub fn stat_sync(&self, fid: FileId) -> Result<FileMetadata> {
        let drawer_meta = self.drawers[&fid.drawer_id].stat_sync(fid.drawer_file_id)?;
        Ok(FileMetadata::from_drawer(fid, drawer_meta))
    }

    /// Get metadata about the given file by name.
    pub fn stat_name_sync(&self, name: &str) -> Result<FileMetadata> {
        ensure!(self.index.contains_key(name), "file not found");
        self.stat_sync(self.index[name])
    }

    /// Read the given file id and return the contents. Blocks until complete.
    pub fn read_sync(&self, fid: FileId) -> Result<Cow<[u8]>> {
        self.drawers[&fid.drawer_id].read_sync(fid.drawer_file_id)
    }

    /// Read the given file id and return the contents. Blocks until complete.
    pub fn read_slice_sync(&self, fid: FileId, extent: Range<usize>) -> Result<Cow<[u8]>> {
        self.drawers[&fid.drawer_id].read_slice_sync(fid.drawer_file_id, extent)
    }

    /// Read the given file id and return a Future with the contents.
    pub async fn read(&self, fid: FileId) -> Result<Vec<u8>> {
        Ok(self.drawers[&fid.drawer_id]
            .read(fid.drawer_file_id)
            .await?)
    }

    /// Read the given file id and return a Future with the given slice from that file.
    pub async fn read_slice(&self, fid: FileId, extent: Range<usize>) -> Result<Vec<u8>> {
        Ok(self.drawers[&fid.drawer_id]
            .read_slice(fid.drawer_file_id, extent)
            .await?)
    }

    /// Read the given file name and return the contents. Blocks until complete.
    pub fn read_name_sync(&self, name: &str) -> Result<Cow<[u8]>> {
        ensure!(self.index.contains_key(name), "file not found: {}", name);
        self.read_sync(self.index[name])
    }

    /// Print out the structure of the catalog to stdout.
    #[allow(unused)]
    pub fn dump_layout(&self) {
        println!("Catalog:");
        let mut drawers = vec![];
        for (drawer_prio, drawer_name) in self.drawer_index.keys() {
            drawers.push((drawer_prio, drawer_name));
        }
        drawers.sort();
        for (drawer_prio, drawer_name) in &drawers {
            println!("    Shelf {} - {}", drawer_prio, drawer_name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DirectoryDrawer;
    use std::path::PathBuf;

    #[test]
    fn test_basic_functionality() -> Result<()> {
        let mut catalog = Catalog::with_drawers(
            "main",
            vec![DirectoryDrawer::from_directory(0, "./masking_test_data/a")?],
        )?;

        // Expect success
        let meta = catalog.stat_name_sync("a.txt")?;
        assert_eq!(meta.name(), "a.txt");
        assert_eq!(
            meta.path(),
            Some(PathBuf::from("./masking_test_data/a/a.txt").as_path())
        );
        let data = catalog.read_name_sync("a.txt")?;
        assert_eq!(data, b"hello" as &[u8]);

        // Missing file
        assert!(catalog.stat_name_sync("a_long_and_silly_name").is_err());
        // Present, but a directory.
        assert!(catalog.stat_name_sync("nested").is_err());

        // Add a second drawer with lower priority.
        catalog.add_drawer(DirectoryDrawer::from_directory(
            -1,
            "./masking_test_data/b",
        )?)?;
        let meta = catalog.stat_name_sync("a.txt")?;
        assert_eq!(meta.name(), "a.txt");
        assert_eq!(
            meta.path(),
            Some(PathBuf::from("./masking_test_data/a/a.txt").as_path())
        );
        let data = catalog.read_name_sync("a.txt")?;
        assert_eq!(data, b"hello" as &[u8]);

        // Add a third drawer with higher priority.
        catalog.add_drawer(DirectoryDrawer::from_directory(1, "./masking_test_data/b")?)?;
        let meta = catalog.stat_name_sync("a.txt")?;
        assert_eq!(meta.name(), "a.txt");
        assert_eq!(
            meta.path(),
            Some(PathBuf::from("./masking_test_data/b/a.txt").as_path())
        );
        let data = catalog.read_name_sync("a.txt")?;
        assert_eq!(data, b"world" as &[u8]);

        Ok(())
    }

    #[tokio::test]
    async fn test_async_functionality() -> Result<()> {
        let catalog = Catalog::with_drawers(
            "main",
            vec![DirectoryDrawer::from_directory(0, "./masking_test_data/a")?],
        )?;

        let meta = catalog.stat_name_sync("a.txt")?;
        let data = catalog.read(meta.id()).await?;
        assert_eq!(data, b"hello" as &[u8]);

        Ok(())
    }
}
