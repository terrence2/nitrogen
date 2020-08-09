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
use crate::{DrawerFileId, DrawerFileMetadata, DrawerInterface};
use async_trait::async_trait;
use failure::{ensure, Fallible};
use std::{borrow::Cow, collections::HashMap, ffi::OsStr, fs, io::Read, path::PathBuf};
use tokio::fs::File as TokioFile;
use tokio::io::AsyncReadExt;

pub struct DirectoryDrawer {
    name: String,
    priority: i64,
    path: PathBuf,
    index: HashMap<DrawerFileId, String>,
}

impl DirectoryDrawer {
    fn populate_from_directory(&mut self, only_extension: Option<&str>) -> Fallible<()> {
        for (i, entry) in fs::read_dir(&self.path)?.enumerate() {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            if let Some(raw_name) = entry.path().file_name() {
                let name = raw_name.to_string_lossy().to_string();
                if let Some(ext) = only_extension {
                    if !name.ends_with(ext) {
                        continue;
                    }
                }
                self.index.insert(DrawerFileId::from_u32(i as u32), name);
            }
        }
        Ok(())
    }

    pub fn from_directory_with_extension<S: AsRef<OsStr> + ?Sized>(
        priority: i64,
        path_name: &S,
        only_extension: &str,
    ) -> Fallible<Box<dyn DrawerInterface>> {
        let path = PathBuf::from(path_name);
        let name = path
            .file_name()
            .expect("a file")
            .to_string_lossy()
            .to_string();
        let mut dd = Self {
            name,
            priority,
            path,
            index: HashMap::new(),
        };
        if only_extension.is_empty() {
            dd.populate_from_directory(None)?;
        } else {
            dd.populate_from_directory(Some(only_extension))?;
        }
        Ok(Box::new(dd))
    }

    pub fn from_directory<S: AsRef<OsStr> + ?Sized>(
        priority: i64,
        path_name: &S,
    ) -> Fallible<Box<dyn DrawerInterface>> {
        Self::from_directory_with_extension(priority, path_name, "")
    }
}

#[async_trait]
impl DrawerInterface for DirectoryDrawer {
    fn index(&self) -> Fallible<HashMap<DrawerFileId, String>> {
        Ok(self.index.clone())
    }

    fn priority(&self) -> i64 {
        self.priority
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn stat_sync(&self, id: DrawerFileId) -> Fallible<DrawerFileMetadata> {
        ensure!(self.index.contains_key(&id), "file not found");
        let mut global_path = self.path.clone();
        global_path.push(&self.index[&id]);
        let meta = fs::metadata(&global_path)?;
        Ok(DrawerFileMetadata {
            drawer_file_id: id,
            name: self.index[&id].clone(),
            compression: None,
            packed_size: meta.len(),
            unpacked_size: meta.len(),
            path: Some(global_path),
        })
    }

    fn read_sync(&self, id: DrawerFileId) -> Fallible<Cow<[u8]>> {
        ensure!(self.index.contains_key(&id), "file not found");
        let mut global_path = self.path.clone();
        global_path.push(&self.index[&id]);
        let mut fp = fs::File::open(&global_path)?;
        let mut content = Vec::new();
        fp.read_to_end(&mut content)?;
        Ok(Cow::from(content))
    }

    async fn read(&self, id: DrawerFileId) -> Fallible<Vec<u8>> {
        ensure!(self.index.contains_key(&id), "file not found");
        let mut global_path = self.path.clone();
        global_path.push(&self.index[&id]);
        let mut fp = TokioFile::open(&global_path).await?;
        let mut content = Vec::new();
        fp.read_to_end(&mut content).await?;
        Ok(content)
    }
}
