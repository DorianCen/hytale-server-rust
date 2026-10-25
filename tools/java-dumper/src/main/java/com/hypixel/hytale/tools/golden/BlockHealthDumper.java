package com.hypixel.hytale.tools.golden;

import com.hypixel.hytale.codec.ExtraInfo;
import com.hypixel.hytale.server.core.modules.blockhealth.BlockHealthSection;
import com.hypixel.hytale.server.core.util.BsonUtil;
import org.bson.BsonDocument;

import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Instant;

/**
 * Writes a Java {@link BlockHealthSection} v0 BSON document with a fixed set of
 * damaged and fragile section indices. The Rust storage gate decodes it with
 * the section codec and checks every health/last-damage/fragility value.
 */
public final class BlockHealthDumper {
    private BlockHealthDumper() {
    }

    public static void main(String[] args) throws Exception {
        if (args.length < 1) {
            System.err.println("usage: BlockHealthDumper <output-directory> [world-directory] [repo-root]");
            System.exit(2);
        }
        final Path outDir = Path.of(args[0]).toAbsolutePath().normalize();
        final BlockHealthSection section = new BlockHealthSection();
        section.damage(17, 0.25f, Instant.ofEpochMilli(1_700_000_000_000L));
        section.damage(1024, 0.75f, Instant.ofEpochMilli(1_700_000_001_234L));
        section.makeFragile(17, 3.5f);
        section.makeFragile(2048, 0.5f);

        final BsonDocument document = BlockHealthSection.CODEC
            .encode(section, new ExtraInfo())
            .asDocument();
        final byte[] bytes = BsonUtil.writeToBytes(document);
        Files.createDirectories(outDir);
        Files.write(outDir.resolve("block-health-section.bson"), bytes);
        System.out.println("block health probe: ok");
    }
}
