package com.hypixel.hytale.tools.golden;

import com.hypixel.hytale.assetstore.AssetPack;
import com.hypixel.hytale.builtin.worldgen.modifier.EventHandler;
import com.hypixel.hytale.common.plugin.PluginManifest;
import com.hypixel.hytale.common.semver.Semver;
import com.hypixel.hytale.math.util.ChunkUtil;
import com.hypixel.hytale.procedurallib.file.FileIO;
import com.hypixel.hytale.procedurallib.json.SeedString;
import com.hypixel.hytale.server.core.asset.AssetModule;
import com.hypixel.hytale.server.core.asset.type.blocktype.config.BlockType;
import com.hypixel.hytale.server.core.asset.type.environment.config.Environment;
import com.hypixel.hytale.server.core.asset.type.fluid.Fluid;
import com.hypixel.hytale.server.core.blocktype.component.BlockPhysics;
import com.hypixel.hytale.server.core.universe.world.chunk.section.BlockComponentSection;
import com.hypixel.hytale.server.core.universe.world.chunk.section.FluidSection;
import com.hypixel.hytale.server.core.universe.world.storage.ChunkStore;
import com.hypixel.hytale.server.core.universe.world.worldgen.GeneratedBlockChunk;
import com.hypixel.hytale.server.core.universe.world.worldgen.GeneratedChunk;
import com.hypixel.hytale.server.core.universe.world.worldgen.GeneratedChunkSection;
import com.hypixel.hytale.server.worldgen.HytaleWorldGenProvider;
import com.hypixel.hytale.server.worldgen.SeedStringResource;
import com.hypixel.hytale.server.worldgen.WorldGenConfig;
import com.hypixel.hytale.server.worldgen.biome.Biome;
import com.hypixel.hytale.server.worldgen.biome.TileBiome;
import com.hypixel.hytale.server.worldgen.chunk.BlockPriorityChunk;
import com.hypixel.hytale.server.worldgen.chunk.ChunkGenerator;
import com.hypixel.hytale.server.worldgen.chunk.ChunkGeneratorExecution;
import com.hypixel.hytale.server.worldgen.loader.AssetFileSystem;
import com.hypixel.hytale.server.worldgen.loader.ChunkGeneratorJsonLoader;
import com.hypixel.hytale.server.worldgen.prefab.PrefabStoreRoot;
import com.hypixel.hytale.server.worldgen.zone.Zone;
import it.unimi.dsi.fastutil.ints.Int2ObjectOpenHashMap;
import it.unimi.dsi.fastutil.ints.IntOpenHashSet;

import java.io.IOException;
import java.io.PrintStream;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CopyOnWriteArrayList;
import java.util.function.IntFunction;

/**
 * Dumps whole generated chunks of the <b>real</b> Default world (the world
 * folder <b>inside the mounted asset pack</b>, {@code Server/World/Default} —
 * byte-identical to the extracted {@code testdata/Default} copy) through the
 * <b>real</b> generation path:
 *
 * <pre>
 *   AssetModule (base asset pack = the mounted Assets.zip)     ← vanilla server boot
 *   WorldGenConfig(packRoot/Server/World/Default, "Default")   ← HytaleWorldGenProvider semantics
 *   SeedString("ChunkGenerator", SeedStringResource(WORLD_GEN, config))
 *   new ChunkGeneratorJsonLoader(seed, config).load()          ← the FULL load incl. prefab store + EventHandler
 *   new ChunkGeneratorExecution(seed, generator, chunk).execute(seed)   ← the same call the server's
 *                                             ChunkGenerator.generate(...) runs on its worker threads
 * </pre>
 *
 * <p>Generation runs on the dumper's main thread (not through the
 * {@code CompletableFuture} executor of {@code ChunkGenerator.generate}) —
 * the execution itself is single-threaded and every random stream is seeded
 * from (seed, coordinates), so the results are identical; the executor only
 * changes the scheduling.
 *
 * <p>Per (seed, chunk) the dump records for every voxel of the 32×320×32
 * chunk:
 *
 * <ul>
 *     <li>the block id + rotation index + filler (as the Java
 *     {@code GeneratedChunkSection} stores them),</li>
 *     <li>the fluid id + fluid level ({@code FluidSection}),</li>
 *     <li>the environment id ({@code GeneratedEnvironment}),</li>
 *     <li>the {@code BlockPriorityChunk} priority byte (the full arbitration
 *     result, incl. the priority-modifier rewriting),</li>
 * </ul>
 *
 * <p>compactly encoded as per-column run-length lists over the tuple
 * {@code (blockId, rotation, filler, fluidId, fluidLevel, environmentId,
 * priority)}, deduplicated per chunk, plus the per-column tint, the
 * environment column runs and the zone/biome ids of the column (resolved
 * through the generator's own cached {@code getZoneBiomeResultAt}). Block,
 * fluid, environment, biome and zone ids are resolved to <b>names</b> in the
 * palettes so the Rust side can compare through its golden registries.
 *
 * <p><b>Not dumped</b> (documented scope):
 * <ul>
 *     <li>the {@code BlockPhysics} support values and the per-block
 *     {@code Holder<ChunkStore>} states written by {@code setBlock} — the
 *     Rust port documents BlockPhysics as a no-op;</li>
 *     <li>the entity chunk (entities are deferred, {@code REMAINING.md} §2)
 *     and the block components (the component system is deferred with
 *     them).</li>
 * </ul>
 *
 * <p>The dump is deterministic: it runs with the single-threaded fork-join
 * pool (see {@code run.sh}), the chunk list and the seed list are fixed and
 * no timestamps are written.
 */
final class ChunkDumper {

    /** The world seeds the chunks are generated for. Must stay in sync with the Rust golden test. */
    private static final int[] WORLD_SEEDS = {0, 1, 42};

    /**
     * The chunk coordinates the Default world is generated at.
     * Must stay in sync with the Rust golden test.
     * (0,0) is the smoke-test chunk; (4,-5) / (9,4) carry the seed-0 unique
     * prefabs (spawn room / temple portal — see the 5c port notes).
     */
    private static final int[][] CHUNKS = {
        {0, 0}, {4, -5}, {9, 4}, {7, 3}, {-12, -25}, {16, -8}, {30, 15},
    };

    /** The world folder inside the asset pack (the vanilla server layout). */
    private static final String PACK_WORLD_DIR = "Server/World/Default";

    /**
     * The unpacked asset tree. The world is booted from the <b>disk</b> tree
     * (not from the zip mount): the Rust port (and the 7d loader golden data)
     * walks {@code testdata/Default}, which is byte-identical to
     * {@code Assets/extracted/Server/World/Default}. Booting the disk tree
     * keeps the directory listings / path comparisons identical between the
     * two sides.
     */
    private static final String EXTRACTED_DIR = "Assets/extracted";

    /**
     * The loader seed label. The vanilla provider
     * ({@code HytaleWorldGenProvider.getGenerator}) always constructs
     * {@code new SeedString<>("ChunkGenerator", ...)}, so that is the default
     * (and the parity target). Set
     * {@code -Dhytale.dumper.loaderSeed=<label>} to dump another convention
     * (e.g. the per-seed {@code Integer.toString(seed)} used by the earlier
     * Phase-2/loader dumps).
     */
    private static final String LOADER_SEED =
        System.getProperty("hytale.dumper.loaderSeed", "ChunkGenerator");

    /** Set {@code -Dhytale.dumper.tint=true} to print the tint entries of one probe column. */
    private static final boolean DEBUG_TINT = Boolean.getBoolean("hytale.dumper.tint");

    /** Set {@code -Dhytale.dumper.caves=true} to print the cave nodes of the probe chunks. */
    private static final boolean DEBUG_CAVES = Boolean.getBoolean("hytale.dumper.caves");

    /** The real stderr (the CoreServer boot replaces {@code System.err}). */
    private static final PrintStream REAL_ERR = System.err;

    /** Whether the {@link AssetModule} singleton stub was installed. */
    private static boolean assetModuleStubbed;

    /** Whether the {@code LegacyModule} singleton stub was installed. */
    private static boolean legacyModuleStubbed;

    /** Whether the {@code BlockTypeModule} singleton stub was installed. */
    private static boolean blockTypeModuleStubbed;

    /** The biome id → name mapping observed during the chunk pass. */
    private static final Map<Integer, String> BIOME_NAMES = new Int2ObjectOpenHashMap<>();

    /** The zone id → name mapping observed during the chunk pass. */
    private static final Map<Integer, String> ZONE_NAMES = new Int2ObjectOpenHashMap<>();

    private ChunkDumper() {
    }

    static String dumpChunks(Path worldDir, Path repoRoot) throws IOException {
        // The registry boot is shared with (and idempotent after) the registry
        // dump; it still uses the zip mount for the asset stores.
        RegistryDumper.boot(repoRoot);

        // The world itself, however, is booted from the UNPACKED tree: the
        // Rust port walks the disk copy (testdata/Default), so the world-gen
        // AssetFileSystem must be rooted at the same kind of tree. The
        // registries stay the ones the zip boot produced (they are already
        // loaded into the static asset maps).
        final Path assetRoot = repoRoot.resolve(EXTRACTED_DIR).normalize();
        try {
            stubAssetModule(assetRoot, repoRoot);
            stubLegacyModule();
            stubBlockTypeModule();
        } catch (Exception e) {
            throw new IOException("Failed to install the AssetModule / LegacyModule singleton stubs", e);
        }

        // The world folder — the AssetFileSystem relativizes every path
        // against the pack root, so the WorldGenConfig path must live inside
        // it.
        final Path worldPath = assetRoot.resolve("Server").resolve("World").resolve("Default");
        if (!Files.isRegularFile(worldPath.resolve("World.json"))) {
            throw new IOException("the unpacked asset tree does not contain " + PACK_WORLD_DIR + "/World.json: "
                + worldPath);
        }

        final WorldGenConfig config = new WorldGenConfig(
            worldPath, HytaleWorldGenProvider.DEFAULT_NAME, HytaleWorldGenProvider.MIN_VERSION);

        // The world-gen modifier handler: acquired for the generator lifetime
        // exactly like the vanilla provider does (the pack ships no modifier
        // for the Default world, so the handler stays empty — which is also
        // what the loader golden dump assumes). It is seed-independent.
        final EventHandler eventHandler = EventHandler.acquire(config.path());

        final Phase2Dumper.JsonWriter w = new Phase2Dumper.JsonWriter();
        w.beginObject();

        // --- meta --------------------------------------------------------
        writeMeta(w, worldPath, assetRoot);

        // --- chunks ---------------------------------------------------------
        // The chunk data is buffered as compact JSON strings because the
        // palettes are only complete after every chunk was generated (the
        // writer is strictly sequential).
        //
        // The LOADER seed label is the world seed (Integer.toString(seed)) —
        // the established golden convention of Phase2Dumper / LoaderDumper
        // (the vanilla provider would use the constant "ChunkGenerator"
        // label; the golden chain is <seed>.Zones.Zone-<name>… instead, and
        // the loader chain is seed-dependent, so one generator is loaded PER
        // SEED).
        final Palettes palettes = new Palettes();
        final StringBuilder chunkBuffer = new StringBuilder(1 << 22);
        boolean firstChunk = true;
        for (int worldSeed : WORLD_SEEDS) {
            final SeedString<SeedStringResource> seed = new SeedString<>(
                LOADER_SEED, new SeedStringResource(PrefabStoreRoot.DEFAULT, config));

            final ChunkGenerator generator;
            try (final var ignored = FileIO.openFileIOSystem(new AssetFileSystem(config))) {
                generator = new ChunkGeneratorJsonLoader(seed, config).load();
            }
            // The generation pass loads prefab buffers lazily through the
            // AssetFileSystem, so it stays open for the lifetime of the
            // dump; and the vanilla worker threads initialize their
            // thread-local resource (it holds the generator back-reference
            // the prefab loading cache resolves through) — the main thread
            // needs the same.
            FileIO.openFileIOSystem(new AssetFileSystem(config));
            ChunkGenerator.getResource().init(generator);

            for (int[] chunkCoord : CHUNKS) {
                if (!firstChunk) {
                    chunkBuffer.append(',');
                }
                firstChunk = false;
                dumpChunk(chunkBuffer, generator, palettes, worldSeed, chunkCoord[0], chunkCoord[1]);
            }
            generator.shutdown();
        }

        writePalettes(w, palettes);
        w.name("chunks");
        w.rawArray(chunkBuffer.toString());

        w.endObject();

        // The event handler stays open for the process lifetime (the vanilla
        // server keeps it for the generator lifetime as well).
        return w.toString();
    }

    // ------------------------------------------------------------------
    // AssetModule singleton stub
    // ------------------------------------------------------------------

    /**
     * Installs the {@link AssetModule} singleton the world-gen
     * {@code AssetFileSystem} requires
     * ({@code AssetModule.get().getBaseAssetPack().getRoot()} — the world-gen
     * file system and the prefab store both resolve through it). The vanilla
     * server registers the base pack through the plugin loader, which the
     * dumper does not boot, so the singleton is replaced by an instance
     * holding exactly one pack: the <b>unpacked</b> asset tree (the same
     * kind of tree the Rust port walks).
     */
    private static void stubAssetModule(Path assetRoot, Path repoRoot) throws Exception {
        if (assetModuleStubbed) {
            return;
        }
        final Class<?> moduleClass = Class.forName("com.hypixel.hytale.server.core.asset.AssetModule");
        final Object module = allocateInstance(moduleClass);
        setStaticField(moduleClass, "instance", module);

        final AssetPack pack = new AssetPack(
            repoRoot.resolve("Assets/Assets.zip"),
            "Assets",
            assetRoot,
            /* fileSystem = */ null,
            /* isImmutable = */ true,
            new PluginManifest(),
            AssetPack.PackSource.CLI);
        final Field packsField = moduleClass.getDeclaredField("assetPacks");
        packsField.setAccessible(true);
        packsField.set(module, new CopyOnWriteArrayList<>(List.of(pack)));

        assetModuleStubbed = true;
    }

    /**
     * Installs the {@code LegacyModule} singleton the chunk model needs:
     * {@code BlockComponentSection.getComponentType()} and
     * {@code FluidSection.getComponentType()} resolve through
     * {@code LegacyModule.get()}, and the vanilla {@code setup()} registers
     * every component type on {@code ChunkStore.REGISTRY}. The dumper does
     * not boot the plugin loader, so the singleton is replaced by an
     * allocated instance and ONLY the component types the world-generation
     * chunk model touches are registered (the packet / saving systems of the
     * vanilla setup are not needed for generation).
     */
    private static void stubLegacyModule() throws Exception {
        if (legacyModuleStubbed) {
            return;
        }
        final Class<?> moduleClass = Class.forName("com.hypixel.hytale.server.core.modules.LegacyModule");
        final Object module = allocateInstance(moduleClass);
        setStaticField(moduleClass, "instance", module);

        final var registry = ChunkStore.REGISTRY;
        setField(module, "fluidSectionComponentType",
            registry.registerComponent(FluidSection.class, "Fluid", FluidSection.CODEC));
        setField(module, "blockComponentSectionComponentType",
            registry.registerComponent(BlockComponentSection.class, "BlockComponent", BlockComponentSection.CODEC));

        legacyModuleStubbed = true;
    }

    /**
     * Installs the {@code BlockTypeModule} singleton the
     * {@code BlockPhysics} support-value component resolves through
     * ({@code BlockPhysics.getComponentType()} →
     * {@code BlockTypeModule.get().getBlockPhysicsComponentType()}). The
     * dump does not persist the BlockPhysics data (the Rust port documents
     * BlockPhysics as a no-op), but the vanilla {@code setBlock} calls it on
     * every write, so the component type has to exist.
     */
    private static void stubBlockTypeModule() throws Exception {
        if (blockTypeModuleStubbed) {
            return;
        }
        final Class<?> moduleClass = Class.forName("com.hypixel.hytale.server.core.blocktype.BlockTypeModule");
        final Object module = allocateInstance(moduleClass);
        setStaticField(moduleClass, "instance", module);

        setField(module, "blockPhysicsComponentType",
            ChunkStore.REGISTRY.registerComponent(BlockPhysics.class, "BlockPhysics", BlockPhysics.CODEC));

        blockTypeModuleStubbed = true;
    }

    // ------------------------------------------------------------------
    // Reflection helpers (duplicated from RegistryDumper, which keeps them
    // private)
    // ------------------------------------------------------------------

    /** Creates an instance without running the constructor. */
    private static Object allocateInstance(Class<?> clazz) throws Exception {
        final Class<?> unsafeClass = Class.forName("sun.misc.Unsafe");
        final Field theUnsafe = unsafeClass.getDeclaredField("theUnsafe");
        theUnsafe.setAccessible(true);
        final Object unsafe = theUnsafe.get(null);
        final Method allocateInstance = unsafeClass.getMethod("allocateInstance", Class.class);
        return allocateInstance.invoke(unsafe, clazz);
    }

    private static void setStaticField(Class<?> clazz, String name, Object value) throws Exception {
        final Field field = clazz.getDeclaredField(name);
        field.setAccessible(true);
        field.set(null, value);
    }

    private static void setField(Object target, String name, Object value) throws Exception {
        setField(target.getClass(), target, name, value);
    }

    private static void setField(Class<?> clazz, Object target, String name, Object value) throws Exception {
        final Field field = clazz.getDeclaredField(name);
        field.setAccessible(true);
        field.set(target, value);
    }

    /** Cached `BiomePatternGenerator.getBiomeIndex(int,int,int)` (protected). */
    private static Method biomeIndexMethod;

    /** Cached `BiomePatternGenerator.pointGenerator` field. */
    private static Field pointGeneratorField;

    /** Cached `IPointGenerator.nearest2D(int,int,int)` method. */
    private static Method nearest2dMethod;

    /** Invokes the protected `BiomePatternGenerator.getBiomeIndex` (debug only). */
    private static double invokeBiomeIndex(Object generator, int seed, int x, int z) {
        try {
            if (biomeIndexMethod == null) {
                final Method method = generator.getClass().getDeclaredMethod(
                    "getBiomeIndex", int.class, int.class, int.class);
                method.setAccessible(true);
                biomeIndexMethod = method;
            }
            return (double) biomeIndexMethod.invoke(generator, seed, x, z);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException("failed to invoke BiomePatternGenerator.getBiomeIndex", e);
        }
    }

    /** Reads the `nearest2D` cell of the biome point generator (debug only). */
    private static double[] invokeNearestCell(Object generator, int seed, int x, int z) {
        try {
            if (pointGeneratorField == null) {
                final Field field = generator.getClass().getDeclaredField("pointGenerator");
                field.setAccessible(true);
                pointGeneratorField = field;
            }
            final Object pointGenerator = pointGeneratorField.get(generator);
            if (nearest2dMethod == null) {
                final Class<?> generatorInterface = Class.forName(
                    "com.hypixel.hytale.procedurallib.logic.point.IPointGenerator");
                final Method method = generatorInterface.getMethod(
                    "nearest2D", int.class, double.class, double.class);
                method.setAccessible(true);
                nearest2dMethod = method;
            }
            final Object buffer = nearest2dMethod.invoke(pointGenerator, seed, (double) x, (double) z);
            final Class<?> bufferClass = Class.forName(
                "com.hypixel.hytale.procedurallib.logic.ResultBuffer$ResultBuffer2d");
            final Field ix = bufferClass.getField("ix");
            final Field iy = bufferClass.getField("iy");
            final Field distance = bufferClass.getField("distance");
            final Field px = bufferClass.getField("x");
            final Field py = bufferClass.getField("y");
            return new double[]{
                ix.getInt(buffer), iy.getInt(buffer),
                distance.getDouble(buffer), px.getDouble(buffer), py.getDouble(buffer)
            };
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException("failed to read the biome point-generator cell", e);
        }
    }

    /** Debug: recursively prints a noise-property tree (seed offsets / types). */
    private static void dumpNoise(Object property, String indent) {
        if (property == null) {
            return;
        }
        try {
            final Class<?> clazz = property.getClass();
            final String name = clazz.getSimpleName();
            if (name.equals("FractalNoiseProperty") || name.equals("SingleNoiseProperty")) {
                final Field seedField = clazz.getDeclaredField("seedOffset");
                seedField.setAccessible(true);
                REAL_ERR.println(indent + name + " seedOffset=" + seedField.get(property));
            } else {
                REAL_ERR.println(indent + name);
            }
            for (String fieldName : new String[]{"noiseProperty", "property", "noise"}) {
                try {
                    final Field field = clazz.getDeclaredField(fieldName);
                    field.setAccessible(true);
                    dumpNoise(field.get(property), indent + "  ");
                } catch (NoSuchFieldException ignored) {
                    // not this kind
                }
            }
            try {
                final Field arrayField = clazz.getDeclaredField("noiseProperties");
                arrayField.setAccessible(true);
                final Object[] array = (Object[]) arrayField.get(property);
                if (array != null) {
                    for (Object element : array) {
                        dumpNoise(element, indent + "  ");
                    }
                }
            } catch (NoSuchFieldException ignored) {
                // not a multi-noise property
            }
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException("noise debug failed", e);
        }
    }

    /** Debug: prints every cave/node that touches the chunk (see {@link #DEBUG_CAVES}). */
    private static void debugCaves(ChunkGenerator generator, int seed, int chunkX, int chunkZ) {
        final long index = ChunkUtil.indexChunk(chunkX, chunkZ);
        final java.util.Set<Object> seen =
            java.util.Collections.newSetFromMap(new java.util.IdentityHashMap<>());
        REAL_ERR.println("CAVE DEBUG seed=" + seed + " chunk=" + chunkX + "," + chunkZ);
        for (Zone zone : generator.getZonePatternProvider().getZones()) {
            if (zone.caveGenerator() == null) continue;
            for (com.hypixel.hytale.server.worldgen.cave.CaveType caveType
                : zone.caveGenerator().getCaveTypes()) {
                REAL_ERR.println(" CAVE MASK type=" + caveType.getName()
                    + " stone=" + caveType.getBlockMask().eval(1527, 0, 0, 0)
                    + " volcanic=" + caveType.getBlockMask().eval(1596, 0, 0, 0)
                    + " air=" + caveType.getBlockMask().eval(0, 0, 0, 0)
                    + " dirt=" + caveType.getBlockMask().eval(1695, 0, 0, 0));
                final double maximumSize = caveType.getMaximumSize();
                final com.hypixel.hytale.procedurallib.logic.point.IPointGenerator points =
                    caveType.getEntryPointGenerator();
                points.collect(seed,
                    ChunkUtil.minBlock(chunkX) - maximumSize,
                    ChunkUtil.minBlock(chunkZ) - maximumSize,
                    ChunkUtil.maxBlock(chunkX) + maximumSize,
                    ChunkUtil.maxBlock(chunkZ) + maximumSize,
                    (dx, dz) -> {
                        final int x = com.hypixel.hytale.math.util.MathUtil.floor(dx);
                        final int z = com.hypixel.hytale.math.util.MathUtil.floor(dz);
                        final var result = generator.getZoneBiomeResultAt(seed, x, z);
                        if (result.getZoneResult().getZone() != zone
                            || !caveType.isEntryThreshold(seed, x, z)) {
                            return;
                        }
                        final int height = generator.getHeight(seed, x, z);
                        if (!caveType.getHeightCondition().eval(seed + 0xF5ACDEB5, x, height, z)) {
                            return;
                        }
                        final var cave = generator.getCave(caveType, seed, x, z);
                        if (cave == null || !cave.contains(index) || !seen.add(cave)) return;
                        REAL_ERR.println(" CAVE type=" + caveType.getName() + " entry=" + x + "," + z
                            + " nodes=" + cave.getNodeCount() + " bounds="
                            + cave.getBounds().getLowBoundX() + ".." + cave.getBounds().getHighBoundX()
                            + "," + cave.getBounds().getLowBoundY() + ".."
                            + cave.getBounds().getHighBoundY() + ","
                            + cave.getBounds().getLowBoundZ() + ".."
                            + cave.getBounds().getHighBoundZ());
                        for (var node : cave.getCaveNodes(index)) {
                            final var b = node.getBounds();
                            REAL_ERR.println("  NODE type=" + node.getCaveNodeType().getName()
                                + " seed=" + node.getSeedOffset()
                                + " shape=" + node.getShape().getClass().getSimpleName()
                                + " bounds=" + b.getLowBoundX() + ".." + b.getHighBoundX()
                                + "," + b.getLowBoundY() + ".." + b.getHighBoundY()
                                + "," + b.getLowBoundZ() + ".." + b.getHighBoundZ()
                                + " end=" + node.getEnd().x + "," + node.getEnd().y + ","
                                + node.getEnd().z);
                            for (var prefab : node.getCavePrefabs()) {
                                final var pb = prefab.getBounds();
                                REAL_ERR.println("   PREFAB name=" + prefab.getPrefab().getName()
                                    + " pos=" + prefab.getX() + "," + prefab.getY() + ","
                                    + prefab.getZ() + " bounds=" + pb.getLowBoundX() + ".."
                                    + pb.getHighBoundX() + "," + pb.getLowBoundY() + ".."
                                    + pb.getHighBoundY() + "," + pb.getLowBoundZ() + ".."
                                    + pb.getHighBoundZ());
                            }
                            final int[][] samples = {
                                {com.hypixel.hytale.math.util.MathUtil.floor(node.getEnd().x),
                                 com.hypixel.hytale.math.util.MathUtil.floor(node.getEnd().z)},
                                {(b.getLowBoundX() + b.getHighBoundX()) / 2,
                                 (b.getLowBoundZ() + b.getHighBoundZ()) / 2},
                                {224, 96},
                                {0, 30}
                            };
                            for (int[] p : samples) {
                                int replaceCount = 0;
                                for (int yy = b.getLowBoundY(); yy <= b.getHighBoundY(); yy++) {
                                    if (node.getShape().shouldReplace(seed, p[0], p[1], yy)) replaceCount++;
                                }
                                REAL_ERR.println("   GEOM x=" + p[0] + " z=" + p[1]
                                    + " floor=" + node.getFloorPosition(seed, p[0], p[1])
                                    + " ceiling=" + node.getCeilingPosition(seed, p[0], p[1])
                                    + " replace=" + replaceCount);
                            }
                        }
                    });
            }
        }
    }

    /** Debug: prints the tint entries of one column (see {@link #DEBUG_TINT}). */
    private static void debugTint(ChunkGenerator generator, int seed, int x, int z) {
        final Biome biome = generator.getZoneBiomeResultAt(seed, x, z).getBiome();
        REAL_ERR.println("TINT DEBUG biome " + biome.getId() + " " + biome.getName()
            + " at (" + x + "," + z + ") -> " + biome.getTintContainer().getTintColorAt(seed, x, z));
        try {
            final Field entriesField = biome.getTintContainer().getClass().getDeclaredField("entries");
            entriesField.setAccessible(true);
            final List<?> entries = (List<?>) entriesField.get(biome.getTintContainer());
            for (int i = 0; i < entries.size(); i++) {
                final Object entry = entries.get(i);
                final Field conditionField = entry.getClass().getDeclaredField("mapCondition");
                conditionField.setAccessible(true);
                final Object condition = conditionField.get(entry);
                final boolean eval = ((com.hypixel.hytale.procedurallib.condition.ICoordinateCondition) condition)
                    .eval(seed, x, z);
                double noise = Double.NaN;
                Object noiseProperty = null;
                try {
                    final Field noiseField = condition.getClass().getDeclaredField("noiseMask");
                    noiseField.setAccessible(true);
                    noiseProperty = noiseField.get(condition);
                    noise = ((com.hypixel.hytale.procedurallib.property.NoiseProperty) noiseProperty)
                        .get(seed, x, z);
                } catch (NoSuchFieldException ignored) {
                    // constant condition
                }
                REAL_ERR.println("  entry " + i + " eval=" + eval + " noise=" + noise);
                dumpNoise(noiseProperty, "    ");
            }
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException("tint debug failed", e);
        }
    }

    // ------------------------------------------------------------------
    // Meta
    // ------------------------------------------------------------------

    private static void writeMeta(Phase2Dumper.JsonWriter w, Path worldPath, Path assetRoot) {
        w.name("meta");
        w.beginObject();
        w.name("class").value("com.hypixel.hytale.server.worldgen.chunk.ChunkGeneratorExecution");
        w.name("format").value("chunk = {seed, index, x, z, cols, columnDefs}; cols[i] is an index into "
            + "columnDefs in x-major / z-minor ChunkUtil.indexColumn order; a column def is "
            + "[tint, biomeId, zoneId, envRuns, blockRuns, parentBiomeId, borderDistance, biomeIndex, cellX, cellY]; "
            + "envRuns entries are [minYInclusive, maxYInclusive, environmentId]; blockRuns entries are "
            + "[yStartInclusive, yEndInclusive, blockId, rotationIndex, filler, fluidId, fluidLevel, priorityByte] — a "
            + "run ends when ANY of the eight values changes; the ids resolve through the palettes; parentBiomeId is "
            + "the raw tile biome BEFORE the custom-biome overlay, borderDistance is the zone border distance, "
            + "biomeIndex is the raw BiomePatternGenerator.getBiomeIndex value and cellX/cellY are the nearest "
            + "biome-grid cell — the last four are debug references, not part of the parity target");
        w.name("boot").value("the vanilla CoreServer asset pipeline booted by RegistryDumper (AssetRegistryLoader "
            + "over Assets/Assets.zip mounted as a zip FileSystem) + an AssetModule singleton stub whose base "
            + "pack is the UNPACKED asset tree (Assets/extracted), so the world-gen AssetFileSystem resolves world "
            + "files and prefabs through the disk tree — the same tree the Rust port walks (testdata/Default is "
            + "byte-identical to Assets/extracted/Server/World/Default)");
        w.name("world").value(PACK_WORLD_DIR + " inside the unpacked asset tree (byte-identical to the extracted "
            + "testdata/Default copy; the AssetFileSystem relativizes every path against the stub pack root)");
        w.name("worldPath").value(worldPath.toString());
        w.name("packRoot").value(assetRoot.toString());
        w.name("load").value("the FULL ChunkGeneratorJsonLoader.load() PER SEED with SeedString(\""
            + LOADER_SEED + "\") + SeedStringResource(PrefabStoreRoot.DEFAULT, config) — the "
            + "vanilla HytaleWorldGenProvider seed label (override with -Dhytale.dumper.loaderSeed); "
            + "the load includes the prefab store (World.json PrefabStore) "
            + "and EventHandler.acquire(worldPath)");
        w.name("generate").value("new ChunkGeneratorExecution(seed, generator, generatedChunk).execute(seed) on "
            + "the dumper's main thread — the same work ChunkGenerator.generate(...) submits to its worker pool; "
            + "every random stream is seeded from (seed, coordinates), so the thread does not matter");
        w.name("seeds").rawArray(joinInts(WORLD_SEEDS));
        w.name("chunks").rawArray(coordPairs(CHUNKS));
        w.name("height").value(ChunkUtil.HEIGHT);
        w.name("stages").value("tint mapping → environment mapping → BlockPopulator → CavePopulator → "
            + "PrefabPopulator → WaterPopulator (the execute() stage order)");
        w.name("notDumped").value("the BlockPhysics support values and the per-block Holder<ChunkStore> states "
            + "(the Rust port documents BlockPhysics as a no-op), the entity chunk and the block components "
            + "(deferred per REMAINING.md §2); the tint and the environment columns ARE dumped");
        w.name("biomeLookup").value("per column through the generator's own cached "
            + "getZoneBiomeResultAt(seed, worldX, worldZ) AFTER the execution — it consumes no generation "
            + "stream (the caches are keyed by seed + coordinates)");
        w.name("deterministic").value("no timestamps are written; the dump is byte-identical across runs");
        w.name("javaVersion").value(System.getProperty("java.version"));
        w.endObject();
    }

    private static String joinInts(int[] values) {
        final StringBuilder sb = new StringBuilder();
        for (int i = 0; i < values.length; i++) {
            if (i > 0) {
                sb.append(',');
            }
            sb.append(values[i]);
        }
        return sb.toString();
    }

    private static String coordPairs(int[][] pairs) {
        final StringBuilder sb = new StringBuilder();
        for (int i = 0; i < pairs.length; i++) {
            if (i > 0) {
                sb.append(',');
            }
            sb.append('[').append(pairs[i][0]).append(',').append(pairs[i][1]).append(']');
        }
        return sb.toString();
    }

    // ------------------------------------------------------------------
    // Palettes
    // ------------------------------------------------------------------

    /** The id sets accumulated over the chunk pass. */
    private static final class Palettes {
        final IntOpenHashSet blockIds = new IntOpenHashSet();
        final IntOpenHashSet fluidIds = new IntOpenHashSet();
        final IntOpenHashSet environmentIds = new IntOpenHashSet();
    }

    private static void writePalettes(Phase2Dumper.JsonWriter w, Palettes palettes) {
        w.name("palettes");
        w.beginObject();

        w.name("blocks");
        writeNamedPalette(w, palettes.blockIds,
            id -> {
                final BlockType type = BlockType.getAssetMap().getAsset(id);
                return type == null ? null : type.getId();
            });

        w.name("fluids");
        w.beginObject();
        for (int id : sorted(palettes.fluidIds)) {
            w.name(Integer.toString(id));
            final Fluid fluid = Fluid.getAssetMap().getAsset(id);
            w.value(fluid == null ? null : fluid.getId());
        }
        w.endObject();

        w.name("environments");
        writeNamedPalette(w, palettes.environmentIds,
            id -> {
                final Environment environment = Environment.getAssetMap().getAsset(id);
                return environment == null ? null : environment.getId();
            });

        w.name("biomes");
        writeNamedPalette(w, BIOME_NAMES.keySet(), id -> BIOME_NAMES.get(id));
        w.name("zones");
        writeNamedPalette(w, ZONE_NAMES.keySet(), id -> ZONE_NAMES.get(id));

        w.endObject();
    }

    private static int[] sorted(IntOpenHashSet ids) {
        final int[] list = ids.toIntArray();
        Arrays.sort(list);
        return list;
    }

    private static void writeNamedPalette(Phase2Dumper.JsonWriter w,
                                          java.util.Set<Integer> ids,
                                          IntFunction<String> namer) {
        w.beginObject();
        final int[] idList = sorted(new IntOpenHashSet(ids));
        for (int id : idList) {
            w.name(Integer.toString(id));
            w.value(namer.apply(id));
        }
        w.endObject();
    }

    // ------------------------------------------------------------------
    // Chunk dumping
    // ------------------------------------------------------------------

    private static void dumpChunk(StringBuilder out,
                                  ChunkGenerator generator,
                                  Palettes palettes,
                                  int seed,
                                  int chunkX,
                                  int chunkZ) {
        final long index = ChunkUtil.indexChunk(chunkX, chunkZ);
        final GeneratedChunk chunk = new GeneratedChunk();
        chunk.getBlockChunk().setCoordinates(index, chunkX, chunkZ);

        final ChunkGeneratorExecution execution = new ChunkGeneratorExecution(seed, generator, chunk);
        execution.execute(seed);

        final GeneratedBlockChunk blockChunk = chunk.getBlockChunk();
        final BlockPriorityChunk priorityChunk = execution.getPriorityChunk();

        if (DEBUG_CAVES && ((chunkX == 0 && chunkZ == 0) || (chunkX == 7 && chunkZ == 3))) {
            debugCaves(generator, seed, chunkX, chunkZ);
        }

        if (DEBUG_TINT && seed == 0 && chunkX == 16 && chunkZ == -8) {
            debugTint(generator, seed, (chunkX << ChunkUtil.BITS) | 20, (chunkZ << ChunkUtil.BITS) | 23);
        }

        out.append("{\"seed\":").append(seed)
            .append(",\"index\":").append(index)
            .append(",\"x\":").append(chunkX)
            .append(",\"z\":").append(chunkZ);

        // The per-column run-length encoding; identical columns are
        // deduplicated into the chunk's columnDefs (the dedup key INCLUDES
        // the biome/zone ids, so two columns only share a def when their
        // whole state matches).
        final List<String> columnDefs = new ArrayList<>();
        final Map<String, Integer> columnIndexes = new HashMap<>();
        final int[] columnRefs = new int[ChunkUtil.SIZE_COLUMNS];

        for (int x = 0; x < ChunkUtil.SIZE; x++) {
            for (int z = 0; z < ChunkUtil.SIZE; z++) {
                final int worldX = (chunkX << ChunkUtil.BITS) | x;
                final int worldZ = (chunkZ << ChunkUtil.BITS) | z;

                // The biome/zone of the column through the generator's own
                // (cached) path — AFTER the execution, so no random stream of
                // the generation is touched.
                final var zoneBiomeResult = generator.getZoneBiomeResultAt(seed, worldX, worldZ);
                final Zone zone = zoneBiomeResult.getZoneResult().getZone();
                final Biome biome = zoneBiomeResult.getBiome();
                final int zoneId = zone.id();
                final int biomeId = biome.getId();
                ZONE_NAMES.putIfAbsent(zoneId, zone.name());
                BIOME_NAMES.putIfAbsent(biomeId, biome.getName());

                // Debug reference: the raw parent tile biome (before the
                // custom-biome overlay) and the zone border distance. Neither
                // is part of the parity target, but they make a terrain
                // divergence attributable to a stage.
                final TileBiome parent = zone.biomePatternGenerator().getBiome(seed, worldX, worldZ);
                final int parentBiomeId = parent == null ? -1 : parent.getId();
                if (parent != null) {
                    BIOME_NAMES.putIfAbsent(parentBiomeId, parent.getName());
                }
                final double borderDistance = zoneBiomeResult.getZoneResult().getBorderDistance();
                final double biomeIndex = invokeBiomeIndex(zone.biomePatternGenerator(), seed, worldX, worldZ);
                final double[] cell = invokeNearestCell(zone.biomePatternGenerator(), seed, worldX, worldZ);
                final int rawTint = biome.getTintContainer().getTintColorAt(seed, worldX, worldZ);

                final String column = encodeColumn(blockChunk, priorityChunk, chunk, palettes,
                    x, z, biomeId, zoneId, parentBiomeId, borderDistance, biomeIndex, cell, rawTint);
                Integer columnIndex = columnIndexes.get(column);
                if (columnIndex == null) {
                    columnIndex = columnDefs.size();
                    columnDefs.add(column);
                    columnIndexes.put(column, columnIndex);
                }
                columnRefs[ChunkUtil.indexColumn(x, z)] = columnIndex;
            }
        }

        out.append(",\"cols\":[");
        for (int i = 0; i < columnRefs.length; i++) {
            if (i > 0) {
                out.append(',');
            }
            out.append(columnRefs[i]);
        }
        out.append("],\"columnDefs\":[");
        for (int i = 0; i < columnDefs.size(); i++) {
            if (i > 0) {
                out.append(',');
            }
            out.append(columnDefs.get(i));
        }
        out.append("]}");
    }

    /**
     * Encodes one (x, z) column as
     * {@code [tint, biomeId, zoneId, envRuns, blockRuns]} — a compact JSON
     * array-of-arrays.
     */
    private static String encodeColumn(GeneratedBlockChunk blockChunk,
                                       BlockPriorityChunk priorityChunk,
                                       GeneratedChunk chunk,
                                       Palettes palettes,
                                       int x,
                                       int z,
                                       int biomeId,
                                       int zoneId,
                                       int parentBiomeId,
                                       double borderDistance,
                                       double biomeIndex,
                                       double[] cell,
                                       int rawTint) {
        final StringBuilder sb = new StringBuilder(160);

        // --- tint ---------------------------------------------------------
        sb.append('[').append(blockChunk.getTint(x, z))
            .append(',').append(biomeId)
            .append(',').append(zoneId)
            .append(',');

        // --- environment runs ----------------------------------------------
        // The environment column carries (maxY exclusive, value) runs; the
        // dump writes them as [minYInclusive, maxYInclusive, id].
        sb.append('[');
        final var environmentColumn = blockChunk.getGeneratedEnvironment().get(x, z);
        for (int i = 0; i < environmentColumn.size(); i++) {
            if (i > 0) {
                sb.append(',');
            }
            final int minY = Math.max(environmentColumn.getValueMin(i), 0);
            final int maxY = Math.min(environmentColumn.getValueMax(i), ChunkUtil.HEIGHT - 1);
            final int envId = environmentColumn.getValue(i);
            palettes.environmentIds.add(envId);
            sb.append('[').append(minY).append(',').append(maxY).append(',').append(envId).append(']');
        }
        sb.append("],");

        // --- block runs ------------------------------------------------------
        // One run per change of the (blockId, rotation, filler, fluidId,
        // fluidLevel, environmentId, priority) tuple.
        sb.append('[');
        boolean firstRun = true;
        int runStart = 0;
        int runBlock = -1, runRotation = -1, runFiller = -1;
        int runFluidId = -1;
        byte runFluidLevel = -1;
        int runPriority = -1;

        for (int y = 0; y <= ChunkUtil.HEIGHT; y++) {
            int block = 0, rotation = 0, filler = 0;
            int fluidId = 0;
            byte fluidLevel = 0;
            int priority = 0;
            if (y < ChunkUtil.HEIGHT) {
                final GeneratedChunkSection section = blockChunk.getSection(y);
                if (section != null) {
                    block = section.getBlock(x, y, z);
                    rotation = section.getRotationIndex(x, y, z);
                    filler = section.getFiller(x, y, z);
                }
                priority = priorityChunk.get(x, y, z) & 0xFF;
                final var holder = chunk.getSections()[ChunkUtil.chunkCoordinate(y)];
                final FluidSection fluidSection = holder.getComponent(FluidSection.getComponentType());
                if (fluidSection != null) {
                    fluidId = fluidSection.getFluidId(x, y, z);
                    fluidLevel = fluidSection.getFluidLevel(x, y, z);
                }
                if (block != 0) {
                    palettes.blockIds.add(block);
                }
                if (fluidId != 0) {
                    palettes.fluidIds.add(fluidId);
                }
            }

            final boolean changed = y == ChunkUtil.HEIGHT
                || y == 0
                || block != runBlock
                || rotation != runRotation
                || filler != runFiller
                || fluidId != runFluidId
                || fluidLevel != runFluidLevel
                || priority != runPriority;
            if (!changed) {
                continue;
            }
            if (y > 0) {
                // Close the previous run [runStart, y - 1].
                if (!firstRun) {
                    sb.append(',');
                }
                firstRun = false;
                sb.append('[').append(runStart).append(',').append(y - 1)
                    .append(',').append(runBlock)
                    .append(',').append(runRotation)
                    .append(',').append(runFiller)
                    .append(',').append(runFluidId)
                    .append(',').append(runFluidLevel)
                    .append(',').append(runPriority)
                    .append(']');
            }
            runStart = y;
            runBlock = block;
            runRotation = rotation;
            runFiller = filler;
            runFluidId = fluidId;
            runFluidLevel = fluidLevel;
            runPriority = priority;
        }
        // y == HEIGHT closes the last run inside the loop (changed == true,
        // closing run [runStart, HEIGHT - 1]).

        // Debug references appended after the parity fields: the raw parent
        // tile biome id, the zone border distance and the biome-pattern index
        // value (all Java hex floats).
        sb.append("],").append(parentBiomeId)
            .append(",\"").append(Double.toHexString(borderDistance)).append('"')
            .append(",\"").append(Double.toHexString(biomeIndex)).append('"')
            .append(',').append((int) cell[0]).append(',').append((int) cell[1])
            .append(',').append(cell[2]).append(',').append(cell[3]).append(',').append(cell[4])
            .append(',').append(rawTint)
            .append(']');
        return sb.toString();
    }
}
