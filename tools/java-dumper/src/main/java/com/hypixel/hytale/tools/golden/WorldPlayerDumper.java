package com.hypixel.hytale.tools.golden;

import com.hypixel.hytale.codec.ExtraInfo;
import com.hypixel.hytale.codec.lookup.Priority;
import com.hypixel.hytale.component.ComponentType;
import com.hypixel.hytale.component.Holder;
import com.hypixel.hytale.math.vector.Rotation3f;
import com.hypixel.hytale.server.core.asset.type.model.config.Model;
import com.hypixel.hytale.server.core.entity.UUIDComponent;
import com.hypixel.hytale.server.core.inventory.InventoryComponent;
import com.hypixel.hytale.server.core.inventory.ItemStack;
import com.hypixel.hytale.server.core.inventory.container.EmptyItemContainer;
import com.hypixel.hytale.server.core.inventory.container.ItemContainer;
import com.hypixel.hytale.server.core.inventory.container.SimpleItemContainer;
import com.hypixel.hytale.server.core.modules.entity.component.PersistentModel;
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
 * <p>The core fixture contains {@code UUID}, {@code Transform} and
 * {@code WorldGenId}. The equipment fixture adds the saved
 * {@code PersistentModel} reference and the armor/hotbar/utility inventory
 * components. Unknown components are preserved by the Rust holder, so a full
 * Java player save still round-trips.
 *
 * <p>Usage (the first three arguments match {@code run.sh}):
 * <pre>
 * WorldPlayerDumper &lt;out-dir&gt; &lt;world-dir&gt; &lt;repo-root&gt;
 *     write|write-equipment|probe|probe-equipment &lt;path&gt;
 * </pre>
 */
public final class WorldPlayerDumper {

    private static final PrintStream REAL_ERR = System.err;

    static final UUID GOLDEN_UUID = UUID.fromString("12345678-1234-5678-9abc-def012345678");
    static final UUID EQUIPMENT_UUID = UUID.fromString("22345678-1234-5678-9abc-def012345678");
    static final double GOLDEN_X = 10.5;
    static final double GOLDEN_Y = 80.0;
    static final double GOLDEN_Z = -3.5;
    static final float GOLDEN_PITCH = 0.25f;
    static final float GOLDEN_YAW = 1.5707964f;
    static final float GOLDEN_ROLL = -0.125f;
    static final int GOLDEN_WORLD_GEN_ID = 42;
    static final String GOLDEN_MODEL_ID = "Player";
    static final float GOLDEN_MODEL_SCALE = 1.0f;
    static final String ARMOR_HEAD = "Armor_Adamantite_Head";
    static final String ARMOR_CHEST = "Armor_Adamantite_Chest";
    static final String ARMOR_HANDS = "Armor_Adamantite_Hands";
    static final String ARMOR_LEGS = "Armor_Adamantite_Legs";
    static final String HOTBAR_ITEM = "Weapon_Longsword_Adamantite";
    static final String UTILITY_ITEM = "Potion_Health";

    private static boolean componentsRegistered;
    private static ComponentType<EntityStore, UUIDComponent> uuidType;
    private static ComponentType<EntityStore, TransformComponent> transformType;
    private static ComponentType<EntityStore, WorldGenId> worldGenIdType;
    private static ComponentType<EntityStore, PersistentModel> persistentModelType;
    private static ComponentType<EntityStore, InventoryComponent.Armor> armorType;
    private static ComponentType<EntityStore, InventoryComponent.Hotbar> hotbarType;
    private static ComponentType<EntityStore, InventoryComponent.Utility> utilityType;

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
            case "write" -> write(path, false);
            case "write-equipment" -> write(path, true);
            case "probe" -> probe(path, false);
            case "probe-equipment" -> probe(path, true);
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
        persistentModelType = EntityStore.REGISTRY.registerComponent(
            PersistentModel.class, "Model", PersistentModel.CODEC);
        ItemContainer.CODEC.register(Priority.DEFAULT, "Simple",
            SimpleItemContainer.class, SimpleItemContainer.CODEC);
        ItemContainer.CODEC.register("Empty",
            EmptyItemContainer.class, EmptyItemContainer.CODEC);
        armorType = EntityStore.REGISTRY.registerComponent(
            InventoryComponent.Armor.class, "ArmorInventory", InventoryComponent.Armor.CODEC);
        hotbarType = EntityStore.REGISTRY.registerComponent(
            InventoryComponent.Hotbar.class, "HotbarInventory", InventoryComponent.Hotbar.CODEC);
        utilityType = EntityStore.REGISTRY.registerComponent(
            InventoryComponent.Utility.class, "UtilityInventory", InventoryComponent.Utility.CODEC);
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

    private static Holder<EntityStore> equipmentHolder() {
        final Holder<EntityStore> holder = EntityStore.REGISTRY.newHolder();
        holder.addComponent(uuidType, new UUIDComponent(EQUIPMENT_UUID));
        holder.addComponent(transformType, new TransformComponent(
            new Vector3d(GOLDEN_X, GOLDEN_Y, GOLDEN_Z),
            new Rotation3f(GOLDEN_PITCH, GOLDEN_YAW, GOLDEN_ROLL)));
        holder.addComponent(worldGenIdType, new WorldGenId(GOLDEN_WORLD_GEN_ID));
        holder.addComponent(persistentModelType, new PersistentModel(
            new Model.ModelReference(GOLDEN_MODEL_ID, GOLDEN_MODEL_SCALE, null, false)));

        final SimpleItemContainer armor = new SimpleItemContainer((short) 4);
        armor.setItemStackForSlot((short) 0, stack(ARMOR_HEAD, 1));
        armor.setItemStackForSlot((short) 1, stack(ARMOR_CHEST, 1));
        armor.setItemStackForSlot((short) 2, stack(ARMOR_HANDS, 1));
        armor.setItemStackForSlot((short) 3, stack(ARMOR_LEGS, 1));
        holder.addComponent(armorType, new InventoryComponent.Armor(armor));

        final SimpleItemContainer hotbar = new SimpleItemContainer((short) 9);
        hotbar.setItemStackForSlot((short) 0, stack(HOTBAR_ITEM, 1));
        holder.addComponent(hotbarType, new InventoryComponent.Hotbar(hotbar, (byte) 0));

        final SimpleItemContainer utility = new SimpleItemContainer((short) 4);
        utility.setItemStackForSlot((short) 0, stack(UTILITY_ITEM, 3));
        holder.addComponent(utilityType, new InventoryComponent.Utility(utility, (byte) 0));
        return holder;
    }

    /**
     * Decode an item stack through the codec so the dumper does not need the
     * full item asset store just to construct the holder.
     */
    private static ItemStack stack(String itemId, int quantity) {
        final BsonDocument document = BsonDocument.parse(
            "{\"Id\":\"" + itemId + "\",\"Quantity\":" + quantity
                + ",\"QualityOverride\":0}");
        return ItemStack.CODEC.decode(document, new ExtraInfo());
    }

    private static Holder<EntityStore> goldenHolder(boolean equipment) {
        return equipment ? equipmentHolder() : goldenHolder();
    }

    private static void write(Path path, boolean equipment) {
        final BsonDocument document = EntityStore.REGISTRY.serialize(goldenHolder(equipment));
        BsonUtil.writeDocument(path, document).join();
        System.out.println("wrote player data: " + path);
    }

    private static void probe(Path path, boolean equipment) {
        final BsonDocument document = BsonUtil.readDocumentNow(path);
        if (document == null) {
            throw new IllegalStateException("could not read player data: " + path);
        }
        final Holder<EntityStore> holder = EntityStore.REGISTRY.deserialize(document);
        require(holder != null, "holder", "non-null", null);

        final UUIDComponent uuid = holder.getComponent(uuidType);
        final UUID expectedUuid = equipment ? EQUIPMENT_UUID : GOLDEN_UUID;
        require(uuid != null && expectedUuid.equals(uuid.getUuid()), "UUID",
            expectedUuid, uuid == null ? null : uuid.getUuid());

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

        if (equipment) {
            final PersistentModel model = holder.getComponent(persistentModelType);
            require(model != null, "Model", "present", null);
            require(GOLDEN_MODEL_ID.equals(model.getModelReference().getModelAssetId()),
                "Model.Model.Id", GOLDEN_MODEL_ID,
                model.getModelReference().getModelAssetId());
            require(close(model.getModelReference().getScale(), GOLDEN_MODEL_SCALE),
                "Model.Model.Scale", GOLDEN_MODEL_SCALE,
                model.getModelReference().getScale());

            final InventoryComponent.Armor armor = holder.getComponent(armorType);
            require(armor != null, "ArmorInventory", "present", null);
            require(ARMOR_HEAD.equals(armor.getInventory().getItemStack((short) 0).getItemId()),
                "ArmorInventory[0]", ARMOR_HEAD,
                armor.getInventory().getItemStack((short) 0).getItemId());
            require(ARMOR_LEGS.equals(armor.getInventory().getItemStack((short) 3).getItemId()),
                "ArmorInventory[3]", ARMOR_LEGS,
                armor.getInventory().getItemStack((short) 3).getItemId());

            final InventoryComponent.Hotbar hotbar = holder.getComponent(hotbarType);
            require(hotbar != null, "HotbarInventory", "present", null);
            require(hotbar.getActiveSlot() == 0, "HotbarInventory.ActiveSlot", 0,
                hotbar.getActiveSlot());
            require(HOTBAR_ITEM.equals(hotbar.getInventory().getItemStack((short) 0).getItemId()),
                "HotbarInventory[0]", HOTBAR_ITEM,
                hotbar.getInventory().getItemStack((short) 0).getItemId());

            final InventoryComponent.Utility utility = holder.getComponent(utilityType);
            require(utility != null, "UtilityInventory", "present", null);
            require(UTILITY_ITEM.equals(utility.getActiveItem().getItemId()),
                "UtilityInventory.ActiveItem", UTILITY_ITEM, utility.getActiveItem().getItemId());
        }

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
        if (equipment) {
            require(components.containsKey("Model")
                    && components.containsKey("ArmorInventory")
                    && components.containsKey("HotbarInventory")
                    && components.containsKey("UtilityInventory"),
                "Components", "[Model, ArmorInventory, HotbarInventory, UtilityInventory]",
                components.keySet());
        }

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
