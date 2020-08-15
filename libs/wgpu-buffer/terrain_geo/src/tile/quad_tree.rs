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
use crate::tile::{ChildIndex, TerrainLevel};
use absolute_unit::{meters, Angle, ArcSeconds};
use catalog::{Catalog, FileId};
use failure::Fallible;
use geodesy::{GeoCenter, Graticule};
use log::trace;
use std::collections::HashMap;

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
    angular_extent: Angle<ArcSeconds>,
    base: Graticule<GeoCenter>,
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
            nodes: Vec::new(),
            votes: HashMap::new(),
            additions: Vec::new(),
            generation: 0,
        };
        let root = obj.link_node(
            prefix,
            TerrainLevel::new(0),
            &TerrainLevel::base(),
            TerrainLevel::base_angular_extent(),
            catalog,
        );
        trace!("loaded quad-tree with {} nodes", obj.nodes.len());
        assert!(root.is_none() || root.unwrap().offset() == 0);
        Ok(obj)
    }

    pub(crate) fn link_node(
        &mut self,
        prefix: &str,
        level: TerrainLevel,
        base: &Graticule<GeoCenter>,
        angular_extent: Angle<ArcSeconds>,
        catalog: &Catalog,
    ) -> Option<QuadTreeId> {
        let filename = format!(
            "{}-L{}-{}-{}.bin",
            prefix,
            level.offset(),
            base.latitude.format_latitude(),
            base.longitude.format_longitude(),
        );
        if let Some(file_id) = catalog.lookup(&filename) {
            let child_offset = TerrainLevel::new(level.offset() + 1);
            let ang = angular_extent / 2.0;
            let h = meters!(0);
            let se_base = Graticule::new(base.latitude, base.longitude + ang, h);
            let nw_base = Graticule::new(base.latitude + ang, base.longitude, h);
            let ne_base = Graticule::new(base.latitude + ang, base.longitude + ang, h);
            let qid = QuadTreeId::new(self.nodes.len());
            self.nodes.push(QuadTreeNode {
                angular_extent,
                base: *base,
                file_id,
                children: [None; 4],
            });
            let sw_node = self.link_node(prefix, child_offset, base, ang, catalog);
            let se_node = self.link_node(prefix, child_offset, &se_base, ang, catalog);
            let nw_node = self.link_node(prefix, child_offset, &nw_base, ang, catalog);
            let ne_node = self.link_node(prefix, child_offset, &ne_base, ang, catalog);
            self.nodes[qid.offset()].children = [sw_node, se_node, nw_node, ne_node];
            return Some(qid);
        }
        None
    }

    pub(crate) fn file_id(&self, id: &QuadTreeId) -> FileId {
        self.nodes[id.offset()].file_id
    }

    pub(crate) fn begin_update(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.additions.clear();
    }

    pub(crate) fn note_required(&mut self, grat: &Graticule<GeoCenter>) {
        self.visit_required_node(self.root, grat);
    }

    fn visit_required_node(&mut self, node_id: QuadTreeId, grat: &Graticule<GeoCenter>) {
        {
            // Ensure that the graticule is actually in this patch.
            let node = &self.nodes[node_id.offset()];
            assert!(grat.latitude >= node.base.latitude);
            assert!(grat.latitude <= node.base.latitude + node.angular_extent);
            assert!(grat.longitude >= node.base.longitude);
            assert!(grat.longitude <= node.base.longitude + node.angular_extent);
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
        let is_northern = grat.latitude > node.base.latitude + (node.angular_extent / 2.0);
        let is_eastern = grat.longitude > node.base.longitude + (node.angular_extent / 2.0);
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
