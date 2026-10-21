package com.hypixel.hytale.tools.golden;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.hypixel.hytale.common.map.IWeightedMap;
import com.hypixel.hytale.common.map.WeightedMap;
import com.hypixel.hytale.common.util.PathUtil;
import com.hypixel.hytale.math.util.FastRandom;
import com.hypixel.hytale.procedurallib.file.FileIO;
import com.hypixel.hytale.procedurallib.json.JsonLoader;
import com.hypixel.hytale.procedurallib.json.PointGeneratorJsonLoader;
import com.hypixel.hytale.procedurallib.json.SeedString;
import com.hypixel.hytale.procedurallib.logic.point.IPointGenerator;
import com.hypixel.hytale.server.worldgen.SeedStringResource;
import com.hypixel.hytale.server.worldgen.biome.Biome;
import com.hypixel.hytale.server.worldgen.biome.BiomeInterpolation;
import com.hypixel.hytale.server.worldgen.biome.BiomePatternGenerator;
import com.hypixel.hytale.server.worldgen.biome.CustomBiome;
import com.hypixel.hytale.server.worldgen.biome.CustomBiomeGenerator;
import com.hypixel.hytale.server.worldgen.biome.TileBiome;
import com.hypixel.hytale.server.worldgen.container.CoverContainer;
import com.hypixel.hytale.server.worldgen.container.EnvironmentContainer;
import com.hypixel.hytale.server.worldgen.container.FadeContainer;
import com.hypixel.hytale.server.worldgen.container.LayerContainer;
import com.hypixel.hytale.server.worldgen.container.PrefabContainer;
import com.hypixel.hytale.server.worldgen.container.TintContainer;
import com.hypixel.hytale.server.worldgen.container.WaterContainer;
import com.hypixel.hytale.server.worldgen.loader.biome.BiomePatternGeneratorJsonLoader;
import com.hypixel.hytale.server.worldgen.loader.biome.CustomBiomeJsonLoader;
import com.hypixel.hytale.server.worldgen.loader.biome.TileBiomeJsonLoader;
import com.hypixel.hytale.server.worldgen.loader.context.BiomeFileContext;
import com.hypixel.hytale.server.worldgen.loader.context.FileContextLoader;
import com.hypixel.hytale.server.worldgen.loader.context.FileLoadingContext;
import com.hypixel.hytale.server.worldgen.loader.context.ZoneFileContext;
import com.hypixel.hytale.server.worldgen.loader.util.ColorUtil;
import com.hypixel.hytale.server.worldgen.util.NoiseBlockArray;
import com.hypixel.hytale.server.worldgen.loader.zone.ZoneColorMappingJsonLoader;
import com.hypixel.hytale.server.worldgen.loader.zone.ZoneJsonLoader;
import com.hypixel.hytale.server.worldgen.loader.zone.ZoneRequirementJsonLoader;
import com.hypixel.hytale.server.worldgen.loader.prefab.BlockPlacementMaskRegistry;
import com.hypixel.hytale.server.worldgen.loader.util.FileMaskCache;
import com.hypixel.hytale.server.worldgen.util.BlockFluidEntry;
import com.hypixel.hytale.server.worldgen.zone.Zone;
import com.hypixel.hytale.server.worldgen.zone.ZoneColorMapping;
import com.hypixel.hytale.server.worldgen.zone.ZoneDiscoveryConfig;
import it.unimi.dsi.fastutil.ints.Int2IntMap;
import it.unimi.dsi.fastutil.objects.Object2ObjectOpenHashMap;

import javax.annotation.Nullable;
import java.io.IOException;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Random;
import java.util.Set;
import java.util.TreeSet;

/**
 * Dumps the golden state of the real Java world-loader chain over a real
 * world folder ({@code World.json} / {@code Zones.json} / {@code Zones/**}),
 * for a fixed set of world seeds.
 *
 * <p>The loader chain is run <em>step-by-step</em> (not through
 * {@code ChunkGeneratorJsonLoader.load()}), because the full boot drags in the
 * asset-pack {@code AssetFileSystem} and the world-gen {@code EventHandler}
 * module path, which are both out of scope for the loader golden data:
 * <ul>
 *     <li>there are no world-gen modifier events in a plain data folder, so
 *     the (static, global) {@code ModifyEvent.dispatch} calls are no-ops -
 *     the scoped handler simply stays empty;</li>
 *     <li>the default {@code FileIOSystem} is pointed at the world folder with
 *     {@link FileIO#setDefaultRoot(Path)}, exactly like the vanilla server
 *     points it at the world directory.</li>
 * </ul>
 *
 * <p>Two deliberate seams are recorded in the dump instead of being resolved:
 * <ul>
 *     <li><b>phase 4 (caves)</b>: the {@code CaveGenerator} internals (the
 *     cave node type trees) are not constructed; only the type names and the
 *     {@code EntryPoints} grid seed offsets of the top-level {@code Caves.json}
 *     {@code Types} array are dumped. The seed offsets are all the Rust port
 *     has to agree on until the cave slice lands.</li>
 *     <li><b>phase 5 (prefabs)</b>: the biome {@code Prefabs} containers
 *     require the real prefab store, so {@code loadPrefabContainer} is stubbed
 *     to {@code null} on the Java side (probe subclasses) and mirrored by the
 *     Rust test ({@code "seam": "phase-5"} in the dump).</li>
 * </ul>
 *
 * <p>The dump is deterministic: floating-point values are written as
 * {@link Double#toHexString(double)} / {@link Float#toHexString(float)} hex
 * strings, and set contents with unspecified iteration order are sorted
 * before writing.
 */
final class LoaderDumper {

    /** The world seeds the loader chain is run for. */
    private static final int[] WORLD_SEEDS = {0, 1, 42, 1234567};

    /** The default / JSON-suffix mask names of the World.json Masks key. */
    private static final String MASK_PNG = "Mask.png";
    private static final String MASK_JSON = "Mask.json";

    /**
     * The (x, z) int coordinates the biome pattern generator is sampled at,
     * with the world seed as the runtime seed.
     * Must stay in sync with the Rust golden test.
     */
    private static final int[][] BIOME_SAMPLE_COORDS = {
        {0, 0}, {1, 0}, {-1, 1}, {7, 3}, {-248, -616}, {1664, 1024}, {12345, -54321}, {-100000, 99999}
    };

    /**
     * The (x, z) int coordinates containers (layers, covers, tint, environment
     * and water) are evaluated at, with {@link #CONTAINER_SAMPLE_SEED} as the
     * runtime seed. Must stay in sync with the Rust golden test.
     */
    private static final int[][] CONTAINER_SAMPLE_COORDS = {{0, 0}, {7, 3}, {-5, 9}};

    /** The fixed pick values the tile biome weighted maps are sampled at. */
    private static final double[] PICK_VALUES = {0.0, 0.25, 0.5, 0.75, 1.0};

    /** The runtime seed all container evaluations use (containers are dumped once). */
    private static final int CONTAINER_SAMPLE_SEED = 0;

    /** The seed used to draw the weighted cover part picks. */
    private static final int COVER_PICK_SEED = 0;

    /**
     * The real stderr. The CoreServer boot replaces {@code System.err} with a
     * logger stream, so the original one is kept for fatal error reports.
     */
    private static final java.io.PrintStream REAL_ERR = System.err;

    /** The stubbed {@link SeedStringResource} handed to every loader. */
    private static SeedStringResource resource;

    /** The world folder the loader chain runs over (equals FileIO's default root). */
    private static Path folder;

    /** The resolved mask provider map of World.json (shared by every seed). */
    private static IWeightedMap<String> maskMap;

    private LoaderDumper() {
    }

    // ------------------------------------------------------------------
    // Entry point
    // ------------------------------------------------------------------

    /**
     * Runs the loader chain over the world folder and returns the dump.
     *
     * @param worldDir the world directory (holds World.json / Zones.json / Zones/)
     * @param repoRoot the repository root (the asset pack is resolved relative
     *                 to it - the container loaders need the real block /
     *                 fluid / environment registries)
     */
    static String dumpLoader(Path worldDir, Path repoRoot) throws IOException {
        // The container loaders resolve blocks / fluids / environments through
        // the real asset registries, so the CoreServer asset pipeline has to be
        // booted first (RegistryDumper caches the boot after the first call).
        RegistryDumper.boot(repoRoot);
        folder = worldDir.toAbsolutePath().normalize();
        FileIO.setDefaultRoot(folder);
        resource = createResource();

        final JsonObject worldJson = loadJson(folder.resolve("World.json"), "World.json");
        final JsonObject zonesJson = getOrLoadJson(
            loadJson(folder.resolve("Zones.json"), "Zones.json"));
        maskMap = buildMaskMap(worldJson);

        final Phase2Dumper.JsonWriter w = new Phase2Dumper.JsonWriter();
        w.beginObject();

        writeMeta(w, folder);

        writeMaskPick(w, worldJson, folder);
        writeZoneRequirement(w, zonesJson, folder);
        // The seed-0 run carries the seed-independent sections (file contexts,
        // color mapping, zone summaries, caves) plus the full biome container
        // dumps, so it runs first and is reused for "seeds".0 / "biomes".
        final SeedRun seedZero = new SeedRun(0, zonesJson, true);
        writeFileContext(w, seedZero);
        writeColorMapping(w, seedZero);
        writeZones(w, seedZero);
        writeCaves(w, seedZero);

        w.name("seeds");
        w.beginObject();
        for (int seed : WORLD_SEEDS) {
            final SeedRun run = (seed == 0) ? seedZero : new SeedRun(seed, zonesJson, false);
            writeSeed(w, run);
        }
        w.endObject();

        w.name("biomes");
        w.beginObject();
        writeBiomeContainers(w, seedZero);
        w.endObject();

        w.endObject();
        return w.toString();
    }

    // ------------------------------------------------------------------
    // Meta
    // ------------------------------------------------------------------

    private static void writeMeta(Phase2Dumper.JsonWriter w, Path folder) {
        w.name("meta");
        w.beginObject();
        w.name("class").value("com.hypixel.hytale.server.worldgen.loader.ChunkGeneratorJsonLoader");
        w.name("format").value("ints/booleans are JSON numbers, doubles are Java Double.toHexString strings "
            + "(the discovery durations and none of the values are floats on this level except the discovery "
            + "durations, which are Java Float.toHexString strings), null is JSON null");
        w.name("worldDir").value(folder.getFileName().toString());
        w.name("fileIO").value("FileIO.setDefaultRoot(<worldDir>) points the default FileIOSystem at the world "
            + "folder; the loaders run step-by-step (no full ChunkGeneratorJsonLoader.load(), no AssetFileSystem "
            + "packs, no EventHandler modifiers - the scoped handler stays empty so ModifyEvent.dispatch no-ops)");
        w.name("stub").value("SeedStringResource is allocated with sun.misc.Unsafe.allocateInstance and filled with "
            + "empty FileMaskCache / BlockPlacementMaskRegistry / uniqueIds registries (config and prefab loader "
            + "stay null); getLoader() is never reached because the PrefabContainer load is stubbed to null");
        w.name("chain").value("SeedString(<seed>).Zones.Zone-<name>[.TileBiome-<name>|TileBiome-<name>]"
            + "[.BiomePatternGenerator|.CustomBiomeGenerator], SeedString(<seed>).ZonePatternGenerator, "
            + "SeedString(<seed>).Zones.Zone-<name>.CaveGenerator-<Name>.CaveType; SeedString.hashCode() = "
            + "seed.hashCode() * 114512143 (int overflow wrap)");
        w.name("customBiomeSeedQuirk").value("Custom. biome files append \"TileBiome-<name>\" (without a leading "
            + "dot) while Tile. biome files append \".TileBiome-<name>\"");
        w.name("biomeIds").value("zone/biome ids come from the real FileLoadingContext counters starting at -1; "
            + "only zones inside the zone requirement are scanned (skipped zones do not consume an id) and biome "
            + "ids are global across zones in file-context order (biome type ordinal, then asset path)");
        w.name("seeds").rawArray(joinInts(WORLD_SEEDS));
        w.name("biomeSampleSeed").value("getBiome/getBiomeDirect/getBiomeIndex samples of the \"seeds\" section use "
            + "the dumped world seed as the runtime seed");
        w.name("biomeSampleCoords").rawArray(coordPairs(BIOME_SAMPLE_COORDS));
        w.name("containerSampleSeed").value("the container values of the \"biomes\" section are evaluated with the "
            + "runtime seed " + CONTAINER_SAMPLE_SEED + "; the cover part picks use FastRandom(" + COVER_PICK_SEED
            + ") per cover entry");
        w.name("containerSampleCoords").rawArray(coordPairs(CONTAINER_SAMPLE_COORDS));
        w.name("pickValues").rawArray(joinHexes(PICK_VALUES));
        w.name("prefabSeam").value("the biome Prefabs containers are not loaded (phase-5 seam; they need the real "
            + "prefab store): loadPrefabContainer is stubbed to null, so every prefab-free value (weights, block "
            + "containers, tint, environment, water, fade, interpolation) is real but getExtents() contributes no "
            + "prefab extents and Zone1_Spawn / Zone1_Temple UniquePrefabs are skipped as well");
        w.name("caveSeam").value("CaveGenerator construction is phase-4 future work: only the cave type names of the "
            + "top-level Types array and the EntryPoints grid seed offsets are dumped. The seed offset is the "
            + "SeedString hash of \"<seed>.Zones.Zone-<name>.CaveGenerator-<Name>.CaveType\" (append(\".CaveGenerator\"), "
            + "append(\"-<Name>\"), append(\".CaveType\")), unless the EntryPoints grid overrides the Seed key, "
            + "in which case appendToOriginal(\"<Seed>\") resets both the original and the chain seed");
        w.name("deterministic").value("no timestamps are written; the dump is byte-identical across runs");
        w.name("javaVersion").value(System.getProperty("java.version"));
        w.endObject();
    }

    // ------------------------------------------------------------------
    // Mask pick (ChunkGeneratorJsonLoader.loadMaskProvider)
    // ------------------------------------------------------------------

    private static void writeMaskPick(Phase2Dumper.JsonWriter w, JsonObject worldJson, Path folder) {
        w.name("maskPick");
        w.beginObject();
        for (int seed : WORLD_SEEDS) {
            final SeedString<SeedStringResource> base = baseSeed(seed);
            final String maskName = maskMap.get(new FastRandom(base.hashCode()));
            final Path maskFile = PathUtil.resolvePathWithinDir(folder, maskName);
            if (maskFile == null) {
                throw new Error("Invalid mask file path: " + maskName);
            }
            final boolean climate = maskFile.getFileName().toString().endsWith(MASK_JSON);

            w.name(Integer.toString(seed));
            w.beginObject();
            w.name("maskName").value(maskName);
            w.name("kind").value(climate ? "Climate" : "Image");
            w.name("randomSeed").value(base.hashCode());
            w.name("mapSize").value(maskMap.size());
            w.endObject();
        }
        w.endObject();
    }

    /**
     * Builds the weighted mask map of World.json exactly like
     * {@code ChunkGeneratorJsonLoader.loadMaskProvider} (defaults to a
     * single "Mask.png" entry when no/empty Masks value is present).
     */
    private static IWeightedMap<String> buildMaskMap(JsonObject worldJson) {
        final WeightedMap.Builder<String> builder = WeightedMap.builder(new String[0]);
        final JsonElement masks = worldJson.get("Masks");
        if (masks == null) {
            builder.put(MASK_PNG, 1);
        } else if (masks.isJsonPrimitive()) {
            builder.put(masks.getAsString(), 1);
        } else if (masks.isJsonArray()) {
            final JsonArray arr = masks.getAsJsonArray();
            if (arr.isEmpty()) {
                builder.put(MASK_PNG, 1);
            } else {
                for (int i = 0; i < arr.size(); i++) {
                    builder.put(arr.get(i).getAsString(), 1);
                }
            }
        } else if (masks.isJsonObject()) {
            final JsonObject obj = masks.getAsJsonObject();
            if (obj.size() == 0) {
                builder.put(MASK_PNG, 1);
            } else {
                for (String key : obj.keySet()) {
                    builder.put(key, obj.get(key).getAsDouble());
                }
            }
        }
        return builder.build();
    }

    /** The FileIO-relative mask name a seed picks from the World.json mask map. */
    private static String maskNameFor(int seed) {
        return maskMap.get(new FastRandom(baseSeed(seed).hashCode()));
    }

    // ------------------------------------------------------------------
    // Zone requirement (ZonePatternProviderJsonLoader.loadZoneRequirement)
    // ------------------------------------------------------------------

    private static void writeZoneRequirement(Phase2Dumper.JsonWriter w, JsonObject zonesJson, Path folder) {
        final Set<String> requirement =
            new ZoneRequirementJsonLoader(baseSeed(0), folder, zonesJson).load();
        final Set<String> sorted = new TreeSet<>(requirement);
        w.name("zoneRequirement").rawArray(joinQuotes(sorted.toArray(new String[0])));
    }

    // ------------------------------------------------------------------
    // One seed run
    // ------------------------------------------------------------------

    /** One run of the loader chain for a single world seed. */
    private static final class SeedRun {
        final int seed;
        final SeedString<SeedStringResource> base;
        final JsonObject zonesRoot;
        /** The mask name the seed picks from the World.json Masks map. */
        final String maskName;
        /** The zone pattern grid (SeedString(<seed>).ZonePatternGenerator). */
        final int zonePatternGridSeedOffset;
        @Nullable final String zonePatternGridSeedKey;
        final List<ZoneInfo> zones = new ArrayList<>();

        SeedRun(int seed, JsonObject zonesJson, boolean fullContainers) {
            this.seed = seed;
            this.base = baseSeed(seed);
            this.zonesRoot = getOrLoadJson(zonesJson);

            this.maskName = maskNameFor(seed);

            final JsonElement grid = child(this.zonesRoot, "GridGenerator");
            final PointProbe gridProbe = new PointProbe(this.base.append(".ZonePatternGenerator"), folder, grid);
            this.zonePatternGridSeedOffset = gridProbe.pubSeed();
            this.zonePatternGridSeedKey = seedKey(grid);

            final FileLoadingContext context =
                new FileContextLoader("Default", folder, loadZoneRequirement(getOrLoadJson(zonesJson))).load();
            final SeedString<SeedStringResource> zonesSeed = base.append(".Zones");
            for (Map.Entry<String, ZoneFileContext> zoneEntry : context.getZones()) {
                zones.add(new ZoneInfo(seed, zonesSeed, zoneEntry.getValue(), fullContainers));
            }
        }
    }

    /** One loaded zone of a seed run. */
    private static final class ZoneInfo {
        /** The world seed this zone info belongs to (the biome sample seed). */
        final int runSeed;
        final ZoneFileContext zoneContext;
        final SeedString<SeedStringResource> zoneSeed;
        final SeedString<SeedStringResource> gridSeed;

        final String zoneName;
        final int zoneId;
        final ZoneDiscoveryConfig discovery;

        final IWeightedMap<TileBiome> tileBiomes;
        final CustomBiome[] customBiomes;
        final BiomePatternGenerator generator;
        /** The "BiomeGenerator" element of Zone.json (may be null). */
        @Nullable final JsonElement biomesRoot;
        @Nullable final String biomeGridSeedKey;
        final int biomeGridSeedOffset;

        /** Full-pass extras (only for the seed-0 run). */
        final List<LoadedBiome> loadedBiomes = new ArrayList<>();

        ZoneInfo(int runSeed, SeedString<SeedStringResource> zonesSeed, ZoneFileContext zoneContext, boolean fullContainers) {
            this.runSeed = runSeed;
            this.zoneContext = zoneContext;
            this.zoneName = zoneContext.getName();
            this.zoneId = zoneContext.getId();
            this.zoneSeed = zonesSeed.append(String.format(".Zone-%s", zoneName));
            this.gridSeed = this.zoneSeed.append(".BiomePatternGenerator");

            final JsonObject zoneJson = loadZoneJson(zoneContext);
            final ProbeZoneJsonLoader zoneLoader = new ProbeZoneJsonLoader(zonesSeed, folder, zoneJson, zoneContext);
            this.discovery = zoneLoader.pubDiscoveryConfig();
            this.biomesRoot = zoneLoader.get("BiomeGenerator");

            this.tileBiomes = loadTileBiomes(zoneSeed, zoneContext, fullContainers);
            this.customBiomes = loadCustomBiomes(zoneSeed, zoneContext, tileBiomes.toArray(), fullContainers);

            this.generator = new BiomePatternGeneratorJsonLoader(zoneSeed, folder,
                biomesRoot, tileBiomes, customBiomes).load();

            final JsonElement grid = child(biomesRoot, "GridGenerator");
            final PointProbe biomeGrid = new PointProbe(gridSeed, folder, grid);
            this.biomeGridSeedOffset = biomeGrid.pubSeed();
            this.biomeGridSeedKey = seedKey(grid);

            if (fullContainers) {
                // tile biomes in map order == file context order
                for (Biome biome : tileBiomes.internalKeys()) {
                    loadedBiomes.add(new LoadedBiome(biome, contextOf(biome, zoneContext, true),
                        prefabKey(biome, zoneContext, true)));
                }
                for (CustomBiome biome : customBiomes) {
                    loadedBiomes.add(new LoadedBiome(biome, contextOf(biome, zoneContext, false),
                        prefabKey(biome, zoneContext, false)));
                }
            }
        }
    }

    /** A fully loaded biome of the seed-0 run (containers real, prefab null). */
    private static final class LoadedBiome {
        final Biome biome;
        final BiomeFileContext context;
        final boolean prefabKey;

        LoadedBiome(Biome biome, BiomeFileContext context, boolean prefabKey) {
            this.biome = biome;
            this.context = context;
            this.prefabKey = prefabKey;
        }
    }

    private static BiomeFileContext contextOf(Biome biome, ZoneFileContext zoneContext, boolean tile) {
        final var registry = tile ? zoneContext.getTileBiomes() : zoneContext.getCustomBiomes();
        for (var entry : registry) {
            if (entry.getValue().getId() == biome.getId()) return entry.getValue();
        }
        throw new IllegalStateException("No file context for biome " + biome.getName() + " (" + biome.getId() + ")");
    }

    private static boolean prefabKey(Biome biome, ZoneFileContext zoneContext, boolean tile) {
        final var registry = tile ? zoneContext.getTileBiomes() : zoneContext.getCustomBiomes();
        for (var entry : registry) {
            if (entry.getValue().getId() == biome.getId()) {
                final JsonObject resolved = getOrLoadJson(loadJsonObject(entry.getValue().getPath()));
                return resolved != null && resolved.has("Prefabs");
            }
        }
        throw new IllegalStateException("No file context for biome " + biome.getName() + " (" + biome.getId() + ")");
    }

    /** Loads a JSON object asset (wrapping the checked IOException). */
    private static JsonObject loadJsonObject(Path file) {
        try {
            return FileIO.load(file, JsonLoader.JSON_OBJ_LOADER);
        } catch (Throwable e) {
            throw new Error("Failed to load " + file, e);
        }
    }

    private static IWeightedMap<TileBiome> loadTileBiomes(SeedString<SeedStringResource> zoneSeed,
                                                          ZoneFileContext zoneContext, boolean fullContainers) {
        final WeightedMap.Builder<TileBiome> builder = WeightedMap.builder(new TileBiome[0]);
        for (Map.Entry<String, BiomeFileContext> entry : zoneContext.getTileBiomes()) {
            final BiomeFileContext biomeContext = entry.getValue();
            final JsonObject biomeJson = loadJsonObject(biomeContext.getPath());
            final ProbeTileBiomeJsonLoader loader = new ProbeTileBiomeJsonLoader(
                zoneSeed, biomeJson, biomeContext);
            final TileBiome biome;
            if (fullContainers) {
                biome = loader.load();
            } else {
                biome = new TileBiome(biomeContext.getId(), biomeName(biomeContext),
                    null, null, null, null, null, null, null, null, null, null,
                    loader.pubWeight(), loader.pubSizeModifier(), loader.pubColor());
            }
            builder.put(biome, biome.getWeight());
        }
        if (builder.size() <= 0) {
            throw new IllegalArgumentException("Could not find any tile biomes for this zone!");
        }
        return builder.build();
    }

    private static CustomBiome[] loadCustomBiomes(SeedString<SeedStringResource> zoneSeed,
                                                  ZoneFileContext zoneContext, Biome[] tileBiomes,
                                                  boolean fullContainers) {
        final List<CustomBiome> biomes = new ArrayList<>();
        for (Map.Entry<String, BiomeFileContext> entry : zoneContext.getCustomBiomes()) {
            final BiomeFileContext biomeContext = entry.getValue();
            final JsonObject biomeJson = loadJsonObject(biomeContext.getPath());
            final ProbeCustomBiomeJsonLoader loader = new ProbeCustomBiomeJsonLoader(
                zoneSeed, biomeJson, biomeContext, tileBiomes);
            final CustomBiome biome;
            if (fullContainers) {
                biome = loader.load();
                if (biome.getCustomBiomeGenerator() == null) {
                    throw new NullPointerException(biomeContext.getPath().toAbsolutePath().toString());
                }
            } else {
                final CustomBiomeGenerator generator = loader.pubCustomBiomeGenerator();
                if (generator == null) {
                    throw new NullPointerException(biomeContext.getPath().toAbsolutePath().toString());
                }
                biome = new CustomBiome(biomeContext.getId(), biomeName(biomeContext),
                    null, generator, null, null, null, null, null, null, null, null, null,
                    loader.pubColor());
            }
            biomes.add(biome);
        }

        final CustomBiome[] array = biomes.toArray(new CustomBiome[0]);
        // The vanilla ZoneCustomBiomesJsonLoader sorts with Arrays.sort (stable)
        // by descending custom biome generator priority.
        Arrays.sort(array,
            (o1, o2) -> Integer.compare(o2.getCustomBiomeGenerator().getPriority(),
                o1.getCustomBiomeGenerator().getPriority()));
        return array;
    }

    /** The biome context name (the AssetPath name with the prefix/suffix already stripped). */
    private static String biomeName(BiomeFileContext context) {
        return context.getName();
    }

    private static JsonObject loadZoneJson(ZoneFileContext zoneContext) {
        try {
            return FileIO.load(zoneContext.getPath().resolve("Zone.json"), JsonLoader.JSON_OBJ_LOADER);
        } catch (Throwable e) {
            throw new Error("Failed to load zone file of " + zoneContext.getName(), e);
        }
    }

    // ------------------------------------------------------------------
    // Seed-independent sections (from the seed-0 run)
    // ------------------------------------------------------------------

    private static void writeFileContext(Phase2Dumper.JsonWriter w, SeedRun run) {
        w.name("fileContext");
        w.beginObject();
        w.name("name").value("Default");
        w.name("worldDir").value("Default");
        w.name("zones");
        w.beginArray();
        for (ZoneInfo info : run.zones) {
            w.beginObject();
            w.name("id").value(info.zoneId);
            w.name("name").value(info.zoneName);
            w.name("zoneSeedChain").value(info.zoneSeed.toString());

            w.name("tileBiomes");
            w.beginArray();
            for (Map.Entry<String, BiomeFileContext> e : info.zoneContext.getTileBiomes()) {
                w.rawObject("{\"id\":" + e.getValue().getId()
                    + ",\"name\":" + Phase2Dumper.JsonWriter.quoteString(e.getValue().getName()) + "}");
            }
            w.endArray();

            w.name("customBiomes");
            w.beginArray();
            for (Map.Entry<String, BiomeFileContext> e : info.zoneContext.getCustomBiomes()) {
                w.rawObject("{\"id\":" + e.getValue().getId()
                    + ",\"name\":" + Phase2Dumper.JsonWriter.quoteString(e.getValue().getName()) + "}");
            }
            w.endArray();

            w.endObject();
        }
        w.endArray();
        w.endObject();
    }

    private static void writeColorMapping(Phase2Dumper.JsonWriter w, SeedRun run) {
        // The zones resolved by the real ZoneColorMappingJsonLoader; the Zone
        // instances themselves only carry the id/name the mapping is compared over.
        final Map<String, Zone> zoneLookup = new HashMap<>();
        for (ZoneInfo info : run.zones) {
            zoneLookup.put(info.zoneName, new Zone(info.zoneId, info.zoneName, ZoneDiscoveryConfig.DEFAULT,
                null, null, null));
        }

        final JsonElement maskMapping = child(run.zonesRoot, "MaskMapping");
        if (maskMapping == null) {
            throw new IllegalArgumentException("Could not find mappings for colors in mask file. Keyword: MaskMapping");
        }
        final ZoneColorMapping mapping =
            new ZoneColorMappingJsonLoader(run.base, folder, maskMapping, zoneLookup).load();

        w.name("colorMapping");
        w.beginArray();
        // The real loader walks the MaskMapping object in JSON file order.
        for (Map.Entry<String, JsonElement> entry : maskMapping.getAsJsonObject().entrySet()) {
            final int rgb = ColorUtil.hexString(entry.getKey());
            w.beginObject();
            w.name("key").value(entry.getKey());
            w.name("rgb").value(rgb);
            w.name("zones");
            final Zone[] zones = mapping.get(rgb);
            if (zones == null) {
                throw new IllegalStateException("Missing color mapping for " + entry.getKey());
            }
            final String[] names = new String[zones.length];
            for (int i = 0; i < zones.length; i++) {
                names[i] = zones[i].name();
            }
            w.rawArray(joinQuotes(names));
            w.endObject();
        }
        w.endArray();
    }

    private static void writeZones(Phase2Dumper.JsonWriter w, SeedRun run) {
        w.name("zones");
        w.beginObject();
        for (ZoneInfo info : run.zones) {
            w.name(info.zoneName);
            w.beginObject();
            w.name("id").value(info.zoneId);
            writeDiscovery(w, info.discovery);

            w.name("tileBiomes");
            w.beginArray();
            for (TileBiome biome : info.tileBiomes.internalKeys()) {
                w.rawObject("{\"id\":" + biome.getId()
                    + ",\"name\":" + Phase2Dumper.JsonWriter.quoteString(biome.getName())
                    + ",\"weight\":" + Phase2Dumper.JsonWriter.quoteString(hex(biome.getWeight()))
                    + ",\"sizeModifier\":" + Phase2Dumper.JsonWriter.quoteString(hex(biome.getSizeModifier()))
                    + ",\"mapColor\":" + biome.getMapColor() + "}");
            }
            w.endArray();

            w.name("tileBiomePicks");
            w.beginArray();
            for (double pick : PICK_VALUES) {
                final TileBiome picked = info.tileBiomes.get(pick);
                w.rawObject(picked == null
                    ? "null"
                    : Phase2Dumper.JsonWriter.quoteString(picked.getName()));
            }
            w.endArray();

            final String[] customNames = new String[info.customBiomes.length];
            for (int i = 0; i < customNames.length; i++) {
                customNames[i] = info.customBiomes[i].getName();
            }
            w.name("customBiomes").rawArray(joinQuotes(customNames));

            w.endObject();
        }
        w.endObject();
    }

    private static void writeDiscovery(Phase2Dumper.JsonWriter w, ZoneDiscoveryConfig discovery) {
        w.name("discovery");
        w.beginObject();
        w.name("display").value(discovery.display());
        if (discovery.zone() == null) {
            w.name("zone").rawObject("null");
        } else {
            w.name("zone").value(discovery.zone());
        }
        if (discovery.soundEventId() == null) {
            w.name("soundEventId").rawObject("null");
        } else {
            w.name("soundEventId").value(discovery.soundEventId());
        }
        if (discovery.icon() == null) {
            w.name("icon").rawObject("null");
        } else {
            w.name("icon").value(discovery.icon());
        }
        w.name("major").value(discovery.major());
        w.name("duration").rawObject(Phase2Dumper.JsonWriter.quoteString(hex(discovery.duration())));
        w.name("fadeInDuration").rawObject(Phase2Dumper.JsonWriter.quoteString(hex(discovery.fadeInDuration())));
        w.name("fadeOutDuration").rawObject(Phase2Dumper.JsonWriter.quoteString(hex(discovery.fadeOutDuration())));
        w.endObject();
    }

    // ------------------------------------------------------------------
    // Caves (phase-4 seam: names + EntryPoints seed offsets only)
    // ------------------------------------------------------------------

    private static void writeCaves(Phase2Dumper.JsonWriter w, SeedRun run) {
        w.name("caves");
        w.beginObject();
        for (ZoneInfo info : run.zones) {
            w.name(info.zoneName);
            final Path caveFolder = info.zoneContext.getPath().resolve("Cave");
            final com.hypixel.hytale.procedurallib.file.AssetPath cavesPath =
                FileIO.resolve(caveFolder.resolve("Caves.json"));
            if (!FileIO.exists(cavesPath)) {
                w.beginObject();
                w.name("present").value(false);
                w.name("types").beginArray().endArray();
                w.endObject();
                continue;
            }
            final JsonObject cavesJson;
            try {
                cavesJson = FileIO.load(cavesPath, JsonLoader.JSON_OBJ_LOADER);
            } catch (IOException e) {
                throw new Error("Error while loading caves for world generator from " + cavesPath, e);
            }

            w.beginObject();
            w.name("present").value(true);
            w.name("types");
            w.beginArray();
            final JsonElement types = cavesJson.get("Types");
            if (types == null || !types.isJsonArray()) {
                throw new IllegalArgumentException("CaveTypes must be a JSON array.");
            }
            final JsonArray typesArray = types.getAsJsonArray();
            for (int i = 0; i < typesArray.size(); i++) {
                final JsonElement entry = getOrLoadJson(typesArray.get(i));
                if (entry == null || !entry.isJsonObject()) {
                    throw new Error("Expected CaveType entry to be a JsonObject at index: " + i);
                }
                final JsonObject typeJson = entry.getAsJsonObject();
                final JsonElement name = typeJson.get("Name");
                if (name == null) {
                    throw new Error("CaveType is missing the 'Name' property");
                }
                if (!name.isJsonPrimitive() || !name.getAsJsonPrimitive().isString()) {
                    throw new Error("CaveType 'Name' property is not a string");
                }
                final String nameString = name.getAsString();
                final JsonElement entryPoints = child(typeJson, "EntryPoints");
                if (rawChild(typeJson, "EntryPoints") == null) {
                    throw new IllegalArgumentException("\"EntryPoints\" is not defined, no spawn information for caves available");
                }

                // <seed>.Zones.Zone-<name>.CaveGenerator-<Name>.CaveType
                final SeedString<SeedStringResource> caveSeed = info.zoneSeed
                    .append(".CaveGenerator")
                    .append(String.format("-%s", nameString))
                    .append(".CaveType");
                final PointProbe probe = new PointProbe(caveSeed, folder, entryPoints);

                w.beginObject();
                w.name("name").value(nameString);
                w.name("entryPoints");
                w.beginObject();
                final String seedKey = seedKey(entryPoints);
                if (seedKey == null) {
                    w.name("seedKey").rawObject("null");
                } else {
                    w.name("seedKey").value(seedKey);
                }
                w.name("seedOffsets");
                w.beginObject();
                for (int seed : WORLD_SEEDS) {
                    // The offset depends on the world seed only through the
                    // SeedString base, so each offset needs its own chain.
                    final SeedString<SeedStringResource> seedChain = baseSeed(seed)
                        .append(String.format(".Zones.Zone-%s", info.zoneName))
                        .append(".CaveGenerator")
                        .append(String.format("-%s", nameString))
                        .append(".CaveType");
                    final PointProbe seedProbe =
                        new PointProbe(seedChain, folder, entryPoints);
                    w.name(Integer.toString(seed)).value(seedProbe.pubSeed());
                }
                w.endObject();
                w.endObject();
                w.endObject();
            }
            w.endArray();
            w.endObject();
        }
        w.endObject();
    }

    // ------------------------------------------------------------------
    // Seeds section
    // ------------------------------------------------------------------

    private static void writeSeed(Phase2Dumper.JsonWriter w, SeedRun run) {
        w.name(Integer.toString(run.seed));
        w.beginObject();
        w.name("seedString").value(run.base.toString());
        w.name("seedHash").value(run.base.hashCode());

        w.name("zonePatternGrid");
        w.beginObject();
        w.name("seedOffset").value(run.zonePatternGridSeedOffset);
        if (run.zonePatternGridSeedKey == null) {
            w.name("seedKey").rawObject("null");
        } else {
            w.name("seedKey").value(run.zonePatternGridSeedKey);
        }
        w.endObject();

        w.name("maskPick").value(run.maskName);
        if (run.maskName != null && run.maskName.endsWith(MASK_JSON)) {
            w.name("maskKind").value("Climate");
        } else {
            w.name("maskKind").value("Image");
        }

        w.name("zones");
        w.beginObject();
        for (ZoneInfo info : run.zones) {
            w.name(info.zoneName);
            w.beginObject();

            w.name("biomePatternGrid");
            w.beginObject();
            w.name("seedOffset").value(info.biomeGridSeedOffset);
            if (info.biomeGridSeedKey == null) {
                w.name("seedKey").rawObject("null");
            } else {
                w.name("seedKey").value(info.biomeGridSeedKey);
            }
            w.name("extents").value(info.generator.getExtents());
            w.endObject();

            w.name("customBiomes").rawArray(joinQuotes(customNames(info)));
            writeBiomeSamples(w, info);
            w.endObject();
        }
        w.endObject();

        w.endObject();
    }

    /** The custom biome names (priority-sorted) of a zone. */
    private static String[] customNames(ZoneInfo info) {
        final String[] names = new String[info.customBiomes.length];
        for (int i = 0; i < names.length; i++) {
            names[i] = info.customBiomes[i].getName();
        }
        return names;
    }

    private static void writeBiomeSamples(Phase2Dumper.JsonWriter w, ZoneInfo info) {
        final int seed = info.runSeed;
        final BiomePatternGenerator generator = info.generator;
        final ProbeBiomePatternGenerator probe = new ProbeBiomePatternGenerator(
            (IPointGenerator) field(generator, "pointGenerator"), info.tileBiomes, info.customBiomes);

        w.name("biomeIndex");
        final StringBuilder indices = new StringBuilder();
        final StringBuilder biomes = new StringBuilder();
        final StringBuilder directs = new StringBuilder();
        for (int[] coord : BIOME_SAMPLE_COORDS) {
            final int x = coord[0], z = coord[1];
            if (indices.length() > 0) indices.append(',');
            indices.append("{\"x\":").append(x).append(",\"z\":").append(z)
                .append(",\"v\":").append(Phase2Dumper.JsonWriter.quoteString(hex(probe.pubBiomeIndex(seed, x, z))))
                .append('}');

            final TileBiome biome = generator.getBiome(seed, x, z);
            if (biomes.length() > 0) biomes.append(',');
            biomes.append("{\"x\":").append(x).append(",\"z\":").append(z)
                .append(",\"v\":").append(biome == null
                    ? "null"
                    : Phase2Dumper.JsonWriter.quoteString(biome.getName()))
                .append('}');

            final TileBiome direct = generator.getBiomeDirect(seed, x, z);
            if (directs.length() > 0) directs.append(',');
            directs.append("{\"x\":").append(x).append(",\"z\":").append(z)
                .append(",\"v\":").append(direct == null
                    ? "null"
                    : Phase2Dumper.JsonWriter.quoteString(direct.getName()))
                .append('}');
        }
        w.rawArray(indices.toString());
        w.name("biome").rawArray(biomes.toString());
        w.name("biomeDirect").rawArray(directs.toString());
    }

    // ------------------------------------------------------------------
    // Biome containers (seed-0 full chain)
    // ------------------------------------------------------------------

    private static void writeBiomeContainers(Phase2Dumper.JsonWriter w, SeedRun run) {
        for (ZoneInfo info : run.zones) {
            for (LoadedBiome loaded : info.loadedBiomes) {
                final Biome biome = loaded.biome;
                w.name(Integer.toString(biome.getId()));
                w.beginObject();
                w.name("id").value(biome.getId());
                w.name("name").value(biome.getName());
                w.name("type").value(biome instanceof TileBiome ? "Tile" : "Custom");
                w.name("zone").value(info.zoneName);
                if (biome instanceof TileBiome tileBiome) {
                    w.name("weight").rawObject(Phase2Dumper.JsonWriter.quoteString(hex(tileBiome.getWeight())));
                    w.name("sizeModifier").rawObject(Phase2Dumper.JsonWriter.quoteString(hex(tileBiome.getSizeModifier())));
                }
                w.name("mapColor").value(biome.getMapColor());
                w.name("prefabContainer");
                w.beginObject();
                w.name("present").value(loaded.prefabKey);
                w.name("seam").value("phase-5");
                w.endObject();
                w.name("heightmapNoisePresent").value(biome.getHeightmapNoise() != null);

                writeInterpolation(w, biome.getInterpolation());
                writeLayerContainer(w, biome.getLayerContainer());
                writeCoverContainer(w, biome.getCoverContainer());
                writeTintContainer(w, biome.getTintContainer());
                writeEnvironmentContainer(w, biome.getEnvironmentContainer());
                writeWaterContainer(w, biome.getWaterContainer());
                writeFadeContainer(w, biome.getFadeContainer());

                w.endObject();
            }
        }
    }

    private static void writeInterpolation(Phase2Dumper.JsonWriter w, BiomeInterpolation interpolation) {
        w.name("interpolation");
        w.beginObject();
        w.name("radius").value(interpolation.getRadius());
        final Int2IntMap radii = (Int2IntMap) field(interpolation, "biomeRadii2");
        final List<Int2IntMap.Entry> entries = new ArrayList<>(radii.int2IntEntrySet());
        entries.sort((o1, o2) -> Integer.compare(o1.getIntKey(), o2.getIntKey()));
        final StringBuilder sb = new StringBuilder();
        for (Int2IntMap.Entry entry : entries) {
            if (sb.length() > 0) sb.append(',');
            sb.append('[').append(entry.getIntKey()).append(',').append(entry.getIntValue()).append(']');
        }
        w.name("biomeRadii2").rawArray(sb.toString());
        w.endObject();
    }

    private static void writeLayerContainer(Phase2Dumper.JsonWriter w, LayerContainer container) {
        w.name("layers");
        w.beginObject();
        w.name("filling").rawObject("{" + blockTriple(container.getFilling()) + "}");
        w.name("fillingEnvironment").value(container.getFillingEnvironment());

        w.name("topBlockAt");
        w.beginArray();
        for (int[] coord : CONTAINER_SAMPLE_COORDS) {
            w.rawObject(coordKey(coord) + blockTriple(container.getTopBlockAt(CONTAINER_SAMPLE_SEED, coord[0], coord[1])) + "}");
        }
        w.endArray();

        w.name("staticLayers");
        w.beginArray();
        for (LayerContainer.StaticLayer layer : container.getStaticLayers()) {
            w.beginObject();
            w.name("environmentId").value(layer.getEnvironmentId());
            final LayerContainer.StaticLayerEntry[] entries =
                (LayerContainer.StaticLayerEntry[]) field(layer, "entries");
            w.name("entries");
            w.beginArray();
            for (LayerContainer.StaticLayerEntry entry : entries) {
                w.beginObject();
                writeNoiseBlockArrayEntries(w, entry.getBlockArray());
                w.name("minInt").rawArray(joinCoords(entry, true, true));
                w.name("maxInt").rawArray(joinCoords(entry, false, true));
                w.name("minValue").rawArray(joinCoordsHex(entry, true, true));
                w.name("maxValue").rawArray(joinCoordsHex(entry, false, true));
                w.endObject();
            }
            w.endArray();
            w.endObject();
        }
        w.endArray();

        w.name("dynamicLayers");
        w.beginArray();
        for (LayerContainer.DynamicLayer layer : container.getDynamicLayers()) {
            w.beginObject();
            w.name("environmentId").value(layer.getEnvironmentId());
            final LayerContainer.DynamicLayerEntry[] entries =
                (LayerContainer.DynamicLayerEntry[]) field(layer, "entries");
            w.name("entries");
            w.beginArray();
            for (LayerContainer.DynamicLayerEntry entry : entries) {
                w.beginObject();
                writeNoiseBlockArrayEntries(w, entry.getBlockArray());
                w.endObject();
            }
            w.endArray();
            final int[] offsets = new int[CONTAINER_SAMPLE_COORDS.length];
            for (int i = 0; i < offsets.length; i++) {
                offsets[i] = layer.getOffset(CONTAINER_SAMPLE_SEED, CONTAINER_SAMPLE_COORDS[i][0], CONTAINER_SAMPLE_COORDS[i][1]);
            }
            w.name("offset").rawArray(joinInts(offsets));
            w.endObject();
        }
        w.endArray();

        w.endObject();
    }

    /** blockId/rotation/fluidId at the sample coordinates of a noise block array. */
    private static void writeNoiseBlockArrayEntries(Phase2Dumper.JsonWriter w, NoiseBlockArray blockArray) {
        final NoiseBlockArray.Entry[] entries = blockArray.getEntries();
        w.name("repetitions");
        w.beginArray();
        for (NoiseBlockArray.Entry entry : entries) {
            final int[] values = new int[CONTAINER_SAMPLE_COORDS.length];
            for (int i = 0; i < values.length; i++) {
                values[i] = entry.getRepetitions(CONTAINER_SAMPLE_SEED,
                    CONTAINER_SAMPLE_COORDS[i][0], CONTAINER_SAMPLE_COORDS[i][1]);
            }
            w.rawArray(joinInts(values));
        }
        w.endArray();
    }

    private static String joinCoords(LayerContainer.StaticLayerEntry entry, boolean min, boolean ints) {
        final StringBuilder sb = new StringBuilder();
        for (int[] coord : CONTAINER_SAMPLE_COORDS) {
            if (sb.length() > 0) sb.append(',');
            // The branches are kept apart: a ternary over an `int` and a
            // `double` expression would promote the integers to doubles.
            if (ints) {
                sb.append(min
                    ? entry.getMinInt(CONTAINER_SAMPLE_SEED, coord[0], coord[1])
                    : entry.getMaxInt(CONTAINER_SAMPLE_SEED, coord[0], coord[1]));
            } else {
                sb.append(min
                    ? entry.getMinValue(CONTAINER_SAMPLE_SEED, coord[0], coord[1])
                    : entry.getMaxValue(CONTAINER_SAMPLE_SEED, coord[0], coord[1]));
            }
        }
        return sb.toString();
    }

    private static String joinCoordsHex(LayerContainer.StaticLayerEntry entry, boolean min, boolean ints) {
        final StringBuilder sb = new StringBuilder();
        for (int[] coord : CONTAINER_SAMPLE_COORDS) {
            if (sb.length() > 0) sb.append(',');
            final double raw;
            if (min) {
                raw = entry.getMinValue(CONTAINER_SAMPLE_SEED, coord[0], coord[1]);
            } else {
                raw = entry.getMaxValue(CONTAINER_SAMPLE_SEED, coord[0], coord[1]);
            }
            sb.append(Phase2Dumper.JsonWriter.quoteString(hex(raw)));
        }
        return sb.toString();
    }

    private static void writeCoverContainer(Phase2Dumper.JsonWriter w, CoverContainer container) {
        w.name("covers");
        w.beginArray();
        for (CoverContainer.CoverContainerEntry entry : container.getEntries()) {
            w.beginObject();
            w.name("density").rawObject(Phase2Dumper.JsonWriter.quoteString(hex(entry.getCoverDensity())));
            w.name("onWater").value(entry.isOnWater());

            final IWeightedMap<CoverContainer.CoverContainerEntry.CoverContainerEntryPart> parts =
                (IWeightedMap<CoverContainer.CoverContainerEntry.CoverContainerEntryPart>)
                    field(entry, "entries");

            w.name("parts");
            w.beginArray();
            parts.forEachEntry((part, weight) -> {
                final BlockFluidEntry block = part.getEntry();
                w.rawObject("{\"weight\":" + Phase2Dumper.JsonWriter.quoteString(hex(weight))
                    + "," + blockTriple(block) + ",\"offset\":" + part.getOffset() + "}");
            });
            w.endArray();

            final CoverContainer.CoverContainerEntry.CoverContainerEntryPart picked =
                parts.get(new FastRandom(COVER_PICK_SEED));
            if (picked == null) {
                w.name("picked").rawObject("null");
            } else {
                w.name("picked")
                    .rawObject("{" + blockTriple(picked.getEntry())
                        + ",\"offset\":" + picked.getOffset() + "}");
            }

            w.endObject();
        }
        w.endArray();
    }

    private static void writeTintContainer(Phase2Dumper.JsonWriter w, TintContainer container) {
        w.name("tint");
        w.beginObject();
        final TintContainer.DefaultTintContainerEntry defaultEntry =
            (TintContainer.DefaultTintContainerEntry) field(container, "defaultEntry");
        w.name("default");
        w.beginObject();
        w.name("colors").rawArray(weightedColors(field(defaultEntry, "colorMapping")));
        w.endObject();

        @SuppressWarnings("unchecked")
        final List<TintContainer.TintContainerEntry> entries = (List<TintContainer.TintContainerEntry>) field(container, "entries");
        w.name("entryCount").value(entries.size());

        w.name("tintColorAt");
        w.beginArray();
        for (int[] coord : CONTAINER_SAMPLE_COORDS) {
            w.rawObject(coordKey(coord) + "\"v\":" + container.getTintColorAt(CONTAINER_SAMPLE_SEED, coord[0], coord[1]) + "}");
        }
        w.endArray();
        w.endObject();
    }

    private static void writeEnvironmentContainer(Phase2Dumper.JsonWriter w, EnvironmentContainer container) {
        w.name("environment");
        w.beginObject();
        final EnvironmentContainer.DefaultEnvironmentContainerEntry defaultEntry =
            (EnvironmentContainer.DefaultEnvironmentContainerEntry) field(container, "defaultEntry");
        w.name("default");
        w.beginObject();
        w.name("environments").rawArray(weightedColors(field(defaultEntry, "environmentMapping")));
        w.endObject();

        final EnvironmentContainer.EnvironmentContainerEntry[] entries =
            (EnvironmentContainer.EnvironmentContainerEntry[]) field(container, "entries");
        w.name("entryCount").value(entries.length);
        w.name("environmentAt");
        w.beginArray();
        for (int[] coord : CONTAINER_SAMPLE_COORDS) {
            w.rawObject(coordKey(coord) + "\"v\":" + container.getEnvironmentAt(CONTAINER_SAMPLE_SEED, coord[0], coord[1]) + "}");
        }
        w.endArray();
        w.endObject();
    }

    /** [{"c":<color>,"w":<weight hex>}, ...] in weighted map order. */
    private static String weightedColors(Object mappingRaw) {
        @SuppressWarnings("unchecked")
        final IWeightedMap<Integer> mapping = (IWeightedMap<Integer>) mappingRaw;
        final StringBuilder sb = new StringBuilder();
        mapping.forEachEntry((color, weight) -> {
            if (sb.length() > 0) sb.append(',');
            sb.append("{\"c\":").append(color)
                .append(",\"w\":").append(Phase2Dumper.JsonWriter.quoteString(hex(weight))).append('}');
        });
        return sb.toString();
    }

    private static void writeWaterContainer(Phase2Dumper.JsonWriter w, WaterContainer container) {
        w.name("water");
        w.beginObject();
        w.name("hasEntries").value(container.hasEntries());
        w.name("entries");
        w.beginArray();
        for (WaterContainer.Entry entry : container.getEntries()) {
            w.beginObject();
            w.name("blockId").value(entry.getBlock());
            w.name("fluidId").value(entry.getFluid());
            final int[] mins = new int[CONTAINER_SAMPLE_COORDS.length];
            final int[] maxs = new int[CONTAINER_SAMPLE_COORDS.length];
            for (int i = 0; i < mins.length; i++) {
                mins[i] = entry.getMin(CONTAINER_SAMPLE_SEED, CONTAINER_SAMPLE_COORDS[i][0], CONTAINER_SAMPLE_COORDS[i][1]);
                maxs[i] = entry.getMax(CONTAINER_SAMPLE_SEED, CONTAINER_SAMPLE_COORDS[i][0], CONTAINER_SAMPLE_COORDS[i][1]);
            }
            w.name("min").rawArray(joinInts(mins));
            w.name("max").rawArray(joinInts(maxs));
            w.endObject();
        }
        w.endArray();
        w.name("maxHeight");
        w.beginArray();
        for (int[] coord : CONTAINER_SAMPLE_COORDS) {
            w.rawObject(coordKey(coord) + "\"v\":" + container.getMaxHeight(CONTAINER_SAMPLE_SEED, coord[0], coord[1]) + "}");
        }
        w.endArray();
        w.endObject();
    }

    private static void writeFadeContainer(Phase2Dumper.JsonWriter w, FadeContainer container) {
        w.name("fade");
        w.beginObject();
        w.name("maskFadeStart").rawObject(Phase2Dumper.JsonWriter.quoteString(hex(container.getMaskFadeStart())));
        w.name("maskFadeLength").rawObject(Phase2Dumper.JsonWriter.quoteString(hex(container.getMaskFadeLength())));
        w.name("maskFadeSum").rawObject(Phase2Dumper.JsonWriter.quoteString(hex(container.getMaskFadeSum())));
        w.name("heightFadeStart").rawObject(Phase2Dumper.JsonWriter.quoteString(hex(container.getHeightFadeStart())));
        w.name("heightFadeLength").rawObject(Phase2Dumper.JsonWriter.quoteString(hex(container.getHeightFadeLength())));
        w.name("heightFadeSum").rawObject(Phase2Dumper.JsonWriter.quoteString(hex(container.getHeightFadeSum())));
        w.name("fadeHeightmap").rawObject(Phase2Dumper.JsonWriter.quoteString(hex(container.getFadeHeightmap())));
        w.endObject();
    }

    // ------------------------------------------------------------------
    // Probe classes (expose protected loader behavior)
    // ------------------------------------------------------------------

    /** Exposes the protected ZoneJsonLoader.loadDiscoveryConfig(). */
    private static final class ProbeZoneJsonLoader extends ZoneJsonLoader {
        ProbeZoneJsonLoader(SeedString<SeedStringResource> seed, Path dataFolder, JsonElement json, ZoneFileContext zoneContext) {
            super(seed, dataFolder, json, zoneContext);
        }

        ZoneDiscoveryConfig pubDiscoveryConfig() {
            return loadDiscoveryConfig();
        }
    }

    /** Exposes the protected TileBiomeJsonLoader weight / size modifier / color
      * getters and stubs the PrefabContainer load (phase-5 seam).
      */
    private static final class ProbeTileBiomeJsonLoader extends TileBiomeJsonLoader {
        ProbeTileBiomeJsonLoader(SeedString<SeedStringResource> seed, JsonObject json, BiomeFileContext biomeContext) {
            super(seed, folder, json, biomeContext);
        }

        double pubWeight() {
            return loadWeight();
        }

        double pubSizeModifier() {
            return loadSizeModifier();
        }

        int pubColor() {
            return loadColor();
        }

        @Nullable
        @Override
        protected PrefabContainer loadPrefabContainer() {
            return null;
        }
    }

    /**
     * Exposes the protected CustomBiomeJsonLoader generator / color getters and
     * stubs the PrefabContainer load (phase-5 seam).
     */
    private static final class ProbeCustomBiomeJsonLoader extends CustomBiomeJsonLoader {
        ProbeCustomBiomeJsonLoader(SeedString<SeedStringResource> seed, JsonObject json, BiomeFileContext biomeContext, Biome[] tileBiomes) {
            super(seed, folder, json, biomeContext, tileBiomes);
        }

        @Nullable
        CustomBiomeGenerator pubCustomBiomeGenerator() {
            return loadCustomBiomeGenerator();
        }

        int pubColor() {
            return loadColor();
        }

        @Nullable
        @Override
        protected PrefabContainer loadPrefabContainer() {
            return null;
        }
    }

    /** Exposes the protected PointGeneratorJsonLoader.loadSeed(). */
    private static final class PointProbe extends PointGeneratorJsonLoader<SeedStringResource> {
        PointProbe(SeedString<SeedStringResource> seed, Path dataFolder, JsonElement json) {
            super(seed, dataFolder, json);
        }

        int pubSeed() {
            return loadSeed();
        }
    }

    /** Exposes the protected BiomePatternGenerator.getBiomeIndex(). */
    private static final class ProbeBiomePatternGenerator extends BiomePatternGenerator {
        ProbeBiomePatternGenerator(IPointGenerator pointGenerator, IWeightedMap<TileBiome> tileBiomes, CustomBiome[] customBiomes) {
            super(pointGenerator, tileBiomes, customBiomes);
        }

        double pubBiomeIndex(int seed, int x, int z) {
            return getBiomeIndex(seed, x, z);
        }
    }

    // ------------------------------------------------------------------
    // Seed strings / helpers
    // ------------------------------------------------------------------

    private static SeedString<SeedStringResource> baseSeed(int seed) {
        return new SeedString<>(Integer.toString(seed), resource);
    }

    private static SeedStringResource createResource() {
        try {
            final SeedStringResource stub = (SeedStringResource) allocateInstance(SeedStringResource.class);
            setField(SeedStringResource.class, stub, "biomeMaskRegistry", new FileMaskCache<>());
            setField(SeedStringResource.class, stub, "blockMaskRegistry", new BlockPlacementMaskRegistry());
            setField(SeedStringResource.class, stub, "uniqueIds", new Object2ObjectOpenHashMap<>());
            return stub;
        } catch (Exception e) {
            throw new Error("Failed to create the SeedStringResource stub", e);
        }
    }

    private static Set<String> loadZoneRequirement(JsonObject zonesJson) {
        return new ZoneRequirementJsonLoader(baseSeed(0), folder, zonesJson).load();
    }

    /** Loads a JSON object file (like FileIO.load(file, JSON_OBJ_LOADER)). */
    private static JsonObject loadJson(Path file, String what) {
        try {
            return FileIO.load(file, JsonLoader.JSON_OBJ_LOADER);
        } catch (IOException e) {
            throw new Error("Failed to read " + what + ": " + file, e);
        }
    }

    /**
     * Replicates {@link JsonLoader#getOrLoad(JsonElement)}: resolves a
     * {@code {"File": ...}} redirect (returns the element unchanged otherwise).
     * {@code null} passes through.
     */
    @Nullable
    private static JsonObject getOrLoadJson(@Nullable JsonElement element) {
        if (element == null || !element.isJsonObject()) {
            return element instanceof JsonObject obj ? obj : null;
        }
        final JsonObject obj = element.getAsJsonObject();
        final JsonElement path = obj.get("File");
        if (path != null && path.isJsonPrimitive() && path.getAsJsonPrimitive().isString()) {
            final JsonElement loaded = loadFile(path.getAsString());
            return loaded instanceof JsonObject res ? res : null;
        }
        return obj;
    }

    /** Replicates JsonLoader.get(name) (child values are File-resolved). */
    @Nullable
    private static JsonElement child(@Nullable JsonElement parent, String key) {
        if (parent == null || !parent.isJsonObject()) return null;
        JsonElement element = parent.getAsJsonObject().get(key);
        if (element != null && element.isJsonObject()) {
            element = getOrLoadJson(element);
        }
        return element;
    }

    /** Replicates JsonLoader.getRaw(name). */
    @Nullable
    private static JsonElement rawChild(@Nullable JsonElement parent, String key) {
        if (parent == null || !parent.isJsonObject()) return null;
        return parent.getAsJsonObject().get(key);
    }

    /** The "Seed" key of a grid element (null when absent or non-string). */
    @Nullable
    private static String seedKey(@Nullable JsonElement grid) {
        if (grid == null || !grid.isJsonObject()) return null;
        final JsonElement key = grid.getAsJsonObject().get("Seed");
        if (key == null || !key.isJsonPrimitive() || !key.getAsJsonPrimitive().isString()) return null;
        return key.getAsString();
    }

    /** Replicates JsonLoader.loadFile (data folder relative, .json appended). */
    private static JsonElement loadFile(String filePath) {
        final Path file = folder
            .resolve(filePath.replace('.', java.io.File.separatorChar) + ".json");
        if (!file.normalize().startsWith(folder.normalize())) {
            throw new IllegalArgumentException("Invalid file reference: " + filePath);
        }
        try {
            return FileIO.load(file, JsonLoader.JSON_LOADER);
        } catch (Throwable e) {
            throw new Error("Error while loading file reference." + file, e);
        }
    }

    /** Flat blockId/rotation/fluidId members (no enclosing braces). */
    private static String blockTriple(BlockFluidEntry entry) {
        return "\"blockId\":" + entry.blockId()
            + ",\"rotation\":" + entry.rotation()
            + ",\"fluidId\":" + entry.fluidId();
    }

    private static String coordKey(int[] coord) {
        return "{\"x\":" + coord[0] + ",\"z\":" + coord[1] + ",";
    }

    private static String coordPairs(int[][] coords) {
        final StringBuilder sb = new StringBuilder();
        for (int[] coord : coords) {
            if (sb.length() > 0) sb.append(',');
            sb.append("[\"").append(coord[0]).append("\",\"").append(coord[1]).append("\"]");
        }
        return sb.toString();
    }

    private static String joinInts(int[] values) {
        final StringBuilder sb = new StringBuilder();
        for (int value : values) {
            if (sb.length() > 0) sb.append(',');
            sb.append(value);
        }
        return sb.toString();
    }

    private static String joinHexes(double[] values) {
        final StringBuilder sb = new StringBuilder();
        for (double value : values) {
            if (sb.length() > 0) sb.append(',');
            sb.append(Phase2Dumper.JsonWriter.quoteString(hex(value)));
        }
        return sb.toString();
    }

    private static String joinQuotes(String[] values) {
        final StringBuilder sb = new StringBuilder();
        for (String value : values) {
            if (sb.length() > 0) sb.append(',');
            sb.append(Phase2Dumper.JsonWriter.quoteString(value));
        }
        return sb.toString();
    }

    private static String hex(double value) {
        return Double.toHexString(value);
    }

    private static String hex(float value) {
        return Float.toHexString(value);
    }

    private static Object allocateInstance(Class<?> clazz) throws Exception {
        final Class<?> unsafeClass = Class.forName("sun.misc.Unsafe");
        final Field theUnsafe = unsafeClass.getDeclaredField("theUnsafe");
        theUnsafe.setAccessible(true);
        final Object unsafe = theUnsafe.get(null);
        final Method allocateInstance = unsafeClass.getMethod("allocateInstance", Class.class);
        return allocateInstance.invoke(unsafe, clazz);
    }

    private static void setField(Class<?> clazz, Object target, String name, Object value) throws Exception {
        final Field f = clazz.getDeclaredField(name);
        f.setAccessible(true);
        f.set(target, value);
    }

    /** Reads a (possibly inherited) field through reflection. */
    private static Object field(Object target, String name) {
        Class<?> clazz = target.getClass();
        while (clazz != null) {
            try {
                final Field f = clazz.getDeclaredField(name);
                f.setAccessible(true);
                return f.get(target);
            } catch (NoSuchFieldException ignored) {
                clazz = clazz.getSuperclass();
            } catch (IllegalAccessException e) {
                throw new IllegalStateException("Cannot read field " + name, e);
            }
        }
        throw new IllegalStateException("No field '" + name + "' on " + target.getClass().getName());
    }
}
