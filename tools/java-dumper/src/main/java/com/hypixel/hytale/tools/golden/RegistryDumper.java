package com.hypixel.hytale.tools.golden;

import com.hypixel.hytale.assetstore.AssetRegistry;
import com.hypixel.hytale.assetstore.AssetStore;
import com.hypixel.hytale.assetstore.iterator.AssetStoreIterator;
import com.hypixel.hytale.assetstore.map.BlockTypeAssetMap;
import com.hypixel.hytale.assetstore.map.DefaultAssetMap;
import com.hypixel.hytale.assetstore.map.IndexedLookupTableAssetMap;
import com.hypixel.hytale.builtin.adventure.farming.config.stages.BlockStateFarmingStageData;
import com.hypixel.hytale.builtin.adventure.farming.config.stages.BlockTypeFarmingStageData;
import com.hypixel.hytale.builtin.adventure.farming.config.stages.PrefabFarmingStageData;
import com.hypixel.hytale.builtin.adventure.farming.config.stages.spread.DirectionalGrowthBehaviour;
import com.hypixel.hytale.builtin.adventure.farming.config.stages.spread.SpreadFarmingStageData;
import com.hypixel.hytale.builtin.adventure.farming.config.stages.spread.SpreadGrowthBehaviour;
import com.hypixel.hytale.builtin.adventure.worldlocationcondition.NeighbourBlockTagsLocationCondition;
import com.hypixel.hytale.builtin.blocktick.procedure.BasicChanceBlockGrowthProcedure;
import com.hypixel.hytale.builtin.blocktick.procedure.SplitChanceBlockGrowthProcedure;
import com.hypixel.hytale.builtin.fallingblocks.BreakFallingBlockImpact;
import com.hypixel.hytale.builtin.fallingblocks.ExplodeFallingBlockImpact;
import com.hypixel.hytale.builtin.fallingblocks.PlaceFallingBlockImpact;
import com.hypixel.hytale.builtin.randomtick.procedures.ChangeIntoBlockProcedure;
import com.hypixel.hytale.builtin.randomtick.procedures.PlaceBlockProcedure;
import com.hypixel.hytale.builtin.randomtick.procedures.SpreadToProcedure;
import com.hypixel.hytale.codec.lookup.Priority;
import com.hypixel.hytale.event.EventBus;
import com.hypixel.hytale.protocol.BenchType;
import com.hypixel.hytale.server.core.asset.HytaleAssetStore;
import com.hypixel.hytale.server.core.asset.type.blocktick.config.RandomTickProcedure;
import com.hypixel.hytale.server.core.asset.type.blocktick.config.TickProcedure;
import com.hypixel.hytale.server.core.asset.type.blocktype.config.BlockType;
import com.hypixel.hytale.server.core.asset.type.blocktype.config.bench.Bench;
import com.hypixel.hytale.server.core.asset.type.blocktype.config.bench.CraftingBench;
import com.hypixel.hytale.server.core.asset.type.blocktype.config.bench.DiagramCraftingBench;
import com.hypixel.hytale.server.core.asset.type.blocktype.config.bench.ProcessingBench;
import com.hypixel.hytale.server.core.asset.type.blocktype.config.bench.StructuralCraftingBench;
import com.hypixel.hytale.server.core.asset.type.blocktype.config.fallingblocks.FallingBlockImpact;
import com.hypixel.hytale.server.core.asset.type.blocktype.config.farming.FarmingStageData;
import com.hypixel.hytale.server.core.asset.type.environment.config.Environment;
import com.hypixel.hytale.server.core.asset.type.entityeffect.config.EntityEffect;
import com.hypixel.hytale.server.core.asset.type.fluid.DefaultFluidTicker;
import com.hypixel.hytale.server.core.asset.type.fluid.FiniteFluidTicker;
import com.hypixel.hytale.server.core.asset.type.fluid.FireFluidTicker;
import com.hypixel.hytale.server.core.asset.type.fluid.Fluid;
import com.hypixel.hytale.server.core.asset.type.fluid.FluidTicker;
import com.hypixel.hytale.server.core.asset.type.gameplay.GameplayConfig;
import com.hypixel.hytale.server.core.asset.type.item.config.Item;
import com.hypixel.hytale.server.core.asset.type.itemanimation.config.ItemPlayerAnimations;
import com.hypixel.hytale.server.core.asset.type.model.config.ModelAsset;
import com.hypixel.hytale.server.core.asset.type.particle.config.ParticleSystem;
import com.hypixel.hytale.server.core.asset.type.soundevent.config.SoundEvent;
import com.hypixel.hytale.server.core.asset.type.tagpattern.config.TagPattern;
import com.hypixel.hytale.server.core.asset.type.trail.config.Trail;
import com.hypixel.hytale.server.core.entity.entities.player.movement.MovementConfig;
import com.hypixel.hytale.server.core.modules.entity.hitboxcollision.HitboxCollisionConfig;
import com.hypixel.hytale.server.core.modules.entity.repulsion.RepulsionConfig;
import com.hypixel.hytale.server.core.modules.entitystats.asset.EntityStatType;
import com.hypixel.hytale.server.core.modules.interaction.interaction.UnarmedInteractions;
import com.hypixel.hytale.server.core.modules.interaction.interaction.config.Interaction;
import com.hypixel.hytale.server.core.modules.interaction.interaction.config.RootInteraction;
import com.hypixel.hytale.server.core.modules.interaction.interaction.config.none.ChangeActiveSlotInteraction;
import com.hypixel.hytale.server.core.modules.interaction.interaction.config.none.simple.SendMessageInteraction;
import com.hypixel.hytale.server.core.modules.interaction.interaction.config.server.combat.DirectionalKnockback;
import com.hypixel.hytale.server.core.modules.interaction.interaction.config.server.combat.ForceKnockback;
import com.hypixel.hytale.server.core.modules.interaction.interaction.config.server.combat.Knockback;
import com.hypixel.hytale.server.core.modules.interaction.interaction.config.server.combat.PointKnockback;
import com.hypixel.hytale.server.core.universe.world.connectedblocks.ConnectedBlockRuleSet;
import com.hypixel.hytale.server.core.universe.world.connectedblocks.CustomConnectedBlockPattern;
import com.hypixel.hytale.server.core.universe.world.connectedblocks.CustomConnectedBlockTemplateAsset;
import com.hypixel.hytale.server.core.universe.world.connectedblocks.CustomTemplateConnectedBlockPattern;
import com.hypixel.hytale.server.core.universe.world.connectedblocks.CustomTemplateConnectedBlockRuleSet;
import com.hypixel.hytale.server.core.universe.world.connectedblocks.builtin.RoofConnectedBlockRuleSet;
import com.hypixel.hytale.server.core.universe.world.connectedblocks.builtin.StairConnectedBlockRuleSet;
import com.hypixel.hytale.server.core.universe.world.connectedblocks.config.PatternedConnectedBlockRuleSet;
import com.hypixel.hytale.server.core.universe.world.connectedblocks.config.PatternedConnectedBlockRuleSetAsset;
import com.hypixel.hytale.server.core.universe.world.connectedblocks.config.rule.ConnectedBlockAndRule;
import com.hypixel.hytale.server.core.universe.world.connectedblocks.config.rule.ConnectedBlockFaceTagRule;
import com.hypixel.hytale.server.core.universe.world.connectedblocks.config.rule.ConnectedBlockNotRule;
import com.hypixel.hytale.server.core.universe.world.connectedblocks.config.rule.ConnectedBlockOrRule;
import com.hypixel.hytale.server.core.universe.world.connectedblocks.config.rule.ConnectedBlockRule;
import com.hypixel.hytale.server.core.universe.world.connectedblocks.config.rule.ConnectedBlockShapeRule;
import com.hypixel.hytale.server.core.universe.world.connectedblocks.config.shape.ConnectedBlockBlockTypeShape;
import com.hypixel.hytale.server.core.universe.world.connectedblocks.config.shape.ConnectedBlockShapeConfig;
import com.hypixel.hytale.server.core.universe.world.connectedblocks.config.shape.ConnectedBlockTagShape;
import com.hypixel.hytale.server.core.universe.world.connectedblocks.config.shape.ConnectedBlockTemplatedShape;
import com.hypixel.hytale.server.core.universe.world.worldlocationcondition.WorldLocationCondition;

import java.io.IOException;
import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;
import java.lang.invoke.MethodType;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;
import java.nio.file.FileSystem;
import java.nio.file.FileSystems;
import java.nio.file.FileVisitOption;
import java.nio.file.FileVisitResult;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.SimpleFileVisitor;
import java.nio.file.attribute.BasicFileAttributes;
import java.util.ArrayList;
import java.util.List;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;

/**
 * Dumps the real block / fluid / environment registries of the base-game asset
 * pack as golden data.
 *
 * <p>The registries are <em>not</em> described by standalone asset files: the
 * block types live as {@code "BlockType"} objects embedded in the
 * {@code Server/Item/Items/**} item assets and are registered as <em>child</em>
 * assets by {@code ContainedAssetCodec} (mode {@code INHERIT_ID_AND_PARENT}).
 * The numeric ids are handed out by {@code BlockTypeAssetMap.putAll0} in
 * asset-load order, so the ids can only be obtained by running the real asset
 * pipeline over the real pack.
 *
 * <p>This dumper therefore boots the vanilla CoreServer asset pipeline
 * ({@code AssetRegistryLoader}), mounts {@code Assets/Assets.zip} as a zip
 * {@link FileSystem} exactly like the vanilla server does and dumps the
 * resulting index tables. The {@code meta} block of the produced JSON
 * documents the exact boot.
 *
 * <p>The dump is deterministic: the file lists are collected with
 * {@link Files#walkFileTree} (the vanilla mechanism) and the ids only depend
 * on that order plus the (deterministic) hash-map iteration orders inside the
 * Java pipeline. No timestamps are written.
 */
final class RegistryDumper {

    /** The pack the registries are dumped from, relative to the repository root. */
    static final String PACK_PATH = "Assets/Assets.zip";

    /** The pack name used for the asset store (the vanilla default pack key). */
    private static final String PACK_NAME = DefaultAssetMap.DEFAULT_PACK_KEY;

    /**
     * The real stderr. The CoreServer boot replaces {@code System.err} with a
     * logger stream, so the original one is kept for the debug output.
     */
    private static final java.io.PrintStream REAL_ERR = System.err;

    /** Set {@code -Dhytale.dumper.debug=true} to print the store statistics. */
    private static final boolean DEBUG = Boolean.getBoolean("hytale.dumper.debug");

    private RegistryDumper() {
    }

    // ------------------------------------------------------------------
    // Entry point
    // ------------------------------------------------------------------

    /**
     * Boots the asset pipeline over the pack and returns the registry dump.
     *
     * @param repoRoot the repository root (the pack is resolved relative to it)
     */
    static String dumpRegistries(Path repoRoot) throws IOException {
        final Path packPath = repoRoot.resolve(PACK_PATH).normalize();
        if (!Files.isRegularFile(packPath)) {
            throw new IOException("asset pack not found: " + packPath
                + " (the pack is required to dump the real registries)");
        }

        // --- boot -------------------------------------------------------
        try {
            installServerStubs();
        } catch (Exception e) {
            throw new IOException("Failed to boot the CoreServer asset pipeline", e);
        }

        // Loading the vanilla registration registers every vanilla asset store
        // (paths, codecs, load order and pre-loaded assets) exactly like the
        // server does.
        try {
            loadVanillaStores();
        } catch (ClassNotFoundException e) {
            throw new IOException("Failed to register the vanilla asset stores", e);
        }
        registerModuleStores();
        registerCodecVariants();

        // --- mount the pack ---------------------------------------------
        try (FileSystem fs = FileSystems.newFileSystem(packPath, (ClassLoader) null)) {
            final Path root = fs.getPath("").toAbsolutePath().normalize();
            loadCommonAssets(root);

            final Path serverDir = root.resolve("Server");
            loadAssetStores(serverDir);

            return writeDump(root);
        }
    }

    // ------------------------------------------------------------------
    // Boot
    // ------------------------------------------------------------------

    /**
     * Installs the minimal server singletons the asset pipeline touches.
     *
     * <p>The vanilla pipeline reaches for {@code HytaleServer.get()} (the event
     * bus) and {@code Universe.get()} (the player count) while loading assets.
     * A full server cannot be booted inside the dumper, so both singletons are
     * replaced by inert stubs: an {@link EventBus} without listeners and a
     * universe without players.
     */
    private static void installServerStubs() throws Exception {
        // The option set has to exist before any CoreServer class is loaded
        // (HytaleAssetStore's static initializer reads it).
        com.hypixel.hytale.server.core.Options.parse(new String[0]);

        final Class<?> serverClass = Class.forName("com.hypixel.hytale.server.core.HytaleServer");
        final Object server = allocateInstance(serverClass);
        setField(serverClass, server, "eventBus", new EventBus(false));
        setStaticField(serverClass, "instance", server);

        final Class<?> universeClass = Class.forName("com.hypixel.hytale.server.core.universe.Universe");
        final Object universe = allocateInstance(universeClass);
        setField(universeClass, universe, "playersByUuid", new ConcurrentHashMap<>());
        setStaticField(universeClass, "instance", universe);
    }

    /** Registers every vanilla asset store (the vanilla static initializer). */
    private static void loadVanillaStores() throws ClassNotFoundException {
        Class.forName("com.hypixel.hytale.server.core.asset.AssetRegistryLoader");
    }

    /**
     * Registers the codec variants the vanilla server registers in its plugin
     * setup.
     *
     * <p>The variant codecs are registered by the plugin modules (the fluid
     * plugin and the block type module), not by {@code AssetRegistryLoader}.
     * Without them the fluid assets (which carry a {@code Ticker}) and the
     * blocks with a {@code Bench} cannot be decoded.
     */
    private static void registerCodecVariants() {
        FluidTicker.CODEC.register(Priority.DEFAULT, "Default", DefaultFluidTicker.class, DefaultFluidTicker.CODEC);
        FluidTicker.CODEC.register("Fire", FireFluidTicker.class, FireFluidTicker.CODEC);
        FluidTicker.CODEC.register("Finite", FiniteFluidTicker.class, FiniteFluidTicker.CODEC);

        Bench.CODEC.register(BenchType.Crafting, CraftingBench.class, CraftingBench.CODEC);
        Bench.CODEC.register(BenchType.Processing, ProcessingBench.class, ProcessingBench.CODEC);
        Bench.CODEC.register(BenchType.DiagramCrafting, DiagramCraftingBench.class, DiagramCraftingBench.CODEC);
        Bench.CODEC.register(BenchType.StructuralCrafting, StructuralCraftingBench.class, StructuralCraftingBench.CODEC);

        // The connected block rule sets of the blocks (the "ConnectedBlockRuleSet"
        // field of a BlockType).
        CustomTemplateConnectedBlockPattern.CODEC.register("Custom", CustomConnectedBlockPattern.class, CustomConnectedBlockPattern.CODEC);
        ConnectedBlockRuleSet.CODEC.register("CustomTemplate", CustomTemplateConnectedBlockRuleSet.class, CustomTemplateConnectedBlockRuleSet.CODEC);
        ConnectedBlockRuleSet.CODEC.register("Patterned", PatternedConnectedBlockRuleSet.class, PatternedConnectedBlockRuleSet.CODEC);
        ConnectedBlockRuleSet.CODEC.register("Stair", StairConnectedBlockRuleSet.class, StairConnectedBlockRuleSet.CODEC);
        ConnectedBlockRuleSet.CODEC.register("Roof", RoofConnectedBlockRuleSet.class, RoofConnectedBlockRuleSet.CODEC);
        ConnectedBlockShapeConfig.CODEC.register("TagPattern", ConnectedBlockTagShape.class, ConnectedBlockTagShape.CODEC);
        ConnectedBlockShapeConfig.CODEC.register("Templated", ConnectedBlockTemplatedShape.class, ConnectedBlockTemplatedShape.CODEC);
        ConnectedBlockShapeConfig.CODEC.register("Block", ConnectedBlockBlockTypeShape.class, ConnectedBlockBlockTypeShape.CODEC);
        ConnectedBlockRule.CODEC.register("FaceTag", ConnectedBlockFaceTagRule.class, ConnectedBlockFaceTagRule.CODEC);
        ConnectedBlockRule.CODEC.register("Not", ConnectedBlockNotRule.class, ConnectedBlockNotRule.CODEC);
        ConnectedBlockRule.CODEC.register("And", ConnectedBlockAndRule.class, ConnectedBlockAndRule.CODEC);
        ConnectedBlockRule.CODEC.register("Or", ConnectedBlockOrRule.class, ConnectedBlockOrRule.CODEC);
        ConnectedBlockRule.CODEC.register("Shape", ConnectedBlockShapeRule.class, ConnectedBlockShapeRule.CODEC);

        // The block tick procedures of the blocks (the "TickProcedure" and
        // "RandomTickProcedure" fields of a BlockType).
        TickProcedure.CODEC.register("BasicChance", BasicChanceBlockGrowthProcedure.class, BasicChanceBlockGrowthProcedure.CODEC);
        TickProcedure.CODEC.register("SplitChance", SplitChanceBlockGrowthProcedure.class, SplitChanceBlockGrowthProcedure.CODEC);
        RandomTickProcedure.CODEC.register("ChangeIntoBlock", ChangeIntoBlockProcedure.class, ChangeIntoBlockProcedure.CODEC);
        RandomTickProcedure.CODEC.register("SpreadTo", SpreadToProcedure.class, SpreadToProcedure.CODEC);
        RandomTickProcedure.CODEC.register("PlaceBlock", PlaceBlockProcedure.class, PlaceBlockProcedure.CODEC);

        // The farming stages of the blocks (the "Farming" field of a BlockType).
        FarmingStageData.CODEC.register("BlockType", BlockTypeFarmingStageData.class, BlockTypeFarmingStageData.CODEC);
        FarmingStageData.CODEC.register("BlockState", BlockStateFarmingStageData.class, BlockStateFarmingStageData.CODEC);
        FarmingStageData.CODEC.register("Prefab", PrefabFarmingStageData.class, PrefabFarmingStageData.CODEC);
        FarmingStageData.CODEC.register("Spread", SpreadFarmingStageData.class, SpreadFarmingStageData.CODEC);
        SpreadGrowthBehaviour.CODEC.register("Directional", DirectionalGrowthBehaviour.class, DirectionalGrowthBehaviour.CODEC);
        WorldLocationCondition.CODEC.register("NeighbourBlockTags", NeighbourBlockTagsLocationCondition.class, NeighbourBlockTagsLocationCondition.CODEC);

        // The falling block impacts of the blocks (the "FallingBlockSettings"
        // field of a BlockType) and the knockbacks of the explosions (the
        // "ExplosionConfig" field of a BlockType).
        FallingBlockImpact.CODEC.register(Priority.DEFAULT, PlaceFallingBlockImpact.ID, PlaceFallingBlockImpact.class, PlaceFallingBlockImpact.CODEC);
        FallingBlockImpact.CODEC.register(BreakFallingBlockImpact.ID, BreakFallingBlockImpact.class, BreakFallingBlockImpact.CODEC);
        FallingBlockImpact.CODEC.register(ExplodeFallingBlockImpact.ID, ExplodeFallingBlockImpact.class, ExplodeFallingBlockImpact.CODEC);
        Knockback.CODEC.register("Directional", DirectionalKnockback.class, DirectionalKnockback.CODEC);
        Knockback.CODEC.register("Point", PointKnockback.class, PointKnockback.CODEC);
        Knockback.CODEC.register("Force", ForceKnockback.class, ForceKnockback.CODEC);
    }

    /**
     * Registers the asset stores the vanilla server registers from its plugin
     * modules (the {@code AssetRegistryLoader} only covers the core ones).
     *
     * <p>Without them the load-order resolver fails, because the core stores
     * declare {@code loadsAfter} / {@code loadsBefore} dependencies on them
     * (e.g. {@code Item} waits for {@code UnarmedInteractions} and
     * {@code RootInteraction}, {@code BlockType} waits for
     * {@code RootInteraction} and {@code CustomConnectedBlockTemplateAsset}).
     *
     * <p>The registrations below are a verbatim copy of the vanilla module
     * registrations; only the packet generators are left out (they are
     * irrelevant for the registry data and would drag in the client protocol).
     */
    private static void registerModuleStores() {
        AssetRegistry.register(HytaleAssetStore.builder(UnarmedInteractions.class, new DefaultAssetMap<>())
            .setPath("Item/Unarmed/Interactions")
            .setCodec(UnarmedInteractions.CODEC)
            .setKeyFunction(UnarmedInteractions::getId)
            .loadsAfter(RootInteraction.class)
            .build());

        AssetRegistry.register(HytaleAssetStore.builder(Interaction.class, new IndexedLookupTableAssetMap<>(Interaction[]::new))
            .setPath("Item/Interactions")
            .setCodec(Interaction.CODEC)
            .setKeyFunction(Interaction::getId)
            .setReplaceOnRemove(id -> new SendMessageInteraction(id, "Missing interaction: " + id))
            .setIsUnknown(Interaction::isUnknown)
            .loadsAfter(EntityStatType.class, EntityEffect.class, Trail.class, ItemPlayerAnimations.class,
                SoundEvent.class, ParticleSystem.class, ModelAsset.class, HitboxCollisionConfig.class)
            .preLoadAssets(List.of(ChangeActiveSlotInteraction.DEFAULT_INTERACTION))
            .build());

        AssetRegistry.register(HytaleAssetStore.builder(RootInteraction.class, new IndexedLookupTableAssetMap<>(RootInteraction[]::new))
            .setPath("Item/RootInteractions")
            .setCodec(RootInteraction.CODEC)
            .setKeyFunction(RootInteraction::getId)
            .setReplaceOnRemove(RootInteraction::new)
            .loadsAfter(Interaction.class)
            .loadsBefore(BlockType.class, Item.class)
            .preLoadAssets(List.of(ChangeActiveSlotInteraction.DEFAULT_ROOT))
            .build());

        AssetRegistry.register(HytaleAssetStore.builder(HitboxCollisionConfig.class, new IndexedLookupTableAssetMap<>(HitboxCollisionConfig[]::new))
            .setPath("Entity/HitboxCollision")
            .setCodec(HitboxCollisionConfig.CODEC)
            .setKeyFunction(HitboxCollisionConfig::getId)
            .setReplaceOnRemove(HitboxCollisionConfig::new)
            .build());

        AssetRegistry.register(HytaleAssetStore.builder(RepulsionConfig.class, new IndexedLookupTableAssetMap<>(RepulsionConfig[]::new))
            .setPath("Entity/Repulsion")
            .setCodec(RepulsionConfig.CODEC)
            .setKeyFunction(RepulsionConfig::getId)
            .setReplaceOnRemove(RepulsionConfig::new)
            .build());

        AssetRegistry.register(HytaleAssetStore.builder(MovementConfig.class, new IndexedLookupTableAssetMap<>(MovementConfig[]::new))
            .setPath("Entity/MovementConfig")
            .setCodec(MovementConfig.CODEC)
            .setKeyFunction(MovementConfig::getId)
            .setReplaceOnRemove(MovementConfig::new)
            .loadsBefore(GameplayConfig.class)
            .preLoadAssets(List.of(MovementConfig.DEFAULT_MOVEMENT))
            .build());

        AssetRegistry.register(HytaleAssetStore.builder(EntityStatType.class, new IndexedLookupTableAssetMap<>(EntityStatType[]::new))
            .setPath("Entity/Stats")
            .setCodec(EntityStatType.CODEC)
            .setKeyFunction(EntityStatType::getId)
            .setReplaceOnRemove(EntityStatType::getUnknownFor)
            .preLoadAssets(List.of(EntityStatType.UNKNOWN))
            .loadsAfter(SoundEvent.class, ParticleSystem.class)
            .build());

        AssetRegistry.register(HytaleAssetStore.builder(PatternedConnectedBlockRuleSetAsset.class, new DefaultAssetMap<>())
            .setPath("Item/ConnectedBlockRuleSets")
            .setKeyFunction(PatternedConnectedBlockRuleSetAsset::getId)
            .setCodec(PatternedConnectedBlockRuleSetAsset.CODEC)
            .loadsAfter(TagPattern.class)
            .loadsBefore(BlockType.class)
            .build());

        AssetRegistry.register(HytaleAssetStore.builder(CustomConnectedBlockTemplateAsset.class, new DefaultAssetMap<>())
            .setPath("Item/CustomConnectedBlockTemplates")
            .setKeyFunction(CustomConnectedBlockTemplateAsset::getId)
            .setCodec(CustomConnectedBlockTemplateAsset.CODEC)
            .build());
    }

    /**
     * Registers the pack's common assets (textures, models, ...) so that the
     * {@code CommonAssetValidator} accepts the texture / model references of
     * the item and block assets.
     *
     * <p>The vanilla server reads the pre-computed {@code CommonAssetsIndex.hashes}
     * index at the pack root; the dumper does exactly the same and only fills
     * the (static) {@code CommonAssetRegistry}.
     */
    private static void loadCommonAssets(Path root) throws IOException {
        final Path index = root.resolve("CommonAssetsIndex.hashes");
        if (!Files.isRegularFile(index)) {
            throw new IOException("CommonAssetsIndex.hashes not found in the pack");
        }

        final Class<?> registryClass;
        final Class<?> fileAssetClass;
        final Class<?> commonAssetClass;
        final MethodHandle fileAsset;
        final Method addCommonAsset;
        try {
            registryClass = Class.forName(
                "com.hypixel.hytale.server.core.asset.common.CommonAssetRegistry");
            commonAssetClass = Class.forName(
                "com.hypixel.hytale.server.core.asset.common.CommonAsset");
            fileAssetClass = Class.forName(
                "com.hypixel.hytale.server.core.asset.common.asset.FileCommonAsset");
            fileAsset = MethodHandles.lookup().findConstructor(fileAssetClass,
                MethodType.methodType(void.class, Path.class, String.class, String.class, byte[].class));
            addCommonAsset = registryClass.getMethod("addCommonAsset", String.class, commonAssetClass);
        } catch (ReflectiveOperationException e) {
            throw new IOException("Cannot access the common asset registry", e);
        }

        final Path common = root.resolve("Common");
        int count = 0;
        for (String line : Files.readAllLines(index, StandardCharsets.UTF_8)) {
            if (line.isEmpty() || line.startsWith("#")) continue;
            final int space = line.indexOf(' ');
            if (space < 0) continue;
            final String hash = line.substring(0, space);
            final String name = line.substring(space + 1);
            try {
                addCommonAsset.invoke(null, PACK_NAME, fileAsset.invoke(common.resolve(name), name, hash, null));
            } catch (Throwable e) {
                throw new IOException("Failed to register the common asset '" + name + "'", e);
            }
            count++;
        }
        if (count == 0) {
            throw new IOException("CommonAssetsIndex.hashes did not contain any asset");
        }
    }

    /**
     * Pre-loads the built-in assets of every store and then loads the pack's
     * {@code Server} tree, mirroring {@code AssetRegistryLoader}.
     *
     * <p>The vanilla loader uses {@code AssetStore.loadAssetsFromDirectory},
     * which walks the directory with {@link Files#walkFileTree}. The dumper
     * collects the very same file list but keeps it in walk order, so the ids
     * follow the vanilla load order.
     */
    @SuppressWarnings({"unchecked", "rawtypes"})
    private static void loadAssetStores(Path serverDir) throws IOException {
        AssetStore.DISABLE_DYNAMIC_DEPENDENCIES = true;

        final var values = AssetRegistry.getStoreMap().values();
        for (AssetStore<?, ?, ?> assetStore : values) {
            assetStore.simplifyLoadBeforeDependencies();
        }

        // --- pre-loaded assets (EMPTY / UNKNOWN / DEBUG_CUBE / ...) -------
        try (var iterator = new AssetStoreIterator(values)) {
            while (iterator.hasNext()) {
                final AssetStore<?, ?, ?> assetStore = iterator.next();
                if (assetStore == null) {
                    throw new IllegalStateException("circular asset store dependency");
                }
                final List preAdded = assetStore.getPreAddedAssets();
                if (preAdded != null && !preAdded.isEmpty()) {
                    assetStore.loadAssets(PACK_NAME, preAdded);
                }
            }
        }

        // --- the pack's asset tree ---------------------------------------
        try (var iterator = new AssetStoreIterator(values)) {
            while (iterator.hasNext()) {
                final AssetStore<?, ?, ?> assetStore = iterator.next();
                if (assetStore == null) {
                    throw new IllegalStateException("circular asset store dependency");
                }
                final String path = assetStore.getPath();
                if (path == null) continue;
                final Path assetsPath = serverDir.resolve(path);
                if (!Files.isDirectory(assetsPath)) continue;

                final List<Path> files = collectFiles(assetsPath, assetStore.getExtension());
                if (files.isEmpty()) continue;
                assetStore.loadAssetsFromPaths(PACK_NAME, files);
            }
        }
    }

    /**
     * Collects the asset files of a directory the way
     * {@code AssetStore.loadAssetsFromDirectory} does (same walk, same filter).
     */
    private static List<Path> collectFiles(Path assetsPath, String extension) throws IOException {
        final List<Path> files = new ArrayList<>();
        final Set<FileVisitOption> options = Set.of();
        Files.walkFileTree(assetsPath, options, Integer.MAX_VALUE, new SimpleFileVisitor<>() {
            @Override
            public FileVisitResult visitFile(Path file, BasicFileAttributes attrs) {
                if (attrs.isRegularFile() && file.toString().endsWith(extension)) {
                    files.add(file);
                }
                return FileVisitResult.CONTINUE;
            }
        });
        return files;
    }

    // ------------------------------------------------------------------
    // Dump
    // ------------------------------------------------------------------

    private static String writeDump(Path root) {
        if (DEBUG) {
            final var itemStore = AssetRegistry.getAssetStore(Item.class);
            REAL_ERR.println("[debug] items loaded: " + itemStore.getAssetMap().getAssetCount());
            REAL_ERR.println("[debug] block types: " + BlockType.getAssetMap().getNextIndex());
            REAL_ERR.println("[debug] fluids: " + Fluid.getAssetMap().getNextIndex());
            REAL_ERR.println("[debug] environments: " + Environment.getAssetMap().getNextIndex());
        }
        final Phase2Dumper.JsonWriter w = new Phase2Dumper.JsonWriter();
        w.beginObject();

        // --- meta --------------------------------------------------------
        w.name("meta");
        w.beginObject();
        meta(w, root);
        w.endObject();

        // --- blocks ------------------------------------------------------
        final BlockTypeAssetMap<String, BlockType> blockMap = BlockType.getAssetMap();
        final StringBuilder blocks = new StringBuilder();
        int blockCount = 0;
        for (int index = 0; index < blockMap.getNextIndex(); index++) {
            final BlockType blockType = blockMap.getAsset(index);
            if (blockType == null) continue;
            if (blockCount > 0) blocks.append(',');
            blocks.append("{\"name\":").append(Phase2Dumper.JsonWriter.quoteString(blockType.getId()));
            blocks.append(",\"index\":").append(index);
            blocks.append(",\"drawType\":").append(Phase2Dumper.JsonWriter.quoteString(blockType.getDrawType().name()));
            blocks.append(",\"drawTypeId\":").append(blockType.getDrawType().getValue());
            blocks.append(",\"unknown\":").append(blockType.isUnknown());
            blocks.append('}');
            blockCount++;
        }

        w.name("blocks");
        w.beginObject();
        w.name("count").value(blockCount);
        w.name("preLoadOrder").rawArray("\"" + BlockType.EMPTY_KEY + "\",\"" + BlockType.UNKNOWN_KEY
            + "\",\"" + BlockType.DEBUG_CUBE_KEY + "\",\"" + BlockType.DEBUG_MODEL_KEY + "\"");
        w.name("emptyId").value(BlockType.EMPTY_ID);
        w.name("unknownId").value(BlockType.UNKNOWN_ID);
        w.name("debugCubeId").value(BlockType.DEBUG_CUBE_ID);
        w.name("debugModelId").value(BlockType.DEBUG_MODEL_ID);
        w.name("assets").rawArray(blocks.toString());
        w.endObject();

        // --- fluids ------------------------------------------------------
        final IndexedLookupTableAssetMap<String, Fluid> fluidMap = Fluid.getAssetMap();
        final StringBuilder fluids = new StringBuilder();
        int fluidCount = 0;
        for (int index = 0; index < fluidMap.getNextIndex(); index++) {
            final Fluid fluid = fluidMap.getAsset(index);
            if (fluid == null) continue;
            if (fluidCount > 0) fluids.append(',');
            fluids.append("{\"name\":").append(Phase2Dumper.JsonWriter.quoteString(fluid.getId()));
            fluids.append(",\"index\":").append(index);
            fluids.append(",\"maxFluidLevel\":").append(fluid.getMaxFluidLevel());
            fluids.append('}');
            fluidCount++;
        }

        w.name("fluids");
        w.beginObject();
        w.name("count").value(fluidCount);
        w.name("assets").rawArray(fluids.toString());
        w.endObject();

        // --- environments ------------------------------------------------
        final IndexedLookupTableAssetMap<String, Environment> environmentMap = Environment.getAssetMap();
        final StringBuilder environments = new StringBuilder();
        int environmentCount = 0;
        for (int index = 0; index < environmentMap.getNextIndex(); index++) {
            final Environment environment = environmentMap.getAsset(index);
            if (environment == null) continue;
            if (environmentCount > 0) environments.append(',');
            environments.append("{\"name\":").append(Phase2Dumper.JsonWriter.quoteString(environment.getId()));
            environments.append(",\"index\":").append(index);
            environments.append('}');
            environmentCount++;
        }

        w.name("environments");
        w.beginObject();
        w.name("count").value(environmentCount);
        w.name("assets").rawArray(environments.toString());
        w.endObject();

        w.endObject();
        return w.toString();
    }

    private static void meta(Phase2Dumper.JsonWriter w, Path root) {
        w.name("class").value("com.hypixel.hytale.server.core.asset.AssetRegistryLoader");
        w.name("pack").value(PACK_PATH);
        w.name("packFormat").value("zip");
        w.name("boot").value("the vanilla CoreServer asset pipeline: Options.parse([]) for the option "
            + "set, an inert HytaleServer/Universe stub (event bus without listeners, universe without "
            + "players) and the vanilla AssetRegistryLoader static initializer, which registers every "
            + "vanilla asset store with its path, codec and load order");
        w.name("moduleStores").value("the vanilla server registers a few more asset stores from its "
            + "plugin modules (UnarmedInteractions, Interaction, RootInteraction, HitboxCollisionConfig, "
            + "RepulsionConfig, MovementConfig, EntityStatType, PatternedConnectedBlockRuleSetAsset and "
            + "CustomConnectedBlockTemplateAsset); they are registered here with the verbatim vanilla "
            + "definitions (without the packet generators, which would drag in the client protocol)");
        w.name("codecVariants").value("the vanilla server registers the codec variants of its plugins in "
            + "the plugin setup (the fluid tickers, the benches, the connected block rule sets and "
            + "shapes, the block tick procedures, the farming stages, the falling block impacts and the "
            + "knockbacks); they are registered here with the verbatim vanilla definitions, because "
            + "without them the corresponding BlockType and Fluid fields cannot be decoded");
        w.name("loadOrder").value("the pack is mounted as a zip FileSystem "
            + "(java.nio.file.FileSystems.newFileSystem) exactly like the vanilla server; the asset files "
            + "of every store are collected with Files.walkFileTree (the vanilla mechanism) and loaded in "
            + "that order, so the ids follow the zip entry order of the pack. The dumper runs with a single "
            + "worker thread of the common fork join pool "
            + "(-Djava.util.concurrent.ForkJoinPool.common.parallelism=1), because the vanilla pipeline "
            + "decodes the assets in parallel and collects the results in a ConcurrentHashMap whose "
            + "iteration order would otherwise depend on the thread scheduling");
        w.name("idOrder").value("ids are handed out by BlockTypeAssetMap.putAll0 / "
            + "IndexedLookupTableAssetMap.putAll0 in asset-load order: the four pre-loaded block types "
            + "(Empty, Unknown, Debug_Cube, Debug_Model) come first (ids 0-3), then the standalone "
            + "block assets of Server/Item/Block/Blocks and finally the block types embedded in the "
            + "Server/Item/Items item assets");
        w.name("idOrderCaveat").value("the ids are pinned to the load order observed by this dumper. "
            + "They are only identical to a real server's ids when the real server loads the same pack "
            + "in the same order; compare against a real-server dump before relying on the ids.");
        w.name("commonAssets").value("the pack's CommonAssetsIndex.hashes is read into the static "
            + "CommonAssetRegistry so that the CommonAssetValidator accepts the texture / model "
            + "references of the item assets");
        w.name("packetGenerators").value("the vanilla packet generators are registered but never "
            + "run: the universe stub has no players, so no packet is produced");
        w.name("blockTypeSource").value("the block type definitions are the \"BlockType\" objects "
            + "embedded in the Server/Item/Items item assets; Item.CODEC registers them as child assets "
            + "of the BlockType store via ContainedAssetCodec (mode INHERIT_ID_AND_PARENT), so the block "
            + "id equals the item key and the parent chain is inherited from the item's \"Parent\"");
        w.name("deterministic").value("no timestamps are written; the dump is byte-identical across "
            + "runs");
        w.name("knownGaps").value("the prefab list assets (Server/PrefabList) cannot be decoded because "
            + "they reference the prefab store, which the dumper does not boot; the single block type "
            + "that references such a list (ExamplePrefabPlacementBlock_Tree) is therefore missing from "
            + "the registry. The asset stores the registries do not depend on (the interaction, particle "
            + "and weather stores) also report load failures for the same reason - they do not influence "
            + "the block, fluid and environment ids.");
        w.name("packRoot").value(root.toString());
    }

    // ------------------------------------------------------------------
    // Reflection helpers (the server singletons cannot be constructed)
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

    private static void setField(Class<?> clazz, Object target, String name, Object value) throws Exception {
        final Field field = clazz.getDeclaredField(name);
        field.setAccessible(true);
        field.set(target, value);
    }

    private static void setStaticField(Class<?> clazz, String name, Object value) throws Exception {
        final Field field = clazz.getDeclaredField(name);
        field.setAccessible(true);
        field.set(null, value);
    }
}
