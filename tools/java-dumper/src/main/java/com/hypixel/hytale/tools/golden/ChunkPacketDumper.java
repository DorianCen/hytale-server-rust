package com.hypixel.hytale.tools.golden;

import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.hypixel.hytale.assetstore.AssetPack;
import com.hypixel.hytale.builtin.worldgen.modifier.EventHandler;
import com.hypixel.hytale.common.plugin.PluginManifest;
import com.hypixel.hytale.component.Holder;
import com.hypixel.hytale.math.util.ChunkUtil;
import com.hypixel.hytale.protocol.CachedPacket;
import com.hypixel.hytale.protocol.ToClientPacket;
import com.hypixel.hytale.procedurallib.file.FileIO;
import com.hypixel.hytale.procedurallib.json.SeedString;
import com.hypixel.hytale.server.core.asset.AssetModule;
import com.hypixel.hytale.server.core.blocktype.BlockTypeModule;
import com.hypixel.hytale.server.core.blocktype.component.BlockPhysics;
import com.hypixel.hytale.server.core.modules.LegacyModule;
import com.hypixel.hytale.server.core.universe.world.chunk.BlockChunk;
import com.hypixel.hytale.server.core.universe.world.chunk.heightmap.HeightmapColumn;
import com.hypixel.hytale.server.core.universe.world.chunk.heightmap.IHeightmap;
import com.hypixel.hytale.server.core.universe.world.chunk.section.BlockComponentSection;
import com.hypixel.hytale.server.core.universe.world.chunk.section.BlockSection;
import com.hypixel.hytale.server.core.universe.world.chunk.section.EnvironmentSection;
import com.hypixel.hytale.server.core.universe.world.chunk.section.FluidSection;
import com.hypixel.hytale.server.core.universe.world.storage.ChunkStore;
import com.hypixel.hytale.server.core.universe.world.worldgen.GeneratedChunk;
import com.hypixel.hytale.server.worldgen.HytaleWorldGenProvider;
import com.hypixel.hytale.server.worldgen.SeedStringResource;
import com.hypixel.hytale.server.worldgen.WorldGenConfig;
import com.hypixel.hytale.server.worldgen.chunk.ChunkGenerator;
import com.hypixel.hytale.server.worldgen.chunk.ChunkGeneratorExecution;
import com.hypixel.hytale.server.worldgen.loader.AssetFileSystem;
import com.hypixel.hytale.server.worldgen.loader.ChunkGeneratorJsonLoader;
import com.hypixel.hytale.server.worldgen.prefab.PrefabStoreRoot;

import java.io.IOException;
import java.io.PrintStream;
import java.lang.foreign.MemorySegment;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.CopyOnWriteArrayList;

/**
 * Dumps the <b>real Java network packets</b> of generated chunk sections.
 *
 * <p>This is the network counterpart of {@link ChunkDumper}: it boots exactly
 * the same real Default-world generator for seed {@code 0} at the fixed chunk
 * coordinates, converts the {@link GeneratedChunk} into the server's
 * {@link BlockChunk} / section model, and then calls the same packet builders
 * CoreServer uses ({@code BlockSection.getCachedChunkPacket},
 * {@code BlockChunk.getCachedColumnPacket/getCachedTintsPacket},
 * {@code EnvironmentSection.getCachedPacket}, {@code FluidSection.getCachedPacket}).
 *
 * <p>Every payload is serialized by the packet object itself
 * ({@code packet.computeSize()} + {@code packet.serialize(MemorySegment, 0)}),
 * with no framing and no compression. The output is a manifest plus one
 * {@code .bin} per packet.
 *
 * <p>Only packets CoreServer's load query systems actually send for a freshly
 * generated chunk are marked {@code sentAtInit = true}: {@code SetColumn},
 * {@code SetChunk} per section and {@code SetChunkEnvironments} per section.
 * {@code SetChunkTintmap} (sent by {@code WorldNotificationHandler.updateChunkTints})
 * and {@code SetFluids} (sent by {@code ChunkSystems.ReplicateChanges}) are also
 * captured from the same chunk state because they are the real Java
 * serializations the Rust encoder must match; their manifest entries carry
 * {@code sentAtInit = false}. No packet bytes are hand-encoded.
 *
 * <p>Usage: {@code ChunkPacketDumper <output-directory> [world-directory] [repo-root]}
 * (same argument order as the other dumpers; the world directory is ignored).
 */
public final class ChunkPacketDumper {

    /** The single world seed of this fixture. */
    private static final int SEED = 0;

    /** The source chunk coordinates (subset of {@code chunks.json}). */
    private static final int[][] CHUNKS = {{0, 0}, {4, -5}};

    /** The world folder inside the unpacked asset tree (vanilla server layout). */
    private static final String PACK_WORLD_DIR = "Server/World/Default";

    /** The unpacked asset tree used for world generation (same as ChunkDumper). */
    private static final String EXTRACTED_DIR = "Assets/extracted";

    /** The vanilla loader seed label (same default as ChunkDumper). */
    private static final String LOADER_SEED =
        System.getProperty("hytale.dumper.loaderSeed", "ChunkGenerator");

    /** The real stderr; the CoreServer boot replaces {@code System.err}. */
    private static final PrintStream REAL_ERR = System.err;

    private static final Gson GSON = new GsonBuilder()
        .setPrettyPrinting()
        .disableHtmlEscaping()
        .create();

    private static boolean assetModuleStubbed;
    private static boolean legacyModuleStubbed;
    private static boolean blockTypeModuleStubbed;

    private ChunkPacketDumper() {
    }

    public static void main(String[] args) throws Exception {
        try {
            run(args);
        } catch (Throwable t) {
            REAL_ERR.println("ChunkPacketDumper failed:");
            t.printStackTrace(REAL_ERR);
            System.exit(1);
        }
    }

    private static void run(String[] args) throws Exception {
        if (args.length < 1) {
            System.err.println("usage: ChunkPacketDumper <output-directory> [world-directory] [repo-root]");
            System.exit(2);
        }

        final Path outDir = Path.of(args[0]).toAbsolutePath().normalize();
        final Path repoRoot = args.length >= 3
            ? Path.of(args[2]).toAbsolutePath().normalize()
            : Path.of("").toAbsolutePath().normalize();

        // The registry boot is the same canonical one the chunk dump uses.
        RegistryDumper.boot(repoRoot);

        final Path assetRoot = repoRoot.resolve(EXTRACTED_DIR).normalize();
        try {
            stubAssetModule(assetRoot, repoRoot);
            stubLegacyModule();
            stubBlockTypeModule();
        } catch (Exception e) {
            throw new IOException("Failed to install the AssetModule / LegacyModule singleton stubs", e);
        }

        final Path worldPath = assetRoot.resolve("Server").resolve("World").resolve("Default");
        if (!Files.isRegularFile(worldPath.resolve("World.json"))) {
            throw new IOException("the unpacked asset tree does not contain " + PACK_WORLD_DIR + "/World.json: "
                + worldPath);
        }

        final WorldGenConfig config = new WorldGenConfig(
            worldPath, HytaleWorldGenProvider.DEFAULT_NAME, HytaleWorldGenProvider.MIN_VERSION);

        // Acquired for the generator lifetime exactly like the vanilla provider.
        EventHandler.acquire(config.path());

        final SeedString<SeedStringResource> seed = new SeedString<>(
            LOADER_SEED, new SeedStringResource(PrefabStoreRoot.DEFAULT, config));

        final ChunkGenerator generator;
        try (final var ignored = FileIO.openFileIOSystem(new AssetFileSystem(config))) {
            generator = new ChunkGeneratorJsonLoader(seed, config).load();
        }
        // The generation pass loads prefab buffers lazily through the
        // AssetFileSystem, and the main thread needs the thread-local resource.
        FileIO.openFileIOSystem(new AssetFileSystem(config));
        ChunkGenerator.getResource().init(generator);

        Files.createDirectories(outDir);
        removeStalePayloads(outDir);

        final JsonObject root = new JsonObject();
        root.addProperty("seed", SEED);
        root.add("meta", meta());

        final JsonArray chunks = new JsonArray();
        long totalBytes = 0;
        int totalPackets = 0;
        for (int[] coord : CHUNKS) {
            final ChunkDump dump = dumpChunk(outDir, generator, coord[0], coord[1]);
            chunks.add(dump.manifest());
            totalBytes += dump.bytes();
            totalPackets += dump.packets();
        }
        root.add("chunks", chunks);

        // State explicitly which of the candidate packet types are not part of
        // the initial chunk-load query set.
        final JsonArray notAtInit = new JsonArray();
        notAtInit.add(notSent("SetChunkTintmap",
            "CoreServer has no load-query system for this packet; it is built by "
                + "BlockChunk.getCachedTintsPacket() and sent by "
                + "WorldNotificationHandler.updateChunkTints() when the tint changes"));
        notAtInit.add(notSent("SetFluids",
            "CoreServer has no load-query system for this packet; it is built by "
                + "FluidSection.getCachedPacket() and sent by ChunkSystems.ReplicateChanges "
                + "when fluid blocks change. Payloads are still captured for non-empty "
                + "fluid sections and marked sentAtInit=false"));
        root.add("packetsNotSentAtInit", notAtInit);

        generator.shutdown();

        final Path manifest = outDir.resolve("manifest.json");
        Files.writeString(manifest, GSON.toJson(root) + "\n", StandardCharsets.UTF_8);

        System.out.printf("ChunkPacketDumper: %d chunks, %d packets, %d payload bytes -> %s%n",
            CHUNKS.length, totalPackets, totalBytes, outDir);
    }

    // ------------------------------------------------------------------
    // Boot stubs (same shape as ChunkDumper)
    // ------------------------------------------------------------------

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
     * Installs the component types the chunk model needs. This is a superset of
     * ChunkDumper's stub: the packet dump also constructs BlockSection /
     * EnvironmentSection holders directly.
     */
    private static void stubLegacyModule() throws Exception {
        if (legacyModuleStubbed) {
            return;
        }
        final Class<?> moduleClass = Class.forName("com.hypixel.hytale.server.core.modules.LegacyModule");
        final Object module = allocateInstance(moduleClass);
        setStaticField(moduleClass, "instance", module);

        final var registry = ChunkStore.REGISTRY;
        setField(module, "blockSectionComponentType",
            registry.registerComponent(BlockSection.class, "Block", BlockSection.CODEC));
        setField(module, "fluidSectionComponentType",
            registry.registerComponent(FluidSection.class, "Fluid", FluidSection.CODEC));
        setField(module, "environmentSectionComponentType",
            registry.registerComponent(EnvironmentSection.class, "Environment", EnvironmentSection.CODEC));
        setField(module, "blockComponentSectionComponentType",
            registry.registerComponent(BlockComponentSection.class, "BlockComponent", BlockComponentSection.CODEC));

        legacyModuleStubbed = true;
    }

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
        final Field field = target.getClass().getDeclaredField(name);
        field.setAccessible(true);
        field.set(target, value);
    }

    // ------------------------------------------------------------------
    // Chunk dump
    // ------------------------------------------------------------------

    private record ChunkDump(JsonObject manifest, int packets, long bytes) {
    }

    private static ChunkDump dumpChunk(Path outDir, ChunkGenerator generator, int chunkX, int chunkZ)
        throws IOException {
        final long index = ChunkUtil.indexChunk(chunkX, chunkZ);
        final GeneratedChunk chunk = new GeneratedChunk();
        chunk.getBlockChunk().setCoordinates(index, chunkX, chunkZ);

        final ChunkGeneratorExecution execution = new ChunkGeneratorExecution(SEED, generator, chunk);
        execution.execute(SEED);

        final Holder<ChunkStore>[] sections = chunk.getSections();

        // Convert the generated sections exactly like GeneratedChunk.toHolder()
        // does: the generated block sections + every environment section are
        // installed into the holders, while the FluidSections added during
        // generation stay where they are.
        final BlockChunk blockChunk = chunk.getBlockChunk().toBlockChunk(sections);

        // ChunkSystems.EnsureSection installs a (possibly empty) BlockSection in
        // every section holder. That is why CoreServer sends a SetChunk for every
        // section, including all-air sections whose data is null.
        for (Holder<ChunkStore> section : sections) {
            section.ensureComponent(BlockSection.getComponentType());
            section.ensureComponent(EnvironmentSection.getComponentType());
        }

        // HeightmapSystems.MigrateHeightmap + the section-resident hook. The
        // dumper has no loader, so IHeightmap.forLoader(null) selects the legacy
        // backend, which is what the legacy BlockChunk column packet expects.
        final HeightmapColumn heightmap = new HeightmapColumn(IHeightmap.forLoader(null));
        heightmap.load(chunkX, chunkZ);
        blockChunk.setHeightmapColumn(heightmap);
        for (int sectionY = 0; sectionY < sections.length; sectionY++) {
            final BlockSection blockSection = sections[sectionY].getComponent(BlockSection.getComponentType());
            if (blockSection != null) {
                heightmap.onSectionResident(sectionY, blockSection, true);
            }
        }

        final Path chunkDir = outDir.resolve(chunkX + "_" + chunkZ);
        Files.createDirectories(chunkDir);

        final JsonArray packets = new JsonArray();
        long bytes = 0;

        // --- column-level packets ----------------------------------------
        // getCachedColumnPacket is private; it is still the exact CoreServer
        // builder, so invoke it rather than reimplementing SetColumn.
        final CachedPacket<?> column = await(invokeColumnPacket(blockChunk));
        bytes += writePacket(chunkDir, "SetColumn",
            columnXZ(chunkX, chunkZ), column, packets, true);

        final CachedPacket<?> tints = blockChunk.getCachedTintsPacket().join();
        bytes += writePacket(chunkDir, "SetChunkTintmap",
            columnXZ(chunkX, chunkZ), tints, packets, false);

        // --- section packets ---------------------------------------------
        // Order: SetChunk then SetChunkEnvironments (the registration order of
        // the load-query systems), then SetFluids.
        int packetCount = 2;
        for (int sectionY = 0; sectionY < sections.length; sectionY++) {
            final BlockSection blockSection = sections[sectionY].getComponent(BlockSection.getComponentType());
            if (blockSection != null) {
                final CachedPacket<?> setChunk =
                    blockSection.getCachedChunkPacket(chunkX, sectionY, chunkZ).join();
                bytes += writePacket(chunkDir, "SetChunk",
                    sectionXyz(chunkX, sectionY, chunkZ), setChunk, packets, true);
                packetCount++;
            }

            final EnvironmentSection environment =
                sections[sectionY].getComponent(EnvironmentSection.getComponentType());
            if (environment != null) {
                final CachedPacket<?> setEnvironments =
                    environment.getCachedPacket(chunkX, sectionY, chunkZ).join();
                bytes += writePacket(chunkDir, "SetChunkEnvironments",
                    sectionXyz(chunkX, sectionY, chunkZ), setEnvironments, packets, true);
                packetCount++;
            }

            final FluidSection fluid = sections[sectionY].getComponent(FluidSection.getComponentType());
            if (fluid != null && !fluid.isEmpty()) {
                // The section coordinates are normally set on section load; set
                // them here so the captured packet carries the real section.
                fluid.load(chunkX, sectionY, chunkZ);
                final CachedPacket<?> setFluids = fluid.getCachedPacket().join();
                bytes += writePacket(chunkDir, "SetFluids",
                    sectionXyz(chunkX, sectionY, chunkZ), setFluids, packets, false);
                packetCount++;
            }
        }

        final JsonObject entry = new JsonObject();
        entry.add("chunk", ints(chunkX, chunkZ));
        entry.add("packets", packets);

        return new ChunkDump(entry, packetCount, bytes);
    }

    @SuppressWarnings("unchecked")
    private static CompletableFuture<? extends CachedPacket<?>> invokeColumnPacket(BlockChunk blockChunk) {
        try {
            final Method method = BlockChunk.class.getDeclaredMethod("getCachedColumnPacket");
            method.setAccessible(true);
            return (CompletableFuture<? extends CachedPacket<?>>) method.invoke(blockChunk);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException("failed to invoke BlockChunk.getCachedColumnPacket", e);
        }
    }

    private static CachedPacket<?> await(CompletableFuture<? extends CachedPacket<?>> future) {
        return future.join();
    }

    private static long writePacket(Path chunkDir, String packetName,
                                    JsonArray coordinates, CachedPacket<?> packet,
                                    JsonArray packets, boolean sentAtInit) throws IOException {
        final String suffix = "_" + coordinates.get(0).getAsInt()
            + "_" + coordinates.get(1).getAsInt();
        final String baseName = coordinates.size() == 2
            ? packetName + suffix
            : packetName + suffix + "_" + coordinates.get(2).getAsInt();
        final String file = chunkDir.getFileName() + "/" + baseName + ".bin";

        final byte[] payload = serialize(packet);
        Files.write(chunkDir.resolve(baseName + ".bin"), payload);

        final JsonObject entry = new JsonObject();
        entry.addProperty("name", packetName);
        entry.addProperty("id", packet.getId());
        entry.addProperty("file", file);
        entry.addProperty("size", payload.length);
        entry.addProperty("sentAtInit", sentAtInit);
        if (coordinates.size() == 3) {
            entry.add("section", coordinates);
        }
        packets.add(entry);
        return payload.length;
    }

    private static byte[] serialize(ToClientPacket packet) {
        final int size = packet.computeSize();
        final byte[] payload = new byte[size];
        final int written = packet.serialize(MemorySegment.ofArray(payload), 0);
        if (written != size) {
            throw new IllegalStateException(packet.getClass().getSimpleName() + ": serialize wrote "
                + written + " bytes but computeSize() returned " + size);
        }
        return payload;
    }

    private static JsonArray columnXZ(int x, int z) {
        final JsonArray array = new JsonArray(2);
        array.add(x);
        array.add(z);
        return array;
    }

    private static JsonArray sectionXyz(int x, int y, int z) {
        final JsonArray array = new JsonArray(3);
        array.add(x);
        array.add(y);
        array.add(z);
        return array;
    }

    private static JsonArray ints(int... values) {
        final JsonArray array = new JsonArray(values.length);
        for (int value : values) {
            array.add(value);
        }
        return array;
    }

    private static JsonObject notSent(String name, String reason) {
        final JsonObject entry = new JsonObject();
        entry.addProperty("name", name);
        entry.addProperty("reason", reason);
        return entry;
    }

    private static JsonObject meta() {
        final JsonObject meta = new JsonObject();
        meta.addProperty("producer", "com.hypixel.hytale.tools.golden.ChunkPacketDumper");
        meta.addProperty("source", "seed 0 chunks (0,0) and (4,-5), the same real Default-world generator "
            + "boot as ChunkDumper (chunks.json)");
        meta.addProperty("world", PACK_WORLD_DIR + " inside Assets/extracted");
        meta.addProperty("loaderSeed", LOADER_SEED);
        meta.addProperty("packetOrder", "per chunk: SetColumn, SetChunkTintmap, then per section "
            + "(ascending section Y): SetChunk, SetChunkEnvironments, SetFluids when the section holds fluid");
        meta.addProperty("payload", "each .bin is the exact uncompressed, unframed payload of the real packet "
            + "object: byte[packet.computeSize()] filled by packet.serialize(MemorySegment.ofArray(bytes), 0)");
        meta.addProperty("sentAtInit", "true for packets produced by CoreServer's chunk/section load query "
            + "systems (SetColumn, SetChunk, SetChunkEnvironments). SetChunkTintmap and SetFluids are real "
            + "serializations of the same chunk state but are only sent on updates, so they are marked false; "
            + "see packetsNotSentAtInit");
        meta.addProperty("deterministic", "fixed seed and chunk list, single-threaded generator and common "
            + "fork-join pool (DUMPER_PARALLELISM=1), no timestamps and no absolute paths");
        return meta;
    }

    private static void removeStalePayloads(Path outDir) throws IOException {
        // The whole chunks-pkts tree is regenerated, but keep the README.
        try (var stream = Files.list(outDir)) {
            for (Path path : stream.toList()) {
                final String name = path.getFileName().toString();
                if (Files.isDirectory(path) && (name.matches("-?\\d+_-?\\d+"))) {
                    deleteRecursively(path);
                } else if (name.endsWith(".bin")) {
                    Files.delete(path);
                }
            }
        }
    }

    private static void deleteRecursively(Path dir) throws IOException {
        try (var stream = Files.list(dir)) {
            for (Path path : stream.toList()) {
                if (Files.isDirectory(path)) {
                    deleteRecursively(path);
                } else {
                    Files.delete(path);
                }
            }
        }
        Files.delete(dir);
    }
}
