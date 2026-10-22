package com.hypixel.hytale.tools.golden;

import com.hypixel.hytale.component.Holder;
import com.hypixel.hytale.math.util.ChunkUtil;
import com.hypixel.hytale.server.core.asset.type.blocktype.config.BlockType;
import com.hypixel.hytale.server.core.universe.world.chunk.ChunkColumn;
import com.hypixel.hytale.server.core.universe.world.chunk.section.BlockSection;
import com.hypixel.hytale.server.core.universe.world.storage.ChunkStore;
import com.hypixel.hytale.server.core.util.BsonUtil;
import com.hypixel.hytale.storage.IndexedStorageFile;
import org.bson.BsonDocument;

import java.io.PrintStream;
import java.nio.ByteBuffer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;

/**
 * Reads a Rust-written world save with the real Java chunk/storage codecs.
 *
 * <p>This is the reverse compatibility gate: the Rust runtime mutates a chunk,
 * writes the region file, and this probe deserializes it with
 * {@code ChunkStore.REGISTRY} and resolves the block name through the real
 * {@code BlockType} asset map.
 *
 * <p>Usage (the first three arguments match {@code run.sh}):
 * <pre>
 * WorldStorageProbe &lt;out-dir&gt; &lt;world-dir&gt; &lt;repo-root&gt;
 *   &lt;save-dir&gt; &lt;chunkX&gt; &lt;chunkZ&gt; &lt;blockX&gt; &lt;blockY&gt; &lt;blockZ&gt; &lt;expected-name&gt;
 * </pre>
 */
public final class WorldStorageProbe {

    private static final PrintStream REAL_ERR = System.err;

    private WorldStorageProbe() {
    }

    public static void main(String[] args) throws Exception {
        try {
            run(args);
        } catch (Throwable t) {
            REAL_ERR.println("WorldStorageProbe failed:");
            t.printStackTrace(REAL_ERR);
            System.exit(1);
        }
    }

    private static void run(String[] args) throws Exception {
        if (args.length < 10) {
            REAL_ERR.println("usage: WorldStorageProbe <out-dir> <world-dir> <repo-root> "
                + "<save-dir> <chunkX> <chunkZ> <blockX> <blockY> <blockZ> <expected-name>");
            System.exit(2);
        }

        final Path repoRoot = Path.of(args[2]).toAbsolutePath().normalize();
        final Path saveDir = Path.of(args[3]).toAbsolutePath().normalize();
        final int chunkX = Integer.parseInt(args[4]);
        final int chunkZ = Integer.parseInt(args[5]);
        final int blockX = Integer.parseInt(args[6]);
        final int blockY = Integer.parseInt(args[7]);
        final int blockZ = Integer.parseInt(args[8]);
        final String expected = args[9];

        RegistryDumper.boot(repoRoot);
        ChunkStoreBootstrap.stubAssetModule(repoRoot.resolve("Assets/extracted"), repoRoot);
        ChunkStoreBootstrap.registerChunkComponents();

        final int regionX = chunkX >> ChunkUtil.BITS;
        final int regionZ = chunkZ >> ChunkUtil.BITS;
        final int blobIndex = ChunkUtil.indexColumn(chunkX & ChunkUtil.SIZE_MASK, chunkZ & ChunkUtil.SIZE_MASK);
        final Path regionFile = saveDir.resolve("chunks").resolve(regionX + "." + regionZ + ".region.bin");
        if (!Files.isRegularFile(regionFile)) {
            throw new IllegalStateException("region file does not exist: " + regionFile);
        }

        final BsonDocument document;
        try (IndexedStorageFile file = IndexedStorageFile.open(regionFile, StandardOpenOption.READ)) {
            final ByteBuffer buffer = file.readBlob(blobIndex);
            if (buffer == null) {
                throw new IllegalStateException("chunk (" + chunkX + ", " + chunkZ
                    + ") is missing blob " + blobIndex + " in " + regionFile);
            }
            document = BsonUtil.readFromBuffer(buffer);
        }

        final Holder<ChunkStore> holder = ChunkStore.REGISTRY.deserialize(document);
        final ChunkColumn column = holder.getComponent(ChunkColumn.getComponentType());
        if (column == null) {
            throw new IllegalStateException("the document has no ChunkColumn component");
        }
        final Holder<ChunkStore>[] sections = column.getSectionHolders();
        final int sectionY = blockY >> ChunkUtil.BITS;
        if (sections == null || sectionY < 0 || sectionY >= sections.length || sections[sectionY] == null) {
            throw new IllegalStateException("section " + sectionY + " is not stored");
        }
        final BlockSection section = sections[sectionY].getComponent(BlockSection.getComponentType());
        if (section == null) {
            throw new IllegalStateException("section " + sectionY + " has no BlockSection component");
        }

        final int blockId = section.get(blockX & ChunkUtil.SIZE_MASK, blockY & ChunkUtil.SIZE_MASK, blockZ & ChunkUtil.SIZE_MASK);
        final String name = BlockType.getAssetMap()
            .getAssetOrDefault(blockId, BlockType.UNKNOWN)
            .getId();

        System.out.printf("chunk (%d, %d) block (%d, %d, %d) = %d (%s)%n",
            chunkX, chunkZ, blockX, blockY, blockZ, blockId, name);
        if (!expected.equals(name)) {
            REAL_ERR.printf("expected %s at (%d, %d, %d), found %s%n",
                expected, blockX, blockY, blockZ, name);
            System.exit(1);
        }
        System.out.println("java reload check: ok");
    }
}
