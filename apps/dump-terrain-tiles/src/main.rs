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
use absolute_unit::{arcseconds, degrees, meters, radians, Angle, Radians};
use failure::Fallible;
use geodesy::{GeoCenter, Graticule};
use rayon::prelude::*;
use std::{
    fs,
    io::stdout,
    path::PathBuf,
    sync::{Arc, RwLock},
};
use structopt::StructOpt;
use terrain_geo::tile::{
    ChildIndex, DataSetCoordinates, DataSetDataKind, TerrainLevel, TILE_PHYSICAL_SIZE, TILE_SAMPLES,
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

fn collect_base_tiles(
    current_level: usize,
    srtm_index: Arc<RwLock<SrtmIndex>>,
    index: Arc<RwLock<MipIndexDataSet>>,
    node: Arc<RwLock<MipTile>>,
    visit_count: &mut usize,
    mmap_count: &mut usize,
    base_tiles: &mut Vec<(Arc<RwLock<MipTile>>, usize)>,
) -> Fallible<()> {
    if current_level < TerrainLevel::arcsecond_level() {
        // Add any missing children.
        {
            let srtm = srtm_index.read().unwrap();
            let mut node = node.write().unwrap();
            let next_level = TerrainLevel::new(current_level + 1);
            for &child_index in &ChildIndex::all() {
                if !node.has_child(child_index) {
                    if srtm.contains_region(node.child_region(child_index)) {
                        node.add_child(next_level, child_index);
                    }
                }
            }
        }
        for maybe_child in node.read().unwrap().maybe_children() {
            if let Some(child) = maybe_child {
                collect_base_tiles(
                    current_level + 1,
                    srtm_index.clone(),
                    index.clone(),
                    child.to_owned(),
                    visit_count,
                    mmap_count,
                    base_tiles,
                )?;
            }
        }
        return Ok(());
    }
    assert_eq!(current_level, TerrainLevel::arcsecond_level());

    if node
        .write()
        .unwrap()
        .maybe_map_data(index.read().unwrap().base_path())?
    {
        *mmap_count += 1;
    }
    base_tiles.push((node, *visit_count));
    *visit_count += 1;
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
    visit_count: usize,
) -> Fallible<()> {
    // Assume that tiles we've already created are good.
    if node
        .read()
        .unwrap()
        .file_exists(index.read().unwrap().base_path())
    {
        return Ok(());
    }

    node.write().unwrap().allocate_scratch_data();

    assert_eq!((-1..TILE_SAMPLES + 1).count(), TILE_PHYSICAL_SIZE);
    let level = node.read().unwrap().level();
    let scale = level.as_scale();
    let extent = level.angular_extent();
    let base = *node.read().unwrap().base_corner_graticule();

    let srtm = srtm_index.read().unwrap();
    println!(
        "visit: level {} @ {} - {}",
        level.offset(),
        base,
        visit_count
    );
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
    // println!();
    // node.read()
    //     .unwrap()
    //     .save_equalized_png(std::path::Path::new("scanout"))?;
    node.write()
        .unwrap()
        .write(index.read().unwrap().base_path())?;

    Ok(())
}

pub fn generate_mip_tile_from_mip(
    index: Arc<RwLock<MipIndexDataSet>>,
    node: Arc<RwLock<MipTile>>,
    offset: usize,
) -> Fallible<()> {
    // Assume that tiles we've already created are good.
    if node
        .read()
        .unwrap()
        .file_exists(index.read().unwrap().base_path())
    {
        assert_eq!(node.read().unwrap().data_state(), "mapped");
        return Ok(());
    }

    node.write().unwrap().allocate_scratch_data();

    assert_eq!((-1..TILE_SAMPLES + 1).count(), TILE_PHYSICAL_SIZE);
    let level = node.read().unwrap().level();
    let scale = level.as_scale();
    let extent = level.angular_extent();
    let base = *node.read().unwrap().base_corner_graticule();

    if offset % 1000 == 0 {
        print!(".");
    }
    //println!("visit: level {} @ {} - {}", level.offset(), base, offset);
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

            let real_grat = Graticule::<GeoCenter>::new(
                arcseconds!(lat_srtm),
                arcseconds!(lon_srtm),
                meters!(0),
            );
            let mut tile = node.write().unwrap();
            let sample = tile.pull_sample(lat_i as i32 + 1, lon_i as i32 + 1, real_grat);
            tile.set_sample(lat_i as i32 + 1, lon_i as i32 + 1, sample as i16);
        }
    }
    // println!();
    // node.read()
    //     .unwrap()
    //     .save_equalized_png(std::path::Path::new("scanout"))?;
    node.write()
        .unwrap()
        .write(index.read().unwrap().base_path())?;

    // Note that though we have non-none tiles, our mips do not line up with the underlying 1
    // degree slicing so we may have corners that are non-None but still empty. This results in
    // some excess empty tiles up the stack and some extra work, but not a huge amount.
    assert!(
        node.read().unwrap().data_state() == "mapped"
            || node.read().unwrap().data_state() == "empty"
    );

    Ok(())
}

fn main() -> Fallible<()> {
    let opt = Opt::from_args();

    fs::create_dir_all(&opt.output_directory)?;

    let srtm = SrtmIndex::from_directory(&opt.srtm_directory)?;

    let mut mip_index = MipIndex::empty(&opt.output_directory);
    let mip_srtm_heights = mip_index.add_data_set(
        "srtmh",
        DataSetDataKind::Height,
        DataSetCoordinates::Spherical,
    )?;
    let root = mip_srtm_heights.write().unwrap().get_root_tile();
    let mut visit_count = 0usize;
    let mut mmap_count = 0usize;
    let mut base_tiles = Vec::new();
    collect_base_tiles(
        0,
        srtm.clone(),
        mip_srtm_heights.clone(),
        root.clone(),
        &mut visit_count,
        &mut mmap_count,
        &mut base_tiles,
    )?;
    println!(
        "Collected {} tiles to build from srtm data; {}; {}",
        base_tiles.len(),
        visit_count,
        mmap_count
    );

    // Subdividing the 1 degree tiles results in 730,452 509" tiles. Some of these on the edges
    // are over water and thus have no height values. This reduces the count to 608,337 tiles
    // that will be mmapped as part of the base layer.
    assert_eq!(visit_count, 730_452);
    if mmap_count < 608_337 {
        base_tiles.par_iter().try_for_each(|(node, visit_count)| {
            generate_mip_tile_from_srtm(
                srtm.clone(),
                mip_srtm_heights.clone(),
                node.to_owned(),
                *visit_count,
            )
        })?;
    } else {
        base_tiles.par_iter().for_each(|(node, visit_count)| {
            node.write().unwrap().promote_absent_to_empty();
        });
    }

    for target_level in (0..=11).rev() {
        let mut level_tiles = Vec::new();
        let mut offset = 0;
        let root = mip_srtm_heights.write().unwrap().get_root_tile();
        collect_tiles_at_level(target_level, 0, root, &mut offset, &mut level_tiles)?;
        println!(
            "Collected {} tiles to build at level {}",
            level_tiles.len(),
            target_level
        );
        level_tiles
            .par_iter_mut()
            .try_for_each::<_, Fallible<()>>(|(tile, offset)| {
                if tile
                    .write()
                    .unwrap()
                    .maybe_map_data(mip_srtm_heights.read().unwrap().base_path())
                    .unwrap()
                {
                    // pass
                }
                Ok(())
            })?;
        level_tiles.par_iter().try_for_each(|(tile, offset)| {
            generate_mip_tile_from_mip(mip_srtm_heights.clone(), tile.to_owned(), *offset)
        })?;
        println!();
    }

    // for i in (0..TerrainLevel::arcsecond_level()).rev() {
    //     process_srtm_at_level(i, 0, &srtm, mip_srtm_heights.clone(), root.clone())?;
    // }
    mip_srtm_heights.read().unwrap().write()?;

    Ok(())
}
