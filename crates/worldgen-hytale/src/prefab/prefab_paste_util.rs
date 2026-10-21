// Ported from: `.hytale-source/.../worldgen/prefab/PrefabPasteUtil.java`
//! Java `com.hypixel.hytale.server.worldgen.prefab.PrefabPasteUtil` — the
//! paste of a prefab into a chunk.
//!
//! # Semantics
//!
//! * The prefab is pasted at the world position `(x, y, z)`; the chunk
//!   position `(cx, y, cz)` is the *local* position of that world position
//!   inside the chunk that is being generated.
//! * Only the columns that are inside the chunk are visited
//!   (`ChunkUtil.isWithinLocalChunk`).
//! * With `fitHeightmap` the prefab follows the terrain: the height of the
//!   world column under the prefab origin becomes the origin height and
//!   every column is offset by the height difference of its own column.
//! * The block mask gates the paste: a block is only written when the mask
//!   allows replacing the block that is already there.
//! * The child prefabs are composed recursively, up to a depth of
//!   [`MAX_RECURSION_DEPTH`] (10).
//!
//! # Deviations
//!
//! * **The entities are not pasted** (`REMAINING.md` § 2).
//! * **The block components are not pasted** — they are the ECS state of the
//!   block (`REMAINING.md` § 2).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use hytale_math::chunk_util;
use hytale_math::fast_random::FastRandom;
use hytale_math::hash_util;
use procedural::noise::condition::{ICoordinateCondition, ICoordinateRndCondition};

use worldgen_model::block_priority::HEIGHT;

use crate::chunk::chunk_generator_execution::ChunkGeneratorExecution;
use crate::prefab::buffer::prefab_buffer::PrefabBuffer;
use crate::prefab::buffer::prefab_buffer_call::PrefabBufferCall;
use crate::prefab::prefab_rotation::PrefabRotation;
use crate::prefab::prefab_weights::PrefabWeights;
use crate::util::condition::BlockMaskCondition;

/// Java `PrefabPasteUtil.MAX_RECURSION_DEPTH` = `10`.
pub const MAX_RECURSION_DEPTH: i32 = 10;

/// The supplier of a prefab — the Java `WorldGenPrefabSupplier` seen through
/// the paste.
pub trait PrefabSupplier {
    /// Java `@Nullable public IPrefabBuffer get()` — the prefab buffer.
    fn get(&self) -> Option<Rc<PrefabBuffer>>;

    /// Java `@Nonnull public String getPrefabName()` — the name the weights
    /// are looked up with.
    fn get_prefab_name(&self) -> String;

    /// Java `buffer.supplier.getLoader().get(path)` — the suppliers of a
    /// child prefab.
    fn get_child_suppliers(&self, path: &str) -> Vec<Rc<dyn PrefabSupplier>>;
}

/// Java `public static class PrefabPasteBuffer extends PrefabBufferCall`.
pub struct PrefabPasteBuffer<'a> {
    /// Java `@Nullable public ChunkGeneratorExecution execution`.
    pub execution: RefCell<Option<&'a mut ChunkGeneratorExecution>>,
    /// Java `public final Vector3i posWorld`.
    pub pos_world: Cell<(i32, i32, i32)>,
    /// Java `public final Vector3i posChunk`.
    pub pos_chunk: Cell<(i32, i32, i32)>,
    /// Java `public final Random childRandom` — used to generate the child
    /// prefabs deterministically.
    pub child_random: RefCell<FastRandom>,
    /// Java `public int originHeight` — the world height at the centre of
    /// the prefab (used with `fitHeightmap`).
    pub origin_height: Cell<i32>,
    /// Java `public int yOffset` — the offset of the current column.
    pub y_offset: Cell<i32>,
    /// Java `public int seed` — the worldgen seed.
    pub seed: Cell<i32>,
    /// Java `public int specificSeed` — the seed of the current prefab
    /// composition.
    pub specific_seed: Cell<i32>,
    /// Java `public int instanceSeed`.
    pub instance_seed: Cell<i32>,
    /// Java `public boolean fitHeightmap`.
    pub fit_heightmap: Cell<bool>,
    /// Java `public boolean deepSearch`.
    pub deep_search: Cell<bool>,
    /// Java `public BlockMaskCondition blockMask`.
    ///
    /// `None` is the Java `BlockMaskCondition.DEFAULT_TRUE` instance (the
    /// identity check `blockMask != DEFAULT_TRUE` skips the mask
    /// evaluation); `Some(mask)` is a *different* instance — exactly like
    /// the Java, where `DEFAULT_TRUE` and `DEFAULT_FALSE` are two distinct
    /// objects with the same content.
    pub block_mask: RefCell<Option<BlockMaskCondition>>,
    /// Java `public int environmentId`.
    pub environment_id: Cell<i32>,
    /// Java `public byte priority`.
    pub priority: Cell<i8>,
    /// Java `public ICoordinateCondition heightCondition`.
    pub height_condition: RefCell<Rc<dyn ICoordinateCondition>>,
    /// Java `public ICoordinateRndCondition spawnCondition`.
    pub spawn_condition: RefCell<Rc<dyn ICoordinateRndCondition>>,
    /// Java `@Nullable public WorldGenPrefabSupplier supplier`.
    pub supplier: RefCell<Option<Rc<dyn PrefabSupplier>>>,
    /// Java `private int depth`.
    depth: Cell<i32>,
    /// Java `public Random random` (the `PrefabBufferCall` field).
    random: RefCell<FastRandom>,
    /// Java `public PrefabRotation rotation` (the `PrefabBufferCall` field).
    rotation: Cell<PrefabRotation>,
}

impl<'a> PrefabPasteBuffer<'a> {
    /// Java `public PrefabPasteBuffer()`.
    pub fn new() -> Self {
        let buffer = Self {
            execution: RefCell::new(None),
            pos_world: Cell::new((0, 0, 0)),
            pos_chunk: Cell::new((0, 0, 0)),
            child_random: RefCell::new(FastRandom::new(0)),
            origin_height: Cell::new(0),
            y_offset: Cell::new(0),
            seed: Cell::new(0),
            specific_seed: Cell::new(0),
            instance_seed: Cell::new(0),
            fit_heightmap: Cell::new(false),
            deep_search: Cell::new(false),
            block_mask: RefCell::new(None),
            environment_id: Cell::new(worldgen_model::chunk::ENVIRONMENT_NOT_SET),
            priority: Cell::new(0),
            height_condition: RefCell::new(Rc::new(
                procedural::noise::condition::DefaultCoordinateCondition::True,
            )),
            spawn_condition: RefCell::new(Rc::new(
                procedural::noise::condition::DefaultCoordinateRndCondition::True,
            )),
            supplier: RefCell::new(None),
            depth: Cell::new(0),
            random: RefCell::new(FastRandom::new(0)),
            rotation: Cell::new(PrefabRotation::Rotation0),
        };
        buffer.reset();
        buffer
    }

    /// Java `public void setSeed(int worldSeed, long externalSeed)`.
    pub fn set_seed(&self, world_seed: i32, external_seed: i64) {
        self.seed.set(world_seed);
        self.specific_seed.set(external_seed as i32);
        self.random.borrow_mut().set_seed(external_seed);
        self.child_random.borrow_mut().set_seed(external_seed);
    }

    /// Java `void reset()`.
    pub fn reset(&self) {
        *self.execution.borrow_mut() = None;
        self.fit_heightmap.set(false);
        self.deep_search.set(false);
        *self.block_mask.borrow_mut() = None;
        self.environment_id
            .set(worldgen_model::chunk::ENVIRONMENT_NOT_SET);
        *self.height_condition.borrow_mut() =
            Rc::new(procedural::noise::condition::DefaultCoordinateCondition::True);
        *self.spawn_condition.borrow_mut() =
            Rc::new(procedural::noise::condition::DefaultCoordinateRndCondition::True);
        *self.supplier.borrow_mut() = None;
        self.depth.set(0);
    }

    /// The recursion depth of the paste.
    pub fn get_depth(&self) -> i32 {
        self.depth.get()
    }
}

impl Default for PrefabPasteBuffer<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl PrefabBufferCall for PrefabPasteBuffer<'_> {
    fn call_random(&self) -> &RefCell<FastRandom> {
        &self.random
    }

    fn call_rotation(&self) -> PrefabRotation {
        self.rotation.get()
    }
}

/// Java `public static void generate(@Nonnull PrefabPasteBuffer buffer, @Nonnull
/// PrefabRotation rotation, @Nonnull WorldGenPrefabSupplier supplier, int x,
/// int y, int z, int cx, int cz)`.
///
/// Deviation: the Java `finally { buffer.reset(); }` does not run when the
/// paste throws (the Rust port has no `finally`); the paste buffer is
/// therefore left dirty after a panic. The cave path allocates a fresh
/// buffer per paste, so the state never leaks.
#[allow(clippy::too_many_arguments)]
pub fn generate(
    buffer: &PrefabPasteBuffer<'_>,
    rotation: PrefabRotation,
    supplier: Rc<dyn PrefabSupplier>,
    x: i32,
    y: i32,
    z: i32,
    cx: i32,
    cz: i32,
) {
    *buffer.supplier.borrow_mut() = Some(Rc::clone(&supplier));
    buffer.pos_world.set((x, y, z));
    buffer.pos_chunk.set((cx, y, cz));
    buffer.rotation.set(rotation);

    generate0(buffer, &supplier);

    buffer.reset();
}

/// Java `private static void generate0(@Nonnull PrefabPasteBuffer _buffer,
/// @Nonnull WorldGenPrefabSupplier supplier)`.
fn generate0(buffer: &PrefabPasteBuffer<'_>, supplier: &Rc<dyn PrefabSupplier>) {
    if buffer.execution.borrow().is_none() {
        return;
    }
    let Some(prefab) = supplier.get() else {
        return;
    };

    if buffer.fit_heightmap.get() {
        let pos_world = buffer.pos_world.get();
        let height = {
            let execution = buffer.execution.borrow();
            let execution = execution.as_ref().expect("the execution is set");
            execution
                .get_chunk_generator()
                .get_height(buffer.seed.get(), pos_world.0, pos_world.2)
        };
        buffer.origin_height.set(height);
        let mut pos_chunk = buffer.pos_chunk.get();
        pos_chunk.1 = height;
        buffer.pos_chunk.set(pos_chunk);
        let mut pos_world = buffer.pos_world.get();
        pos_world.1 = height;
        buffer.pos_world.set(pos_world);
    }

    let pos_world = buffer.pos_world.get();
    buffer.instance_seed.set(get_instance_seed(
        buffer.seed.get(),
        pos_world.0,
        pos_world.1,
        pos_world.2,
    ));

    let access = prefab.new_access();

    let mut column_predicate = |cx: i32, cz: i32, _blocks: usize, buffer: &PrefabPasteBuffer| {
        let pos_chunk = buffer.pos_chunk.get();
        let bx = cx + pos_chunk.0;
        let bz = cz + pos_chunk.2;
        if !chunk_util::is_within_local_chunk(bx, bz) {
            return false;
        }
        if buffer.fit_heightmap.get() {
            let pos_world = buffer.pos_world.get();
            let height = {
                let execution = buffer.execution.borrow();
                let execution = execution.as_ref().expect("the execution is set");
                execution.get_chunk_generator().get_height(
                    buffer.seed.get(),
                    pos_world.0 + cx,
                    pos_world.2 + cz,
                )
            };
            buffer.y_offset.set(height - buffer.origin_height.get());
        } else {
            buffer.y_offset.set(0);
        }
        true
    };

    let mut block_consumer = |visit: crate::prefab::buffer::prefab_buffer::PrefabBlockVisit<'_>,
                              buffer: &PrefabPasteBuffer| {
        let pos_chunk = buffer.pos_chunk.get();
        let bx = visit.x + pos_chunk.0;
        let by = visit.y + pos_chunk.1 + buffer.y_offset.get();
        let bz = visit.z + pos_chunk.2;
        if !(chunk_util::MIN_Y..HEIGHT).contains(&by) {
            return;
        }

        let mut execution = buffer.execution.borrow_mut();
        let execution = execution.as_mut().expect("the execution is set");

        if let Some(mask) = buffer.block_mask.borrow().as_ref() {
            let current_block = execution.get_block(bx, by, bz);
            let current_fluid = execution.get_fluid(bx, by, bz);
            if !mask.eval(current_block, current_fluid, visit.block_id, visit.fluid_id) {
                return;
            }
        }

        execution.set_block_with(
            bx,
            by,
            bz,
            buffer.priority.get(),
            visit.block_id,
            visit.support_value,
            visit.rotation,
            visit.filler,
        );
        execution.set_fluid(
            bx,
            by,
            bz,
            buffer.priority.get(),
            visit.fluid_id,
            visit.fluid_level,
        );
        execution.set_environment(bx, by, bz, buffer.environment_id.get());
    };

    let mut child_consumer = |visit: crate::prefab::buffer::prefab_buffer::PrefabChildVisit<'_>,
                              buffer: &PrefabPasteBuffer| {
        if buffer.depth.get() >= MAX_RECURSION_DEPTH {
            return;
        }
        buffer.depth.set(buffer.depth.get() + 1);

        // Record the buffer state before recursively generating the children.
        let local = buffer.pos_chunk.get();
        let world = buffer.pos_world.get();
        let y_offset = buffer.y_offset.get();
        let origin_height = buffer.origin_height.get();
        let specific_seed = buffer.specific_seed.get();
        let instance_seed = buffer.instance_seed.get();
        let rotation = buffer.rotation.get();
        let fit_heightmap = buffer.fit_heightmap.get();

        generate_child(
            visit.x,
            visit.y,
            visit.z,
            visit.path,
            visit.fit_heightmap,
            visit.inherit_seed,
            visit.inherit_height_condition,
            visit.weights,
            visit.rotation,
            buffer,
        );

        // Reset the buffer to the previous state.
        buffer.pos_chunk.set(local);
        buffer.pos_world.set(world);
        buffer.y_offset.set(y_offset);
        buffer.origin_height.set(origin_height);
        buffer.rotation.set(rotation);
        buffer.specific_seed.set(specific_seed);
        buffer.instance_seed.set(instance_seed);
        buffer.fit_heightmap.set(fit_heightmap);

        buffer.depth.set(buffer.depth.get() - 1);
    };

    access.for_each(
        &mut column_predicate,
        &mut block_consumer,
        Some(&mut child_consumer),
        buffer,
    );
}

/// Java `private static void generateChild(int cx, int cy, int cz, String
/// path, boolean fitHeightmap, boolean inheritSeed, boolean
/// inheritHeightCondition, @Nonnull PrefabWeights weights, @Nonnull
/// PrefabRotation rotation, @Nonnull PrefabPasteBuffer buffer, Random
/// random)`.
#[allow(clippy::too_many_arguments)]
fn generate_child(
    cx: i32,
    cy: i32,
    cz: i32,
    path: &str,
    fit_heightmap: bool,
    inherit_seed: bool,
    inherit_height_condition: bool,
    weights: &PrefabWeights,
    rotation: PrefabRotation,
    buffer: &PrefabPasteBuffer<'_>,
) {
    let parent_specific_seed = buffer.specific_seed.get();
    let parent_fit_heightmap = buffer.fit_heightmap.get();

    let mut pos_chunk = buffer.pos_chunk.get();
    pos_chunk = (pos_chunk.0 + cx, pos_chunk.1 + cy, pos_chunk.2 + cz);
    buffer.pos_chunk.set(pos_chunk);
    let mut pos_world = buffer.pos_world.get();
    pos_world = (pos_world.0 + cx, pos_world.1 + cy, pos_world.2 + cz);
    buffer.pos_world.set(pos_world);
    buffer.fit_heightmap.set(fit_heightmap);
    buffer.rotation.set(buffer.rotation.get().add(rotation));

    // Java: "TODO: Is this a bug: Logic seems to be inverted?" — the Java
    // keeps the parent seed when the child does *not* inherit the seed.
    if !inherit_seed {
        buffer.specific_seed.set(parent_specific_seed);
    } else {
        let mut specific_seed =
            hash_util::hash4(parent_specific_seed as i64, cx as i64, cy as i64, cz as i64) as i32;
        if specific_seed == parent_specific_seed {
            specific_seed += 1;
        }
        buffer.specific_seed.set(specific_seed);
    }

    if parent_fit_heightmap {
        let pos_world = buffer.pos_world.get();
        let height = {
            let execution = buffer.execution.borrow();
            let execution = execution.as_ref().expect("the execution is set");
            execution
                .get_chunk_generator()
                .get_height(buffer.seed.get(), pos_world.0, pos_world.2)
        };
        let y_offset = height - buffer.origin_height.get();
        let mut pos_chunk = buffer.pos_chunk.get();
        pos_chunk.1 += y_offset;
        buffer.pos_chunk.set(pos_chunk);
        let mut pos_world = buffer.pos_world.get();
        pos_world.1 += y_offset;
        buffer.pos_world.set(pos_world);
    }

    let pos_chunk = buffer.pos_chunk.get();
    if pos_chunk.1 >= chunk_util::MIN_Y
        && pos_chunk.1 < HEIGHT
        && chunk_util::is_within_local_chunk(pos_chunk.0, pos_chunk.2)
    {
        let mut execution = buffer.execution.borrow_mut();
        let execution = execution.as_mut().expect("the execution is set");
        execution.set_block(
            pos_chunk.0,
            pos_chunk.1,
            pos_chunk.2,
            buffer.priority.get(),
            0,
        );
    }

    // Optionally check if the child prefab still matches the height threshold
    // spawn condition of the parent.
    if inherit_height_condition {
        let pos_world = buffer.pos_world.get();
        let spawn_condition = Rc::clone(&buffer.spawn_condition.borrow());
        let mut child_random = buffer.child_random.borrow_mut();
        if !spawn_condition.eval(
            buffer.seed.get(),
            pos_world.0,
            pos_world.2,
            pos_world.1,
            &mut child_random,
        ) {
            return;
        }
    }

    let Some(supplier) = buffer.supplier.borrow().clone() else {
        return;
    };
    let prefab_suppliers = supplier.get_child_suppliers(path);
    if prefab_suppliers.is_empty() {
        return;
    }
    let prefab_supplier = {
        let mut child_random = buffer.child_random.borrow_mut();
        next_prefab(&prefab_suppliers, weights, &mut child_random)
    };
    generate0(buffer, &prefab_supplier);
}

/// Java `private static WorldGenPrefabSupplier nextPrefab(@Nonnull Random
/// random, @Nonnull WorldGenPrefabSupplier[] prefabSuppliers, @Nonnull
/// PrefabWeights weights)`.
fn next_prefab(
    prefab_suppliers: &[Rc<dyn PrefabSupplier>],
    weights: &PrefabWeights,
    random: &mut FastRandom,
) -> Rc<dyn PrefabSupplier> {
    if prefab_suppliers.len() == 1 {
        return Rc::clone(&prefab_suppliers[0]);
    }

    let mut prefab = None;
    if weights.size() > 0 {
        prefab = weights
            .get(
                prefab_suppliers,
                &|supplier: &Rc<dyn PrefabSupplier>| supplier.get_prefab_name(),
                random,
            )
            .cloned();
    }

    // Fall back to picking randomly if the weighted selection did not
    // produce a result.
    match prefab {
        Some(prefab) => prefab,
        None => Rc::clone(
            &prefab_suppliers[random.next_int_bound(prefab_suppliers.len() as i32) as usize],
        ),
    }
}

/// Java `private static int getInstanceSeed(final int seed, @Nonnull final
/// Vector3i position)`.
fn get_instance_seed(seed: i32, x: i32, y: i32, z: i32) -> i32 {
    let hash = hash_util::hash4(seed as i64, x as i64, y as i64, z as i64);
    // Java `HashCommon.long2int(long)`.
    ((hash as u64 >> 32) as u32 as i32) ^ (hash as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cave::test_stubs::chunk_generator;
    use crate::chunk::chunk_generator_execution::ChunkGeneratorExecution;
    use crate::prefab::buffer::prefab_buffer::PrefabBuffer;
    use crate::prefab::buffer::prefab_buffer_block_entry::PrefabBufferBlockEntry;
    use crate::prefab::prefab_weights::PrefabWeights;
    use worldgen_model::chunk::GeneratedChunk;

    /// A prefab supplier over a synthetic buffer.
    struct TestSupplier {
        buffer: Rc<PrefabBuffer>,
        name: String,
        children: Vec<Rc<dyn PrefabSupplier>>,
    }

    impl TestSupplier {
        fn new(buffer: Rc<PrefabBuffer>, name: &str) -> Self {
            Self {
                buffer,
                name: name.to_string(),
                children: Vec::new(),
            }
        }

        fn with_children(mut self, children: Vec<Rc<dyn PrefabSupplier>>) -> Self {
            self.children = children;
            self
        }
    }

    impl PrefabSupplier for TestSupplier {
        fn get(&self) -> Option<Rc<PrefabBuffer>> {
            Some(Rc::clone(&self.buffer))
        }

        fn get_prefab_name(&self) -> String {
            self.name.clone()
        }

        fn get_child_suppliers(&self, path: &str) -> Vec<Rc<dyn PrefabSupplier>> {
            // The store resolves the *path* of the child prefab.
            self.children
                .iter()
                .filter(|child| child.get_prefab_name() == path)
                .cloned()
                .collect()
        }
    }

    /// A prefab with the given blocks (the `(x, y, z, block)` tuples).
    fn prefab(blocks: &[(i32, i32, i32, i32)]) -> Rc<PrefabBuffer> {
        let mut builder = PrefabBuffer::new_builder();
        builder.set_anchor((0, 0, 0));
        let mut columns: std::collections::BTreeMap<(i32, i32), Vec<PrefabBufferBlockEntry>> =
            std::collections::BTreeMap::new();
        for (x, y, z, block) in blocks {
            columns
                .entry((*x, *z))
                .or_default()
                .push(PrefabBufferBlockEntry::new(*y, *block, "Rock_Stone"));
        }
        for ((x, z), mut entries) in columns {
            entries.sort_by_key(|entry| entry.y);
            builder.add_column(x, z, entries);
        }
        Rc::new(builder.build())
    }

    /// A prefab of a single block.
    fn single_block(block: i32) -> Rc<PrefabBuffer> {
        prefab(&[(0, 0, 0, block)])
    }

    /// The execution of a chunk at the origin.
    fn execution() -> ChunkGeneratorExecution {
        let chunk = GeneratedChunk::with_coordinates(0, 0, 0);
        ChunkGeneratorExecution::new(0, chunk_generator(), chunk)
    }

    #[test]
    fn the_paste_writes_the_blocks_of_the_prefab() {
        let mut execution = execution();
        let buffer = PrefabPasteBuffer::new();
        *buffer.execution.borrow_mut() = Some(&mut execution);
        buffer
            .priority
            .set(worldgen_model::block_priority::PREFAB_CAVE);

        let supplier: Rc<dyn PrefabSupplier> = Rc::new(TestSupplier::new(single_block(42), "test"));
        generate(&buffer, PrefabRotation::Rotation0, supplier, 0, 0, 0, 0, 0);

        assert_eq!(execution.get_block(0, 0, 0), 42);
        assert_eq!(
            execution.get_priority_chunk().get(0, 0, 0),
            worldgen_model::block_priority::PREFAB_CAVE
        );
    }

    #[test]
    fn the_paste_only_reaches_the_columns_of_the_chunk() {
        let mut execution = execution();
        let buffer = PrefabPasteBuffer::new();
        *buffer.execution.borrow_mut() = Some(&mut execution);

        // A block outside of the chunk (the local x is 40).
        let supplier: Rc<dyn PrefabSupplier> =
            Rc::new(TestSupplier::new(prefab(&[(40, 0, 0, 7)]), "test"));
        generate(&buffer, PrefabRotation::Rotation0, supplier, 0, 0, 0, 0, 0);

        assert_eq!(execution.get_block(0, 0, 0), 0);
    }

    #[test]
    fn the_block_mask_gates_the_paste() {
        let mut execution = execution();
        // The terrain is written first (the filling priority).
        execution.set_block(0, 0, 0, worldgen_model::block_priority::FILLING, 5);

        let buffer = PrefabPasteBuffer::new();
        *buffer.execution.borrow_mut() = Some(&mut execution);
        // The default mask only replaces an empty cell.
        *buffer.block_mask.borrow_mut() = Some(BlockMaskCondition::new());

        let supplier: Rc<dyn PrefabSupplier> = Rc::new(TestSupplier::new(single_block(42), "test"));
        generate(&buffer, PrefabRotation::Rotation0, supplier, 0, 0, 0, 0, 0);

        // The block was not replaced (the cell is not empty).
        assert_eq!(execution.get_block(0, 0, 0), 5);
    }

    #[test]
    fn the_child_prefabs_are_composed() {
        let mut execution = execution();
        let buffer = PrefabPasteBuffer::new();
        *buffer.execution.borrow_mut() = Some(&mut execution);

        // The parent has a child prefab at (1, 0, 0).
        let mut builder = PrefabBuffer::new_builder();
        builder.add_column(0, 0, vec![PrefabBufferBlockEntry::new(0, 1, "Rock_Stone")]);
        builder.add_child_prefab(
            1,
            0,
            0,
            "child",
            false,
            false,
            false,
            PrefabWeights::none(),
            PrefabRotation::Rotation0,
        );
        let parent = Rc::new(builder.build());

        let child: Rc<dyn PrefabSupplier> = Rc::new(TestSupplier::new(single_block(99), "child"));
        let supplier: Rc<dyn PrefabSupplier> =
            Rc::new(TestSupplier::new(parent, "parent").with_children(vec![child]));

        generate(&buffer, PrefabRotation::Rotation0, supplier, 0, 0, 0, 0, 0);

        assert_eq!(execution.get_block(0, 0, 0), 1, "the parent block");
        assert_eq!(execution.get_block(1, 0, 0), 99, "the child block");
    }

    #[test]
    fn the_rotation_rotates_the_blocks() {
        let mut execution = execution();
        let buffer = PrefabPasteBuffer::new();
        *buffer.execution.borrow_mut() = Some(&mut execution);

        // A block at the local (1, 0, 0); the prefab is pasted at (20, 0, 20)
        // so that the rotated position stays inside the chunk.
        let supplier: Rc<dyn PrefabSupplier> =
            Rc::new(TestSupplier::new(prefab(&[(1, 0, 0, 42)]), "test"));
        generate(
            &buffer,
            PrefabRotation::Rotation90,
            supplier,
            20,
            0,
            20,
            20,
            20,
        );

        // The 90° rotation maps (1, 0) to (0, -1).
        assert_eq!(execution.get_block(20, 0, 19), 42);
        assert_eq!(execution.get_block(21, 0, 20), 0);
    }

    #[test]
    fn the_fluid_and_the_environment_are_written() {
        let mut execution = execution();
        let buffer = PrefabPasteBuffer::new();
        *buffer.execution.borrow_mut() = Some(&mut execution);
        buffer.environment_id.set(58);

        let mut builder = PrefabBuffer::new_builder();
        let mut entry = PrefabBufferBlockEntry::new(0, 42, "Rock_Stone");
        entry.fluid_id = 2;
        entry.fluid_level = 8;
        builder.add_column(0, 0, vec![entry]);
        let prefab = Rc::new(builder.build());

        let supplier: Rc<dyn PrefabSupplier> = Rc::new(TestSupplier::new(prefab, "test"));
        generate(&buffer, PrefabRotation::Rotation0, supplier, 0, 0, 0, 0, 0);

        assert_eq!(execution.get_fluid(0, 0, 0), 2);
        assert_eq!(execution.get_chunk().get_environment(0, 0, 0), 58);
    }
}
