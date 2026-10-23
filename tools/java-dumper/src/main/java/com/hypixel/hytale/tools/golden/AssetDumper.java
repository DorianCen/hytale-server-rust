package com.hypixel.hytale.tools.golden;

import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.hypixel.hytale.assetstore.AssetLoadResult;
import com.hypixel.hytale.assetstore.AssetMap;
import com.hypixel.hytale.assetstore.AssetRegistry;
import com.hypixel.hytale.assetstore.AssetStore;
import com.hypixel.hytale.assetstore.map.BlockTypeAssetMap;
import com.hypixel.hytale.assetstore.map.DefaultAssetMap;
import com.hypixel.hytale.assetstore.map.IndexedAssetMap;
import com.hypixel.hytale.assetstore.map.IndexedLookupTableAssetMap;
import com.hypixel.hytale.protocol.Packet;
import com.hypixel.hytale.protocol.ProtocolSettings;
import com.hypixel.hytale.server.core.asset.HytaleAssetStore;
import com.hypixel.hytale.server.core.asset.packet.AssetPacketGenerator;

import java.io.IOException;
import java.io.Reader;
import java.lang.foreign.MemorySegment;
import java.lang.reflect.Field;
import java.lang.reflect.Modifier;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashSet;
import java.util.IdentityHashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.StringJoiner;
import java.util.TreeSet;

/**
 * Emits the asset-store golden data of the real CoreServer asset pipeline.
 *
 * <p>For every selected store in
 * {@code tools/asset-golden-dumper/asset-stores.json} the dumper:
 *
 * <ol>
 *   <li>boots the vanilla asset pipeline exactly like {@link RegistryDumper}
 *       ({@code AssetRegistryLoader} over {@code Assets/Assets.zip} mounted as
 *       a zip {@link java.nio.file.FileSystem}, with the common fork-join pool
 *       pinned to a single worker);</li>
 *   <li>calls the store's real packet generator
 *       ({@code generateInitPacket(assetMap, assets)});</li>
 *   <li>serializes the packet with the protocol path
 *       ({@code new byte[packet.computeSize()]} +
 *       {@code packet.serialize(MemorySegment, 0)}) and writes the
 *       <em>uncompressed payload</em> to {@code <Store>.bin} (no framing
 *       header, no zstd compression);</li>
 *   <li>writes {@code index.json} with the store metadata, the sorted keys,
 *       the numeric ids, the counts, the packet id/size and the source
 *       pack.</li>
 * </ol>
 *
 * <p>The default set is P0 + P1 + {@link #DEFAULT_EXTRA_ROOTS} and their
 * {@code loadsAfter} dependency closure. The selector can be narrowed from the
 * command line:
 *
 * <pre>
 *   --priority P0[,P1,...]   select stores by priority
 *   --stores A,B,C           select explicit stores by class simple name
 *   --closure                also include the transitive loadsAfter closure
 * </pre>
 *
 * <p>Without a selector the phase-2 default is emitted. Passing
 * {@code --priority P0} reproduces the old P0-only dump.
 *
 * <p>Stores without a packet generator (the Java server has none for
 * {@code BlockTypeListAsset} and {@code PrefabListAsset}) produce no
 * {@code .bin}; they are still listed in {@code index.json} and any decode
 * failures are recorded there.
 *
 * <p>Usage: {@code AssetDumper <output-directory> [world-directory] [repo-root]
 * [selector...]} (the same argument order as {@link GoldenDumper}, so
 * {@code run.sh} can invoke it with {@code DUMPER_MAIN=...}). The world
 * directory is ignored.
 */
public final class AssetDumper {

    /**
     * The real stderr. The CoreServer boot replaces {@code System.err} with a
     * logger stream, so the original one is kept for reporting failures.
     */
    private static final java.io.PrintStream REAL_ERR = System.err;

    /** The asset-store inventory produced by T08. */
    private static final String STORE_INVENTORY = "tools/asset-golden-dumper/asset-stores.json";

    /**
     * The phase-2 default also dumps these P2/P3 stores, and the transitive
     * {@code loadsAfter} closure of this list. They are the indexes referenced
     * by the P0/P1 packets.
     */
    private static final List<String> DEFAULT_EXTRA_ROOTS = List.of(
        "FluidFX",
        "ItemSoundSet",
        "SoundEvent",
        "TagPattern",
        "ParticleSystem",
        "ParticleSpawner",
        "ItemReticleConfig",
        "ItemPlayerAnimations"
    );

    /** The package of the generated protocol objects (the only ones walked). */
    private static final String PROTOCOL_PACKAGE = "com.hypixel.hytale.protocol";

    private static final Gson GSON = new GsonBuilder()
        .setPrettyPrinting()
        .disableHtmlEscaping()
        .create();

    private AssetDumper() {
    }

    public static void main(String[] args) throws Exception {
        try {
            run(args);
        } catch (Throwable t) {
            REAL_ERR.println("AssetDumper failed:");
            t.printStackTrace(REAL_ERR);
            System.exit(1);
        }
    }

    private static void run(String[] args) throws Exception {
        if (args.length < 1) {
            System.err.println("usage: AssetDumper <output-directory> [world-directory] [repo-root] "
                + "[--priority P0,P1] [--stores A,B] [--closure]");
            System.exit(2);
        }

        final Path outDir = Path.of(args[0]).toAbsolutePath().normalize();
        final Path repoRoot = args.length >= 3
            ? Path.of(args[2]).toAbsolutePath().normalize()
            : Path.of("").toAbsolutePath().normalize();
        final String packPath = System.getenv().getOrDefault("ASSET_PACK", RegistryDumper.PACK_PATH);

        final List<JsonObject> specs = loadStoreSpecs(repoRoot);
        final String[] selectorArgs = args.length > 3
            ? Arrays.copyOfRange(args, 3, args.length)
            : new String[0];
        final Selection selection = selectStores(specs, selectorArgs);
        final Path packRoot = RegistryDumper.boot(repoRoot, packPath);

        Files.createDirectories(outDir);
        removeStalePayloads(outDir);

        final JsonArray stores = new JsonArray();
        final JsonArray missingPackets = new JsonArray();
        final JsonArray packetErrors = new JsonArray();
        long totalBytes = 0;
        int withPacket = 0;

        for (JsonObject spec : specs) {
            final String storeName = string(spec, "class");
            if (!selection.includes(storeName)) continue;
            final StoreDump dump = dumpStore(spec, packRoot, outDir);
            stores.add(dump.index());
            if (dump.packetSize() >= 0) {
                withPacket++;
                totalBytes += dump.packetSize();
            } else if ("none".equals(string(dump.index(), "packetGenerator"))) {
                missingPackets.add(storeName);
            } else {
                packetErrors.add(storeName);
            }
            System.out.printf("  %-24s count=%-6d packet=%-10s size=%d%n",
                dump.storeName(),
                dump.count(),
                dump.packetId() < 0 ? "-" : Integer.toString(dump.packetId()),
                dump.packetSize());
        }

        final JsonObject root = new JsonObject();
        root.add("meta", meta(packPath, selection, missingPackets, packetErrors));
        root.add("stores", stores);

        final Path index = outDir.resolve("index.json");
        Files.writeString(index, GSON.toJson(root) + "\n", StandardCharsets.UTF_8);

        System.out.printf("AssetDumper: %d stores, %d packets, %d payload bytes -> %s%n",
            stores.size(), withPacket, totalBytes, outDir);
    }

    // ------------------------------------------------------------------
    // Inventory
    // ------------------------------------------------------------------

    /** Reads the store specifications from the T08 inventory. */
    private static List<JsonObject> loadStoreSpecs(Path repoRoot) throws IOException {
        final Path inventory = repoRoot.resolve(STORE_INVENTORY).normalize();
        if (!Files.isRegularFile(inventory)) {
            throw new IOException("asset store inventory not found: " + inventory);
        }
        try (Reader reader = Files.newBufferedReader(inventory, StandardCharsets.UTF_8)) {
            final JsonArray array = GSON.fromJson(reader, JsonArray.class);
            final List<JsonObject> specs = new ArrayList<>();
            for (JsonElement element : array) {
                specs.add(element.getAsJsonObject());
            }
            return specs;
        }
    }

    // ------------------------------------------------------------------
    // Store selection
    // ------------------------------------------------------------------

    /** The stores the dumper should emit. */
    private record Selection(Set<String> stores, String description, Set<String> missingDependencies) {
        boolean includes(String storeName) {
            return stores.contains(storeName);
        }
    }

    /**
     * Parses {@code --priority}, {@code --stores} and {@code --closure}. With no
     * selector, emits the phase-2 default: P0 + P1 + the P2/P3 roots above and
     * the closure of those roots only (so a P0/P1 dependency such as
     * {@code GameplayConfig -> Item} does not pull the whole P2 graph).
     */
    private static Selection selectStores(List<JsonObject> specs, String[] args) {
        final Map<String, JsonObject> byName = new LinkedHashMap<>();
        for (JsonObject spec : specs) {
            byName.put(string(spec, "class"), spec);
        }

        final Set<String> priorities = new HashSet<>();
        final Set<String> explicit = new HashSet<>();
        boolean closure = false;
        for (int i = 0; i < args.length; i++) {
            final String arg = args[i];
            if (arg.equals("--closure")) {
                closure = true;
            } else if (arg.equals("--priority") || arg.startsWith("--priority=")) {
                final String value = arg.startsWith("--priority=")
                    ? arg.substring("--priority=".length())
                    : requireValue(args, ++i, "--priority");
                for (String priority : value.split(",")) {
                    if (!priority.isBlank()) priorities.add(priority.trim());
                }
            } else if (arg.equals("--stores") || arg.startsWith("--stores=")) {
                final String value = arg.startsWith("--stores=")
                    ? arg.substring("--stores=".length())
                    : requireValue(args, ++i, "--stores");
                for (String store : value.split(",")) {
                    if (!store.isBlank()) explicit.add(store.trim());
                }
            } else {
                throw new IllegalArgumentException("unknown AssetDumper selector `" + arg + "`");
            }
        }

        for (String store : explicit) {
            if (!byName.containsKey(store)) {
                throw new IllegalArgumentException("unknown asset store `" + store + "`");
            }
        }

        final boolean defaultSelection = priorities.isEmpty() && explicit.isEmpty();
        final Set<String> base = new HashSet<>();
        final Set<String> closureRoots = new HashSet<>();
        final StringJoiner description = new StringJoiner(", ");

        if (defaultSelection) {
            for (JsonObject spec : specs) {
                final String priority = string(spec, "priority");
                if (priority.equals("P0") || priority.equals("P1")) {
                    base.add(string(spec, "class"));
                }
            }
            base.addAll(DEFAULT_EXTRA_ROOTS);
            closureRoots.addAll(DEFAULT_EXTRA_ROOTS);
            description.add("phase2: priority in {P0,P1} + extra roots");
            description.add("closure of " + String.join(",", DEFAULT_EXTRA_ROOTS));
        } else {
            if (!priorities.isEmpty()) {
                for (JsonObject spec : specs) {
                    if (priorities.contains(string(spec, "priority"))) {
                        base.add(string(spec, "class"));
                    }
                }
                description.add("priority in " + new TreeSet<>(priorities));
            }
            if (!explicit.isEmpty()) {
                base.addAll(explicit);
                description.add("stores " + new TreeSet<>(explicit));
            }
            if (closure) {
                closureRoots.addAll(base);
                description.add("transitive loadsAfter closure");
            }
        }

        final Set<String> missing = new TreeSet<>();
        if (!closureRoots.isEmpty()) {
            expandDependencies(base, closureRoots, byName, missing);
        }

        return new Selection(base, description.toString(), missing);
    }

    private static String requireValue(String[] args, int index, String flag) {
        if (index >= args.length) {
            throw new IllegalArgumentException("missing value for " + flag);
        }
        return args[index];
    }

    /** Adds the transitive {@code loadsAfter} dependencies of {@code roots}. */
    private static void expandDependencies(Set<String> selected, Set<String> roots,
                                           Map<String, JsonObject> byName, Set<String> missing) {
        final ArrayDeque<String> queue = new ArrayDeque<>(roots);
        final Set<String> visited = new HashSet<>();
        while (!queue.isEmpty()) {
            final String name = queue.removeFirst();
            if (!visited.add(name)) continue;
            final JsonObject spec = byName.get(name);
            if (spec == null) {
                missing.add(name);
                continue;
            }
            selected.add(name);
            final JsonElement deps = spec.get("loadsAfter");
            if (deps == null || deps.isJsonNull()) continue;
            for (JsonElement dep : deps.getAsJsonArray()) {
                queue.addLast(dep.getAsString());
            }
        }
    }


    /** The result of dumping one store. */
    private record StoreDump(String storeName, int count, int packetId, int packetSize, JsonObject index) {
    }

    @SuppressWarnings({"rawtypes", "unchecked"})
    private static StoreDump dumpStore(JsonObject spec, Path packRoot, Path outDir) throws IOException {
        final String storeName = string(spec, "class");
        final Class<?> assetClass = findStoreClass(storeName);
        final AssetStore store = AssetRegistry.getStoreMap().get(assetClass);
        if (store == null) {
            throw new IllegalStateException("selected store is not registered: " + storeName);
        }
        if (!"String".equals(string(spec, "key"))) {
            throw new IllegalStateException("selected store " + storeName + " has an unsupported key type: "
                + string(spec, "key") + " (the dumper only supports String keys)");
        }

        final AssetMap map = store.getAssetMap();
        final Map assets = map.getAssetMap();

        final List<String> keys = new ArrayList<>(assets.size());
        for (Object key : assets.keySet()) {
            keys.add((String) key);
        }
        Collections.sort(keys);

        // The asset map is an open-addressing hash map, so its iteration order
        // is not semantic. The packet generators build insertion-ordered maps
        // from it, therefore the entries are handed over in sorted key order:
        // that makes the serialized payload independent of the map layout.
        final Map sortedAssets = new LinkedHashMap<>(assets.size());
        for (String key : keys) {
            sortedAssets.put(key, assets.get(key));
        }

        final JsonObject entry = new JsonObject();
        entry.addProperty("store", storeName);
        entry.addProperty("class", assetClass.getName());
        entry.addProperty("path", nullToEmpty(store.getPath()));
        entry.addProperty("map", string(spec, "map"));
        entry.addProperty("keyType", string(spec, "key"));
        entry.addProperty("priority", string(spec, "priority"));
        entry.addProperty("count", assets.size());
        entry.addProperty("packKey", DefaultAssetMap.DEFAULT_PACK_KEY);

        final boolean indexed = map instanceof BlockTypeAssetMap
            || map instanceof IndexedLookupTableAssetMap
            || map instanceof IndexedAssetMap;
        if (indexed) {
            final int maxId = nextIndex(map);
            entry.addProperty("maxId", maxId);
            final JsonObject ids = new JsonObject();
            for (String key : keys) {
                final int id = indexOf(map, key);
                if (id != Integer.MIN_VALUE) {
                    ids.addProperty(key, id);
                }
            }
            entry.add("ids", ids);
        }
        final JsonArray keyArray = new JsonArray(keys.size());
        for (String key : keys) {
            keyArray.add(key);
        }
        entry.add("keys", keyArray);

        final AssetPacketGenerator generator = store instanceof HytaleAssetStore hytale
            ? hytale.getPacketGenerator()
            : null;
        if (generator == null) {
            entry.addProperty("packetGenerator", "none");
            entry.addProperty("packetClass", "");
            entry.addProperty("packetId", -1);
            entry.addProperty("packetSize", -1);
            entry.addProperty("bin", "");
            entry.addProperty("sha256", "");
        } else {
            try {
                final Packet packet = (Packet) generator.generateInitPacket(map, sortedAssets);
                canonicalizeMaps(packet);
                final int size = packet.computeSize();
                final byte[] payload = new byte[size];
                final int written = packet.serialize(MemorySegment.ofArray(payload), 0);
                if (written != size) {
                    throw new IllegalStateException(storeName + ": packet.serialize wrote " + written
                        + " bytes but computeSize() returned " + size);
                }
                final String bin = storeName + ".bin";
                Files.write(outDir.resolve(bin), payload);

                entry.addProperty("packetGenerator", generator.getClass().getSimpleName());
                entry.addProperty("packetClass", packet.getClass().getName());
                entry.addProperty("packetId", packet.getId());
                entry.addProperty("packetSize", size);
                entry.addProperty("bin", bin);
                entry.addProperty("sha256", sha256(payload));
            } catch (Exception error) {
                // Some P1 stores need a server module singleton that the
                // canonical asset-only boot does not install (for example
                // BlockSetModule). Record the store metadata and the failure
                // instead of aborting the whole dump.
                REAL_ERR.printf("  %-24s packet generation failed: %s%n",
                    storeName, error);
                entry.addProperty("packetGenerator", generator.getClass().getSimpleName());
                entry.addProperty("packetClass", "");
                entry.addProperty("packetId", -1);
                entry.addProperty("packetSize", -1);
                entry.addProperty("bin", "");
                entry.addProperty("sha256", "");
                entry.addProperty("packetError", error.toString());
            }
        }

        final JsonObject failures = loadFailures(assetClass, packRoot);
        if (failures != null) {
            entry.add("loadFailures", failures);
        }

        return new StoreDump(storeName, assets.size(),
            entry.get("packetId").getAsInt(), entry.get("packetSize").getAsInt(), entry);
    }

    /** Resolves the simple class name from the inventory to the registered store class. */
    private static Class<?> findStoreClass(String simpleName) {
        Class<?> found = null;
        for (Class<?> assetClass : AssetRegistry.getStoreMap().keySet()) {
            if (assetClass.getSimpleName().equals(simpleName)) {
                if (found != null && found != assetClass) {
                    throw new IllegalStateException("ambiguous selected store class name: " + simpleName);
                }
                found = assetClass;
            }
        }
        if (found == null) {
            throw new IllegalStateException("selected store is not registered: " + simpleName);
        }
        return found;
    }

    private static int indexOf(AssetMap<?, ?> map, String key) {
        if (map instanceof BlockTypeAssetMap<?, ?> blockMap) {
            return ((BlockTypeAssetMap) blockMap).getIndex(key);
        }
        if (map instanceof IndexedLookupTableAssetMap<?, ?> lookupMap) {
            return ((IndexedLookupTableAssetMap) lookupMap).getIndex(key);
        }
        if (map instanceof IndexedAssetMap<?, ?> indexedMap) {
            return ((IndexedAssetMap) indexedMap).getIndex(key);
        }
        return Integer.MIN_VALUE;
    }

    private static int nextIndex(AssetMap<?, ?> map) {
        if (map instanceof BlockTypeAssetMap<?, ?> blockMap) {
            return blockMap.getNextIndex();
        }
        if (map instanceof IndexedLookupTableAssetMap<?, ?> lookupMap) {
            return lookupMap.getNextIndex();
        }
        if (map instanceof IndexedAssetMap<?, ?> indexedMap) {
            return indexedMap.getNextIndex();
        }
        return -1;
    }

    // ------------------------------------------------------------------
    // Deterministic serialization
    // ------------------------------------------------------------------

    /**
     * Canonicalizes the iteration order of every map in the generated packet
     * object graph.
     *
     * <p>The protocol serializes dictionaries by iterating the field's
     * {@link Map}. The vanilla packet generators build several maps with
     * <em>enum</em> keys in fastutil hash maps (e.g.
     * {@code ItemBase.interactions} keyed by {@code InteractionType} and
     * {@code BlockType.support} keyed by {@code BlockNeighbor}); an enum's
     * {@link Object#hashCode()} is its identity hash, so the iteration order
     * changes from JVM run to JVM run and the payload would not be
     * byte-stable. Replacing those maps with a {@link LinkedHashMap} sorted by
     * a deterministic key order keeps the exact same entries (map order is not
     * semantic) and makes {@code computeSize}/{@code serialize} reproducible.
     *
     * <p>Only {@code com.hypixel.hytale.protocol} objects are visited; the
     * server-side asset objects are never modified.
     */
    private static void canonicalizeMaps(Object packet) {
        canonicalizeMaps(packet, Collections.newSetFromMap(new IdentityHashMap<>()));
    }

    private static void canonicalizeMaps(Object value, Set<Object> visited) {
        if (value == null || !visited.add(value)) return;

        final Class<?> type = value.getClass();
        if (type.isArray()) {
            if (!type.getComponentType().isPrimitive()) {
                for (Object element : (Object[]) value) {
                    canonicalizeMaps(element, visited);
                }
            }
            return;
        }
        if (type.isEnum() || !type.getName().startsWith(PROTOCOL_PACKAGE)) return;

        for (Field field : fieldsOf(type)) {
            if (Modifier.isStatic(field.getModifiers())) continue;
            final Object fieldValue;
            try {
                fieldValue = field.get(value);
            } catch (IllegalAccessException e) {
                throw new IllegalStateException("Cannot read protocol field " + field, e);
            }
            if (fieldValue == null) continue;

            if (fieldValue instanceof Map<?, ?> map) {
                final List<Map.Entry<?, ?>> entries = new ArrayList<>(map.entrySet());
                entries.sort((a, b) -> compareKeys(a.getKey(), b.getKey()));
                final Map<Object, Object> sorted = new LinkedHashMap<>(entries.size());
                for (Map.Entry<?, ?> entry : entries) {
                    sorted.put(entry.getKey(), entry.getValue());
                    canonicalizeMaps(entry.getValue(), visited);
                }
                try {
                    field.set(value, sorted);
                } catch (IllegalAccessException e) {
                    throw new IllegalStateException("Cannot replace protocol map field " + field, e);
                }
            } else {
                canonicalizeMaps(fieldValue, visited);
            }
        }
    }

    /** A deterministic total order over the map key types the protocol uses. */
    private static int compareKeys(Object a, Object b) {
        if (a == b) return 0;
        if (a instanceof Enum<?> ea && b instanceof Enum<?> eb) {
            final int byClass = ea.getDeclaringClass().getName().compareTo(eb.getDeclaringClass().getName());
            if (byClass != 0) return byClass;
            return Integer.compare(ea.ordinal(), eb.ordinal());
        }
        if (a instanceof Number na && b instanceof Number nb) {
            if (a instanceof Float || a instanceof Double || b instanceof Float || b instanceof Double) {
                return Double.compare(na.doubleValue(), nb.doubleValue());
            }
            return Long.compare(na.longValue(), nb.longValue());
        }
        if (a instanceof String sa && b instanceof String sb) {
            return sa.compareTo(sb);
        }
        final int byClass = a.getClass().getName().compareTo(b.getClass().getName());
        if (byClass != 0) return byClass;
        return String.valueOf(a).compareTo(String.valueOf(b));
    }

    /** The non-static fields of a class and its superclasses, cached. */
    private static Field[] fieldsOf(Class<?> type) {
        return FIELDS.get(type);
    }

    private static final ClassValue<Field[]> FIELDS = new ClassValue<>() {
        @Override
        protected Field[] computeValue(Class<?> type) {
            final List<Field> fields = new ArrayList<>();
            for (Class<?> current = type; current != null && current != Object.class;
                 current = current.getSuperclass()) {
                if (!current.getName().startsWith(PROTOCOL_PACKAGE)) break;
                for (Field field : current.getDeclaredFields()) {
                    if (Modifier.isStatic(field.getModifiers())) continue;
                    field.setAccessible(true);
                    fields.add(field);
                }
            }
            return fields.toArray(Field[]::new);
        }
    };

    // ------------------------------------------------------------------
    // Load failures
    // ------------------------------------------------------------------

    /**
     * The decode failures of a store (including its child asset stores), or
     * {@code null} when everything decoded. The paths are pack-relative so the
     * golden data never contains an absolute path.
     */
    private static JsonObject loadFailures(Class<?> assetClass, Path packRoot) {
        final AssetLoadResult<?, ?> result = RegistryDumper.getLoadResult(assetClass);
        if (result == null) return null;
        final JsonObject failures = new JsonObject();
        final Set<String> keys = new TreeSet<>();
        final Set<String> paths = new TreeSet<>();
        collectFailures(result, packRoot, keys, paths, null);
        if (keys.isEmpty() && paths.isEmpty()) return null;
        failures.add("failedKeys", strings(keys));
        failures.add("failedPaths", strings(paths));
        return failures;
    }

    @SuppressWarnings("unchecked")
    private static void collectFailures(AssetLoadResult<?, ?> result, Path packRoot,
                                        Set<String> keys, Set<String> paths, String prefix) {
        for (Object key : result.getFailedToLoadKeys()) {
            keys.add(prefix == null ? String.valueOf(key) : prefix + key);
        }
        for (Path path : result.getFailedToLoadPaths()) {
            String relative;
            try {
                relative = packRoot.relativize(path).toString();
            } catch (IllegalArgumentException e) {
                relative = path.getFileName().toString();
            }
            paths.add(prefix == null ? relative : prefix + relative);
        }
        for (Map.Entry<Class<?>, AssetLoadResult<?, ?>> child : childResults(result).entrySet()) {
            collectFailures(child.getValue(), packRoot, keys, paths,
                child.getKey().getSimpleName() + ":");
        }
    }

    /**
     * The child asset results of a load result. The upstream class has no
     * getter for them; reflection keeps this dumper independent of the
     * read-only reference sources.
     */
    @SuppressWarnings("unchecked")
    private static Map<Class<?>, AssetLoadResult<?, ?>> childResults(AssetLoadResult<?, ?> result) {
        try {
            final Field field = AssetLoadResult.class.getDeclaredField("childAssetResults");
            field.setAccessible(true);
            return (Map<Class<?>, AssetLoadResult<?, ?>>) field.get(result);
        } catch (ReflectiveOperationException e) {
            return Map.of();
        }
    }

    // ------------------------------------------------------------------
    // index.json metadata
    // ------------------------------------------------------------------

    private static JsonObject meta(String packPath, Selection selection, JsonArray missingPackets,
                                   JsonArray packetErrors) {
        final JsonObject meta = new JsonObject();
        meta.addProperty("producer", "com.hypixel.hytale.tools.golden.AssetDumper");
        meta.addProperty("inventory", STORE_INVENTORY);
        meta.addProperty("storeFilter", selection.description());
        meta.addProperty("selectedStoreCount", selection.stores().size());
        meta.addProperty("pack", packPath);
        meta.addProperty("packFormat", "zip");
        meta.addProperty("packKey", DefaultAssetMap.DEFAULT_PACK_KEY);
        meta.addProperty("protocolVersion", ProtocolSettings.PROTOCOL_VERSION);
        meta.addProperty("protocolCrc", ProtocolSettings.PROTOCOL_CRC);
        meta.addProperty("protocolBuildNumber", ProtocolSettings.PROTOCOL_BUILD_NUMBER);
        meta.addProperty("parallelism", Integer.getInteger("java.util.concurrent.ForkJoinPool.common.parallelism", 1));
        meta.addProperty("boot", "the vanilla CoreServer asset pipeline exactly like RegistryDumper: "
            + "Options.parse([]) for the option set, inert HytaleServer/Universe stubs, the "
            + "AssetRegistryLoader static initializer for the vanilla stores, the plugin-module stores and "
            + "codec variants, the pack's CommonAssetsIndex.hashes and the pack mounted as a zip FileSystem; "
            + "the asset files are collected with Files.walkFileTree (the vanilla mechanism)");
        meta.addProperty("payload", "each <Store>.bin is the exact uncompressed payload of "
            + "packetGenerator.generateInitPacket(assetMap, assets): a byte[packet.computeSize()] filled by "
            + "packet.serialize(MemorySegment.ofArray(bytes), 0). It is not framed (no packet id/length "
            + "header) and not zstd-compressed");
        meta.addProperty("indexOrder", "stores follow the selected stores' inventory order; the keys array and the "
            + "ids object are sorted lexicographically by key; ids are the numeric ids of the store's map");
        meta.addProperty("missingPackets", missingPackets.size() == 0
            ? "no selected store is missing a packet generator"
            : "selected stores without a packet generator in the Java server: " + missingPackets);
        if (packetErrors.size() > 0) {
            meta.addProperty("packetErrors",
                "selected stores whose packet generator needs an un-booted server module; "
                    + "the index entry carries packetError and has no .bin: " + packetErrors);
        }
        if (!selection.missingDependencies().isEmpty()) {
            meta.addProperty("missingDependencies",
                "selected loadsAfter dependencies that are not asset stores in the 54-store inventory: "
                    + selection.missingDependencies());
        }
        meta.addProperty("loadFailures", "when a store (or a child asset store) had decode failures, the store "
            + "entry carries loadFailures.failedKeys / failedPaths (pack-relative); child-store failures are "
            + "prefixed with '<ChildStore>:'. Known gaps of the canonical boot: PrefabListAsset references the "
            + "prefab store, which this dumper does not boot - PrefabReference.processPrefabPath -> "
            + "PrefabStore.getAssetPrefabsPath -> AssetModule.getBaseAssetPack() throws NullPointerException "
            + "because AssetModule.get() is null - so all 78 lists fail and the single block type that references "
            + "such a list (ExamplePrefabPlacementBlock_Tree) is missing from BlockType; the non-P0 "
            + "interaction/camera stores also report decode failures because their plugin codec variants are not "
            + "registered. None of these change the block/fluid/environment ids, which match registries.json "
            + "exactly");
        meta.addProperty("deterministic", "no timestamps and no absolute paths are written; the pipeline "
            + "runs with a single common fork-join worker, so two runs are byte-identical");
        return meta;
    }

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    private static void removeStalePayloads(Path outDir) throws IOException {
        try (var stream = Files.list(outDir)) {
            for (Path path : stream.filter(p -> p.getFileName().toString().endsWith(".bin")).toList()) {
                Files.delete(path);
            }
        }
    }

    private static String sha256(byte[] payload) {
        try {
            final byte[] digest = MessageDigest.getInstance("SHA-256").digest(payload);
            final StringBuilder sb = new StringBuilder(digest.length * 2);
            for (byte b : digest) {
                sb.append(Character.forDigit((b >> 4) & 0xF, 16));
                sb.append(Character.forDigit(b & 0xF, 16));
            }
            return sb.toString();
        } catch (NoSuchAlgorithmException e) {
            throw new IllegalStateException(e);
        }
    }

    private static JsonArray strings(Set<String> values) {
        final JsonArray array = new JsonArray(values.size());
        for (String value : values) {
            array.add(value);
        }
        return array;
    }

    private static String string(JsonObject object, String name) {
        final JsonElement element = object.get(name);
        return element == null || element.isJsonNull() ? "" : element.getAsString();
    }

    private static String nullToEmpty(String value) {
        return value == null ? "" : value;
    }
}
