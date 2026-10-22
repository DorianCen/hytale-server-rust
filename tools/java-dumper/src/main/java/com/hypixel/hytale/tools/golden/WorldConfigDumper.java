package com.hypixel.hytale.tools.golden;

import com.hypixel.hytale.codec.ExtraInfo;
import com.hypixel.hytale.math.shape.Box2D;
import com.hypixel.hytale.math.vector.Transform;
import com.hypixel.hytale.protocol.Color;
import com.hypixel.hytale.protocol.GameMode;
import com.hypixel.hytale.server.core.universe.world.ClientEffectWorldSettings;
import com.hypixel.hytale.server.core.universe.world.WorldConfig;
import com.hypixel.hytale.server.core.universe.world.spawn.GlobalSpawnProvider;
import com.hypixel.hytale.server.core.universe.world.spawn.ISpawnProvider;
import com.hypixel.hytale.server.core.universe.world.worldgen.provider.IWorldGenProvider;
import com.hypixel.hytale.server.core.universe.world.worldgen.provider.VoidWorldGenProvider;
import com.hypixel.hytale.server.core.universe.world.worldmap.provider.DisabledWorldMapProvider;
import com.hypixel.hytale.server.core.universe.world.worldmap.provider.IWorldMapProvider;
import com.hypixel.hytale.server.core.universe.world.storage.provider.DefaultChunkStorageProvider;
import com.hypixel.hytale.server.core.universe.world.storage.provider.IChunkStorageProvider;
import com.hypixel.hytale.server.core.universe.world.storage.resources.DefaultResourceStorageProvider;
import com.hypixel.hytale.server.core.universe.world.storage.resources.IResourceStorageProvider;
import com.hypixel.hytale.server.core.util.BsonUtil;
import org.bson.BsonDocument;

import java.io.PrintStream;
import java.nio.file.Path;
import java.time.Instant;
import java.util.Set;
import java.util.UUID;

/**
 * Writes and checks the per-world {@code config.json} with the real
 * {@link WorldConfig} codec.
 *
 * <p>The world config is the sibling of the chunk region files: Java loads and
 * saves it through {@code WorldConfig.load}/{@code WorldConfig.save}, which is a
 * BSON document encoded as strict extended JSON.
 *
 * <p>Usage (the first three arguments match {@code run.sh}):
 * <pre>
 * WorldConfigDumper &lt;out-dir&gt; &lt;world-dir&gt; &lt;repo-root&gt; write &lt;path&gt;
 * WorldConfigDumper &lt;out-dir&gt; &lt;world-dir&gt; &lt;repo-root&gt; probe &lt;path&gt;
 * </pre>
 */
public final class WorldConfigDumper {

    private static final PrintStream REAL_ERR = System.err;

    static final UUID GOLDEN_UUID = UUID.fromString("12345678-1234-5678-9abc-def012345678");
    static final String GOLDEN_DISPLAY_NAME = "Rust Golden World";
    static final long GOLDEN_SEED = 1_234_567_890_123L;
    static final String GOLDEN_GAME_TIME = "2026-10-22T12:34:56Z";
    static final String GOLDEN_GAMEPLAY_CONFIG = "Default";

    private WorldConfigDumper() {
    }

    public static void main(String[] args) throws Exception {
        try {
            run(args);
        } catch (Throwable t) {
            REAL_ERR.println("WorldConfigDumper failed:");
            t.printStackTrace(REAL_ERR);
            System.exit(1);
        }
    }

    private static void run(String[] args) throws Exception {
        if (args.length < 5) {
            REAL_ERR.println("usage: WorldConfigDumper <out-dir> <world-dir> <repo-root> "
                + "write|probe <path>");
            System.exit(2);
        }

        final Path repoRoot = Path.of(args[2]).toAbsolutePath().normalize();
        final Path path = Path.of(args[4]).toAbsolutePath().normalize();
        // WorldConfig's static initialiser builds the gameplay-config defaults, which
        // read the real asset stores (EntityEffect, CombatConfig, ...).
        RegistryDumper.boot(repoRoot);
        registerProviders();

        switch (args[3]) {
            case "write" -> write(path);
            case "probe" -> probe(path);
            default -> {
                REAL_ERR.println("unknown mode: " + args[3]);
                System.exit(2);
            }
        }
    }

    /** The provider codecs are registered by {@code Universe} during a real boot. */
    private static void registerProviders() {
        ISpawnProvider.CODEC.register("Global", GlobalSpawnProvider.class, GlobalSpawnProvider.CODEC);
        IWorldGenProvider.CODEC.register(VoidWorldGenProvider.ID, VoidWorldGenProvider.class, VoidWorldGenProvider.CODEC);
        IWorldMapProvider.CODEC.register(DisabledWorldMapProvider.ID, DisabledWorldMapProvider.class, DisabledWorldMapProvider.CODEC);
        IChunkStorageProvider.CODEC.register(
            DefaultChunkStorageProvider.ID, DefaultChunkStorageProvider.class, DefaultChunkStorageProvider.CODEC);
        IResourceStorageProvider.CODEC.register(
            DefaultResourceStorageProvider.ID, DefaultResourceStorageProvider.class, DefaultResourceStorageProvider.CODEC);
    }

    private static void write(Path path) {
        final WorldConfig config = new WorldConfig();
        config.setUuid(GOLDEN_UUID);
        config.setDisplayName(GOLDEN_DISPLAY_NAME);
        config.setSeed(GOLDEN_SEED);
        config.setSpawnProvider(new GlobalSpawnProvider(
            new Transform(10.5, 80.0, -3.5, 0.0f, 90.0f, 0.0f)));
        config.setWorldGenProvider(new VoidWorldGenProvider(
            new Color((byte) 0x5b, (byte) 0x9e, (byte) 0x28), "Env_Zone1_Plains"));
        config.setWorldMapProvider(new DisabledWorldMapProvider());
        config.setChunkStorageProvider(new DefaultChunkStorageProvider());
        config.getChunkConfig().setPregenerateRegion(new Box2D(-512, -512, 512, 512));
        config.getChunkConfig().setKeepLoadedRegion(new Box2D(-64, -64, 64, 64));
        config.setTicking(false);
        config.setBlockTicking(true);
        config.setPvpEnabled(true);
        config.setFallDamageEnabled(false);
        config.setGameTimePaused(true);
        config.setGameTime(Instant.parse(GOLDEN_GAME_TIME));
        config.setGameMode(GameMode.Adventure);
        config.setGameplayConfig(GOLDEN_GAMEPLAY_CONFIG);
        config.setSpawningNPC(false);
        config.setBlockSpawnersResolved(true);
        config.setIsSpawnMarkersEnabled(false);
        config.setIsAllNPCFrozen(true);
        config.setSavingPlayers(false);
        config.setCanSaveChunks(true);
        config.setSaveNewChunks(false);
        config.setCanUnloadChunks(false);
        config.setObjectiveMarkersEnabled(true);
        config.setCompassUpdating(false);
        config.setDisabledFluidTickers(Set.of("Fluid=Water"));
        final ClientEffectWorldSettings effects = new ClientEffectWorldSettings();
        effects.setSunHeightPercent(75.0f);
        effects.setSunAngleRadians(1.25f);
        effects.setBloomIntensity(0.5f);
        effects.setBloomPower(4.0f);
        effects.setSunIntensity(0.75f);
        effects.setSunshaftIntensity(0.25f);
        effects.setSunshaftScaleFactor(2.0f);
        config.setClientEffects(effects);

        WorldConfig.save(path, config).join();
        System.out.println("wrote world config: " + path);
    }

    private static void probe(Path path) {
        final BsonDocument document = BsonUtil.readDocumentNow(path);
        if (document == null) {
            throw new IllegalStateException("could not read world config: " + path);
        }
        // RegistryDumper.boot cannot load the GameplayConfig assets (they depend on
        // the camera-effect store), so the codec validator for that key cannot run
        // in this harness. Check the stored value, then decode the rest with the
        // real codec. The value is the Java default; a production boot validates it.
        require(document.containsKey("GameplayConfig"), "GameplayConfig", GOLDEN_GAMEPLAY_CONFIG, "<missing>");
        final String storedGameplayConfig = document.getString("GameplayConfig").getValue();
        final BsonDocument withoutGameplayConfig = document.clone();
        withoutGameplayConfig.remove("GameplayConfig");
        final WorldConfig config = WorldConfig.CODEC.decode(withoutGameplayConfig, new ExtraInfo());
        require(GOLDEN_GAMEPLAY_CONFIG.equals(storedGameplayConfig), "GameplayConfig",
            GOLDEN_GAMEPLAY_CONFIG, storedGameplayConfig);
        require(config.getUuid().equals(GOLDEN_UUID), "UUID", GOLDEN_UUID, config.getUuid());
        require(GOLDEN_DISPLAY_NAME.equals(config.getDisplayName()), "DisplayName",
            GOLDEN_DISPLAY_NAME, config.getDisplayName());
        require(config.getSeed() == GOLDEN_SEED, "Seed", GOLDEN_SEED, config.getSeed());
        require(config.getGameTime().equals(Instant.parse(GOLDEN_GAME_TIME)), "GameTime",
            GOLDEN_GAME_TIME, config.getGameTime());
        require(GOLDEN_GAMEPLAY_CONFIG.equals(config.getGameplayConfig()), "GameplayConfig",
            GOLDEN_GAMEPLAY_CONFIG, config.getGameplayConfig());
        require(!config.isTicking(), "IsTicking", false, true);
        require(config.isPvpEnabled(), "IsPvpEnabled", true, false);
        require(!config.isFallDamageEnabled(), "IsFallDamageEnabled", false, true);
        require(config.isGameTimePaused(), "IsGameTimePaused", true, false);
        require(config.getGameMode() == GameMode.Adventure, "GameMode",
            GameMode.Adventure, config.getGameMode());
        require(!config.isSavingPlayers(), "IsSavingPlayers", false, true);
        require(!config.shouldSaveNewChunks(), "SaveNewChunks", false, true);
        require(!config.canUnloadChunks(), "IsUnloadingChunks", false, true);
        require(!config.isCompassUpdating(), "IsCompassUpdating", false, true);
        require(!config.isSpawningNPC(), "IsSpawningNPC", false, true);
        require(config.getDisabledFluidTickers().equals(Set.of("Fluid=Water")),
            "DisabledFluidTickers", "[Fluid=Water]", config.getDisabledFluidTickers());

        System.out.printf("world config %s: uuid=%s seed=%d name=%s%n",
            path, config.getUuid(), config.getSeed(), config.getDisplayName());
        System.out.println("java world config check: ok");
    }

    private static void require(boolean condition, String field, Object expected, Object actual) {
        if (!condition) {
            REAL_ERR.printf("expected %s = %s, found %s%n", field, expected, actual);
            System.exit(1);
        }
    }
}
