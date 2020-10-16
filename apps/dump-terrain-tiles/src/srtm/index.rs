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
use crate::{mip::Region, srtm::tile::Tile};
use absolute_unit::{arcseconds, degrees, meters, ArcSeconds, Degrees, Meters};
use failure::Fallible;
use geodesy::{Cartesian, GeoCenter, GeoSurface, Graticule};
use parking_lot::RwLock;
use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};
use terrain_geo::tile::TerrainLevel;

pub struct Index {
    tiles: Vec<Tile>,
    // by latitude, then longitude
    by_graticule: HashMap<i16, HashMap<i16, usize>>,
}

impl Index {
    pub fn max_resolution_level() -> usize {
        TerrainLevel::arcsecond_level()
    }

    pub fn from_directory(directory: &Path) -> Fallible<Arc<RwLock<Self>>> {
        let mut index_filename = PathBuf::from(directory);
        index_filename.push("srtm30m_bounding_boxes.json");

        let mut index_file = File::open(index_filename.as_path())?;
        let mut index_content = String::new();
        index_file.read_to_string(&mut index_content)?;

        let index_json = json::parse(&index_content)?;
        assert_eq!(index_json["type"], "FeatureCollection");
        let features = &index_json["features"];
        let mut tiles = Vec::new();
        for feature in features.members() {
            let tile = Tile::from_feature(&feature, directory)?;
            tiles.push(tile);
        }

        let mut by_graticule = HashMap::new();
        for (i, tile) in tiles.iter().enumerate() {
            let lon = tile.longitude();
            by_graticule
                .entry(tile.latitude())
                .or_insert_with(HashMap::new)
                .insert(lon, i);
        }

        println!("Mapped {} SRTM tiles", tiles.len());
        Ok(Arc::new(RwLock::new(Self {
            tiles,
            by_graticule,
        })))
    }

    /// Check if this tile-set has any tiles that overlap with the given region.
    pub fn contains_region(&self, region: Region) -> bool {
        // Figure out what integer latitudes lie on or in the region.
        let lo_lat = region.base.lat::<Degrees>().floor() as i16;
        let hi_lat = degrees!(region.base.latitude + region.extent).floor() as i16;
        let lo_lon = region.base.lon::<Degrees>().floor() as i16;
        let hi_lon = degrees!(region.base.longitude + region.extent).floor() as i16;
        for lat in lo_lat..=hi_lat {
            if let Some(by_lon) = self.by_graticule.get(&lat) {
                for lon in lo_lon..=hi_lon {
                    if by_lon.get(&lon).is_some() {
                        return true;
                    }
                }
            }
        }
        false
    }

    #[allow(unused)]
    pub fn sample_linear(&self, grat: &Graticule<GeoSurface>) -> f32 {
        let lat = Tile::index(grat.lat());
        let lon = Tile::index(grat.lon());
        if let Some(row) = self.by_graticule.get(&lat) {
            if let Some(&tile_id) = row.get(&lon) {
                return self.tiles[tile_id].sample_linear(grat);
            }
        }
        0f32
    }

    pub fn sample_nearest(&self, grat: &Graticule<GeoSurface>) -> i16 {
        let lat = Tile::index(grat.lat());
        let lon = Tile::index(grat.lon());
        if let Some(row) = self.by_graticule.get(&lat) {
            if let Some(&tile_id) = row.get(&lon) {
                return self.tiles[tile_id].sample_nearest(grat);
            }
        }
        0
    }

    fn unit_offset(
        grat: &Graticule<GeoSurface>,
        lat_offset: i16,
        lon_offset: i16,
    ) -> Graticule<GeoSurface> {
        Graticule::<GeoSurface>::new(
            degrees!(grat.lat::<ArcSeconds>() + arcseconds!(lat_offset))
                .clamp(degrees!(-60), degrees!(60)),
            degrees!(grat.lon::<ArcSeconds>() + arcseconds!(lon_offset))
                .wrap(degrees!(-180), degrees!(180)),
            meters!(0),
        )
    }

    // Note that we compute the normal on the tangent plane.
    // TODO: How much "droop" is there over a single arcsecond? Is it enough that it could
    //       be an important factor in some cases? Or does flattening cancel out the potential
    //       droop anyway? It certainly does in the height=0 case and I'd guess it does in other
    //       cases as well. Would be nice to prove though.
    // TODO: How much does latitude affect things? Should we just compute real coordinates from
    //       our graticule?
    pub fn compute_normal_at(&self, grat: &Graticule<GeoSurface>) -> [i16; 2] {
        // Compute 9 tap locations for computing our normal.
        let g_c = *grat;
        let g_sw = Self::unit_offset(grat, -1, -1);
        let g_s = Self::unit_offset(grat, -1, 0);
        let g_se = Self::unit_offset(grat, -1, 1);
        let g_w = Self::unit_offset(grat, 0, -1);
        let g_e = Self::unit_offset(grat, 0, 1);
        let g_nw = Self::unit_offset(grat, 1, -1);
        let g_n = Self::unit_offset(grat, 1, 0);
        let g_ne = Self::unit_offset(grat, 1, 1);

        // Sample at 8 surrounding locations plus the center.
        let h_c = self.sample_nearest(&g_c) as f64;
        let h_sw = self.sample_nearest(&g_sw) as f64;
        let h_s = self.sample_nearest(&g_s) as f64;
        let h_se = self.sample_nearest(&g_se) as f64;
        let h_w = self.sample_nearest(&g_w) as f64;
        let h_e = self.sample_nearest(&g_e) as f64;
        let h_nw = self.sample_nearest(&g_nw) as f64;
        let h_n = self.sample_nearest(&g_n) as f64;
        let h_ne = self.sample_nearest(&g_ne) as f64;

        // Convert all graticules to raw cartesian.
        let c_c = Cartesian::<GeoCenter, Meters>::from(Graticule::<GeoCenter>::from(g_c)).vec64();
        let c_sw = Cartesian::<GeoCenter, Meters>::from(Graticule::<GeoCenter>::from(g_sw)).vec64();
        let c_s = Cartesian::<GeoCenter, Meters>::from(Graticule::<GeoCenter>::from(g_s)).vec64();
        let c_se = Cartesian::<GeoCenter, Meters>::from(Graticule::<GeoCenter>::from(g_se)).vec64();
        let c_w = Cartesian::<GeoCenter, Meters>::from(Graticule::<GeoCenter>::from(g_w)).vec64();
        let c_e = Cartesian::<GeoCenter, Meters>::from(Graticule::<GeoCenter>::from(g_e)).vec64();
        let c_nw = Cartesian::<GeoCenter, Meters>::from(Graticule::<GeoCenter>::from(g_nw)).vec64();
        let c_n = Cartesian::<GeoCenter, Meters>::from(Graticule::<GeoCenter>::from(g_n)).vec64();
        let c_ne = Cartesian::<GeoCenter, Meters>::from(Graticule::<GeoCenter>::from(g_ne)).vec64();

        // Displace by heights.
        let v_c = c_c + c_c.normalize() * h_c;
        let mut v_sw = c_sw + c_sw.normalize() * h_sw;
        let mut v_s = c_s + c_s.normalize() * h_s;
        let mut v_se = c_se + c_se.normalize() * h_se;
        let mut v_w = c_w + c_w.normalize() * h_w;
        let mut v_e = c_e + c_e.normalize() * h_e;
        let mut v_nw = c_nw + c_nw.normalize() * h_nw;
        let mut v_n = c_n + c_n.normalize() * h_n;
        let mut v_ne = c_ne + c_ne.normalize() * h_ne;

        // Center around the displaced center point.
        v_sw -= v_c;
        v_s -= v_c;
        v_se -= v_c;
        v_w -= v_c;
        v_e -= v_c;
        v_nw -= v_c;
        v_n -= v_c;
        v_ne -= v_c;

        // Get right handed normals.
        let avg_normal = ((v_sw.cross(&v_s).normalize()
            + v_s.cross(&v_se).normalize()
            + v_se.cross(&v_e).normalize()
            + v_e.cross(&v_ne).normalize()
            + v_ne.cross(&v_n).normalize()
            + v_n.cross(&v_nw).normalize()
            + v_nw.cross(&v_w).normalize()
            + v_w.cross(&v_sw).normalize())
            / 8f64)
            .normalize();

        // Note that this result is relative to `norm`.
        let norm_cart = Cartesian::<GeoCenter, Meters>::from(avg_normal);
        let norm_grat = Graticule::<GeoCenter>::from(norm_cart);
        // let lat_deg = rel.lat::<Degrees>().f64();
        // let lon_deg = rel.lon::<Degrees>().f64();
        let lat_deg = (norm_grat.lat::<Degrees>() - grat.lat::<Degrees>()).f64();
        let lon_deg = (norm_grat.lon::<Degrees>() - grat.lon::<Degrees>()).f64();
        let lat = (lat_deg / 90f64 * (1 << 15) as f64).round() as i16;
        let lon = (lon_deg / 180f64 * (1 << 15) as f64).round() as i16;
        // println!("LL: {} => {}, {}", grat, lat_deg, lon_deg);

        [lat, lon]
    }
}
