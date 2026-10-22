package com.hypixel.hytale.tools.golden;

import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.hypixel.hytale.assetstore.AssetPack;
import com.hypixel.hytale.builtin.worldgen.modifier.EventHandler;
import com.hypixel.hytale.common.plugin.PluginManifest;
import com.hypixel.hytale.component.ComponentRegistry;
import com.hypixel.hytale.component.Holder;
import com.hypixel.hytale.math.util.ChunkUtil;
import com.hypixel.hytale.procedurallib.file.FileIO;
import com.hypixel.hytale.procedurallib.json.SeedString;
import com.hypixel.hytale.server.core.modules.LegacyModule;
import com.hypixel.hytale.server.core.universe.world.chunk.BlockChunk;
import com.hypixel.hytale.server.core.universe.world.chunk.ChunkColumn;
import com.hypixel.hytale.server.core.universe.world.chunk.WorldChunk;
import com.hypixel.hytale.server.core.universe.world.chunk.heightmap.HeightmapColumn;
import com.hypixel.hytale.server.core.universe.world.chunk.heightmap.IHeightmap;
import com.hypixel.hytale.server.core.universe.world.chunk.section.BlockSection;
import com.hypixel.hytale.server.core.universe.world.chunk.section.ChunkSection;
import com.hypixel.hytale.server.core.universe.world.chunk.section.EnvironmentSection;
import com.hypixel.hytale.server.core.universe.world.chunk.section.FluidSection;
import com.hypixel.hytale.server.core.universe.world.storage.ChunkStore;
import com.hypixel.hytale.server.core.universe.world.worldgen.GeneratedChunk;
import com.hypixel.hytale.server.core.util.BsonUtil;
import com.hypixel.hytale.server.worldgen.HytaleWorldGenProvider;
import com.hypixel.hytale.server.worldgen.SeedStringResource;
import com.hypixel.hytale.server.worldgen.WorldGenConfig;
import com.hypixel.hytale.server.worldgen.chunk.ChunkGenerator;
import com.hypixel.hytale.server.worldgen.chunk.ChunkGeneratorExecution;
import com.hypixel.hytale.server.worldgen.loader.AssetFileSystem;
import com.hypixel.hytale.server.worldgen.loader.ChunkGeneratorJsonLoader;
import com.hypixel.hytale.server.worldgen.prefab.PrefabStoreRoot;
import com.hypixel.hytale.storage.IndexedStorageFile;
import org.bson.BsonDocument;

import java.io.IOException;
import java.io.PrintStream;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.List;
import java.util.concurrent.CopyOnWriteArrayList;

/**
 * Writes a real Java {@code IndexedStorage} world save for the fixed seed-0
 * chunks, so the Rust BSON chunk codec can be validated against the Java
 * component serializer instead of a synthetic document.
 *
 * <p>The chunk state is built exactly like {@link ChunkPacketDumper}: the real
 * Default-world generator runs for chunks {@code (0,0)} and {@code (4,-5)}, the
 * generated sections are converted into the server's runtime chunk model, and
 * the legacy heightmap is populated the same way a loaded column would be.
 * The resulting {@code ChunkStore} holder is serialized with
 * {@code ChunkStore.REGISTRY.serialize(holder)} and stored in
 * {@code chunks/<regionX>.<regionZ>.region.bin} at
 * {@code ChunkUtil.indexColumn(localX, localZ)}.
 *
 * <p>Usage: {@code StorageDumper <output-directory> [world-directory] [repo-root]}.
 */
public final class StorageDumper {

    private static final int SEED = 0;

    private static final int[][] CHUNKS = {{0, 0}, {4, -5}};

    private static final String PACK_WORLD_DIR = "Server/World/Default";

    private static final String EXTRACTED_DIR = "Assets/extracted";

    private static final String LOADER_SEED =
        System.getProperty("hytale.dumper.loaderSeed", "ChunkGenerator");

    private static final PrintStream REAL_ERR = System.err;

    private static final Gson GSON = new GsonBuilder()
        .setPrettyPrinting()
        .disableHtmlEscaping()
        .create();

    private static boolean assetModuleStubbed;
    private static boolean legacyModuleStubbed;

    private StorageDumper() {
    }

    public static void main(String[] args) throws Exception {
        try {
            run(args);
        } catch (Throwable t) {
            REAL_ERR.println("StorageDumper failed:");
            t.printStackTrace(REAL_ERR);
            System.exit(1);
        }
    }

    private static void run(String[] args) throws Exception {
        if (args.length < 1) {
            System.err.println("usage: StorageDumper <output-directory> [world-directory] [repo-root]");
            System.exit(2);
        }

        final Path outDir = Path.of(args[0]).toAbsolutePath().normalize();
        final Path repoRoot = args.length >= 3
            ? Path.of(args[2]).toAbsolutePath().normalize()
            : Path.of("").toAbsolutePath().normalize();

        RegistryDumper.boot(repoRoot);

        final Path assetRoot = repoRoot.resolve(EXTRACTED_DIR).normalize();
        stubAssetModule(assetRoot, repoRoot);
        registerChunkComponents();

        final Path worldPath = assetRoot.resolve("Server").resolve("World").resolve("Default");
        if (!Files.isRegularFile(worldPath.resolve("World.json"))) {
            throw new IOException("the unpacked asset tree does not contain " + PACK_WORLD_DIR + "/World.json: "
                + worldPath);
        }

        final WorldGenConfig config = new WorldGenConfig(
            worldPath, HytaleWorldGenProvider.DEFAULT_NAME, HytaleWorldGenProvider.MIN_VERSION);

        EventHandler.acquire(config.path());

        final SeedString<SeedStringResource> seed = new SeedString<>(
            LOADER_SEED, new SeedStringResource(PrefabStoreRoot.DEFAULT, config));

        final ChunkGenerator generator;
        try (final var ignored = FileIO.openFileIOSystem(new AssetFileSystem(config))) {
            generator = new ChunkGeneratorJsonLoader(seed, config).load();
        }
        FileIO.openFileIOSystem(new AssetFileSystem(config));
        ChunkGenerator.getResource().init(generator);

        final Path chunksDir = outDir.resolve("chunks");
        Files.createDirectories(chunksDir);
        removeStaleRegions(chunksDir);

        final JsonObject root = new JsonObject();
        root.addProperty("seed", SEED);
        root.add("meta", meta());

        final JsonArray chunkEntries = new JsonArray();
        for (int[] coord : CHUNKS) {
            chunkEntries.add(dumpChunk(chunksDir, generator, coord[0], coord[1]));
        }
        root.add("chunks", chunkEntries);

        generator.shutdown();

        Files.writeString(outDir.resolve("manifest.json"), GSON.toJson(root) + "\n", StandardCharsets.UTF_8);
        System.out.printf("StorageDumper: %d chunks, %d region file(s) -> %s%n",
            CHUNKS.length, countRegions(chunksDir), outDir);
    }

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
     * Registers the component types the saved chunk document references, using
     * the same ids as {@code LegacyModule.setup()} so the Java reader can load
     * the fixture.
     */
    private static void registerChunkComponents() throws Exception {
        if (legacyModuleStubbed) {
            return;
        }
        final Class<?> moduleClass = Class.forName("com.hypixel.hytale.server.core.modules.LegacyModule");
        final Object module = allocateInstance(moduleClass);
        setStaticField(moduleClass, "instance", module);

        final ComponentRegistry<ChunkStore> registry = ChunkStore.REGISTRY;
        setField(module, "worldChunkComponentType",
            registry.registerComponent(WorldChunk.class, "WorldChunk", WorldChunk.CODEC));
        setField(module, "blockChunkComponentType",
            registry.registerComponent(BlockChunk.class, "BlockChunk", BlockChunk.CODEC));
        setField(module, "chunkColumnComponentType",
            registry.registerComponent(ChunkColumn.class, "ChunkColumn", ChunkColumn.CODEC));
        setField(module, "heightmapColumnComponentType",
            registry.registerComponent(HeightmapColumn.class, "HeightmapColumn", HeightmapColumn.CODEC));
        IHeightmap.CODEC.register("Legacy", com.hypixel.hytale.server.core.universe.world.chunk.heightmap.LegacyHeightmap.class,
            com.hypixel.hytale.server.core.universe.world.chunk.heightmap.LegacyHeightmap.CODEC);
        IHeightmap.CODEC.register("Tree", com.hypixel.hytale.server.core.universe.world.chunk.heightmap.TreeHeightmap.class,
            com.hypixel.hytale.server.core.universe.world.chunk.heightmap.TreeHeightmap.CODEC);
        setField(module, "chunkSectionComponentType",
            registry.registerComponent(ChunkSection.class, "ChunkSection", ChunkSection.CODEC));
        setField(module, "blockSectionComponentType",
            registry.registerComponent(BlockSection.class, "Block", BlockSection.CODEC));
        setField(module, "fluidSectionComponentType",
            registry.registerComponent(FluidSection.class, "Fluid", FluidSection.CODEC));
        setField(module, "environmentSectionComponentType",
            registry.registerComponent(EnvironmentSection.class, "Environment", EnvironmentSection.CODEC));
        setField(module, "blockComponentSectionComponentType",
            registry.registerComponent(com.hypixel.hytale.server.core.universe.world.chunk.section.BlockComponentSection.class,
                "BlockComponent",
                com.hypixel.hytale.server.core.universe.world.chunk.section.BlockComponentSection.CODEC));
        setField(module, "entitySectionComponentType",
            registry.registerComponent(com.hypixel.hytale.server.core.universe.world.chunk.section.EntitySection.class,
                "Entity",
                com.hypixel.hytale.server.core.universe.world.chunk.section.EntitySection.CODEC));

        final Class<?> blockTypeModuleClass =
            Class.forName("com.hypixel.hytale.server.core.blocktype.BlockTypeModule");
        final Object blockTypeModule = allocateInstance(blockTypeModuleClass);
        setStaticField(blockTypeModuleClass, "instance", blockTypeModule);
        setField(blockTypeModule, "blockPhysicsComponentType",
            registry.registerComponent(
                com.hypixel.hytale.server.core.blocktype.component.BlockPhysics.class,
                "BlockPhysics",
                com.hypixel.hytale.server.core.blocktype.component.BlockPhysics.CODEC));

        legacyModuleStubbed = true;
    }

    private static JsonObject dumpChunk(Path chunksDir, ChunkGenerator generator, int chunkX, int chunkZ)
        throws IOException {
        final long index = ChunkUtil.indexChunk(chunkX, chunkZ);
        final GeneratedChunk chunk = new GeneratedChunk();
        chunk.getBlockChunk().setCoordinates(index, chunkX, chunkZ);

        final ChunkGeneratorExecution execution = new ChunkGeneratorExecution(SEED, generator, chunk);
        execution.execute(SEED);

        final Holder<ChunkStore>[] sections = chunk.getSections();
        final BlockChunk blockChunk = chunk.getBlockChunk().toBlockChunk(sections);

        for (Holder<ChunkStore> section : sections) {
            section.ensureComponent(BlockSection.getComponentType());
            section.ensureComponent(EnvironmentSection.getComponentType());
            section.ensureComponent(ChunkSection.getComponentType());
        }

        final HeightmapColumn heightmap = new HeightmapColumn(IHeightmap.forLoader(null));
        heightmap.load(chunkX, chunkZ);
        blockChunk.setHeightmapColumn(heightmap);
        for (int sectionY = 0; sectionY < sections.length; sectionY++) {
            final BlockSection blockSection = sections[sectionY].getComponent(BlockSection.getComponentType());
            if (blockSection != null) {
                heightmap.onSectionResident(sectionY, blockSection, true);
            }
        }

        final Holder<ChunkStore> holder = chunk.toHolder();
        holder.putComponent(HeightmapColumn.getComponentType(), heightmap);

        final BsonDocument document = ChunkStore.REGISTRY.serialize(holder);
        final byte[] bytes = BsonUtil.writeToBytes(document);

        final int regionX = chunkX >> ChunkUtil.BITS;
        final int regionZ = chunkZ >> ChunkUtil.BITS;
        final int localX = chunkX & ChunkUtil.SIZE_MASK;
        final int localZ = chunkZ & ChunkUtil.SIZE_MASK;
        final int blobIndex = ChunkUtil.indexColumn(localX, localZ);

        final Path regionFile = chunksDir.resolve(regionX + "." + regionZ + ".region.bin");
        try (final IndexedStorageFile file = IndexedStorageFile.open(
            regionFile,
            StandardOpenOption.CREATE,
            StandardOpenOption.READ,
            StandardOpenOption.WRITE)) {
            file.setCompressionLevel(3);
            file.writeBlob(blobIndex, ByteBuffer.wrap(bytes));
            file.force(false);
        }

        final JsonObject entry = new JsonObject();
        entry.add("chunk", ints(chunkX, chunkZ));
        entry.addProperty("region", regionFile.getFileName().toString());
        entry.addProperty("index", blobIndex);
        entry.addProperty("documentBytes", bytes.length);
        return entry;
    }

    private static JsonObject meta() {
        final JsonObject meta = new JsonObject();
        meta.addProperty("producer", "com.hypixel.hytale.tools.golden.StorageDumper");
        meta.addProperty("source", "seed 0 chunks (0,0) and (4,-5), the real Default-world generator "
            + "and the server's runtime chunk model");
        meta.addProperty("layout", "chunks/<regionX>.<regionZ>.region.bin, blob index "
            + "ChunkUtil.indexColumn(chunkX & 31, chunkZ & 31)");
        meta.addProperty("payload", "ChunkStore.REGISTRY.serialize(holder) written by BsonUtil.writeToBytes; "
            + "components: WorldChunk, BlockChunk, ChunkColumn (sections), HeightmapColumn");
        meta.addProperty("heightmap", "Legacy backend, populated like a loaded legacy column");
        meta.addProperty("deterministic", "fixed seed and chunk list, single-threaded generator and common "
            + "fork-join pool (DUMPER_PARALLELISM=1), no timestamps and no absolute paths");
        return meta;
    }

    private static int countRegions(Path chunksDir) throws IOException {
        try (var stream = Files.list(chunksDir)) {
            return (int) stream.filter(path -> path.getFileName().toString().endsWith(".region.bin")).count();
        }
    }

    private static void removeStaleRegions(Path chunksDir) throws IOException {
        try (var stream = Files.list(chunksDir)) {
            for (Path path : stream.toList()) {
                if (path.getFileName().toString().endsWith(".region.bin")) {
                    Files.delete(path);
                }
            }
        }
    }

    private static JsonArray ints(int... values) {
        final JsonArray array = new JsonArray(values.length);
        for (int value : values) {
            array.add(value);
        }
        return array;
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
}
