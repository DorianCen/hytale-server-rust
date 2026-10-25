package com.hypixel.hytale.tools.golden;

import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.hypixel.hytale.server.core.asset.type.blocktype.config.BlockType;
import com.hypixel.hytale.server.core.asset.type.fluid.DefaultFluidTicker;
import com.hypixel.hytale.server.core.asset.type.fluid.Fluid;
import com.hypixel.hytale.server.core.asset.type.fluid.FluidTicker;

import java.lang.reflect.Field;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;

/**
 * Emits the Java fluid-ticker oracle consumed by the Rust fluid tests.
 *
 * <p>The probe boots the real CoreServer asset pipeline, then evaluates
 * {@link FluidTicker#blocksFluidFrom} for representative hitboxes and dumps
 * the decoded ticker fields for every P0 fluid. This keeps the Rust
 * face-coverage implementation and the ticker configuration parser honest
 * without inventing expected values.
 */
public final class FluidTickerProbe {
    private static final Gson GSON = new GsonBuilder()
        .setPrettyPrinting()
        .disableHtmlEscaping()
        .create();

    private static final List<String> BLOCKS = List.of(
        "Rock_Stone",
        "Build_Black_Stairs",
        "Cloth_Roof_Blue_Flat",
        "Deco_Bone_Skulls_Feran_Wall"
    );

    private static final List<String> FLUIDS = List.of(
        "Water_Source",
        "Water",
        "Water_Finite",
        "Lava_Source",
        "Lava",
        "Fire",
        "Dungeon_Fire_NoSpread",
        "Tar_Source",
        "Tar"
    );

    private FluidTickerProbe() {
    }

    public static void main(String[] args) throws Exception {
        if (args.length < 3) {
            System.err.println("usage: FluidTickerProbe <output-directory> <world-directory> <repo-root>");
            System.exit(2);
        }
        final Path outDir = Path.of(args[0]).toAbsolutePath().normalize();
        final Path repoRoot = Path.of(args[2]).toAbsolutePath().normalize();
        RegistryDumper.boot(repoRoot);

        final var ticker = DefaultFluidTicker.INSTANCE;
        final JsonObject root = new JsonObject();
        final JsonObject blocks = new JsonObject();
        for (final String key : BLOCKS) {
            final BlockType block = BlockType.getAssetMap().getAsset(key);
            if (block == null) continue;
            final JsonObject entry = new JsonObject();
            entry.addProperty("fullySolid", FluidTicker.isFullySolid(block));
            entry.addProperty("solid", FluidTicker.isSolid(block));
            final JsonObject directions = new JsonObject();
            for (final Map.Entry<String, int[]> direction : Map.of(
                "east", new int[]{1, 0},
                "west", new int[]{-1, 0},
                "south", new int[]{0, 1},
                "north", new int[]{0, -1}
            ).entrySet()) {
                directions.addProperty(
                    direction.getKey(),
                    ticker.blocksFluidFrom(block, 0, direction.getValue()[0], direction.getValue()[1], 0)
                );
            }
            entry.add("blocksFluidFrom", directions);
            blocks.add(key, entry);
        }
        root.add("blocks", blocks);

        final JsonObject fluids = new JsonObject();
        for (final String key : FLUIDS) {
            final Fluid fluid = Fluid.getAssetMap().getAsset(key);
            if (fluid == null) continue;
            final FluidTicker value = fluid.getTicker();
            final JsonObject entry = new JsonObject();
            entry.addProperty("ticker", value.getClass().getSimpleName());
            entry.addProperty("maxFluidLevel", fluid.getMaxFluidLevel());
            entry.addProperty("flowRate", get(value, "flowRate", Float.class));
            entry.addProperty("canDemote", get(value, "canDemote", Boolean.class));
            entry.addProperty("supportedBy", get(value, "supportedBy", String.class));
            entry.addProperty("spreadFluid", get(value, "spreadFluid", String.class));
            entry.add("collisions", collisions(value));
            entry.add("flammability", flammability(value));
            fluids.add(key, entry);
        }
        root.add("fluids", fluids);

        Files.createDirectories(outDir);
        Files.writeString(
            outDir.resolve("fluid-tickers.json"),
            GSON.toJson(root) + "\n",
            StandardCharsets.UTF_8
        );
        System.out.println("fluid ticker probe: ok");
    }

    private static JsonObject collisions(FluidTicker ticker) {
        final JsonObject result = new JsonObject();
        final Object raw = get(ticker, "rawCollisionMap", Object.class);
        if (!(raw instanceof Map<?, ?> map)) return result;
        final TreeMap<String, JsonObject> sorted = new TreeMap<>();
        for (final Map.Entry<?, ?> entry : map.entrySet()) {
            final Object config = entry.getValue();
            final JsonObject item = new JsonObject();
            item.addProperty("blockToPlace", get(config, "blockToPlace", String.class));
            item.addProperty("soundEvent", get(config, "soundEvent", String.class));
            item.addProperty("placeFluid", get(config, "placeFluid", Boolean.class));
            sorted.put(String.valueOf(entry.getKey()), item);
        }
        for (final Map.Entry<String, JsonObject> entry : sorted.entrySet()) {
            result.add(entry.getKey(), entry.getValue());
        }
        return result;
    }

    private static JsonArray flammability(FluidTicker ticker) {
        final JsonArray result = new JsonArray();
        final Object raw = get(ticker, "rawFlammabilityConfigs", Object.class);
        if (!(raw instanceof Object[] configs)) return result;
        for (final Object config : configs) {
            final JsonObject item = new JsonObject();
            item.addProperty("priority", get(config, "priority", Integer.class));
            item.addProperty("burnLevel", get(config, "burnLevel", Byte.class));
            item.addProperty("burnChance", get(config, "burnChance", Float.class));
            item.addProperty("resultingBlock", get(config, "resultingBlock", String.class));
            item.addProperty("resultingState", get(config, "resultingState", String.class));
            final Object tagPatternId = get(config, "tagPatternId", Object.class);
            item.addProperty("tagPattern", tagPatternId == null ? null : tagPatternId.toString());
            result.add(item);
        }
        return result;
    }

    private static <T> T get(Object target, String name, Class<T> type) {
        Class<?> current = target.getClass();
        while (current != null) {
            try {
                final Field field = current.getDeclaredField(name);
                field.setAccessible(true);
                return type.cast(field.get(target));
            } catch (NoSuchFieldException e) {
                current = current.getSuperclass();
            } catch (ReflectiveOperationException | ClassCastException e) {
                return null;
            }
        }
        return null;
    }
}
