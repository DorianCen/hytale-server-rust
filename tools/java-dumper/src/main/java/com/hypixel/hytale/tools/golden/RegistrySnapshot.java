package com.hypixel.hytale.tools.golden;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * Writes a snapshot of the block / fluid / environment registries that are
 * <em>currently loaded in this JVM</em>, in the schema the Rust port consumes
 * ({@code testdata/golden/registries.json}).
 *
 * <p>This is the server-side integration point for the Rust world generator:
 * the numeric ids are assigned by {@code BlockTypeAssetMap.putAll0} in
 * asset-load order, and the vanilla pipeline decodes assets in parallel, so
 * the ids can differ between boots (a few sibling ids and {@code Lava}/{@code
 * Tar}; see {@code REMAINING.md} §6). A server must therefore hand the Rust
 * port its own snapshot instead of the canonical single-threaded golden dump
 * when the numeric ids have to match.
 *
 * <h2>Server usage</h2>
 *
 * <p>Call this after {@code AssetRegistryLoader.loadAssets(...)} has
 * completed (including all mods). The class only reads the already-loaded
 * static asset maps; it does not boot anything:
 *
 * <pre>{@code
 * // e.g. from a startup task / admin command / plugin:
 * RegistrySnapshot.write(Path.of("run/worldgen-registries.json"));
 * }</pre>
 *
 * <p>Then pass the file to the Rust generator:
 * {@code worldgen-cli ... --registries run/worldgen-registries.json}, or
 * {@code WorldGenOptions { registries_path: Some(...), .. }} /
 * {@code ChunkGeneratorJsonLoader::with_registries_path(...)} when embedding
 * the service.
 *
 * <p>The written file has the same {@code blocks} / {@code fluids} /
 * {@code environments} arrays as the golden dump; the Rust loader ignores the
 * {@code meta} block. A snapshot also contains modded block types and closes
 * the golden dump's {@code ExamplePrefabPlacementBlock_Tree} gap, because the
 * live server has the prefab-list store loaded.
 *
 * <h2>Standalone usage (testing / offline validation)</h2>
 *
 * <p>{@link #main(String[])} boots the vanilla pipeline over
 * {@code Assets/Assets.zip} in this process (the same stub boot as
 * {@code RegistryDumper}) and then writes the snapshot. This is useful for
 * validating the writer and the Rust reader, but it is <em>not</em> a live
 * server: its ids depend on the common fork-join parallelism
 * ({@code -Djava.util.concurrent.ForkJoinPool.common.parallelism=<n>}).
 *
 * <pre>{@code
 * # via run.sh's classpath builder:
 * DUMPER_MAIN=com.hypixel.hytale.tools.golden.RegistrySnapshot \
 *   tools/java-dumper/run.sh /tmp/worldgen-registries.json
 * }</pre>
 */
public final class RegistrySnapshot {

    private RegistrySnapshot() {
    }

    /**
     * Writes the snapshot of the currently loaded registries.
     *
     * @param output the target JSON file (parent directories are created)
     * @throws IOException           if the file cannot be written
     * @throws IllegalStateException if the CoreServer asset pipeline has not
     *                               been loaded in this JVM
     */
    public static void write(Path output) throws IOException {
        write(output, null);
    }

    /**
     * Writes the snapshot of the currently loaded registries and records the
     * asset-pack root in the meta block.
     *
     * @param output   the target JSON file (parent directories are created)
     * @param packRoot the asset-pack root for the meta block, or {@code null}
     * @throws IOException           if the file cannot be written
     * @throws IllegalStateException if the CoreServer asset pipeline has not
     *                               been loaded in this JVM
     */
    public static void write(Path output, Path packRoot) throws IOException {
        final String json = RegistryDumper.writeSnapshot(packRoot);
        final Path parent = output.toAbsolutePath().getParent();
        if (parent != null) {
            Files.createDirectories(parent);
        }
        Files.writeString(output, json + "\n", StandardCharsets.UTF_8);
    }

    /**
     * Standalone entry point: boots the vanilla asset pipeline over the pack
     * and writes the snapshot.
     *
     * <p>Arguments: {@code <output> [worldDir] [repoRoot]}. The world
     * directory is accepted for symmetry with {@code GoldenDumper} and
     * ignored; the repository root defaults to the current directory.
     */
    public static void main(String[] args) throws IOException {
        if (args.length < 1) {
            System.err.println("usage: RegistrySnapshot <output.json> [worldDir] [repoRoot]");
            System.exit(2);
            return;
        }
        Path output = Path.of(args[0]);
        if (Files.isDirectory(output)) {
            // `run.sh` creates its output argument as a directory; accept it
            // and write the conventional file name inside it.
            output = output.resolve("registries.json");
        }
        final Path repoRoot = args.length > 2
            ? Path.of(args[2]).toAbsolutePath().normalize()
            : Path.of("").toAbsolutePath().normalize();

        final Path packRoot = RegistryDumper.boot(repoRoot);
        write(output, packRoot);
        System.err.println("Registry snapshot written to " + output.toAbsolutePath()
            + " (blocks=" + com.hypixel.hytale.server.core.asset.type.blocktype.config.BlockType
                .getAssetMap().getNextIndex()
            + ", fluids=" + com.hypixel.hytale.server.core.asset.type.fluid.Fluid
                .getAssetMap().getNextIndex()
            + ", environments=" + com.hypixel.hytale.server.core.asset.type.environment.config.Environment
                .getAssetMap().getNextIndex()
            + ")");
    }
}
