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
use std::{
    io::{stdout, Write},
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

fn process_srtm_at_level(
    target_level: usize,
    current_level: usize,
    srtm: &SrtmIndex,
    index: Arc<RwLock<MipIndexDataSet>>,
    node: Arc<RwLock<MipTile>>,
) -> Fallible<()> {
    if current_level < target_level {
        if !node.read().unwrap().has_children() {
            let _sw_tile = node
                .write()
                .unwrap()
                .add_child(TerrainLevel::new(current_level + 1), ChildIndex::SouthWest);
            let _se_tile = node
                .write()
                .unwrap()
                .add_child(TerrainLevel::new(current_level + 1), ChildIndex::SouthEast);
            let _nw_tile = node
                .write()
                .unwrap()
                .add_child(TerrainLevel::new(current_level + 1), ChildIndex::NorthWest);
            let _ne_tile = node
                .write()
                .unwrap()
                .add_child(TerrainLevel::new(current_level + 1), ChildIndex::NorthEast);
            return process_srtm_at_level(target_level, current_level, srtm, index, node);
        }
        for maybe_child in node.read().unwrap().maybe_children() {
            if let Some(child) = maybe_child {
                process_srtm_at_level(
                    target_level,
                    current_level + 1,
                    srtm,
                    index.clone(),
                    child.to_owned(),
                )?;
            }
        }
        return Ok(());
    }
    assert_eq!(target_level, current_level);

    if node
        .read()
        .unwrap()
        .file_exists(index.read().unwrap().base_path())
    {
        return Ok(());
    }

    assert_eq!((-1..TILE_SAMPLES + 1).count(), TILE_PHYSICAL_SIZE);

    let level = TerrainLevel::new(target_level);
    let scale = level.as_scale();
    let base = *node.read().unwrap().base_corner_graticule();
    println!("building: level {} @ {}", level.offset(), base);
    for lat_i in -1..TILE_SAMPLES + 1 {
        if lat_i % 8 == 0 {
            print!(".");
            stdout().flush()?;
        }

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
            // FIXME: sample regions
            let height = srtm.sample_nearest(&srtm_grat);
            node.write()
                .unwrap()
                .set_sample(lat_i as i32 + 1, lon_i as i32 + 1, height);
        }
    }
    println!();
    node.read()
        .unwrap()
        .save_equalized_png(std::path::Path::new("scanout"))?;
    node.read()
        .unwrap()
        .write(index.read().unwrap().base_path())?;
    Ok(())
}

fn main() -> Fallible<()> {
    let opt = Opt::from_args();

    let srtm = SrtmIndex::from_directory(&opt.srtm_directory)?;

    let mut mip_index = MipIndex::empty(&opt.output_directory);
    let mip_srtm_heights = mip_index.add_data_set(
        "srtmh",
        DataSetDataKind::Height,
        DataSetCoordinates::Spherical,
    )?;
    let root = mip_srtm_heights.write().unwrap().get_root_tile();
    for i in 0..5 {
        process_srtm_at_level(i, 0, &srtm, mip_srtm_heights.clone(), root.clone())?;
    }
    mip_srtm_heights.read().unwrap().write()?;

    Ok(())
}
