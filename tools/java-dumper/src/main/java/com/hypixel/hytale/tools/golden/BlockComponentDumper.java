package com.hypixel.hytale.tools.golden;

import com.hypixel.hytale.component.Holder;
import com.hypixel.hytale.server.core.universe.world.meta.state.BlockMapMarker;
import com.hypixel.hytale.server.core.universe.world.storage.ChunkStore;
import org.bson.BsonDocument;
import org.bson.BsonValue;

import java.io.PrintStream;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * Dumps the canonical {@code ChunkStore.REGISTRY} holder for the block
 * components of the real {@code Hunt/Generic/Site} prefab.
 *
 * <p>The regular prefab deserializer used by the chunk goldens deliberately
 * skips block component holders, and booting the full server just to decode one
 * marker is unnecessary. This tool registers the real {@code BlockMapMarker}
 * codec, deserializes the prefab's {@code components} document and serializes
 * the resulting holder back through the same registry. It is compiled and run
 * against the release {@code HytaleServer.jar} by
 * {@code run-block-components.sh}.
 *
 * <p>Usage: {@code BlockComponentDumper <repository-root> <output-directory>}
 */
public final class BlockComponentDumper {

    private static final PrintStream REAL_ERR = System.err;
    private static final Path PREFAB =
        Path.of("Assets/extracted/Server/Prefabs/Hunt/Generic/Site.prefab.json");
    private static final String BLOCK_NAME = "Hunt_Trap";

    private BlockComponentDumper() {
    }

    public static void main(String[] args) {
        try {
            run(args);
        } catch (Throwable t) {
            REAL_ERR.println("BlockComponentDumper failed:");
            t.printStackTrace(REAL_ERR);
            System.exit(1);
        }
    }

    private static void run(String[] args) throws Exception {
        if (args.length != 2) {
            REAL_ERR.println("usage: BlockComponentDumper <repository-root> <output-directory>");
            System.exit(2);
        }
        final Path repoRoot = Path.of(args[0]).toAbsolutePath().normalize();
        final Path outDir = Path.of(args[1]).toAbsolutePath().normalize();

        ChunkStore.REGISTRY.registerComponent(
            BlockMapMarker.class, "BlockMapMarker", BlockMapMarker.CODEC);

        final BsonDocument prefab = BsonDocument.parse(
            Files.readString(repoRoot.resolve(PREFAB)));
        final BsonDocument components = findBlockComponents(prefab);
        final Holder<ChunkStore> holder = ChunkStore.REGISTRY.deserialize(components);
        final BsonDocument serialized = ChunkStore.REGISTRY.serialize(holder);

        Files.createDirectories(outDir);
        Files.writeString(outDir.resolve("hunt-site.json"), serialized.toJson() + "\n");
    }

    private static BsonDocument findBlockComponents(BsonDocument prefab) {
        for (BsonValue value : prefab.getArray("blocks")) {
            final BsonDocument block = value.asDocument();
            final BsonValue name = block.get("name");
            if (name == null || !name.asString().getValue().equals(BLOCK_NAME)) {
                continue;
            }
            final BsonValue components = block.get("components");
            if (components == null) {
                throw new IllegalStateException("prefab block has no components: " + BLOCK_NAME);
            }
            return components.asDocument();
        }
        throw new IllegalStateException("prefab block not found: " + BLOCK_NAME);
    }
}
