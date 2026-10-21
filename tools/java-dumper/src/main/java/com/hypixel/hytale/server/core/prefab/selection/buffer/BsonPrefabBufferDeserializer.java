package com.hypixel.hytale.server.core.prefab.selection.buffer;

// ---------------------------------------------------------------------------
// GOLDEN-DUMPER PATCH of the CoreServer BsonPrefabBufferDeserializer
// (the .hytale-source tree is read-only reference material; this copy is
// compiled into tools/java-dumper, which precedes the module jars on the
// dumper's classpath, so the JVM loads this version).
//
// THE DEVIATIONS: the entity holders are NOT decoded, and neither are the
// block "components" states. The Rust port
// stubs prefab entities (REMAINING.md §2 of the rewrite — only blocks,
// fluids and environments are pasted), so the golden parity target excludes
// them; the vanilla entity decode would additionally fail on the pack's
// entity component codecs the dumper does not boot (the Camera plugin's
// CameraEffect "CameraShake" lookup and friends). Skipping the decode
// mirrors the Rust semantics exactly: the buffer keeps every block / fluid /
// environment and an empty entity column map.
// ---------------------------------------------------------------------------

import com.hypixel.hytale.assetstore.map.BlockTypeAssetMap;
import com.hypixel.hytale.component.Holder;
import com.hypixel.hytale.math.util.MathUtil;
import com.hypixel.hytale.server.core.asset.type.blocktype.config.BlockMigration;
import com.hypixel.hytale.server.core.asset.type.blocktype.config.BlockType;
import com.hypixel.hytale.server.core.asset.type.blocktype.config.RotationTuple;
import com.hypixel.hytale.server.core.asset.type.fluid.Fluid;
import com.hypixel.hytale.server.core.modules.entity.component.TransformComponent;
import com.hypixel.hytale.server.core.prefab.config.SelectionPrefabSerializer;
import com.hypixel.hytale.server.core.prefab.selection.buffer.impl.PrefabBuffer;
import com.hypixel.hytale.server.core.prefab.selection.buffer.impl.PrefabBufferBlockEntry;
import com.hypixel.hytale.server.core.universe.world.storage.ChunkStore;
import com.hypixel.hytale.server.core.universe.world.storage.EntityStore;
import com.hypixel.hytale.server.core.util.FillerBlockUtil;
import it.unimi.dsi.fastutil.ints.Int2ObjectMap;
import it.unimi.dsi.fastutil.ints.Int2ObjectOpenHashMap;
import it.unimi.dsi.fastutil.objects.ObjectArrayList;
import org.bson.BsonDocument;
import org.bson.BsonInt32;
import org.bson.BsonValue;
import org.joml.Vector3i;

import javax.annotation.Nonnull;
import javax.annotation.Nullable;
import java.nio.file.Path;
import java.util.Arrays;
import java.util.Comparator;
import java.util.List;
import java.util.Map;
import java.util.function.Function;

public class BsonPrefabBufferDeserializer {
    public static final BsonPrefabBufferDeserializer INSTANCE = new BsonPrefabBufferDeserializer();

    // Assume earlier migrations don't need to be run
    public static final BsonInt32 LEGACY_BLOCK_ID_VERSION = new BsonInt32(8);
    private static final BsonInt32 DEFAULT_SUPPORT_VALUE = new BsonInt32(0);
    private static final BsonInt32 DEFAULT_FILLER_VALUE = new BsonInt32(FillerBlockUtil.NO_FILLER);
    private static final BsonInt32 DEFAULT_ROTATION_VALUE = new BsonInt32(RotationTuple.NONE_INDEX);

    @Nonnull
    public PrefabBuffer deserialize(Path path, @Nonnull BsonDocument document) {
        final var versionValue = document.get("version");
        final int version = versionValue != null ? versionValue.asInt32().getValue() : -1;
        if (version < 8) throw new IllegalArgumentException("Prefab version is too old: " + version);
        if (version > SelectionPrefabSerializer.VERSION) {
            throw new IllegalArgumentException("Prefab version is too new: " + version + " by expected " + SelectionPrefabSerializer.VERSION);
        }

        final Vector3i anchor = new Vector3i();
        anchor.x = document.getInt32("anchorX").getValue();
        anchor.y = document.getInt32("anchorY").getValue();
        anchor.z = document.getInt32("anchorZ").getValue();

        int blockIdVersion = document.getInt32("blockIdVersion", LEGACY_BLOCK_ID_VERSION).getValue();

        Function<String, String> blockMigration = null;
        Map<Integer, BlockMigration> blockMigrationMap = BlockMigration.getAssetMap().getAssetMap();
        int v = blockIdVersion;
        BlockMigration migration = blockMigrationMap.get(v);
        while (migration != null) {
            if (blockMigration == null) {
                blockMigration = migration::getMigration;
            } else {
                blockMigration = blockMigration.andThen(migration::getMigration);
            }
            migration = blockMigrationMap.get(++v);
        }

        // columnIndex -> (y -> blockIds, chances, states)
        final var columnMap = new Int2ObjectOpenHashMap<Int2ObjectMap<PrefabBufferBlockEntry>>();

        final var builder = PrefabBuffer.newBuilder();
        builder.setAnchor(anchor);

        final var blocksValue = document.get("blocks");
        if (blocksValue != null) {
            final var assetMap = BlockType.getAssetMap();
            final var blocksArray = blocksValue.asArray();
            for (var blockValue : blocksArray) {
                final var blockDocument = blockValue.asDocument();

                // Note: Since compact prefabs aren't to be modified we bake in the anchor offset
                final int realX = blockDocument.getInt32("x").getValue();
                final int realY = blockDocument.getInt32("y").getValue();
                final int realZ = blockDocument.getInt32("z").getValue();
                final int x = realX - anchor.x;
                final int y = realY - anchor.y;
                final int z = realZ - anchor.z;

                // Validate x, y, z will fit inside the compact buffer
                if (Short.MIN_VALUE > x || x > Short.MAX_VALUE) {
                    throw new IllegalArgumentException("Violation X: Short.MIN_VALUE < " + x + " < Short.MAX_VALUE");
                }
                if (Short.MIN_VALUE > y || y > Short.MAX_VALUE) {
                    throw new IllegalArgumentException("Violation Y: Short.MIN_VALUE < " + y + " < Short.MAX_VALUE");
                }
                if (Short.MIN_VALUE > z || z > Short.MAX_VALUE) {
                    throw new IllegalArgumentException("Violation Z: Short.MIN_VALUE < " + z + " < Short.MAX_VALUE");
                }

                final var blockEntry = builder.newBlockEntry(y);
                try {
                    deserializeBlockType(blockEntry, blockDocument, assetMap, blockMigration);
                } catch (Throwable t) {
                    throw new IllegalStateException("Failed to load block type for " + path + " at " + realX + ", " + realY + ", " + realZ, t);
                }
                deserializeState(blockEntry, blockDocument);
                // TODO collectPrefabMarker(x, y, z, state, childMarkers);

                blockEntry.supportValue = (byte) blockDocument.getInt32("support", DEFAULT_SUPPORT_VALUE).getValue();
                blockEntry.filler = blockDocument.getInt32("filler", DEFAULT_FILLER_VALUE).getValue();
                blockEntry.rotation = blockDocument.getInt32("rotation", DEFAULT_ROTATION_VALUE).getValue();

                final int columnIndex = MathUtil.packInt(x, z);
                var column = columnMap.get(columnIndex);
                if (column == null) columnMap.put(columnIndex, column = new Int2ObjectOpenHashMap<>());
                PrefabBufferBlockEntry existing = column.putIfAbsent(y, blockEntry);
                if (existing != null) {
                    throw new IllegalStateException("Block is already present in column. Given: " + realX + ", " + realY + ", " + realZ + ", " + blockEntry.blockTypeKey + " - Existing: " + existing.y + ", " + existing.blockTypeKey);
                }
            }
        }

        final var fluidsValue = document.get("fluids");
        if (fluidsValue != null) {
            final var assetMap = Fluid.getAssetMap();
            final var fluidsArray = fluidsValue.asArray();
            for (var fluidValue : fluidsArray) {
                final var fluidDocument = fluidValue.asDocument();

                // Note: Since compact prefabs aren't to be modified we bake in the anchor offset
                final int realX = fluidDocument.getInt32("x").getValue();
                final int realY = fluidDocument.getInt32("y").getValue();
                final int realZ = fluidDocument.getInt32("z").getValue();
                final int x = realX - anchor.x;
                final int y = realY - anchor.y;
                final int z = realZ - anchor.z;

                // Validate x, y, z will fit inside the compact buffer
                if (Short.MIN_VALUE > x || x > Short.MAX_VALUE) {
                    throw new IllegalArgumentException("Violation X: Short.MIN_VALUE < " + x + " < Short.MAX_VALUE");
                }
                if (Short.MIN_VALUE > y || y > Short.MAX_VALUE) {
                    throw new IllegalArgumentException("Violation Y: Short.MIN_VALUE < " + y + " < Short.MAX_VALUE");
                }
                if (Short.MIN_VALUE > z || z > Short.MAX_VALUE) {
                    throw new IllegalArgumentException("Violation Z: Short.MIN_VALUE < " + z + " < Short.MAX_VALUE");
                }

                final int columnIndex = MathUtil.packInt(x, z);
                var column = columnMap.get(columnIndex);
                if (column == null) columnMap.put(columnIndex, column = new Int2ObjectOpenHashMap<>());
                PrefabBufferBlockEntry entry = column.computeIfAbsent(y, builder::newBlockEntry);
                var fluidName = fluidDocument.getString("name").getValue();
                entry.fluidId = Fluid.getFluidIdOrUnknown(fluidName, "Unknown fluid '%s'", fluidName);
                entry.fluidLevel = (byte) fluidDocument.getInt32("level").getValue();
            }
        }

        final var entityMap = deserializeEntityHolders(document, anchor);

        columnMap.int2ObjectEntrySet().fastForEach(entry -> {
            final var columnIndex = entry.getIntKey();
            final int x = MathUtil.unpackLeft(columnIndex);
            final int z = MathUtil.unpackRight(columnIndex);
            final var columnBlockMap = entry.getValue();

            final var entries = columnBlockMap.values().toArray(PrefabBufferBlockEntry[]::new);
            Arrays.sort(entries, Comparator.comparingInt(o -> o.y));

            // remove so we can later iterate over left overs
            final var entityColumn = entityMap.remove(columnIndex);
            final var entityArray = entityColumn != null && !entityColumn.isEmpty() ? entityColumn.toArray(Holder[]::new) : null;

            builder.addColumn(x, z, entries, entityArray);
        });

        // most entities have already been added, but this one contains empty block columns
        entityMap.int2ObjectEntrySet().fastForEach(entry -> {
            final var columnIndex = entry.getIntKey();
            final int x = MathUtil.unpackLeft(columnIndex);
            final int z = MathUtil.unpackRight(columnIndex);
            final var entityColumn = entry.getValue();
            final var entityArray = !entityColumn.isEmpty() ? entityColumn.toArray(Holder[]::new) : null;

            if (entityArray != null) builder.addColumn(x, z, PrefabBufferBlockEntry.EMPTY_ARRAY, entityArray);
        });

        return builder.build();
    }

    private static void deserializeBlockType(@Nonnull PrefabBufferBlockEntry blockEntry, @Nonnull BsonDocument blockDocument,
                                             @Nonnull BlockTypeAssetMap<String, BlockType> assetMap,
                                             @Nullable Function<String, String> blockMigration) {
        final var blockType = blockDocument.getString("name").getValue();
        final int idx = blockType.indexOf('%');

        String blockTypeStr;
        if (idx != -1) {
            blockTypeStr = blockType.substring(idx + 1);
        } else {
            blockTypeStr = blockType;
        }

        blockEntry.blockTypeKey = blockTypeStr;
        if (blockMigration != null) {
            blockEntry.blockTypeKey = blockMigration.apply(blockEntry.blockTypeKey);
        }
        blockEntry.blockId = BlockType.getBlockIdOrUnknown(assetMap, blockEntry.blockTypeKey, "Failed to find block. Given %s", blockTypeStr);

        if (idx != -1) {
            final var chanceString = blockType.substring(0, idx);
            final float chancePercent = Float.parseFloat(chanceString);
            if (chancePercent < 0.0f) {
                throw new IllegalArgumentException("Chance is smaller than 0%. Given: " + chancePercent);
            }
            if (chancePercent > 100.0f) {
                throw new IllegalArgumentException("Chance is larger than 100%. Given: " + chancePercent);
            }
            blockEntry.chance = chancePercent / 100.0f;
        }
    }

    /**
     * GOLDEN-DUMPER PATCH: the vanilla implementation decodes the block's
     * "components" state (the ECS holder — including the PrefabSpawnerBlock
     * component that drives the child-prefab mechanism). The Rust port skips
     * the block components (no ECS — the locked CoreServer-boundary decision,
     * `REMAINING.md` § 2), so the dumper mirrors that: no state decode.
     */
    private static void deserializeState(@Nonnull PrefabBufferBlockEntry blockEntry, @Nonnull BsonDocument blockDocument) {
    }

    @Nonnull
    /**
     * GOLDEN-DUMPER PATCH: the vanilla implementation decodes the "entities"
     * array into {@link Holder<EntityStore>} holders (which needs the booted
     * entity modules and the entity component codecs of every builtin
     * plugin). The dumper keeps the deferred-entity semantics of the Rust
     * port: an EMPTY entity map, no decode.
     */
    private static Int2ObjectOpenHashMap<List<Holder<EntityStore>>> deserializeEntityHolders(@Nonnull BsonDocument document, @Nonnull Vector3i anchor) {
        return new Int2ObjectOpenHashMap<>();
    }
}
