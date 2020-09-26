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
mod mip;
mod srtm;

use crate::{
    mip::{MipIndex, MipIndexDataSet, MipTile},
    srtm::SrtmIndex,
};
use absolute_unit::{arcseconds, degrees, meters, radians, Angle, ArcSeconds, Radians};
use failure::Fallible;
use geodesy::{GeoCenter, Graticule};
use rayon::prelude::*;
use std::{
    fs,
    io::{stdout, Write},
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    time::Instant,
};
use structopt::StructOpt;
use terrain_geo::tile::{
    ChildIndex, DataSetCoordinates, DataSetDataKind, LayerPackBuilder, TerrainLevel,
    TILE_PHYSICAL_SIZE, TILE_SAMPLES,
};

#[derive(Debug, StructOpt)]
#[structopt(
    name = "dump-terrain-tiles",
    about = "Slice various data sets into the formats we need."
)]
struct Opt {
    /// Slice srtm into tiles
    #[structopt(short, long)]
    srtm_directory: PathBuf,

    /// The directory to save work to.
    #[structopt(short, long)]
    output_directory: PathBuf,

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

    pub fn finish(&self) {
        println!(", completed in {:?}", self.start_time.elapsed());
    }
}

fn build_trees(
    current_level: usize,
    srtm_index: Arc<RwLock<SrtmIndex>>,
    height_index: Arc<RwLock<MipIndexDataSet>>,
    height_node: Arc<RwLock<MipTile>>,
    node_count: &mut usize,
    leaf_count: &mut usize,
) -> Fallible<()> {
    if current_level < TerrainLevel::arcsecond_level() {
        {
            let srtm = srtm_index.read().unwrap();
            let mut height_node = height_node.write().unwrap();
            let next_level = TerrainLevel::new(current_level + 1);
            for &child_index in &ChildIndex::all() {
                if !height_node.has_child(child_index)
                    && srtm.contains_region(height_node.child_region(child_index))
                {
                    *node_count += 1;
                    height_node.add_child(next_level, child_index);
                }
            }
        }
        for maybe_child in height_node.read().unwrap().maybe_children() {
            if let Some(child) = maybe_child {
                build_trees(
                    current_level + 1,
                    srtm_index.clone(),
                    height_index.clone(),
                    child.to_owned(),
                    node_count,
                    leaf_count,
                )?;
            }
        }
        return Ok(());
    }
    assert_eq!(current_level, TerrainLevel::arcsecond_level());

    *leaf_count += 1;
    Ok(())
}

fn collect_tiles_at_level(
    target_level: usize,
    current_level: usize,
    node: Arc<RwLock<MipTile>>,
    offset: &mut usize,
    level_tiles: &mut Vec<(Arc<RwLock<MipTile>>, usize)>,
) -> Fallible<()> {
    if current_level < target_level {
        for maybe_child in node.read().unwrap().maybe_children() {
            if let Some(child) = maybe_child {
                collect_tiles_at_level(
                    target_level,
                    current_level + 1,
                    child.to_owned(),
                    offset,
                    level_tiles,
                )?;
            }
        }
        return Ok(());
    }
    assert_eq!(current_level, target_level);
    level_tiles.push((node, *offset));
    *offset += 1;
    Ok(())
}

pub fn generate_mip_tile_from_srtm(
    srtm_index: Arc<RwLock<SrtmIndex>>,
    index: Arc<RwLock<MipIndexDataSet>>,
    node: Arc<RwLock<MipTile>>,
    dump_png: bool,
) -> Fallible<()> {
    // Assume that tiles we've already created are good.
    if node
        .read()
        .unwrap()
        .file_exists(index.read().unwrap().work_path())
    {
        return Ok(());
    }

    node.write().unwrap().allocate_scratch_data();

    assert_eq!((-1..TILE_SAMPLES + 1).count(), TILE_PHYSICAL_SIZE);
    let level = node.read().unwrap().level();
    let scale = level.as_scale();
    let base = *node.read().unwrap().base_corner_graticule();

    let srtm = srtm_index.read().unwrap();
    // println!("visit: level {} @ {} - {}", level.offset(), base, offset);
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

            let srtm_grat = Graticule::<GeoCenter>::new(
                arcseconds!(lat_srtm),
                arcseconds!(lon_srtm),
                meters!(0),
            );
            let height = srtm.sample_nearest(&srtm_grat);
            node.write()
                .unwrap()
                .set_sample(lat_i as i32 + 1, lon_i as i32 + 1, height);
        }
    }
    if dump_png {
        node.read()
            .unwrap()
            .save_equalized_png(std::path::Path::new("__dump__/terrain-tiles/"))?;
    }
    node.write()
        .unwrap()
        .write(index.read().unwrap().work_path(), false)?;

    Ok(())
}

pub fn generate_mip_tile_from_mip(
    index: Arc<RwLock<MipIndexDataSet>>,
    node: Arc<RwLock<MipTile>>,
    dump_png: bool,
) -> Fallible<()> {
    // Assume that tiles we've already created are good.
    if node
        .read()
        .unwrap()
        .file_exists(index.read().unwrap().work_path())
    {
        assert_eq!(node.read().unwrap().data_state(), "mapped");
        return Ok(());
    }

    node.write().unwrap().allocate_scratch_data();

    for lat_i in -1..TILE_SAMPLES + 1 {
        // 'actual' unfolds infinitely
        // 'srtm' is clamped or wrapped as appropriate to srtm extents
        for lon_i in -1..TILE_SAMPLES + 1 {
            let mut tile = node.write().unwrap();
            let sample = tile.pull_sample(lat_i as i32 + 1, lon_i as i32 + 1);
            tile.set_sample(lat_i as i32 + 1, lon_i as i32 + 1, sample as i16);
        }
    }
    if dump_png {
        node.read()
            .unwrap()
            .save_equalized_png(std::path::Path::new("__dump__/terrain-tiles/"))?;
    }
    node.write()
        .unwrap()
        .write(index.read().unwrap().work_path(), true)?;

    // Note that though we have non-none tiles, our mips do not line up with the underlying 1
    // degree slicing so we may have corners that are non-None but still empty. This results in
    // some excess empty tiles up the stack and some extra work, but not a huge amount.
    assert!(
        node.read().unwrap().data_state() == "mapped"
            || node.read().unwrap().data_state() == "empty"
    );

    Ok(())
}

fn map_all_available_tile(
    tiles: &mut Vec<(Arc<RwLock<MipTile>>, usize)>,
    path: &Path,
) -> Fallible<usize> {
    let mmap_count = Arc::new(RwLock::new(0usize));
    tiles
        .par_iter_mut()
        .try_for_each::<_, Fallible<()>>(|(tile, _offset)| {
            if tile.write().unwrap().maybe_map_data(path).unwrap() {
                *mmap_count.write().unwrap() += 1;
            }
            Ok(())
        })?;
    println!(
        "  Mmapped {} existing tiles in {:?}",
        mmap_count.read().unwrap(),
        path
    );
    let cnt = *mmap_count.read().unwrap();
    Ok(cnt)
}

fn write_layer_pack(
    height_tiles: &[(Arc<RwLock<MipTile>>, usize)],
    index: Arc<RwLock<MipIndexDataSet>>,
    target_level: usize,
    force: bool,
) -> Fallible<()> {
    let layer_pack_path = index.read().unwrap().base_path().join(&format!(
        "{}-L{:02}.mip",
        index.read().unwrap().prefix(),
        target_level
    ));
    if layer_pack_path.exists() && !force {
        return Ok(());
    }
    let mut layer_pack_builder = LayerPackBuilder::new(&layer_pack_path)?;
    let mut progress = InlinePercentProgress::new("  Reserving pack entries:", height_tiles.len());
    for (tile, _) in height_tiles {
        let tile = tile.read().unwrap();
        let base_lat_as = tile.base().lat::<ArcSeconds>().round() as i32;
        let base_lon_as = tile.base().lon::<ArcSeconds>().round() as i32;
        layer_pack_builder.reserve(
            (base_lat_as, base_lon_as),
            tile.index_in_parent().to_index() as u32,
            tile.raw_data(),
        );
        progress.poke();
    }
    layer_pack_builder.write_header(
        target_level as u32,
        TerrainLevel::new(target_level).angular_extent().round() as i32,
    )?;
    progress.finish();
    let mut progress =
        InlinePercentProgress::new("  Writing Pack File:", EXPECT_LAYER_COUNTS[target_level].0);
    for (tile, _) in height_tiles {
        layer_pack_builder.push_tile(tile.read().unwrap().raw_data())?;
        progress.poke();
    }
    progress.finish();
    Ok(())
}

const EXPECT_LAYER_COUNTS: [(usize, usize); 13] = [
    (1, 1),
    (4, 4),
    (8, 8),
    (12, 12),
    (39, 39),
    (131, 124),
    (362, 342),
    (1_144, 1_048),
    (3_718, 3_350),
    (13_258, 11_607),
    (48_817, 41_859),
    (186_811, 157_455),
    (730_452, 608_337),
];

fn main() -> Fallible<()> {
    let opt = Opt::from_args();

    fs::create_dir_all(&opt.output_directory)?;

    let srtm = SrtmIndex::from_directory(&opt.srtm_directory)?;
    assert_eq!((-1..TILE_SAMPLES + 1).count(), TILE_PHYSICAL_SIZE);

    let mut mip_index = MipIndex::empty(&opt.output_directory);
    let mip_srtm_heights = mip_index.add_data_set(
        "srtmh",
        DataSetDataKind::Height,
        DataSetCoordinates::Spherical,
    )?;
    let _mip_srtm_normals = mip_index.add_data_set(
        "srtmn",
        DataSetDataKind::Normal,
        DataSetCoordinates::Spherical,
    )?;
    let height_root = mip_srtm_heights.write().unwrap().get_root_tile();
    let mut node_count = 0usize;
    let mut leaf_count = 0usize;
    build_trees(
        0,
        srtm.clone(),
        mip_srtm_heights.clone(),
        height_root,
        &mut node_count,
        &mut leaf_count,
    )?;
    // Subdividing the 1 degree tiles results in 730,452 509" tiles. Some of these on the edges
    // are over water and thus have no height values. This reduces the count to 608,337 tiles
    // that will be mmapped as part of the base layer.
    assert_eq!(node_count, 984_756);
    assert_eq!(leaf_count, 730_452);
    println!("Built tree with {} nodes", node_count);

    // Generate each level from the bottom up, mipmapping as we go.
    for target_level in (0..=TerrainLevel::arcsecond_level()).rev() {
        println!("Level {}:", target_level);
        let mut height_tiles = Vec::new();
        let mut offset = 0;
        let height_root = mip_srtm_heights.write().unwrap().get_root_tile();
        collect_tiles_at_level(target_level, 0, height_root, &mut offset, &mut height_tiles)?;
        println!(
            "  Collected {} tiles to build at level {}",
            height_tiles.len(),
            target_level
        );
        assert_eq!(height_tiles.len(), EXPECT_LAYER_COUNTS[target_level].0);
        let mmap_count = map_all_available_tile(
            &mut height_tiles,
            mip_srtm_heights.read().unwrap().work_path(),
        )?;
        if mmap_count < EXPECT_LAYER_COUNTS[target_level].1 {
            let progress = Arc::new(RwLock::new(InlinePercentProgress::new(
                "  Building tiles:",
                height_tiles.len(),
            )));
            if target_level == TerrainLevel::arcsecond_level() {
                height_tiles.par_iter().try_for_each(|(tile, _)| {
                    progress.write().unwrap().poke();
                    generate_mip_tile_from_srtm(
                        srtm.clone(),
                        mip_srtm_heights.clone(),
                        tile.to_owned(),
                        opt.dump_png,
                    )
                })?;
            } else {
                height_tiles.par_iter().try_for_each(|(tile, _)| {
                    progress.write().unwrap().poke();
                    generate_mip_tile_from_mip(
                        mip_srtm_heights.clone(),
                        tile.to_owned(),
                        opt.dump_png,
                    )
                })?;
            }
            progress.write().unwrap().finish();
        } else {
            println!("  Found all tiles on disk - promoting absent to empty");
            height_tiles.par_iter().for_each(|(node, _offset)| {
                node.write().unwrap().promote_absent_to_empty();
            });
        }
        write_layer_pack(
            &height_tiles,
            mip_srtm_heights.clone(),
            target_level,
            opt.force,
        )?;
    }

    // Write out our top level index of the data.
    mip_srtm_heights.read().unwrap().write()?;

    Ok(())
}
