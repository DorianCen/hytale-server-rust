package com.hypixel.hytale.plugins.registrysnapshot;

import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import com.hypixel.hytale.assetstore.map.BlockTypeAssetMap;
import com.hypixel.hytale.assetstore.map.IndexedLookupTableAssetMap;
import com.hypixel.hytale.server.core.HytaleServer;
import com.hypixel.hytale.server.core.asset.type.blocktype.config.BlockFace;
import com.hypixel.hytale.server.core.asset.type.blocktype.config.BlockType;
import com.hypixel.hytale.server.core.asset.type.blocktype.config.RotationTuple;
import com.hypixel.hytale.server.core.asset.type.environment.config.Environment;
import com.hypixel.hytale.server.core.asset.type.fluid.Fluid;
import com.hypixel.hytale.server.core.plugin.JavaPlugin;
import com.hypixel.hytale.server.core.plugin.JavaPluginInit;

import javax.annotation.Nonnull;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.logging.Level;

/**
 * Writes a live snapshot of the block / fluid / environment registries in the
 * schema the Rust world generator consumes (`registries.json`).
 *
 * <p>The plugin dumps in {@link #start()}, i.e. after the server has completed
 * {@code AssetRegistryLoader.loadAssets} (including every mod/override) and
 * before the first world is generated. This captures the ids of *this* boot:
 * the vanilla asset pipeline decodes in parallel, so a few sibling ids are
 * assigned in scheduling order and cannot be reproduced by a separate
 * process.
 *
 * <p>System properties:
 * <ul>
 *   <li>{@code worldgen.snapshot.output=<path>} — the target JSON file
 *       (required; no file is written when it is absent).</li>
 *   <li>{@code worldgen.snapshot.shutdown=true} — stop the server after the
 *       snapshot was written (useful for a headless capture run).</li>
 * </ul>
 */
public class RegistrySnapshotPlugin extends JavaPlugin {

    /** The output path property. */
    public static final String OUTPUT_PROPERTY = "worldgen.snapshot.output";

    /** The shutdown-after-write property. */
    public static final String SHUTDOWN_PROPERTY = "worldgen.snapshot.shutdown";

    public RegistrySnapshotPlugin(@Nonnull JavaPluginInit init) {
        super(init);
    }

    @Override
    protected void start() {
        final String output = System.getProperty(OUTPUT_PROPERTY);
        if (output == null || output.isBlank()) {
            getLogger().at(Level.INFO).log(
                "No -D%s=<path> set; not writing a registry snapshot.", OUTPUT_PROPERTY);
            return;
        }

        final Path target = Path.of(output).toAbsolutePath().normalize();
        try {
            writeSnapshot(target);
            getLogger().at(Level.INFO).log("Wrote the registry snapshot to %s", target);
        } catch (Throwable t) {
            getLogger().at(Level.SEVERE).withCause(t).log("Failed to write the registry snapshot");
            return;
        }

        if (Boolean.getBoolean(SHUTDOWN_PROPERTY)) {
            // Do not stop inside the plugin-manager start loop; let it finish.
            final Thread stopper = new Thread(() -> {
                try {
                    Thread.sleep(1000L);
                } catch (InterruptedException e) {
                    Thread.currentThread().interrupt();
                }
                HytaleServer.get().shutdownServer();
            }, "worldgen-snapshot-shutdown");
            stopper.setDaemon(true);
            stopper.start();
        }
    }

    /**
     * Writes the snapshot of the currently loaded maps.
     *
     * @param target the JSON file (parent directories are created)
     */
    public static void writeSnapshot(@Nonnull Path target) throws java.io.IOException {
        final JsonObject root = new JsonObject();

        final JsonObject meta = new JsonObject();
        meta.addProperty("producer", "RegistrySnapshotPlugin (live server asset maps)");
        meta.addProperty("idOrder", "ids of this server boot; sibling ids may differ "
            + "from other boots because the asset pipeline decodes in parallel");
        root.add("meta", meta);

        // --- blocks ------------------------------------------------------
        final BlockTypeAssetMap<String, BlockType> blockMap = BlockType.getAssetMap();
        final JsonArray blocks = new JsonArray();
        for (int index = 0; index < blockMap.getNextIndex(); index++) {
            final BlockType blockType = blockMap.getAsset(index);
            if (blockType == null) continue;
            final JsonObject block = new JsonObject();
            block.addProperty("name", blockType.getId());
            block.addProperty("index", index);
            block.addProperty("drawType", blockType.getDrawType().name());
            block.addProperty("drawTypeId", blockType.getDrawType().getValue());
            block.addProperty("opacity", blockType.getOpacity().name());
            block.addProperty("unknown", blockType.isUnknown());
            final JsonArray support = supportArray(blockType);
            if (support != null) {
                block.add("support", support);
            }
            blocks.add(block);
        }
        final JsonObject blockSection = new JsonObject();
        blockSection.addProperty("count", blocks.size());
        blockSection.add("assets", blocks);
        root.add("blocks", blockSection);

        // --- fluids ------------------------------------------------------
        final IndexedLookupTableAssetMap<String, Fluid> fluidMap = Fluid.getAssetMap();
        final JsonArray fluids = new JsonArray();
        for (int index = 0; index < fluidMap.getNextIndex(); index++) {
            final Fluid fluid = fluidMap.getAsset(index);
            if (fluid == null) continue;
            final JsonObject entry = new JsonObject();
            entry.addProperty("name", fluid.getId());
            entry.addProperty("index", index);
            entry.addProperty("maxFluidLevel", fluid.getMaxFluidLevel());
            fluids.add(entry);
        }
        final JsonObject fluidSection = new JsonObject();
        fluidSection.addProperty("count", fluids.size());
        fluidSection.add("assets", fluids);
        root.add("fluids", fluidSection);

        // --- environments ------------------------------------------------
        final IndexedLookupTableAssetMap<String, Environment> environmentMap = Environment.getAssetMap();
        final JsonArray environments = new JsonArray();
        for (int index = 0; index < environmentMap.getNextIndex(); index++) {
            final Environment environment = environmentMap.getAsset(index);
            if (environment == null) continue;
            final JsonObject entry = new JsonObject();
            entry.addProperty("name", environment.getId());
            entry.addProperty("index", index);
            environments.add(entry);
        }
        final JsonObject environmentSection = new JsonObject();
        environmentSection.addProperty("count", environments.size());
        environmentSection.add("assets", environments);
        root.add("environments", environmentSection);

        final Path parent = target.getParent();
        if (parent != null) {
            Files.createDirectories(parent);
        }
        final String json = new GsonBuilder().setPrettyPrinting().create().toJson(root);
        Files.writeString(target, json + "\n", StandardCharsets.UTF_8);
    }

    /**
     * The per-{@code RotationTuple} DOWN/UP support probes, or {@code null}
     * when the block has no support map (the Java {@code null}).
     */
    private static JsonArray supportArray(@Nonnull BlockType blockType) {
        if (blockType.getSupport(RotationTuple.NONE_INDEX) == null) {
            return null;
        }
        final JsonArray array = new JsonArray();
        for (int rotation = 0; rotation < RotationTuple.VALUES.length; rotation++) {
            final var supports = blockType.getSupport(rotation);
            if (supports == null) {
                array.add(JsonNull.INSTANCE);
                continue;
            }
            final JsonObject entry = new JsonObject();
            entry.addProperty("down", supports.containsKey(BlockFace.DOWN));
            entry.addProperty("up", supports.containsKey(BlockFace.UP));
            array.add(entry);
        }
        return array;
    }
}
