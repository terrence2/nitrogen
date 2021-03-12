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
use anyhow::{bail, ensure, Result};
use glob::{MatchOptions, Pattern};
use log::debug;
use smallvec::SmallVec;
use std::{borrow::Cow, collections::HashMap, ops::Range};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
struct DrawerId(u16);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
struct ShelfId(u16);

pub const DEFAULT_LABEL: &str = "default";

pub fn from_utf8_string(input: Cow<[u8]>) -> Result<Cow<str>> {
    Ok(match input {
        Cow::Borrowed(r) => Cow::from(std::str::from_utf8(r)?),
        Cow::Owned(o) => Cow::from(String::from_utf8(o)?),
    })
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct FileId {
    drawer_file_id: DrawerFileId,
    shelf_id: ShelfId,
    drawer_id: DrawerId,
}

pub struct Catalog {
    last_shelf: u16,
    shelf_index: HashMap<String, ShelfId>,
    shelves: HashMap<ShelfId, Shelf>,
    default_label: String,
}

// A catalog is a uniform, indexed interface to a collection of Drawers. This
// allows a game engine to expose several sources of data through a single interface.
// Common uses are allowing loose files when developing, while shipping compacted
// asset packs in production and combining data from multiple packs, e.g. when assets
// are shipped on multiple disks, or where assets may get extended and overridden
// with mod content.
//
// In addition to the above behavior, files in a catalog may be tagged with a label,
// allowing for multiple sets of unrelated data. This is useful for testing multiple
// file-sets at once, e.g. in a multi-game situation.
impl Catalog {
    /// Create and return a new empty catalog.
    pub fn empty() -> Self {
        let shelf_id = ShelfId(0);
        let mut shelf_index = HashMap::new();
        shelf_index.insert(DEFAULT_LABEL.to_owned(), shelf_id);
        let mut shelves = HashMap::new();
        shelves.insert(shelf_id, Shelf::empty());
        Self {
            last_shelf: 1,
            shelf_index,
            shelves,
            default_label: DEFAULT_LABEL.to_owned(),
        }
    }

    /// Create a new catalog with the given drawers.
    pub fn with_drawers(mut drawers: Vec<Box<dyn DrawerInterface>>) -> Result<Self> {
        let mut catalog = Self::empty();
        for drawer in drawers.drain(..) {
            catalog.add_labeled_drawer(DEFAULT_LABEL, drawer)?;
        }
        Ok(catalog)
    }

    /// Add a drawer full of files to the catalog.
    pub fn add_drawer(&mut self, drawer: Box<dyn DrawerInterface>) -> Result<()> {
        self.add_labeled_drawer(&self.default_label.clone(), drawer)
    }

    /// Get the label of a given file by id.
    pub fn file_label(&self, fid: FileId) -> Result<String> {
        for (name, &sid) in &self.shelf_index {
            if fid.shelf_id == sid {
                return Ok(name.to_owned());
            }
        }
        bail!("unknown shelf")
    }

    /// Check if the given name exists in the catalog.
    pub fn exists(&self, name: &str) -> bool {
        self.exists_labeled(&self.default_label, name)
    }

    /// Get the file id of the given name.
    pub fn lookup(&self, name: &str) -> Option<FileId> {
        self.lookup_labeled(&self.default_label, name)
    }

    /// Find all files that match the given glob. If with_extension is provided, the name must also
    /// have the given extension. This can greatly improve the speed of matching, if the extension
    /// is known up front.
    pub fn find_matching(
        &self,
        glob: &str,
        with_extension: Option<&str>,
    ) -> Result<SmallVec<[FileId; 4]>> {
        self.find_labeled_matching(&self.default_label, glob, with_extension)
    }

    // TODO: replace uses and remove.
    pub fn find_matching_names(&self, glob: &str) -> Result<Vec<String>> {
        self.find_labeled_matching_names(&self.default_label, glob)
    }

    /// Get metadata about the given file by id.
    pub fn stat_sync(&self, fid: FileId) -> Result<FileMetadata> {
        self.shelves[&fid.shelf_id].stat_sync(fid)
    }

    /// Read the given file id and return the contents. Blocks until complete.
    pub fn read_sync(&self, fid: FileId) -> Result<Cow<[u8]>> {
        self.shelves[&fid.shelf_id].read_sync(fid)
    }

    /// Read the given file id and return the contents. Blocks until complete.
    pub fn read_slice_sync(&self, fid: FileId, extent: Range<usize>) -> Result<Cow<[u8]>> {
        self.shelves[&fid.shelf_id].read_slice_sync(fid, extent)
    }

    /// Read the given file id and return a Future with the contents.
    pub async fn read(&self, fid: FileId) -> Result<Vec<u8>> {
        Ok(self.shelves[&fid.shelf_id].read(fid).await?)
    }

    /// Read the given file id and return a Future with the given slice from that file.
    pub async fn read_slice(&self, fid: FileId, extent: Range<usize>) -> Result<Vec<u8>> {
        Ok(self.shelves[&fid.shelf_id].read_slice(fid, extent).await?)
    }

    /// Get metadata about the given file by name.
    pub fn stat_name_sync(&self, name: &str) -> Result<FileMetadata> {
        self.stat_labeled_name_sync(&self.default_label, name)
    }

    /// Read the given file name and return the contents. Blocks until complete.
    pub fn read_name_sync(&self, name: &str) -> Result<Cow<[u8]>> {
        self.read_labeled_name_sync(&self.default_label, name)
    }

    pub fn add_labeled_drawer(
        &mut self,
        label: &str,
        drawer: Box<dyn DrawerInterface>,
    ) -> Result<()> {
        debug!("add_labeled_drawer: {}, {}", label, drawer.name());
        if !self.shelf_index.contains_key(label) {
            let shelf_id = ShelfId(self.last_shelf);
            self.last_shelf += 1;
            self.shelf_index.insert(label.to_owned(), shelf_id);
            self.shelves.insert(shelf_id, Shelf::empty());
        }
        let shelf_id = self.shelf_index[label];
        self.shelves
            .get_mut(&shelf_id)
            .unwrap()
            .add_drawer(shelf_id, drawer)
    }

    pub fn find_labeled_matching(
        &self,
        label: &str,
        glob: &str,
        with_extension: Option<&str>,
    ) -> Result<SmallVec<[FileId; 4]>> {
        self.shelves[&self.shelf_index[label]].find_matching(glob, with_extension)
    }

    pub fn exists_labeled(&self, label: &str, name: &str) -> bool {
        let exists = self.shelves[&self.shelf_index[label]]
            .index
            .contains_key(name);
        debug!("exists_labeled {}:{} => {}", label, name, exists);
        exists
    }

    pub fn lookup_labeled(&self, label: &str, name: &str) -> Option<FileId> {
        self.shelves[&self.shelf_index[label]]
            .index
            .get(name)
            .copied()
    }

    pub fn find_labeled_matching_names(&self, label: &str, glob: &str) -> Result<Vec<String>> {
        self.shelves[&self.shelf_index[label]].find_matching_names(glob)
    }

    pub fn stat_labeled_name_sync(&self, label: &str, name: &str) -> Result<FileMetadata> {
        self.shelves[&self.shelf_index[label]].stat_name_sync(name)
    }

    pub fn read_labeled_name_sync(&self, label: &str, name: &str) -> Result<Cow<[u8]>> {
        self.shelves[&self.shelf_index[label]].read_name_sync(name)
    }

    pub fn default_label(&self) -> &str {
        &self.default_label
    }

    pub fn set_default_label(&mut self, context: &str) {
        assert!(
            self.shelf_index.contains_key(context),
            "cannot set default label to unknown shelf: {}",
            context
        );
        self.default_label = context.to_owned()
    }

    #[allow(unused)]
    pub fn dump_layout(&self) {
        println!("Catalog:");
        for (shelf_name, shelf_id) in &self.shelf_index {
            println!("  Shelf {}", shelf_name);
            for (drawer_prio, drawer_name) in self.shelves[shelf_id].drawer_index.keys() {
                println!("    Drawer {} - {}", drawer_prio, drawer_name,);
            }
        }
    }
}

// A shelf is a subset of a catalog that contains the same label.
pub struct Shelf {
    last_drawer: u16,
    drawer_index: HashMap<(i64, String), DrawerId>,
    drawers: HashMap<DrawerId, Box<dyn DrawerInterface>>,
    index: HashMap<String, FileId>,
}

impl Shelf {
    pub fn empty() -> Self {
        Self {
            last_drawer: 0,
            drawer_index: HashMap::new(),
            drawers: HashMap::new(),
            index: HashMap::new(),
        }
    }

    fn add_drawer(&mut self, shelf_id: ShelfId, drawer: Box<dyn DrawerInterface>) -> Result<()> {
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
                    shelf_id,
                    drawer_id,
                },
            );
        }
        Ok(())
    }

    pub fn find_matching(
        &self,
        glob: &str,
        with_extension: Option<&str>,
    ) -> Result<SmallVec<[FileId; 4]>> {
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

    pub fn find_matching_names(&self, glob: &str) -> Result<Vec<String>> {
        let mut matching = Vec::new();
        let opts = MatchOptions {
            case_sensitive: false,
            require_literal_leading_dot: false,
            require_literal_separator: true,
        };
        let pattern = Pattern::new(glob)?;
        for key in self.index.keys() {
            if pattern.matches_with(key, opts) {
                matching.push(key.to_owned());
            }
        }
        Ok(matching)
    }

    pub fn stat_sync(&self, fid: FileId) -> Result<FileMetadata> {
        let drawer_meta = self.drawers[&fid.drawer_id].stat_sync(fid.drawer_file_id)?;
        Ok(FileMetadata::from_drawer(fid, drawer_meta))
    }

    pub fn stat_name_sync(&self, name: &str) -> Result<FileMetadata> {
        ensure!(self.index.contains_key(name), "file not found");
        self.stat_sync(self.index[name])
    }

    pub fn read_sync(&self, fid: FileId) -> Result<Cow<[u8]>> {
        self.drawers[&fid.drawer_id].read_sync(fid.drawer_file_id)
    }

    pub fn read_slice_sync(&self, fid: FileId, extent: Range<usize>) -> Result<Cow<[u8]>> {
        self.drawers[&fid.drawer_id].read_slice_sync(fid.drawer_file_id, extent)
    }

    pub async fn read(&self, fid: FileId) -> Result<Vec<u8>> {
        Ok(self.drawers[&fid.drawer_id]
            .read(fid.drawer_file_id)
            .await?)
    }

    pub async fn read_slice(&self, fid: FileId, extent: Range<usize>) -> Result<Vec<u8>> {
        Ok(self.drawers[&fid.drawer_id]
            .read_slice(fid.drawer_file_id, extent)
            .await?)
    }

    pub fn read_name_sync(&self, name: &str) -> Result<Cow<[u8]>> {
        ensure!(self.index.contains_key(name), "file not found");
        self.read_sync(self.index[name])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DirectoryDrawer;
    use std::path::PathBuf;

    #[test]
    fn test_basic_functionality() -> Result<()> {
        let mut catalog = Catalog::with_drawers(vec![DirectoryDrawer::from_directory(
            0,
            "./masking_test_data/a",
        )?])?;

        // Expect success
        let meta = catalog.stat_name_sync("a.txt")?;
        assert_eq!(meta.name, "a.txt");
        assert_eq!(
            meta.path,
            Some(PathBuf::from("./masking_test_data/a/a.txt"))
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
        assert_eq!(meta.name, "a.txt");
        assert_eq!(
            meta.path,
            Some(PathBuf::from("./masking_test_data/a/a.txt"))
        );
        let data = catalog.read_name_sync("a.txt")?;
        assert_eq!(data, b"hello" as &[u8]);

        // Add a third drawer with higher priority.
        catalog.add_drawer(DirectoryDrawer::from_directory(1, "./masking_test_data/b")?)?;
        let meta = catalog.stat_name_sync("a.txt")?;
        assert_eq!(meta.name, "a.txt");
        assert_eq!(
            meta.path,
            Some(PathBuf::from("./masking_test_data/b/a.txt"))
        );
        let data = catalog.read_name_sync("a.txt")?;
        assert_eq!(data, b"world" as &[u8]);

        Ok(())
    }

    #[tokio::test]
    async fn test_async_functionality() -> Result<()> {
        let catalog = Catalog::with_drawers(vec![DirectoryDrawer::from_directory(
            0,
            "./masking_test_data/a",
        )?])?;

        let meta = catalog.stat_name_sync("a.txt")?;
        let data = catalog.read(meta.id).await?;
        assert_eq!(data, b"hello" as &[u8]);

        Ok(())
    }
}
