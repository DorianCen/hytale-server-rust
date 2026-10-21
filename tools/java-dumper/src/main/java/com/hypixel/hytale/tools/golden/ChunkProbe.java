package com.hypixel.hytale.tools.golden;

import java.nio.file.Path;

/** Temporary probe: runs only the chunk dump (for fast iteration). */
public final class ChunkProbe {
    public static void main(String[] args) throws Exception {
        final Path output = Path.of(args[0]);
        final Path worldDir = Path.of(args[1]);
        final Path repoRoot = Path.of(args[2]);
        final long start = System.nanoTime();
        final String json = ChunkDumper.dumpChunks(worldDir, repoRoot);
        System.err.println("chunk dump took " + (System.nanoTime() - start) / 1_000_000 + " ms, "
            + json.length() + " chars");
        java.nio.file.Files.writeString(output.resolve("chunks.json"), json);
    }
}
