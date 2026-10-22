package com.hypixel.hytale.tools.golden;

import com.hypixel.hytale.assetstore.AssetPack;
import com.hypixel.hytale.common.plugin.PluginManifest;
import com.hypixel.hytale.component.ComponentRegistry;
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
import com.hypixel.hytale.server.core.blocktype.component.BlockPhysics;

import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.file.Path;
import java.util.List;
import java.util.concurrent.CopyOnWriteArrayList;

/**
 * Shared boot for the storage tools: the minimal {@code AssetModule} and
 * {@code LegacyModule} singletons plus the chunk component registrations.
 *
 * <p>The tools do not start a full server; they install the same component ids
 * as {@code LegacyModule.setup()} so the real {@code ChunkStore.REGISTRY} codec
 * can serialize and deserialize chunk holder documents.
 */
final class ChunkStoreBootstrap {

    private static boolean assetModuleStubbed;
    private static boolean legacyModuleStubbed;

    private ChunkStoreBootstrap() {
    }

    static void stubAssetModule(Path assetRoot, Path repoRoot) throws Exception {
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

    static void registerChunkComponents() throws Exception {
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
        IHeightmap.CODEC.register("Legacy",
            com.hypixel.hytale.server.core.universe.world.chunk.heightmap.LegacyHeightmap.class,
            com.hypixel.hytale.server.core.universe.world.chunk.heightmap.LegacyHeightmap.CODEC);
        IHeightmap.CODEC.register("Tree",
            com.hypixel.hytale.server.core.universe.world.chunk.heightmap.TreeHeightmap.class,
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
            registry.registerComponent(
                com.hypixel.hytale.server.core.universe.world.chunk.section.BlockComponentSection.class,
                "BlockComponent",
                com.hypixel.hytale.server.core.universe.world.chunk.section.BlockComponentSection.CODEC));
        setField(module, "entitySectionComponentType",
            registry.registerComponent(
                com.hypixel.hytale.server.core.universe.world.chunk.section.EntitySection.class,
                "Entity",
                com.hypixel.hytale.server.core.universe.world.chunk.section.EntitySection.CODEC));

        final Class<?> blockTypeModuleClass =
            Class.forName("com.hypixel.hytale.server.core.blocktype.BlockTypeModule");
        final Object blockTypeModule = allocateInstance(blockTypeModuleClass);
        setStaticField(blockTypeModuleClass, "instance", blockTypeModule);
        setField(blockTypeModule, "blockPhysicsComponentType",
            registry.registerComponent(BlockPhysics.class, "BlockPhysics", BlockPhysics.CODEC));

        legacyModuleStubbed = true;
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
