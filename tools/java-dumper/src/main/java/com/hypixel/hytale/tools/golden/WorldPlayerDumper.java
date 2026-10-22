package com.hypixel.hytale.tools.golden;

import com.hypixel.hytale.component.ComponentType;
import com.hypixel.hytale.component.Holder;
import com.hypixel.hytale.math.vector.Rotation3f;
import com.hypixel.hytale.server.core.entity.UUIDComponent;
import com.hypixel.hytale.server.core.modules.entity.component.TransformComponent;
import com.hypixel.hytale.server.core.modules.entity.component.WorldGenId;
import com.hypixel.hytale.server.core.universe.world.storage.EntityStore;
import com.hypixel.hytale.server.core.util.BsonUtil;
import org.bson.BsonDocument;
import org.joml.Vector3d;

import java.io.PrintStream;
import java.nio.file.Path;
import java.util.UUID;

/**
 * Writes and checks a player holder document with the real
 * {@code EntityStore.REGISTRY} codec, mirroring
 * {@code DiskPlayerStorageProvider.DiskPlayerStorage}.
 *
 * <p>The fixture contains the core components this server ports first:
 * {@code UUID}, {@code Transform} and {@code WorldGenId}. Unknown components are
 * preserved by the Rust holder, so a full Java player save still round-trips.
 *
 * <p>Usage (the first three arguments match {@code run.sh}):
 * <pre>
 * WorldPlayerDumper &lt;out-dir&gt; &lt;world-dir&gt; &lt;repo-root&gt; write &lt;path&gt;
 * WorldPlayerDumper &lt;out-dir&gt; &lt;world-dir&gt; &lt;repo-root&gt; probe &lt;path&gt;
 * </pre>
 */
public final class WorldPlayerDumper {

    private static final PrintStream REAL_ERR = System.err;

    static final UUID GOLDEN_UUID = UUID.fromString("12345678-1234-5678-9abc-def012345678");
    static final double GOLDEN_X = 10.5;
    static final double GOLDEN_Y = 80.0;
    static final double GOLDEN_Z = -3.5;
    static final float GOLDEN_PITCH = 0.25f;
    static final float GOLDEN_YAW = 1.5707964f;
    static final float GOLDEN_ROLL = -0.125f;
    static final int GOLDEN_WORLD_GEN_ID = 42;

    private static boolean componentsRegistered;
    private static ComponentType<EntityStore, UUIDComponent> uuidType;
    private static ComponentType<EntityStore, TransformComponent> transformType;
    private static ComponentType<EntityStore, WorldGenId> worldGenIdType;

    private WorldPlayerDumper() {
    }

    public static void main(String[] args) throws Exception {
        try {
            run(args);
        } catch (Throwable t) {
            REAL_ERR.println("WorldPlayerDumper failed:");
            t.printStackTrace(REAL_ERR);
            System.exit(1);
        }
    }

    private static void run(String[] args) throws Exception {
        if (args.length < 5) {
            REAL_ERR.println("usage: WorldPlayerDumper <out-dir> <world-dir> <repo-root> "
                + "write|probe <path>");
            System.exit(2);
        }

        final Path path = Path.of(args[4]).toAbsolutePath().normalize();
        registerComponents();

        switch (args[3]) {
            case "write" -> write(path);
            case "probe" -> probe(path);
            default -> {
                REAL_ERR.println("unknown mode: " + args[3]);
                System.exit(2);
            }
        }
    }

    /** Register the same ids as {@code EntityModule.setup()} for the core set. */
    private static void registerComponents() {
        if (componentsRegistered) {
            return;
        }
        uuidType = EntityStore.REGISTRY.registerComponent(UUIDComponent.class, "UUID", UUIDComponent.CODEC);
        transformType = EntityStore.REGISTRY.registerComponent(
            TransformComponent.class, "Transform", TransformComponent.CODEC);
        worldGenIdType = EntityStore.REGISTRY.registerComponent(
            WorldGenId.class, "WorldGenId", WorldGenId.CODEC);
        componentsRegistered = true;
    }

    private static Holder<EntityStore> goldenHolder() {
        final Holder<EntityStore> holder = EntityStore.REGISTRY.newHolder();
        holder.addComponent(uuidType, new UUIDComponent(GOLDEN_UUID));
        holder.addComponent(transformType, new TransformComponent(
            new Vector3d(GOLDEN_X, GOLDEN_Y, GOLDEN_Z),
            new Rotation3f(GOLDEN_PITCH, GOLDEN_YAW, GOLDEN_ROLL)));
        holder.addComponent(worldGenIdType, new WorldGenId(GOLDEN_WORLD_GEN_ID));
        return holder;
    }

    private static void write(Path path) {
        final BsonDocument document = EntityStore.REGISTRY.serialize(goldenHolder());
        BsonUtil.writeDocument(path, document).join();
        System.out.println("wrote player data: " + path);
    }

    private static void probe(Path path) {
        final BsonDocument document = BsonUtil.readDocumentNow(path);
        if (document == null) {
            throw new IllegalStateException("could not read player data: " + path);
        }
        final Holder<EntityStore> holder = EntityStore.REGISTRY.deserialize(document);
        require(holder != null, "holder", "non-null", null);

        final UUIDComponent uuid = holder.getComponent(uuidType);
        require(uuid != null && GOLDEN_UUID.equals(uuid.getUuid()), "UUID",
            GOLDEN_UUID, uuid == null ? null : uuid.getUuid());

        final TransformComponent transform = holder.getComponent(transformType);
        require(transform != null, "Transform", "present", null);
        require(close(transform.getPosition().x, GOLDEN_X), "Transform.Position.X", GOLDEN_X,
            transform.getPosition().x);
        require(close(transform.getPosition().y, GOLDEN_Y), "Transform.Position.Y", GOLDEN_Y,
            transform.getPosition().y);
        require(close(transform.getPosition().z, GOLDEN_Z), "Transform.Position.Z", GOLDEN_Z,
            transform.getPosition().z);
        require(close(transform.getRotation().x, GOLDEN_PITCH), "Transform.Rotation.Pitch",
            GOLDEN_PITCH, transform.getRotation().x);
        require(close(transform.getRotation().y, GOLDEN_YAW), "Transform.Rotation.Yaw",
            GOLDEN_YAW, transform.getRotation().y);
        require(close(transform.getRotation().z, GOLDEN_ROLL), "Transform.Rotation.Roll",
            GOLDEN_ROLL, transform.getRotation().z);

        final WorldGenId worldGenId = holder.getComponent(worldGenIdType);
        require(worldGenId != null && worldGenId.getWorldGenId() == GOLDEN_WORLD_GEN_ID,
            "WorldGenId", GOLDEN_WORLD_GEN_ID, worldGenId == null ? null : worldGenId.getWorldGenId());

        // The holder must serialize back into the same component map, which is
        // what DiskPlayerStorageProvider writes on the next save.
        final BsonDocument reserialized = EntityStore.REGISTRY.serialize(holder);
        final BsonDocument components = reserialized.getDocument("Components");
        require(components != null
                && components.containsKey("UUID")
                && components.containsKey("Transform")
                && components.containsKey("WorldGenId"),
            "Components", "[UUID, Transform, WorldGenId]",
            components == null ? null : components.keySet());

        System.out.printf("player %s: position=(%.1f, %.1f, %.1f) worldGenId=%d%n",
            uuid.getUuid(), transform.getPosition().x, transform.getPosition().y,
            transform.getPosition().z, worldGenId.getWorldGenId());
        System.out.println("java player data check: ok");
    }

    private static boolean close(double actual, double expected) {
        return Math.abs(actual - expected) < 1.0e-6;
    }

    private static void require(boolean condition, String field, Object expected, Object actual) {
        if (!condition) {
            REAL_ERR.printf("expected %s = %s, found %s%n", field, expected, actual);
            System.exit(1);
        }
    }
}
