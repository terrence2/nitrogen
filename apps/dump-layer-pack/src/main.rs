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
use failure::Fallible;
use memmap::MmapOptions;
use std::{fs::File, mem, path::PathBuf};
use structopt::StructOpt;
use terrain_geo::tile::{ChildIndex, LayerPackHeader, LayerPackIndexItem};

#[derive(Debug, StructOpt)]
#[structopt(name = "dump-layer-pack", about = "Show the contents of layer packs.")]
struct Opt {
    /// Latitude to filter.
    #[structopt(long)]
    latitude: Option<i32>,

    /// Longitude to filter
    #[structopt(long)]
    longitude: Option<i32>,

    /// Dump detailed tile info for all tiles printed.
    #[structopt(short, long)]
    dump_tile: bool,

    /// Layer pack file to look at.
    #[structopt(parse(from_os_str))]
    input: PathBuf,
}

fn main() -> Fallible<()> {
    let opt = Opt::from_args();

    let fp = File::open(&opt.input)?;
    let mmap = unsafe { MmapOptions::new().map(&fp)? };
    let header = LayerPackHeader::overlay(&mmap[0..mem::size_of::<LayerPackHeader>()]);

    println!("version: {}", header.version());
    println!("level: {}", header.tile_level());
    println!(
        "extent: {:.02} degrees ({} as)",
        header.angular_extent_as() as f64 / 3_600f64,
        header.angular_extent_as(),
    );
    println!("tiles: {}", header.tile_count());
    println!(
        "index: {} bytes",
        header.tile_start() - header.index_start()
    );
    let items = LayerPackIndexItem::overlay_slice(&mmap[header.index_start()..header.tile_start()]);
    for item in items {
        if let Some(latitude) = opt.latitude {
            if item.base_lat_as() != latitude {
                continue;
            }
        }
        if let Some(longitude) = opt.longitude {
            if item.base_lon_as() != longitude {
                continue;
            }
        }
        println!(
            "  {:>10}, {:>10}: {:?}",
            item.base_lat_as(),
            item.base_lon_as(),
            ChildIndex::from_index(item.index_in_parent() as usize)
        );
        if opt.dump_tile {
            let st = item.tile_start();
            let ed = item.tile_end();
            let data = &mmap[st as usize..ed as usize];
            for b in data {
                println!("  {:02X}", b);
            }
        }
    }

    Ok(())
}
