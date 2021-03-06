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
mod bmng;
mod mip;
mod srtm;

use crate::{
    bmng::BmngIndex,
    mip::{DataSource, MipIndex, MipIndexDataSet, MipTile, NeighborIndex},
    srtm::SrtmIndex,
};
use absolute_unit::{arcseconds, degrees, meters, radians, Angle, Radians};
use anyhow::Result;
use bzip2::{read::BzEncoder, Compression};
use geodesy::{GeoSurface, Graticule};
use parking_lot::{Mutex, RwLock};
use rayon::prelude::*;
use std::{
    cmp::Ordering,
    fs,
    io::{stdout, Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};
use structopt::StructOpt;
use terrain_geo::tile::{
    ChildIndex, DataSetCoordinates, DataSetDataKind, LayerPackBuilder, TerrainLevel,
    TileCompression, TILE_PHYSICAL_SIZE, TILE_SAMPLES,
};

#[derive(Debug, StructOpt)]
#[structopt(
    name = "dump-terrain-tiles",
    about = "Slice various data sets into the formats we need."
)]
struct Opt {
    /// Slice srtm into tiles
    #[structopt(short, long)]
    srtm_directory: Option<PathBuf>,

    /// Slice bmng into tiles
    #[structopt(short, long)]
    bmng_directory: Option<PathBuf>,

    #[structopt(long)]
    serialize: bool,

    /// The directory to save work to.
    #[structopt(short, long)]
    output_directory: PathBuf,

    /// Compute and print some aggregate statistics.
    #[structopt(short, long)]
    compression: Option<String>,

    /// Dump equalized PNGs of created tiles.
    #[structopt(short, long)]
    dump_png: bool,

    /// Overwrite existing files.
    #[structopt(short, long)]
    force: bool,
}

#[inline]
pub fn minimum_longitude() -> Angle<Radians> {
    radians!(degrees!(-180))
}

#[inline]
pub fn maximum_longitude() -> Angle<Radians> {
    radians!(degrees!(180))
}

#[inline]
pub fn minimum_latitude() -> Angle<Radians> {
    radians!(degrees!(-90))
}

#[inline]
pub fn maximum_latitude() -> Angle<Radians> {
    radians!(degrees!(90))
}

struct InlinePercentProgress {
    total: usize,
    current: usize,
    start_time: Instant,
}

impl InlinePercentProgress {
    pub fn new(label: &str, total: usize) -> Self {
        print!("{} 000.00%", label);
        stdout().flush().ok();
        Self {
            total,
            current: 0,
            start_time: Instant::now(),
        }
    }

    pub fn set(&mut self, current: usize) {
        self.current = current;
        let percent = (self.current as f64 / self.total as f64) * 100f64;
        print!(
            "\x1B[7D{:03}.{:02}%",
            percent.floor() as u8,
            ((percent - percent.floor()) * 100f64) as u8
        );
        stdout().flush().ok();
    }

    pub fn poke(&mut self) {
        self.set(self.current + 1);
    }

    pub fn poke_chunk(&mut self, chunk_size: usize) {
        self.set(self.current + chunk_size);
    }

    pub fn finish(&self) {
        println!(", completed in {:?}", self.start_time.elapsed());
    }
}

fn make_neighbors(
    tile_ref: Arc<RwLock<MipTile>>,
    neighbors: [Option<Arc<RwLock<MipTile>>>; NeighborIndex::len()],
) {
    tile_ref.write().set_neighbors(neighbors.clone());

    let tile = tile_ref.read();
    for &child_index in &ChildIndex::all() {
        if let Some(child) = tile.get_child(child_index) {
            let child_neighbors = match child_index {
                ChildIndex::SouthWest => [
                    nc(&neighbors, NeighborIndex::West, ChildIndex::SouthEast),
                    nc(&neighbors, NeighborIndex::SouthWest, ChildIndex::NorthEast),
                    nc(&neighbors, NeighborIndex::South, ChildIndex::NorthWest),
                    nc(&neighbors, NeighborIndex::South, ChildIndex::NorthEast),
                    tile.get_child(ChildIndex::SouthEast),
                    tile.get_child(ChildIndex::NorthEast),
                    tile.get_child(ChildIndex::NorthWest),
                    nc(&neighbors, NeighborIndex::West, ChildIndex::NorthEast),
                ],
                ChildIndex::SouthEast => [
                    tile.get_child(ChildIndex::SouthWest),
                    nc(&neighbors, NeighborIndex::South, ChildIndex::NorthWest),
                    nc(&neighbors, NeighborIndex::South, ChildIndex::NorthEast),
                    nc(&neighbors, NeighborIndex::SouthEast, ChildIndex::NorthWest),
                    nc(&neighbors, NeighborIndex::East, ChildIndex::SouthWest),
                    nc(&neighbors, NeighborIndex::East, ChildIndex::NorthWest),
                    tile.get_child(ChildIndex::NorthEast),
                    tile.get_child(ChildIndex::NorthWest),
                ],
                ChildIndex::NorthWest => [
                    nc(&neighbors, NeighborIndex::West, ChildIndex::NorthEast),
                    nc(&neighbors, NeighborIndex::West, ChildIndex::SouthEast),
                    tile.get_child(ChildIndex::SouthWest),
                    tile.get_child(ChildIndex::SouthEast),
                    tile.get_child(ChildIndex::NorthEast),
                    nc(&neighbors, NeighborIndex::North, ChildIndex::SouthEast),
                    nc(&neighbors, NeighborIndex::North, ChildIndex::SouthWest),
                    nc(&neighbors, NeighborIndex::NorthWest, ChildIndex::SouthEast),
                ],
                ChildIndex::NorthEast => [
                    tile.get_child(ChildIndex::NorthWest),
                    tile.get_child(ChildIndex::SouthWest),
                    tile.get_child(ChildIndex::SouthEast),
                    nc(&neighbors, NeighborIndex::East, ChildIndex::SouthWest),
                    nc(&neighbors, NeighborIndex::East, ChildIndex::NorthWest),
                    nc(&neighbors, NeighborIndex::NorthEast, ChildIndex::SouthWest),
                    nc(&neighbors, NeighborIndex::North, ChildIndex::SouthEast),
                    nc(&neighbors, NeighborIndex::North, ChildIndex::SouthWest),
                ],
            };
            make_neighbors(child, child_neighbors)
        }
    }
}

// get_neighbor_child -- shortened for clarity
fn nc(
    neighbors: &[Option<Arc<RwLock<MipTile>>>],
    neighbor_index: NeighborIndex,
    child_index: ChildIndex,
) -> Option<Arc<RwLock<MipTile>>> {
    neighbors[neighbor_index.index()]
        .clone()
        .and_then(|t| t.read().get_child(child_index))
}

fn build_tree(
    current_level: usize,
    source: Arc<RwLock<dyn DataSource>>,
    data_set: Arc<RwLock<MipIndexDataSet>>,
    tile_ref: Arc<RwLock<MipTile>>,
    node_count: &mut usize,
    leaf_count: &mut usize,
) -> Result<()> {
    if current_level < source.read().root_level().offset() {
        {
            let src = source.read();
            let mut tile = tile_ref.write();
            let next_level = TerrainLevel::new(current_level + 1);
            for &child_index in &ChildIndex::all() {
                if !tile.has_child(child_index)
                    && src.contains_region(&tile.child_region(child_index))
                {
                    *node_count += 1;
                    tile.add_child(next_level, child_index);
                }
            }
        }
        for child in tile_ref.read().maybe_children().iter().flatten() {
            build_tree(
                current_level + 1,
                source.clone(),
                data_set.clone(),
                child.to_owned(),
                node_count,
                leaf_count,
            )?;
        }
        return Ok(());
    }
    debug_assert_eq!(current_level, source.read().root_level().offset());
    *leaf_count += 1;
    Ok(())
}

fn collect_tiles_at_level(
    target_level: usize,
    current_level: usize,
    node: Arc<RwLock<MipTile>>,
    offset: &mut usize,
    level_tiles: &mut Vec<(Arc<RwLock<MipTile>>, usize)>,
) -> Result<()> {
    if current_level < target_level {
        for child in node.read().maybe_children().iter().flatten() {
            collect_tiles_at_level(
                target_level,
                current_level + 1,
                child.to_owned(),
                offset,
                level_tiles,
            )?;
        }
        return Ok(());
    }
    assert_eq!(current_level, target_level);
    level_tiles.push((node, *offset));
    *offset += 1;
    Ok(())
}

pub fn generate_mip_tile_from_source(
    source: Arc<RwLock<dyn DataSource>>,
    index: Arc<RwLock<MipIndexDataSet>>,
    node: Arc<RwLock<MipTile>>,
    dump_png: bool,
) -> Result<()> {
    // Assume that tiles we've already created are good.
    if node.read().file_exists(index.read().work_path()) {
        return Ok(());
    }

    node.write().allocate_scratch_data(index.read().kind());

    assert_eq!((-1..TILE_SAMPLES + 1).count(), TILE_PHYSICAL_SIZE);
    let level = node.read().level();
    let scale = level.as_scale();
    let base = node.read().base_graticule();
    let kind = index.read().kind();

    let mut sum_height = 0;
    let source = source.read();
    // println!("visit: level {} @ {}", level.offset(), base);
    for lat_i in -1..TILE_SAMPLES + 1 {
        // 'actual' unfolds infinitely
        // 'srtm' is clamped or wrapped as appropriate to srtm extents

        let lat_actual = degrees!(base.latitude + (scale * lat_i));
        let lat_srtm = radians!(if lat_actual > degrees!(90) {
            degrees!(90)
        } else if lat_actual < degrees!(-90) {
            degrees!(-90)
        } else {
            lat_actual
        });

        for lon_i in -1..TILE_SAMPLES + 1 {
            let lon_actual = degrees!(base.longitude + (scale * lon_i));
            let mut lon_srtm = lon_actual;
            while lon_srtm < degrees!(-180) {
                lon_srtm += degrees!(360);
            }
            while lon_srtm > degrees!(180) {
                lon_srtm -= degrees!(360);
            }

            let srtm_grat = Graticule::<GeoSurface>::new(
                arcseconds!(lat_srtm),
                arcseconds!(lon_srtm),
                meters!(0),
            );

            // FIXME: compute base normals
            match kind {
                DataSetDataKind::Height => {
                    let height = source.sample_nearest_height(&srtm_grat);
                    sum_height += height as i32;
                    node.write()
                        .set_height_sample(lat_i as i32 + 1, lon_i as i32 + 1, height);
                }
                DataSetDataKind::Normal => {
                    sum_height += source.sample_nearest_height(&srtm_grat) as i32;
                    let normal = source.compute_local_normal(&srtm_grat);
                    node.write()
                        .set_normal_sample(lat_i as i32 + 1, lon_i as i32 + 1, normal);
                }
                DataSetDataKind::Color => {
                    sum_height += 1;
                    let color = source.sample_color(&srtm_grat);
                    node.write()
                        .set_color_sample(lat_i as i32 + 1, lon_i as i32 + 1, color);
                }
            }
        }
    }
    if dump_png {
        node.read().save_equalized_png(
            index.read().kind(),
            std::path::Path::new("__dump__/terrain-tiles/"),
        )?;
    }
    node.write().write(index.read().work_path(), sum_height)?;

    Ok(())
}

pub fn generate_mip_tile_from_mip(
    index: Arc<RwLock<MipIndexDataSet>>,
    node: Arc<RwLock<MipTile>>,
    dump_png: bool,
) -> Result<()> {
    // Assume that tiles we've already created are good.
    if node.read().file_exists(index.read().work_path()) {
        assert!(node.read().data_state().starts_with("mapped"));
        return Ok(());
    }

    node.write().allocate_scratch_data(index.read().kind());

    let kind = index.read().kind();

    for lat_off in 0i32..TILE_PHYSICAL_SIZE as i32 {
        // 'actual' unfolds infinitely
        // 'srtm' is clamped or wrapped as appropriate to srtm extents
        for lon_off in 0i32..TILE_PHYSICAL_SIZE as i32 {
            let mut tile = node.write();
            match kind {
                DataSetDataKind::Height => {
                    let height = tile.pull_height_sample(lat_off, lon_off);
                    tile.set_height_sample(lat_off as i32, lon_off as i32, height);
                }
                DataSetDataKind::Normal => {
                    let normal = tile.pull_normal_sample(lat_off, lon_off);
                    tile.set_normal_sample(lat_off, lon_off, normal);
                }
                DataSetDataKind::Color => {
                    let color = tile.pull_color_sample(lat_off, lon_off);
                    tile.set_color_sample(lat_off, lon_off, color);
                }
            }
        }
    }
    if dump_png {
        node.read().save_equalized_png(
            index.read().kind(),
            std::path::Path::new("__dump__/terrain-tiles/"),
        )?;
    }
    // Note: if we got here, then there are tiles below us and we expect to generate something.
    node.write().write(index.read().work_path(), 1)?;

    // Note that though we have non-none tiles, our mips do not line up with the underlying 1
    // degree slicing so we may have corners that are non-None but still empty. This results in
    // some excess empty tiles up the stack and some extra work, but not a huge amount.
    assert!(node.read().data_state().starts_with("mapped") || node.read().data_state() == "empty");

    Ok(())
}

fn map_all_available_tile(
    kind: DataSetDataKind,
    tiles: &mut Vec<(Arc<RwLock<MipTile>>, usize)>,
    path: &Path,
    serialize: bool,
) -> Result<usize> {
    let mmap_count = Arc::new(RwLock::new(0usize));
    if serialize {
        for (tile, _offset) in tiles {
            if tile.write().maybe_map_data(kind, path).expect("mmap file") {
                *mmap_count.write() += 1;
            }
        }
    } else {
        tiles
            .par_chunks_mut(256)
            .try_for_each::<_, Result<()>>(|chunk| {
                for (tile, _offset) in chunk {
                    if tile.write().maybe_map_data(kind, path).expect("mmap file") {
                        *mmap_count.write() += 1;
                    }
                }
                Ok(())
            })?;
    }
    println!(
        "  Mmapped {} existing tiles in {:?}",
        mmap_count.read(),
        path
    );
    let cnt = *mmap_count.read();
    Ok(cnt)
}

fn write_layer_pack(
    tiles: &[(Arc<RwLock<MipTile>>, usize)],
    dataset: Arc<RwLock<MipIndexDataSet>>,
    target_level: usize,
    force: bool,
    compression: TileCompression,
) -> Result<()> {
    if tiles.is_empty() {
        println!("  skipping write because pack is empty");
        return Ok(());
    }
    let layer_pack_path = dataset.read().base_path().join(&format!(
        "{}-L{:02}.mip",
        dataset.read().prefix(),
        target_level
    ));
    if layer_pack_path.exists() && !force {
        println!(
            "  skipping write because have layerpack file at {:?}",
            layer_pack_path
        );
        return Ok(());
    }
    let layer_pack_builder = Mutex::new(LayerPackBuilder::new(
        &layer_pack_path,
        tiles.len(),
        target_level as u32,
        compression,
        TerrainLevel::new(target_level).angular_extent().round() as i32,
    )?);
    let progress = Mutex::new(InlinePercentProgress::new(
        "  Writing Pack File:",
        tiles.len(),
    ));
    let chunk_size = 128;
    tiles.par_chunks(chunk_size).for_each(|chunk| {
        for (tile, _) in chunk {
            let tile = tile.read();
            let data = match compression {
                TileCompression::None => tile.raw_data().to_owned(),
                TileCompression::Bz2 => {
                    let mut compressed = Vec::new();
                    BzEncoder::new(tile.raw_data(), Compression::best())
                        .read_to_end(&mut compressed)
                        .expect("compress buffer");
                    compressed
                }
            };
            layer_pack_builder
                .lock()
                .push_tile(tile.base(), tile.index_in_parent().to_index() as u32, &data)
                .unwrap();
        }
        progress.lock().poke_chunk(chunk_size);
    });
    progress.lock().finish();
    Ok(())
}

fn main() -> Result<()> {
    let opt = Opt::from_args();
    let compression = if let Some(comp) = opt.compression {
        match comp.as_str() {
            "none" => TileCompression::None,
            "bz2" => TileCompression::Bz2,
            _ => panic!("Unknown tile compression format: expected bz2 or none"),
        }
    } else {
        TileCompression::None
    };
    let dump_png = opt.dump_png;

    fs::create_dir_all(&opt.output_directory)?;

    let mut mip_index = MipIndex::empty(&opt.output_directory);

    if let Some(directory) = opt.bmng_directory.as_ref() {
        for month in 1..=12 {
            let mut month_dir = directory.to_owned();
            month_dir.push(format!("month{:02}", month));
            let bmng = BmngIndex::from_directory(month, &month_dir)?;
            mip_index.add_data_set(
                Box::leak(format!("bmng-{:02}", month).into_boxed_str()),
                DataSetDataKind::Color,
                DataSetCoordinates::Spherical,
                bmng,
            )?;
        }
    }

    if let Some(directory) = opt.srtm_directory.as_ref() {
        let srtm = SrtmIndex::from_directory(directory)?;
        mip_index.add_data_set(
            "srtmh",
            DataSetDataKind::Height,
            DataSetCoordinates::Spherical,
            srtm.clone(),
        )?;
        mip_index.add_data_set(
            "srtmn",
            DataSetDataKind::Normal,
            DataSetCoordinates::Spherical,
            srtm,
        )?;
    }

    for dataset in mip_index.data_sets(DataSetCoordinates::Spherical) {
        let start = Instant::now();
        let root = dataset.write().get_root_tile();
        let mut node_count = 0usize;
        let mut leaf_count = 0usize;
        build_tree(
            0,
            dataset.read().source(),
            dataset.clone(),
            root.clone(),
            &mut node_count,
            &mut leaf_count,
        )?;
        make_neighbors(root, [None, None, None, None, None, None, None, None]);
        // Subdividing the 1 degree tiles results in 730,452 509" tiles. Some of these on the edges
        // are over water and thus have no height values. This reduces the count to 608,337 tiles
        // that will be mmapped as part of the base layer.
        // assert_eq!(node_count, 984_756);
        // assert_eq!(leaf_count, 730_452);
        println!(
            "Built {} tile tree with {} nodes in {:?}",
            dataset.read().prefix(),
            node_count,
            start.elapsed()
        );
    }

    // Generate each level from the bottom up, mipmapping as we go.
    for target_level in (0..=SrtmIndex::max_resolution_level()).rev() {
        for dataset in mip_index.data_sets(DataSetCoordinates::Spherical) {
            println!("{} Level {}:", dataset.read().prefix(), target_level);
            let expect_intersecting = dataset
                .read()
                .source()
                .read()
                .expect_intersecting_tiles(target_level);
            let expect_present = dataset
                .read()
                .source()
                .read()
                .expect_present_tiles(target_level);
            let mut current_tiles = Vec::new();
            let mut offset = 0;
            let root_tile = dataset.write().get_root_tile();
            collect_tiles_at_level(target_level, 0, root_tile, &mut offset, &mut current_tiles)?;
            println!(
                "  Collected {} tiles to build at level {} [expect: {}]",
                current_tiles.len(),
                target_level,
                expect_intersecting
            );
            assert_eq!(current_tiles.len(), expect_intersecting);
            let mmap_count = map_all_available_tile(
                dataset.read().kind(),
                &mut current_tiles,
                dataset.read().work_path(),
                opt.serialize,
            )?;
            assert!(mmap_count <= expect_intersecting);
            assert!(mmap_count <= *expect_present.end());
            if !expect_present.contains(&mmap_count) {
                let progress = Arc::new(RwLock::new(InlinePercentProgress::new(
                    "  Building tiles:",
                    current_tiles.len(),
                )));

                match (
                    opt.serialize,
                    target_level.cmp(&dataset.read().source().read().root_level().offset()),
                ) {
                    (true, Ordering::Equal) => {
                        for (tile, _) in &current_tiles {
                            generate_mip_tile_from_source(
                                dataset.read().source(),
                                dataset.clone(),
                                tile.to_owned(),
                                dump_png,
                            )
                            .unwrap();
                            progress.write().poke();
                        }
                    }
                    (true, Ordering::Less) => {
                        for (tile, _) in &current_tiles {
                            generate_mip_tile_from_mip(dataset.clone(), tile.to_owned(), dump_png)
                                .expect("generate_mip_tile_from_mip");
                            progress.write().poke();
                        }
                    }
                    (false, Ordering::Equal) => {
                        current_tiles.par_chunks(1024).for_each(|chunk| {
                            for (tile, _) in chunk {
                                generate_mip_tile_from_source(
                                    dataset.read().source(),
                                    dataset.clone(),
                                    tile.to_owned(),
                                    dump_png,
                                )
                                .expect("generate_mip_tile_from_srtm");
                                progress.write().poke();
                            }
                        });
                    }
                    (false, Ordering::Less) => {
                        current_tiles.par_chunks(1024).for_each(|chunk| {
                            for (tile, _) in chunk {
                                generate_mip_tile_from_mip(
                                    dataset.clone(),
                                    tile.to_owned(),
                                    dump_png,
                                )
                                .expect("generate_mip_tile_from_mip");
                                progress.write().poke();
                            }
                        });
                    }
                    (_, Ordering::Greater) => panic!("root level larger than current target level"),
                }
                progress.write().finish();
            } else {
                println!("  Found all tiles on disk - demoting absent to empty");
                current_tiles.par_chunks(4096).for_each(|chunk| {
                    for (node, _) in chunk {
                        node.write().promote_absent_to_empty();
                    }
                });
            }

            // Verify that we do not have any absent tiles. All tiles should be empty or have mmapped data.
            println!("  Verifying tile states");
            for (tile, _) in &current_tiles {
                let tile = tile.read();
                assert!(tile.data().is_empty() || tile.data().is_mapped());
            }

            // Filter down to mapped / non-empty tiles.
            println!("  Filtering non-data tiles");
            current_tiles = current_tiles
                .drain(..)
                .filter(|(tile, _)| tile.read().data().is_mapped())
                .collect::<Vec<_>>();

            // FIXME: why are there more normals tiles?
            //assert_eq!(current_tiles.len(), EXPECT_LAYER_COUNTS[target_level].1);

            write_layer_pack(
                &current_tiles,
                dataset.clone(),
                target_level,
                opt.force,
                compression,
            )?;
        }
    }

    // Write out our top level index of the data.
    for dataset in mip_index.data_sets(DataSetCoordinates::Spherical) {
        dataset.read().write()?;
    }

    Ok(())
}
