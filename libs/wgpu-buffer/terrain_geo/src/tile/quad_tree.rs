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
use crate::tile::{
    ChildIndex, LayerPack, LayerPackIndexItem, TerrainLevel, TileCompression, TILE_EXTENT,
};
use absolute_unit::{Angle, ArcSeconds};
use catalog::{Catalog, FileId};
use failure::{ensure, Fallible};
use fxhash::FxHashMap;
use geometry::AABB2;
use log::trace;
use std::{collections::HashMap, ops::Range};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub(crate) struct QuadTreeId {
    id: u32,
}

impl QuadTreeId {
    fn new(id: usize) -> Self {
        assert!(id < u32::MAX as usize);
        Self { id: id as u32 }
    }

    fn empty() -> Self {
        Self { id: u32::MAX }
    }

    fn is_empty(&self) -> bool {
        self.id == u32::MAX
    }

    fn offset(&self) -> usize {
        self.id as usize
    }
}

struct QuadTreeNode {
    span: Range<usize>,
    children: [QuadTreeId; 4],
    base: (i32, i32), // lat, lon
    level: u8,
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
    layer_packs: Vec<LayerPack>,

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
    pub(crate) fn from_layers(prefix: &str, catalog: &Catalog) -> Fallible<Self> {
        // Find all layers in this set.
        let mut layer_packs = Vec::new();
        let layer_glob = format!("{}-L??.mip", prefix);
        for layer_fid in catalog.find_labeled_matching("default", &layer_glob, Some("mip"))? {
            layer_packs.push(LayerPack::new(layer_fid, catalog)?);
        }
        layer_packs.sort_by_key(|lp| *lp.terrain_level());
        ensure!(!layer_packs.is_empty());
        let mut node_count = 0;
        for (i, lp) in layer_packs.iter().enumerate() {
            assert_eq!(lp.terrain_level(), &TerrainLevel::new(i));
            node_count += lp.tile_count();
        }

        let mut obj = Self {
            root: QuadTreeId::new(0),
            nodes: Vec::with_capacity(node_count),
            layer_packs,
            votes: HashMap::new(),
            additions: Vec::new(),
            generation: 0,
        };

        let mut acc = FxHashMap::default();
        for i in 0..obj.layer_packs.len() {
            obj.link_layer(&mut acc, i, catalog)?;
        }

        trace!("loaded quad-tree with {} nodes", obj.nodes.len());
        Ok(obj)
    }

    fn link_layer(
        &mut self,
        acc: &mut FxHashMap<(i32, i32), QuadTreeId>,
        layer_num: usize,
        catalog: &Catalog,
    ) -> Fallible<()> {
        // Create a node for each item at level i, given we have created nodes for i-1 and that
        // the base of those nodes are in `acc`.
        let extent = self.layer_packs[layer_num].angular_extent_as();

        // Note: we have to overlay manually because the data may not be mapped if the item was a raw file.
        let mut id_update_cursor = self.nodes.len();
        let index_bytes = self.layer_packs[layer_num].index_bytes(catalog).unwrap();
        let raw_index = LayerPackIndexItem::overlay_slice(&index_bytes)?;
        for item in raw_index {
            let base = (item.base_lat_as(), item.base_lon_as());
            // FIXME: are we making a decision to not support 32bit here?
            let span = item.tile_start() as usize..item.tile_end() as usize;
            let id = QuadTreeId::new(self.nodes.len());
            self.nodes.push(QuadTreeNode {
                span,
                children: [QuadTreeId::empty(); 4],
                base,
                level: layer_num as u8,
            });
            let parent_index = ChildIndex::from_index(item.index_in_parent() as usize);
            let parent_base = match parent_index {
                ChildIndex::SouthWest => base,
                ChildIndex::SouthEast => (base.0, base.1 - extent),
                ChildIndex::NorthWest => (base.0 - extent, base.1),
                ChildIndex::NorthEast => (base.0 - extent, base.1 - extent),
            };

            if layer_num != 0 {
                let parent_id = acc[&parent_base];
                assert_eq!(
                    self.nodes[parent_id.offset()].children[parent_index.to_index()],
                    QuadTreeId::empty()
                );
                self.nodes[parent_id.offset()].children[parent_index.to_index()] = id;
            }
        }

        acc.clear();
        if layer_num != 12 {
            for item in raw_index {
                let base = (item.base_lat_as(), item.base_lon_as());
                let id = QuadTreeId::new(id_update_cursor);
                id_update_cursor += 1;
                acc.insert(base, id);
            }
        }

        #[cfg(debug_assertions)]
        self.sanity_check_tree(&self.nodes[self.root.offset()]);

        Ok(())
    }

    #[cfg(debug_assertions)]
    fn sanity_check_tree(&self, node: &QuadTreeNode) {
        if !node.children[ChildIndex::SouthWest.to_index()].is_empty() {
            let child_id = node.children[ChildIndex::SouthWest.to_index()];
            let child = &self.nodes[child_id.offset()];
            assert_eq!(node.base.0, child.base.0);
            assert_eq!(node.base.1, child.base.1);
            assert_eq!(node.level, child.level - 1);
            self.sanity_check_tree(child);
        }
        if !node.children[ChildIndex::SouthEast.to_index()].is_empty() {
            let child_id = node.children[ChildIndex::SouthEast.to_index()];
            let child = &self.nodes[child_id.offset()];
            let extent = self.angular_extent_as(&child_id);
            assert_eq!(node.base.0, child.base.0);
            assert_eq!(node.base.1 + extent, child.base.1);
            assert_eq!(node.level, child.level - 1);
            self.sanity_check_tree(child);
        }
        if !node.children[ChildIndex::NorthWest.to_index()].is_empty() {
            let child_id = node.children[ChildIndex::NorthWest.to_index()];
            let child = &self.nodes[child_id.offset()];
            let extent = self.angular_extent_as(&child_id);
            assert_eq!(node.base.0 + extent, child.base.0);
            assert_eq!(node.base.1, child.base.1);
            assert_eq!(node.level, child.level - 1);
            self.sanity_check_tree(child);
        }
        if !node.children[ChildIndex::NorthEast.to_index()].is_empty() {
            let child_id = node.children[ChildIndex::NorthEast.to_index()];
            let child = &self.nodes[child_id.offset()];
            let extent = self.angular_extent_as(&child_id);
            assert_eq!(node.base.0 + extent, child.base.0);
            assert_eq!(node.base.1 + extent, child.base.1);
            assert_eq!(node.level, child.level - 1);
            self.sanity_check_tree(child);
        }
    }

    pub(crate) fn tile_compression(&self, id: &QuadTreeId) -> TileCompression {
        let level = self.nodes[id.offset()].level as usize;
        self.layer_packs[level].tile_compression()
    }

    pub(crate) fn file_id(&self, id: &QuadTreeId) -> FileId {
        let level = self.nodes[id.offset()].level as usize;
        self.layer_packs[level].file_id()
    }

    pub(crate) fn file_extent(&self, id: &QuadTreeId) -> Range<usize> {
        self.nodes[id.offset()].span.clone()
    }

    pub(crate) fn base(&self, id: &QuadTreeId) -> (i32, i32) {
        self.nodes[id.offset()].base
    }

    pub(crate) fn level(&self, id: &QuadTreeId) -> u8 {
        self.nodes[id.offset()].level
    }

    pub(crate) fn angular_extent_as(&self, id: &QuadTreeId) -> i32 {
        let level = self.nodes[id.offset()].level as usize;
        self.layer_packs[level].angular_extent_as()
    }

    pub(crate) fn aabb_as(&self, id: &QuadTreeId) -> AABB2<i32> {
        let extent = self.angular_extent_as(id);
        let node = &self.nodes[id.offset()];
        AABB2::new(
            [node.base.0, node.base.1],
            [node.base.0 + extent, node.base.1 + extent],
        )
    }

    pub(crate) fn begin_update(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.additions.clear();
    }

    pub(crate) fn note_required(&mut self, window: &AABB2<i32>, resolution: Angle<ArcSeconds>) {
        self.visit_required_node(0, self.root, window, resolution);
    }

    fn visit_required_node(
        &mut self,
        level: usize,
        id: QuadTreeId,
        window: &AABB2<i32>,
        resolution: Angle<ArcSeconds>,
    ) {
        debug_assert!(window.overlaps(&self.aabb_as(&id)));

        // Ensure we have a votes structure, potentially noting the addition of a node.
        // We cannot use `.entry` because of the need to re-borrow self here.
        #[allow(clippy::map_entry)]
        if !self.votes.contains_key(&id) {
            self.additions.push(id);
            self.votes.insert(
                id,
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
            let vote_ref = self.votes.get_mut(&id).unwrap();
            vote_ref.generation = self.generation;
            vote_ref.votes += 1;
        }

        // Exit before recursing if we have enough pixels at the current level,
        // based on the edge length of the triangle defining the window. Don't forget
        // the nyquist sampling theorem.
        if (self.angular_extent_as(&id) as f64 / TILE_EXTENT as f64) < resolution.f64() / 2. {
            return;
        }

        // If we have not yet reached full refinement, continue walking down.
        let children = self.nodes[id.offset()].children;
        for child in &children {
            if !child.is_empty() && self.aabb_as(child).overlaps(window) {
                self.visit_required_node(level + 1, *child, window, resolution);
            }
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
