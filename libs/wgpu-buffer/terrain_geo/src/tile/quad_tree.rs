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
use crate::tile::{ChildIndex, TerrainLevel};
use absolute_unit::{arcseconds, meters, Angle, ArcSeconds};
use catalog::{Catalog, FileId};
use failure::Fallible;
use geodesy::{GeoCenter, Graticule};
use log::trace;
use std::{collections::HashMap, fmt::Write};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub(crate) struct QuadTreeId {
    id: u32,
}

impl QuadTreeId {
    fn new(id: usize) -> Self {
        assert!(id < u32::MAX as usize);
        Self { id: id as u32 }
    }

    fn offset(&self) -> usize {
        self.id as usize
    }
}

struct QuadTreeNode {
    extent_as: i32,
    base_lat_as: i32,
    base_lon_as: i32,
    file_id: FileId,
    children: [Option<QuadTreeId>; 4],
}

struct NodeVotes {
    // Number of times that this node was sampled. This is used to prioritize uploads.
    votes: u32,

    // The current generation of this node. Bumped on each frame. If it gets behind, we
    // know that this node is no longer in the visible set.
    generation: u32,
}

pub(crate) struct QuadTree {
    // An immutable tree discovered based on what nodes are available in catalog at startup.
    root: QuadTreeId,
    nodes: Vec<QuadTreeNode>,

    // Note that we may be tracking many hundreds of thousands of tiles, so we want to avoid
    // visiting each tile per frame. To do this we use the following side structures to track
    // current visibility and priority.
    //
    // Update algorithm:
    //   Bump global generation count at the start of the update.
    //   For each note_resize:
    //     For each node walking to target node:
    //       if the nodes generation is older than this generation, reset the vote count
    //       set the node generation to current generation
    //       bump the vote count on the node
    //       if not present in `votes`
    //         note the new node
    //         insert the node into by `votes`
    //   At end, clear out any `votes` that are behind the current generation
    votes: HashMap<QuadTreeId, NodeVotes>,
    additions: Vec<QuadTreeId>,
    generation: u32,
}

impl QuadTree {
    pub(crate) fn from_catalog(prefix: &str, catalog: &Catalog) -> Fallible<Self> {
        let mut obj = Self {
            root: QuadTreeId::new(0),
            nodes: Vec::with_capacity(2_000_000),
            votes: HashMap::new(),
            additions: Vec::new(),
            generation: 0,
        };
        let extent_as = arcseconds!(TerrainLevel::base_angular_extent()).f64() as i32;
        let base_lat_as = TerrainLevel::base().lat::<ArcSeconds>().f64() as i32;
        let base_lon_as = TerrainLevel::base().lon::<ArcSeconds>().f64() as i32;
        let mut scratch_filename = String::with_capacity(1_024);
        let root = obj.link_node(
            prefix,
            TerrainLevel::new(0),
            base_lat_as,
            base_lon_as,
            extent_as,
            catalog,
            &mut scratch_filename,
        );
        trace!("loaded quad-tree with {} nodes", obj.nodes.len());
        assert!(root.is_none() || root.unwrap().offset() == 0);
        Ok(obj)
    }

    fn split_hms(v: i32) -> (i32, i32, i32) {
        let d = v / 3_600;
        let m = v / 60 - d * 60;
        let s = v - d * 3_600 - m * 60;
        (d, m, s)
    }

    fn write_latitude(mut lat: i32, out: &mut String) -> Fallible<()> {
        let lat_hemi = if lat >= 0 {
            "N"
        } else {
            lat = -lat;
            "S"
        };
        let (d, m, s) = Self::split_hms(lat);
        Ok(write!(out, "{}{:03}d{:02}m{:02}s", lat_hemi, d, m, s)?)
    }

    fn write_longitude(mut lon: i32, out: &mut String) -> Fallible<()> {
        let lon_hemi = if lon >= 0 {
            "E"
        } else {
            lon = -lon;
            "W"
        };
        let (d, m, s) = Self::split_hms(lon);
        Ok(write!(out, "{}{:03}d{:02}m{:02}s", lon_hemi, d, m, s)?)
    }

    pub(crate) fn link_node(
        &mut self,
        prefix: &str,
        level: TerrainLevel,
        base_lat_as: i32,
        base_lon_as: i32,
        extent_as: i32,
        catalog: &Catalog,
        filename: &mut String,
    ) -> Option<QuadTreeId> {
        // Note: this is a bit weird, but avoids the allocation.
        filename.clear();
        write!(filename, "{}-L{}-", prefix, level.offset()).unwrap();
        Self::write_latitude(base_lat_as, filename).unwrap();
        write!(filename, "-").unwrap();
        Self::write_longitude(base_lon_as, filename).unwrap();
        write!(filename, ".bin").unwrap();

        if let Some(file_id) = catalog.lookup(&filename) {
            let child_offset = TerrainLevel::new(level.offset() + 1);
            let ang = extent_as / 2;
            let qid = QuadTreeId::new(self.nodes.len());
            self.nodes.push(QuadTreeNode {
                extent_as,
                base_lat_as,
                base_lon_as,
                file_id,
                children: [None; 4],
            });
            if child_offset.offset() <= TerrainLevel::arcsecond_level() {
                let sw_base = (base_lat_as, base_lon_as);
                let se_base = (base_lat_as, base_lon_as + ang);
                let nw_base = (base_lat_as + ang, base_lon_as);
                let ne_base = (base_lat_as + ang, base_lon_as + ang);
                let sw_node = self.link_node(
                    prefix,
                    child_offset,
                    sw_base.0,
                    sw_base.1,
                    ang,
                    catalog,
                    filename,
                );
                let se_node = self.link_node(
                    prefix,
                    child_offset,
                    se_base.0,
                    se_base.1,
                    ang,
                    catalog,
                    filename,
                );
                let nw_node = self.link_node(
                    prefix,
                    child_offset,
                    nw_base.0,
                    nw_base.1,
                    ang,
                    catalog,
                    filename,
                );
                let ne_node = self.link_node(
                    prefix,
                    child_offset,
                    ne_base.0,
                    ne_base.1,
                    ang,
                    catalog,
                    filename,
                );
                self.nodes[qid.offset()].children = [sw_node, se_node, nw_node, ne_node];
            }
            return Some(qid);
        }
        None
    }

    pub(crate) fn file_id(&self, id: &QuadTreeId) -> FileId {
        self.nodes[id.offset()].file_id
    }

    pub(crate) fn base(&self, id: &QuadTreeId) -> (i32, i32) {
        (
            self.nodes[id.offset()].base_lat_as,
            self.nodes[id.offset()].base_lon_as,
        )
    }

    pub(crate) fn angular_extent(&self, id: &QuadTreeId) -> i32 {
        self.nodes[id.offset()].extent_as
    }

    pub(crate) fn begin_update(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.additions.clear();
    }

    pub(crate) fn note_required(&mut self, grat: &Graticule<GeoCenter>) {
        self.visit_required_node(self.root, grat);
    }

    fn visit_required_node(&mut self, node_id: QuadTreeId, grat: &Graticule<GeoCenter>) {
        let lat = grat.lat::<ArcSeconds>().f64() as i32;
        let lon = grat.lon::<ArcSeconds>().f64() as i32;
        {
            // Ensure that the graticule is actually in this patch.
            let node = &self.nodes[node_id.offset()];
            assert!(lat >= node.base_lat_as);
            assert!(lat <= node.base_lat_as + node.extent_as);
            assert!(lon >= node.base_lon_as);
            assert!(lon <= node.base_lon_as + node.extent_as);
        }

        // Ensure we have a votes structure, potentially noting the addition of a node.
        // We cannot use `.entry` because of the need to re-borrow self here.
        #[allow(clippy::map_entry)]
        if !self.votes.contains_key(&node_id) {
            self.additions.push(node_id);
            self.votes.insert(
                node_id,
                NodeVotes {
                    votes: 0,
                    generation: self.generation,
                },
            );
        }
        // If the node was already in the vote set, but from the prior frame, it will still
        // have the previous generation. We need to reset the vote count for the current frame.
        // Note that we reset all vote counts at the end of the frame, meaning that we do not
        // need to take a branch on the generation in our inner loop here.
        {
            let vote_ref = self.votes.get_mut(&node_id).unwrap();
            vote_ref.generation = self.generation;
            vote_ref.votes += 1;
        }

        // If we have not yet reached full refinement, continue walking down.
        // FIXME: exit before recursing if we have enough pixels at the current level,
        //        based on the edge length of the patch from which grat came and our known gpu
        //        subdivision level.

        // Our assertion in the head that we are inside the patch simplifies our check here.
        let node = &self.nodes[node_id.offset()];
        let is_northern = lat > node.base_lat_as + (node.extent_as / 2);
        let is_eastern = lon > node.base_lon_as + (node.extent_as / 2);
        let child_index = match (is_northern, is_eastern) {
            (true, true) => ChildIndex::NorthEast,
            (true, false) => ChildIndex::NorthWest,
            (false, true) => ChildIndex::SouthEast,
            (false, false) => ChildIndex::SouthWest,
        };
        if let Some(child_id) = node.children[child_index.to_index()] {
            self.visit_required_node(child_id, grat);
        }
    }

    pub(crate) fn finish_update(
        &mut self,
        additions: &mut Vec<(u32, QuadTreeId)>,
        removals: &mut Vec<QuadTreeId>,
    ) {
        for node_id in self.additions.drain(..) {
            additions.push((self.votes[&node_id].votes, node_id));
        }
        for (node_id, votes) in self.votes.iter() {
            if votes.generation != self.generation {
                removals.push(*node_id);
            }
        }
        let gen = self.generation;
        self.votes.retain(|_, v| v.generation == gen);
    }
}
