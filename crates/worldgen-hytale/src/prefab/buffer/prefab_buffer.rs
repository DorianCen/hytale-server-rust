// Ported from: `.hytale-source/.../CoreServer/.../prefab/selection/buffer/impl/PrefabBuffer.java`
//! and `.../prefab/selection/buffer/impl/IPrefabBuffer.java`
//! Java `com.hypixel.hytale.server.core.prefab.selection.buffer.impl.PrefabBuffer`
//! — the in-memory form of a prefab: its anchor, its bounds, its columns of
//! blocks and its child prefabs.
//!
//! # Layout
//!
//! The Java buffer stores every column in a packed byte segment (a `short`
//! mask per block plus the variable-width id / y-offset / chance /
//! rotation / filler / fluid fields) inside one big arena. The Rust port
//! stores the *same values* in a plain vector of
//! [`PrefabBufferBlockEntry`] per column, and the columns themselves in a
//! fastutil-compatible open hash map (see
//! [`int_object_open_hash_map`](crate::prefab::buffer::int_object_open_hash_map))
//! so that the **iteration order** — which is observable through the block
//! chance rolls — matches the Java one.
//!
//! Every height / index computation of the Java buffer is reproduced
//! exactly:
//!
//! | Java                              | Rust                                       |
//! |-----------------------------------|--------------------------------------------|
//! | `getMinYAt`                       | the `y` of the **first** entry of the column |
//! | `getMaxYAt`                       | the `y` of the **last** entry of the column  |
//! | `getMinX/MaxX(rotation)`          | the rotated corners of the `(min, max)` box  |
//! | `getBlockId` / `getFiller` / `getRotationIndex` | the matching entry of the column |
//!
//! The columns are addressed with the Java key
//! `MathUtil.packInt(x, z) = (x << 16) | (z & 0xFFFF)`; the *lookup* of
//! `getMinYAt` / `getMaxYAt` rotates the queried coordinates (the Java
//! `// TODO I think we need to inverse the rotation here..` quirk) and the
//! Rust port reproduces that quirk.

use hytale_math::math_util;

use worldgen_model::block_key::RotationTuple;

use crate::prefab::buffer::int_object_open_hash_map::IntObjectOpenHashMap;
use crate::prefab::buffer::prefab_buffer_block_entry::PrefabBufferBlockEntry;
use crate::prefab::buffer::prefab_buffer_call::PrefabBufferCall;
use crate::prefab::buffer::prefab_buffer_column::PrefabBufferColumn;
use crate::prefab::prefab_rotation::PrefabRotation;
use crate::prefab::prefab_weights::PrefabWeights;

/// Java `PrefabBuffer.DEFAULT_CHANCE` = `1.0f`.
pub const DEFAULT_CHANCE: f32 = 1.0;

/// Java `PrefabBuffer.ChildPrefab` — a prefab that is placed inside another
/// prefab.
#[derive(Clone, Debug, PartialEq)]
pub struct ChildPrefab {
    /// Java `private final int x`.
    pub x: i32,
    /// Java `private final int y`.
    pub y: i32,
    /// Java `private final int z`.
    pub z: i32,
    /// Java `@Nonnull private final String path` — the prefab key of the
    /// child.
    pub path: String,
    /// Java `private final boolean fitHeightmap`.
    pub fit_heightmap: bool,
    /// Java `private final boolean inheritSeed`.
    pub inherit_seed: bool,
    /// Java `private final boolean inheritHeightCondition`.
    pub inherit_height_condition: bool,
    /// Java `@Nonnull private final PrefabWeights weights`.
    pub weights: PrefabWeights,
    /// Java `@Nonnull private final PrefabRotation rotation`.
    pub rotation: PrefabRotation,
}

/// The block a [`PrefabBufferAccessor::for_each`] traversal reports.
pub struct PrefabBlockVisit<'a> {
    /// Java `int x` — the rotated x coordinate.
    pub x: i32,
    /// Java `int y`.
    pub y: i32,
    /// Java `int z` — the rotated z coordinate.
    pub z: i32,
    /// Java `int blockId`.
    pub block_id: i32,
    /// Java `int supportValue`.
    pub support_value: i32,
    /// Java `int rotation` — the *rotated* rotation index.
    pub rotation: i32,
    /// Java `int filler` — the *rotated* filler offset.
    pub filler: i32,
    /// Java `int fluidId`.
    pub fluid_id: i32,
    /// Java `byte fluidLevel`.
    pub fluid_level: i8,
    /// The entry the visit was built from.
    pub entry: &'a PrefabBufferBlockEntry,
}

/// The child prefab a [`PrefabBufferAccessor::for_each`] traversal reports.
pub struct PrefabChildVisit<'a> {
    /// Java `int x` — the rotated x coordinate.
    pub x: i32,
    /// Java `int y`.
    pub y: i32,
    /// Java `int z` — the rotated z coordinate.
    pub z: i32,
    /// Java `String path`.
    pub path: &'a str,
    /// Java `boolean fitHeightmap`.
    pub fit_heightmap: bool,
    /// Java `boolean inheritSeed`.
    pub inherit_seed: bool,
    /// Java `boolean inheritHeightCondition`.
    pub inherit_height_condition: bool,
    /// Java `PrefabWeights weights`.
    pub weights: &'a PrefabWeights,
    /// Java `PrefabRotation rotation`.
    pub rotation: PrefabRotation,
}

/// Java `public class PrefabBuffer`.
#[derive(Clone, Debug, PartialEq)]
pub struct PrefabBuffer {
    /// Java `@Nonnull private final Vector3i anchor`.
    pub anchor: (i32, i32, i32),
    /// Java `@Nonnull private final Vector3i min`.
    pub min: (i32, i32, i32),
    /// Java `@Nonnull private final Vector3i max`.
    pub max: (i32, i32, i32),
    /// Java `@Nonnull private final Int2ObjectMap<PrefabBufferColumn>
    /// columns` — the fastutil-compatible column map.
    pub columns: IntObjectOpenHashMap<PrefabBufferColumn>,
    /// Java `@Nonnull private final ChildPrefab[] childPrefabs`.
    pub child_prefabs: Vec<ChildPrefab>,
}

impl PrefabBuffer {
    /// Java `public static Builder newBuilder()`.
    pub fn new_builder() -> PrefabBufferBuilder {
        PrefabBufferBuilder::new()
    }

    /// Java `public PrefabBufferAccessor newAccess()`.
    pub fn new_access(&self) -> PrefabBufferAccessor<'_> {
        PrefabBufferAccessor { buffer: self }
    }

    /// Java `public int getAnchorX()`.
    pub fn get_anchor_x(&self) -> i32 {
        self.anchor.0
    }

    /// Java `public int getAnchorY()`.
    pub fn get_anchor_y(&self) -> i32 {
        self.anchor.1
    }

    /// Java `public int getAnchorZ()`.
    pub fn get_anchor_z(&self) -> i32 {
        self.anchor.2
    }
}

/// Java `public static class PrefabBuffer.Builder`.
#[derive(Debug, Default)]
pub struct PrefabBufferBuilder {
    /// Java `private final Vector3i min, max`.
    min: (i32, i32, i32),
    /// Java `private final Vector3i max`.
    max: (i32, i32, i32),
    /// Java `private final Int2ObjectMap<BuilderColumn> builderColumns`.
    builder_columns: Vec<(i32, i32, Vec<PrefabBufferBlockEntry>)>,
    /// Java `private final List<ChildPrefab> childPrefabs`.
    child_prefabs: Vec<ChildPrefab>,
    /// Java `private Vector3i anchor`.
    anchor: (i32, i32, i32),
}

impl PrefabBufferBuilder {
    /// Java `private Builder()`.
    pub fn new() -> Self {
        Self {
            min: (i32::MAX, i32::MAX, i32::MAX),
            max: (i32::MIN, i32::MIN, i32::MIN),
            builder_columns: Vec::new(),
            child_prefabs: Vec::new(),
            anchor: (0, 0, 0),
        }
    }

    /// Java `public void setAnchor(@Nonnull Vector3i anchor)`.
    pub fn set_anchor(&mut self, anchor: (i32, i32, i32)) {
        self.anchor = anchor;
    }

    /// Java `public void addColumn(int x, int z, @Nonnull
    /// PrefabBufferBlockEntry[] entries, @Nullable Holder<EntityStore>[]
    /// entityHolders)`.
    ///
    /// Deviation: the entity holders are not ported (`REMAINING.md` § 2), so
    /// a column without blocks is dropped (the Java builder keeps it when
    /// it carries entities).
    pub fn add_column(&mut self, x: i32, z: i32, entries: Vec<PrefabBufferBlockEntry>) {
        if x < i16::MIN as i32 {
            panic!("x is smaller than {}. Given: {x}", i16::MIN);
        }
        if x > i16::MAX as i32 {
            panic!("x is larger than {}. Given: {x}", i16::MAX);
        }
        if z < i16::MIN as i32 {
            panic!("z is smaller than {}. Given: {z}", i16::MIN);
        }
        if z > i16::MAX as i32 {
            panic!("z is larger than {}. Given: {z}", i16::MAX);
        }

        if entries.is_empty() {
            return;
        }

        if self
            .builder_columns
            .iter()
            .any(|(cx, cz, _)| *cx == x && *cz == z)
        {
            panic!("Duplicate column");
        }

        let mut last_y = i32::MIN;
        for entry in &entries {
            if entry.y <= last_y {
                panic!("Y Values are not sequential. {last_y} -> {}", entry.y);
            }
            let offset = if last_y == i32::MIN {
                0
            } else {
                entry.y - last_y
            };
            if offset > 0xFFFF {
                panic!("Offset is larger than {}. Given: {offset}", 0xFFFF);
            }
            last_y = entry.y;
        }

        self.builder_columns.push((x, z, entries));
    }

    /// Java `public void addChildPrefab(int x, int y, int z, @Nonnull String
    /// path, boolean fitHeightmap, boolean inheritSeed, boolean
    /// inheritHeightCondition, @Nullable PrefabWeights weights, @Nonnull
    /// PrefabRotation rotation)`.
    #[allow(clippy::too_many_arguments)]
    pub fn add_child_prefab(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        path: &str,
        fit_heightmap: bool,
        inherit_seed: bool,
        inherit_height_condition: bool,
        weights: PrefabWeights,
        rotation: PrefabRotation,
    ) {
        self.child_prefabs.push(ChildPrefab {
            x,
            y,
            z,
            path: path.to_string(),
            fit_heightmap,
            inherit_seed,
            inherit_height_condition,
            weights,
            rotation,
        });
    }

    /// Java `@Nonnull public PrefabBuffer build()`.
    pub fn build(self) -> PrefabBuffer {
        let mut min = self.min;
        let mut max = self.max;
        let mut columns: IntObjectOpenHashMap<PrefabBufferColumn> = IntObjectOpenHashMap::new();

        for (x, z, entries) in self.builder_columns {
            if let Some(first) = entries.first() {
                if first.y < min.1 {
                    min.1 = first.y;
                }
            }
            if let Some(last) = entries.last() {
                if last.y > max.1 {
                    max.1 = last.y;
                }
            }
            if x < min.0 {
                min.0 = x;
            }
            if x > max.0 {
                max.0 = x;
            }
            if z < min.2 {
                min.2 = z;
            }
            if z > max.2 {
                max.2 = z;
            }
            columns.put(math_util::pack_int(x, z), PrefabBufferColumn::new(entries));
        }

        // Java: "If the prefab is empty then don't use the initial values for
        // min/max instead we assign them to 0".
        if columns.is_empty() {
            min = (0, 0, 0);
            max = (0, 0, 0);
        }

        PrefabBuffer {
            anchor: self.anchor,
            min,
            max,
            columns,
            child_prefabs: self.child_prefabs,
        }
    }
}

/// Java `public static class PrefabBuffer.PrefabBufferAccessor implements
/// IPrefabBuffer`.
pub struct PrefabBufferAccessor<'a> {
    /// Java `@Nonnull private final PrefabBuffer prefabBuffer`.
    pub buffer: &'a PrefabBuffer,
}

impl<'a> PrefabBufferAccessor<'a> {
    /// Java `public int getAnchorX()`.
    pub fn get_anchor_x(&self) -> i32 {
        self.buffer.get_anchor_x()
    }

    /// Java `public int getAnchorY()`.
    pub fn get_anchor_y(&self) -> i32 {
        self.buffer.get_anchor_y()
    }

    /// Java `public int getAnchorZ()`.
    pub fn get_anchor_z(&self) -> i32 {
        self.buffer.get_anchor_z()
    }

    /// Java `public int getMinX(@Nonnull PrefabRotation rotation)`.
    pub fn get_min_x(&self, rotation: PrefabRotation) -> i32 {
        rotation
            .get_x(self.buffer.min.0, self.buffer.min.2)
            .min(rotation.get_x(self.buffer.max.0, self.buffer.max.2))
    }

    /// Java `public int getMinY()`.
    pub fn get_min_y(&self) -> i32 {
        self.buffer.min.1
    }

    /// Java `public int getMinZ(@Nonnull PrefabRotation rotation)`.
    pub fn get_min_z(&self, rotation: PrefabRotation) -> i32 {
        rotation
            .get_z(self.buffer.min.0, self.buffer.min.2)
            .min(rotation.get_z(self.buffer.max.0, self.buffer.max.2))
    }

    /// Java `public int getMaxX(@Nonnull PrefabRotation rotation)`.
    pub fn get_max_x(&self, rotation: PrefabRotation) -> i32 {
        rotation
            .get_x(self.buffer.min.0, self.buffer.min.2)
            .max(rotation.get_x(self.buffer.max.0, self.buffer.max.2))
    }

    /// Java `public int getMaxY()`.
    pub fn get_max_y(&self) -> i32 {
        self.buffer.max.1
    }

    /// Java `public int getMaxZ(@Nonnull PrefabRotation rotation)`.
    pub fn get_max_z(&self, rotation: PrefabRotation) -> i32 {
        rotation
            .get_z(self.buffer.min.0, self.buffer.min.2)
            .max(rotation.get_z(self.buffer.max.0, self.buffer.max.2))
    }

    /// Java `public int getColumnCount()`.
    pub fn get_column_count(&self) -> i32 {
        self.buffer.columns.size()
    }

    /// Java `@Nonnull public ChildPrefab[] getChildPrefabs()`.
    pub fn get_child_prefabs(&self) -> &[ChildPrefab] {
        &self.buffer.child_prefabs
    }

    /// Java `default int getMaximumExtend()`.
    pub fn get_maximum_extend(&self) -> i32 {
        let mut max = 0;
        for rotation in PrefabRotation::VALUES {
            let x = self.get_max_x(rotation) - self.get_min_x(rotation);
            if x > max {
                max = x;
            }
            let z = self.get_max_z(rotation) - self.get_min_z(rotation);
            if z > max {
                max = z;
            }
        }
        max
    }

    /// Java `public int getMinYAt(@Nonnull PrefabRotation rotation, int x, int
    /// z)`.
    ///
    /// The Java code rotates the *queried* coordinates (the `// TODO I think
    /// we need to inverse the rotation here..` quirk) — the Rust port
    /// reproduces it.
    pub fn get_min_y_at(&self, rotation: PrefabRotation, x: i32, z: i32) -> i32 {
        let rotated_x = rotation.get_x(x, z);
        let rotated_z = rotation.get_z(x, z);
        let column = self
            .buffer
            .columns
            .get(math_util::pack_int(rotated_x, rotated_z));
        match column {
            Some(column) if column.get_block_count() > 0 => column.entries[0].y,
            _ => -1,
        }
    }

    /// Java `public int getMaxYAt(@Nonnull PrefabRotation rotation, int x, int
    /// z)`.
    pub fn get_max_y_at(&self, rotation: PrefabRotation, x: i32, z: i32) -> i32 {
        let rotated_x = rotation.get_x(x, z);
        let rotated_z = rotation.get_z(x, z);
        let column = self
            .buffer
            .columns
            .get(math_util::pack_int(rotated_x, rotated_z));
        match column {
            Some(column) if column.get_block_count() > 0 => {
                column.entries[column.entries.len() - 1].y
            }
            _ => -1,
        }
    }

    /// Java `public int getBlockId(int x, int y, int z)`.
    pub fn get_block_id(&self, x: i32, y: i32, z: i32) -> i32 {
        match self.find_entry(x, y, z) {
            Some(entry) => entry.block_id,
            None => 0,
        }
    }

    /// Java `public int getFiller(int x, int y, int z)`.
    pub fn get_filler(&self, x: i32, y: i32, z: i32) -> i32 {
        match self.find_entry(x, y, z) {
            Some(entry) => entry.filler,
            None => worldgen_model::block_key::FillerBlockUtil::NO_FILLER,
        }
    }

    /// Java `public int getRotationIndex(int x, int y, int z)`.
    pub fn get_rotation_index(&self, x: i32, y: i32, z: i32) -> i32 {
        match self.find_entry(x, y, z) {
            Some(entry) => entry.rotation,
            None => RotationTuple::NONE_INDEX,
        }
    }

    /// The entry of the column `(x, z)` at the height `y`.
    ///
    /// Java walks the packed column and returns `0` / `NO_FILLER` /
    /// `NONE_INDEX` when the height is not occupied. A block with a chance
    /// cannot be addressed (the Java code throws
    /// `UnsupportedOperationException("Unable to access block with
    /// chance!")`).
    fn find_entry(&self, x: i32, y: i32, z: i32) -> Option<&PrefabBufferBlockEntry> {
        let column = self.buffer.columns.get(math_util::pack_int(x, z))?;
        if column.get_block_count() == 0 {
            return None;
        }
        // The column is ordered by ascending `y`, so the lookup is a binary
        // search (the Java code walks the packed column linearly).
        let index = column
            .entries
            .binary_search_by(|entry| entry.y.cmp(&y))
            .ok()?;
        let entry = &column.entries[index];
        if entry.chance < DEFAULT_CHANCE {
            panic!("Unable to access block with chance!");
        }
        Some(entry)
    }

    /// Java `public <T extends PrefabBufferCall> void forEach(@Nonnull
    /// ColumnPredicate<T> columnPredicate, @Nonnull BlockConsumer<T>
    /// blockConsumer, @Nullable EntityConsumer<T> entityConsumer, @Nullable
    /// ChildConsumer<T> childConsumer, @Nonnull T t)`.
    ///
    /// Deviation: the entity consumer is not ported (`REMAINING.md` § 2) —
    /// the prefab entities are stubbed.
    ///
    /// The call context is taken by reference (the Java traversal is
    /// re-entrant: the child consumer calls back into the traversal).
    #[allow(clippy::type_complexity)]
    pub fn for_each<T: PrefabBufferCall>(
        &self,
        column_predicate: &mut dyn FnMut(i32, i32, usize, &T) -> bool,
        block_consumer: &mut dyn FnMut(PrefabBlockVisit<'_>, &T),
        child_consumer: Option<&mut dyn FnMut(PrefabChildVisit<'_>, &T)>,
        t: &T,
    ) {
        let rotation = t.call_rotation();

        // The Java traversal walks the fastutil map in its iteration order.
        let mut columns: Vec<(i32, &PrefabBufferColumn)> = Vec::new();
        for (key, column) in self.buffer.columns.iter() {
            columns.push((key, column));
        }

        for (column_index, column) in columns {
            let cx = math_util::unpack_left_int(column_index);
            let cz = math_util::unpack_right_int(column_index);

            let x = rotation.get_x(cx, cz);
            let z = rotation.get_z(cx, cz);

            let block_count = column.get_block_count();
            if !column_predicate(x, z, block_count, t) {
                continue;
            }

            for entry in &column.entries {
                // The Java code skips the blocks whose chance test fails; the
                // random draw happens *before* the rotation / filler
                // rotation.
                if entry.chance < DEFAULT_CHANCE
                    && entry.chance < t.call_random().borrow_mut().next_float()
                {
                    continue;
                }
                let block_rotation = rotation.get_rotation(entry.rotation);
                let filler = rotation.get_filler(entry.filler);
                block_consumer(
                    PrefabBlockVisit {
                        x,
                        y: entry.y,
                        z,
                        block_id: entry.block_id,
                        support_value: entry.support_value as i32,
                        rotation: block_rotation,
                        filler,
                        fluid_id: entry.fluid_id,
                        fluid_level: entry.fluid_level,
                        entry,
                    },
                    t,
                );
            }
        }

        if let Some(child_consumer) = child_consumer {
            for child in &self.buffer.child_prefabs {
                let x = rotation.get_x(child.x, child.z);
                let z = rotation.get_z(child.x, child.z);
                child_consumer(
                    PrefabChildVisit {
                        x,
                        y: child.y,
                        z,
                        path: &child.path,
                        fit_heightmap: child.fit_heightmap,
                        inherit_seed: child.inherit_seed,
                        inherit_height_condition: child.inherit_height_condition,
                        weights: &child.weights,
                        rotation: child.rotation,
                    },
                    t,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefab::buffer::prefab_buffer_call::PrefabBufferCallState;
    use hytale_math::fast_random::FastRandom;

    /// A buffer with the given columns (`(x, z, [(y, block)])`).
    #[allow(clippy::type_complexity)]
    fn buffer(columns: &[(i32, i32, &[(i32, i32)])]) -> PrefabBuffer {
        let mut builder = PrefabBuffer::new_builder();
        builder.set_anchor((1, 2, 3));
        for (x, z, entries) in columns {
            let entries = entries
                .iter()
                .map(|(y, block)| PrefabBufferBlockEntry::new(*y, *block, "Rock_Stone"))
                .collect();
            builder.add_column(*x, *z, entries);
        }
        builder.build()
    }

    #[test]
    fn the_anchor_and_the_extents_are_kept() {
        let prefab = buffer(&[
            (1, 2, &[(0, 5), (3, 6)]),
            (3, -2, &[(-10, 7)]),
            (8, 10, &[(20, 8)]),
        ]);
        let access = prefab.new_access();
        assert_eq!(access.get_anchor_x(), 1);
        assert_eq!(access.get_anchor_y(), 2);
        assert_eq!(access.get_anchor_z(), 3);
        // The Java `PrefabBufferTest.minMax` expectations.
        assert_eq!(access.get_min_x(PrefabRotation::Rotation0), 1);
        assert_eq!(access.get_max_x(PrefabRotation::Rotation0), 8);
        assert_eq!(access.get_min_z(PrefabRotation::Rotation0), -2);
        assert_eq!(access.get_max_z(PrefabRotation::Rotation0), 10);
        assert_eq!(access.get_min_y(), -10);
        assert_eq!(access.get_max_y(), 20);
        assert_eq!(access.get_column_count(), 3);
    }

    #[test]
    fn the_per_column_heights_are_the_first_and_the_last_entry() {
        let prefab = buffer(&[
            (1, 2, &[(0, 5), (3, 6)]),
            (3, -2, &[(-10, 7)]),
            (8, 10, &[(20, 8)]),
        ]);
        let access = prefab.new_access();
        // The Java `PrefabBufferTest.minY` expectations.
        assert_eq!(access.get_min_y_at(PrefabRotation::Rotation0, 1, 2), 0);
        assert_eq!(access.get_max_y_at(PrefabRotation::Rotation0, 1, 2), 3);
        assert_eq!(access.get_min_y_at(PrefabRotation::Rotation0, 3, -2), -10);
        assert_eq!(access.get_max_y_at(PrefabRotation::Rotation0, 3, -2), -10);
        assert_eq!(access.get_min_y_at(PrefabRotation::Rotation0, 8, 10), 20);
        assert_eq!(access.get_max_y_at(PrefabRotation::Rotation0, 8, 10), 20);
        // A column without blocks reports -1.
        assert_eq!(access.get_min_y_at(PrefabRotation::Rotation0, 0, 0), -1);
        assert_eq!(access.get_max_y_at(PrefabRotation::Rotation0, 0, 0), -1);
    }

    #[test]
    fn the_blocks_are_addressed_by_their_coordinates() {
        let prefab = buffer(&[(1, 2, &[(0, 5), (3, 6)])]);
        let access = prefab.new_access();
        assert_eq!(access.get_block_id(1, 0, 2), 5);
        assert_eq!(access.get_block_id(1, 3, 2), 6);
        assert_eq!(access.get_block_id(1, 1, 2), 0);
        assert_eq!(access.get_block_id(0, 0, 0), 0);
        assert_eq!(access.get_filler(1, 0, 2), 0);
        assert_eq!(access.get_rotation_index(1, 0, 2), 0);
    }

    #[test]
    fn a_duplicate_column_is_rejected() {
        let result = std::panic::catch_unwind(|| {
            let mut builder = PrefabBuffer::new_builder();
            builder.add_column(1, 2, vec![PrefabBufferBlockEntry::new(0, 1, "Rock_Stone")]);
            builder.add_column(1, 2, vec![PrefabBufferBlockEntry::new(0, 1, "Rock_Stone")]);
        });
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = if let Some(message) = payload.downcast_ref::<String>() {
            message.clone()
        } else if let Some(message) = payload.downcast_ref::<&'static str>() {
            (*message).to_string()
        } else {
            "<non-string panic>".to_string()
        };
        assert_eq!(message, "Duplicate column");
    }

    #[test]
    fn the_y_values_must_be_sequential() {
        let result = std::panic::catch_unwind(|| {
            let mut builder = PrefabBuffer::new_builder();
            builder.add_column(
                1,
                2,
                vec![
                    PrefabBufferBlockEntry::new(5, 1, "Rock_Stone"),
                    PrefabBufferBlockEntry::new(3, 1, "Rock_Stone"),
                ],
            );
        });
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = if let Some(message) = payload.downcast_ref::<String>() {
            message.clone()
        } else if let Some(message) = payload.downcast_ref::<&'static str>() {
            (*message).to_string()
        } else {
            "<non-string panic>".to_string()
        };
        assert_eq!(message, "Y Values are not sequential. 5 -> 3");
    }

    #[test]
    fn an_empty_prefab_has_zero_bounds() {
        let prefab = PrefabBuffer::new_builder().build();
        let access = prefab.new_access();
        assert_eq!(access.get_min_x(PrefabRotation::Rotation0), 0);
        assert_eq!(access.get_max_x(PrefabRotation::Rotation0), 0);
        assert_eq!(access.get_min_y(), 0);
        assert_eq!(access.get_max_y(), 0);
        assert_eq!(access.get_column_count(), 0);
    }

    #[test]
    fn the_traversal_reports_the_columns_and_the_blocks() {
        let prefab = buffer(&[(1, 2, &[(0, 5), (3, 6)]), (3, -2, &[(-10, 7)])]);
        let access = prefab.new_access();
        let call = PrefabBufferCallState::new(FastRandom::new(0), PrefabRotation::Rotation0);

        let mut columns = Vec::new();
        let mut blocks = Vec::new();
        access.for_each(
            &mut |x, z, _count, _t| {
                columns.push((x, z));
                true
            },
            &mut |visit, _t| blocks.push((visit.x, visit.y, visit.z, visit.block_id)),
            None,
            &call,
        );

        // The columns are visited in the fastutil iteration order.
        assert_eq!(columns.len(), 2);
        assert!(columns.contains(&(1, 2)));
        assert!(columns.contains(&(3, -2)));
        // The blocks of a column are reported in ascending y.
        let first: Vec<_> = blocks
            .iter()
            .filter(|(x, _, z, _)| *x == 1 && *z == 2)
            .collect();
        assert_eq!(first.len(), 2);
        assert_eq!(first[0].1, 0);
        assert_eq!(first[1].1, 3);
    }

    #[test]
    fn the_rotation_rotates_the_reported_coordinates() {
        let prefab = buffer(&[(1, 2, &[(0, 5)])]);
        let access = prefab.new_access();
        let call = PrefabBufferCallState::new(FastRandom::new(0), PrefabRotation::Rotation90);
        let mut blocks = Vec::new();
        access.for_each(
            &mut |_x, _z, _count, _t| true,
            &mut |visit, _t| blocks.push((visit.x, visit.y, visit.z)),
            None,
            &call,
        );
        // The 90° rotation maps (1, 2) to (2, -1).
        assert_eq!(blocks, vec![(2, 0, -1)]);
    }
}
