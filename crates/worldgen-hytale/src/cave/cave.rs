// Ported from: `.hytale-source/.../worldgen/cave/Cave.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.Cave` — the compiled cave
//! system of one cave type at one position.
//!
//! The cave collects its nodes per chunk; [`Cave::compile`] freezes the raw
//! per-chunk lists into arrays that are sorted by the priority of the node
//! type (the lower the priority, the earlier the node is written to the
//! world).

use std::collections::HashMap;
use std::rc::Rc;

use crate::cave::cave_type::CaveType;
use crate::cave::element::cave_element::CaveElement;
use crate::cave::element::cave_node::CaveNode;
use crate::util::bounds::WorldBounds;

/// Java `class Cave`.
pub struct Cave {
    /// Java `@Nullable private Long2ObjectMap<List<CaveNode>> rawChunkNodeMap`
    /// — `None` after [`Cave::compile`].
    raw_chunk_node_map: Option<HashMap<i64, Vec<Rc<CaveNode>>>>,
    /// Java `private Long2ObjectMap<CaveNode[]> chunkNodeMap`.
    chunk_node_map: HashMap<i64, Vec<Rc<CaveNode>>>,
    /// Java `private final CaveType caveType`.
    cave_type: Rc<CaveType>,
    /// Java `@Nonnull private final WorldBounds bounds`.
    bounds: WorldBounds,
    /// Java `private int nodeCount`.
    node_count: i32,
}

impl Cave {
    /// Java `Cave(CaveType caveType)`.
    pub fn new(cave_type: Rc<CaveType>) -> Self {
        Self {
            raw_chunk_node_map: Some(HashMap::new()),
            chunk_node_map: HashMap::new(),
            cave_type,
            bounds: WorldBounds::new(),
            node_count: 0,
        }
    }

    /// Java `long getNodeCount()`.
    pub fn get_node_count(&self) -> i64 {
        self.node_count as i64
    }

    /// Java `@Nonnull CaveType getCaveType()`.
    pub fn get_cave_type(&self) -> &Rc<CaveType> {
        &self.cave_type
    }

    /// Java `@Nonnull WorldBounds getBounds()`.
    pub fn get_bounds(&self) -> WorldBounds {
        self.bounds
    }

    /// Java `void addNode(CaveNode element)`.
    ///
    /// The Java method compiles the node, grows the cave bounds with the
    /// node bounds and files the node under every chunk the node touches.
    pub fn add_node(&mut self, element: &Rc<CaveNode>) {
        element.compile();

        self.bounds.include_bounds(&element.get_bounds());

        let raw_chunk_node_map = self
            .raw_chunk_node_map
            .as_mut()
            .expect("Cave.addNode: the cave was already compiled (Java: NullPointerException)");
        element.for_each_chunk(&mut |chunk| {
            raw_chunk_node_map
                .entry(chunk)
                .or_default()
                .push(Rc::clone(element));
        });

        self.node_count += 1;
    }

    /// Java `boolean contains(long chunkIndex)`.
    pub fn contains(&self, chunk_index: i64) -> bool {
        self.chunk_node_map.contains_key(&chunk_index)
    }

    /// Java `CaveNode[] getCaveNodes(long chunkIndex)` — `None` for an
    /// unknown chunk (the Java method returns `null`).
    pub fn get_cave_nodes(&self, chunk_index: i64) -> Option<&[Rc<CaveNode>]> {
        self.chunk_node_map
            .get(&chunk_index)
            .map(|nodes| nodes.as_slice())
    }

    /// Java `void compile()`.
    ///
    /// Panics when the cave is compiled twice — the Java code throws a
    /// `NullPointerException` in the same case.
    pub fn compile(&mut self) {
        self.compile_node_map();
    }

    /// Java `private void compileNodeMap()`.
    fn compile_node_map(&mut self) {
        let raw_chunk_node_map = self
            .raw_chunk_node_map
            .take()
            .expect("Cave.compile: the cave was already compiled (Java: NullPointerException)");

        let mut chunk_node_map = HashMap::with_capacity(raw_chunk_node_map.len());
        for (chunk, mut nodes) in raw_chunk_node_map {
            // Java: `ObjectArrays.mergeSort(array, ORDER)` with
            // `ORDER = Comparator.comparingInt(o -> o.getCaveNodeType().
            // getPriority())` — the merge sort is stable, and so is the Rust
            // `sort_by_key`, so nodes with the same priority keep their
            // insertion order.
            nodes.sort_by_key(|node| node.get_cave_node_type().get_priority());
            chunk_node_map.insert(chunk, nodes);
        }
        self.chunk_node_map = chunk_node_map;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cave::test_stubs::{cave_node_type, cave_type, StubShape};

    /// A node whose bounds cover the given block range.
    fn node(priority: i32, min: i32, max: i32) -> Rc<CaveNode> {
        Rc::new(CaveNode::new(
            0,
            cave_node_type(priority),
            Rc::new(StubShape::new(
                WorldBounds::with_bounds(min, 0, 0, max, 0, 0),
                0.0,
                0.0,
            )),
            0.0,
            0.0,
        ))
    }

    #[test]
    fn a_new_cave_is_empty() {
        let cave = Cave::new(cave_type("cave"));
        assert_eq!(cave.get_node_count(), 0);
        assert!(!cave.contains(0));
        assert!(cave.get_cave_nodes(0).is_none());
    }

    #[test]
    fn a_node_is_filed_under_every_chunk_it_touches() {
        let mut cave = Cave::new(cave_type("cave"));
        // The bounds cover blocks 0..=63 → chunks 0 and 1.
        cave.add_node(&node(0, 0, 63));
        assert_eq!(cave.get_node_count(), 1);

        cave.compile();
        assert!(cave.contains(hytale_math::chunk_util::index_chunk(0, 0)));
        assert!(cave.contains(hytale_math::chunk_util::index_chunk(1, 0)));
        assert!(!cave.contains(hytale_math::chunk_util::index_chunk(2, 0)));
        assert_eq!(
            cave.get_cave_nodes(hytale_math::chunk_util::index_chunk(0, 0))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn the_cave_bounds_grow_with_the_nodes() {
        let mut cave = Cave::new(cave_type("cave"));
        cave.add_node(&node(0, -16, 16));
        let bounds = cave.get_bounds();
        assert_eq!(bounds.min_x, -16);
        assert_eq!(bounds.max_x, 16);
    }

    #[test]
    fn the_nodes_are_sorted_by_their_type_priority() {
        let mut cave = Cave::new(cave_type("cave"));
        cave.add_node(&node(5, 0, 31));
        cave.add_node(&node(1, 0, 31));
        cave.add_node(&node(3, 0, 31));
        cave.compile();

        let nodes = cave
            .get_cave_nodes(hytale_math::chunk_util::index_chunk(0, 0))
            .unwrap();
        let priorities: Vec<i32> = nodes
            .iter()
            .map(|node| node.get_cave_node_type().get_priority())
            .collect();
        assert_eq!(priorities, vec![1, 3, 5]);
    }

    #[test]
    #[should_panic(expected = "already compiled")]
    fn compiling_twice_panics_like_the_java_null_pointer() {
        let mut cave = Cave::new(cave_type("cave"));
        cave.compile();
        cave.compile();
    }
}
