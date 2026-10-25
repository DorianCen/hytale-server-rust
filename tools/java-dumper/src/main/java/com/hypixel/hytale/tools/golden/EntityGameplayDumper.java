package com.hypixel.hytale.tools.golden;

import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.hypixel.hytale.server.core.asset.type.entityeffect.config.EntityEffect;
import com.hypixel.hytale.server.core.asset.type.projectile.config.Projectile;
import com.hypixel.hytale.server.core.modules.entity.damage.DamageCause;
import com.hypixel.hytale.server.core.modules.entity.damage.ResistanceModifier;
import com.hypixel.hytale.server.core.modules.entitystats.asset.EntityStatType;
import com.hypixel.hytale.server.core.modules.entitystats.modifier.StaticModifier;
import com.hypixel.hytale.server.core.modules.projectile.config.GroundFollowPhysicsConfig;
import com.hypixel.hytale.server.core.modules.projectile.config.PhysicsConfig;
import com.hypixel.hytale.server.core.modules.projectile.config.ProjectileConfig;
import com.hypixel.hytale.server.core.modules.projectile.config.StandardPhysicsConfig;

import java.lang.reflect.Field;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;

/**
 * Emits the loaded {@code EntityStatType}, {@code EntityEffect} and
 * {@code DamageCause} stores as deterministic JSON.
 *
 * <p>The ids come from the real asset pipeline ({@link RegistryDumper#boot}),
 * so they match the {@code IndexedLookupTableAssetMap} order the Java server
 * uses. Only fields the Rust gameplay core models are emitted; presentation
 * assets (particles/models/sounds) are intentionally left out.
 *
 * <p>Two runs are byte-identical: keys are sorted by asset id/key and no
 * timestamps or absolute paths are written.
 *
 * <p>Usage: {@code EntityGameplayDumper <output-directory> [world-directory]
 * [repo-root]} (the same argument order as {@link GoldenDumper}).
 */
public final class EntityGameplayDumper {

    private static final Gson GSON = new GsonBuilder()
        .setPrettyPrinting()
        .disableHtmlEscaping()
        .serializeNulls()
        .create();

    private EntityGameplayDumper() {
    }

    public static void main(String[] args) throws Exception {
        if (args.length < 1) {
            System.err.println("usage: EntityGameplayDumper <output-directory> [world-directory] [repo-root]");
            System.exit(2);
        }
        final Path outDir = Path.of(args[0]).toAbsolutePath().normalize();
        final Path repoRoot = args.length >= 3
            ? Path.of(args[2]).toAbsolutePath().normalize()
            : Path.of("").toAbsolutePath().normalize();
        final String packPath = System.getenv().getOrDefault("ASSET_PACK", RegistryDumper.PACK_PATH);

        RegistryDumper.enableGameplayConditions();
        RegistryDumper.boot(repoRoot, packPath);

        final JsonObject root = new JsonObject();
        final JsonObject meta = new JsonObject();
        meta.addProperty("producer", EntityGameplayDumper.class.getName());
        meta.addProperty("pack", packPath);
        meta.addProperty("boot", "the vanilla CoreServer asset pipeline (RegistryDumper.boot): "
            + "AssetRegistryLoader stores, module stores, preloaded Unknown entries and the pack "
            + "Server tree loaded in Java walk order");
        meta.addProperty("ids", "IndexedLookupTableAssetMap ids from the loaded store");
        meta.addProperty("order", "stats/effects/damageCauses objects are sorted by asset id, then key");
        meta.addProperty("fields", "only the gameplay-core fields modelled by hytale-entities; "
            + "presentation assets (models, particles, sounds, cameras) are omitted");
        root.add("meta", meta);
        root.add("stats", dumpStats());
        root.add("effects", dumpEffects());
        root.add("damageCauses", dumpDamageCauses());
        root.add("projectiles", dumpProjectiles());
        root.add("projectileConfigs", dumpProjectileConfigs());

        Files.createDirectories(outDir);
        Files.writeString(outDir.resolve("entity-gameplay.json"), GSON.toJson(root) + "\n");
        System.out.println("entity gameplay dump: ok");
    }

    private static JsonObject dumpStats() {
        final Map<String, EntityStatType> assets = new TreeMap<>(EntityStatType.getAssetMap().getAssetMap());
        final JsonObject out = new JsonObject();
        for (Map.Entry<String, EntityStatType> entry : assets.entrySet()) {
            final EntityStatType stat = entry.getValue();
            final JsonObject json = new JsonObject();
            json.addProperty("id", stat.getId());
            json.addProperty("index", EntityStatType.getAssetMap().getIndex(stat.getId()));
            json.addProperty("initialValue", stat.getInitialValue());
            json.addProperty("min", stat.getMin());
            json.addProperty("max", stat.getMax());
            json.addProperty("shared", stat.isShared());
            json.addProperty("resetType", stat.getResetBehavior().name());
            json.addProperty("ignoreInvulnerability", stat.getIgnoreInvulnerability());
            json.addProperty("hideFromTooltip", field(stat, "hideFromTooltip", Boolean.class));
            final JsonArray regenerating = new JsonArray();
            final EntityStatType.Regenerating[] rules = stat.getRegenerating();
            if (rules != null) {
                for (EntityStatType.Regenerating rule : rules) {
                    final JsonObject regen = new JsonObject();
                    regen.addProperty("interval", rule.getInterval());
                    regen.addProperty("amount", rule.getAmount());
                    regen.addProperty("clampAtZero", field(rule, "clampAtZero", Boolean.class));
                    regen.addProperty("regenType", rule.getRegenType().name());
                    regen.addProperty("conditionCount",
                        rule.getConditions() == null ? 0 : rule.getConditions().length);
                    regen.addProperty("modifierCount",
                        rule.getModifiers() == null ? 0 : rule.getModifiers().length);
                    regenerating.add(regen);
                }
            }
            json.add("regenerating", regenerating);
            out.add(entry.getKey(), json);
        }
        return out;
    }

    private static JsonObject dumpEffects() {
        final Map<String, EntityEffect> assets = new TreeMap<>(EntityEffect.getAssetMap().getAssetMap());
        final JsonObject out = new JsonObject();
        for (Map.Entry<String, EntityEffect> entry : assets.entrySet()) {
            final EntityEffect effect = entry.getValue();
            final JsonObject json = new JsonObject();
            json.addProperty("id", effect.getId());
            json.addProperty("index", EntityEffect.getAssetMap().getIndex(effect.getId()));
            json.addProperty("name", effect.getName());
            json.addProperty("duration", effect.getDuration());
            json.addProperty("overlapBehavior", effect.getOverlapBehavior().name());
            json.addProperty("removalBehavior", effect.getRemovalBehavior().name());
            json.addProperty("infinite", effect.isInfinite());
            json.addProperty("debuff", effect.isDebuff());
            json.addProperty("statusEffectIcon", effect.getStatusEffectIcon());
            json.addProperty("invulnerable", effect.isInvulnerable());
            json.addProperty("valueType", effect.getValueType().name());
            json.addProperty("damageCalculatorCooldown", effect.getDamageCalculatorCooldown());
            json.addProperty("hasDamageCalculator", effect.getDamageCalculator() != null);
            json.addProperty("hasDamageEffects", effect.getDamageEffects() != null);
            json.addProperty("hasApplicationEffects", effect.getApplicationEffects() != null);
            json.addProperty("applyConditionCount",
                effect.getApplyConditions() == null ? 0 : effect.getApplyConditions().length);

            final JsonObject entityStats = new JsonObject();
            if (effect.getEntityStats() != null) {
                effect.getEntityStats().int2FloatEntrySet().forEach(stat -> {
                    entityStats.addProperty(statName(stat.getIntKey()), stat.getFloatValue());
                });
            }
            json.add("entityStats", entityStats);

            final JsonObject statModifiers = new JsonObject();
            if (effect.getStatModifiers() != null) {
                effect.getStatModifiers().int2ObjectEntrySet().forEach(stat -> {
                    final JsonArray modifiers = new JsonArray();
                    for (StaticModifier modifier : stat.getValue()) {
                        final JsonObject value = new JsonObject();
                        value.addProperty("target", modifier.getTarget().name());
                        value.addProperty("calculationType", modifier.getCalculationType().name());
                        value.addProperty("amount", modifier.getAmount());
                        modifiers.add(value);
                    }
                    statModifiers.add(statName(stat.getIntKey()), modifiers);
                });
            }
            json.add("statModifiers", statModifiers);

            json.add("damageResistance", resistanceMap(effect.getDamageResistanceValues()));
            json.add("outgoingDamage", resistanceMap(effect.getOutgoingDamageValues()));
            out.add(entry.getKey(), json);
        }
        return out;
    }

    private static JsonObject dumpDamageCauses() {
        final Map<String, DamageCause> assets = new TreeMap<>(DamageCause.getAssetMap().getAssetMap());
        final JsonObject out = new JsonObject();
        for (Map.Entry<String, DamageCause> entry : assets.entrySet()) {
            final DamageCause cause = entry.getValue();
            final JsonObject json = new JsonObject();
            json.addProperty("id", cause.getId());
            json.addProperty("index", DamageCause.getAssetMap().getIndex(cause.getId()));
            json.addProperty("inherits", cause.getInherits());
            json.addProperty("damageTextColor", field(cause, "damageTextColor", String.class));
            json.addProperty("durabilityLoss", cause.isDurabilityLoss());
            json.addProperty("staminaLoss", cause.isStaminaLoss());
            json.addProperty("bypassResistances", cause.doesBypassResistances());
            json.addProperty("animationId", cause.getAnimationId());
            json.addProperty("deathAnimationId", cause.getDeathAnimationId());
            json.addProperty("deathMessageKey", cause.getDeathMessageKey());
            out.add(entry.getKey(), json);
        }
        return out;
    }

    private static JsonObject dumpProjectiles() {
        final Map<String, Projectile> assets = new TreeMap<>(Projectile.getAssetMap().getAssetMap());
        final JsonObject out = new JsonObject();
        for (Map.Entry<String, Projectile> entry : assets.entrySet()) {
            final Projectile p = entry.getValue();
            final JsonObject json = new JsonObject();
            json.addProperty("id", p.getId());
            json.addProperty("appearance", p.getAppearance());
            json.addProperty("radius", p.getRadius());
            json.addProperty("height", p.getHeight());
            json.addProperty("verticalCenterShot", p.getVerticalCenterShot());
            json.addProperty("horizontalCenterShot", p.getHorizontalCenterShot());
            json.addProperty("depthShot", p.getDepthShot());
            json.addProperty("pitchAdjustShot", p.isPitchAdjustShot());
            json.addProperty("muzzleVelocity", p.getMuzzleVelocity());
            json.addProperty("terminalVelocity", p.getTerminalVelocity());
            json.addProperty("gravity", p.getGravity());
            json.addProperty("bounciness", p.getBounciness());
            json.addProperty("impactSlowdown", p.getImpactSlowdown());
            json.addProperty("sticksVertically", p.isSticksVertically());
            json.addProperty("computeYaw", p.isComputeYaw());
            json.addProperty("computePitch", p.isComputePitch());
            json.addProperty("computeRoll", p.isComputeRoll());
            json.addProperty("rotationMode", p.getRotationMode().name());
            json.addProperty("timeToLive", p.getTimeToLive());
            json.addProperty("damage", p.getDamage());
            json.addProperty("deadTime", p.getDeadTime());
            json.addProperty("deadTimeMiss", p.getDeadTimeMiss());
            json.addProperty("deathEffectsOnHit", p.isDeathEffectsOnHit());
            json.addProperty("density", p.getDensity());
            json.addProperty("waterTerminalVelocityMultiplier", p.getWaterTerminalVelocityMultiplier());
            json.addProperty("waterHitImpulseLoss", p.getWaterHitImpulseLoss());
            json.addProperty("dampingRotation", p.getDampingRotation());
            json.addProperty("rotationSpeedVelocityRatio", p.getRotationSpeedVelocityRatio());
            json.addProperty("swimmingDampingFactor", p.getSwimmingDampingFactor());
            json.addProperty("bounceSoundEventId", p.getBounceSoundEventId());
            json.addProperty("hitSoundEventId", p.getHitSoundEventId());
            json.addProperty("missSoundEventId", p.getMissSoundEventId());
            json.addProperty("deathSoundEventId", p.getDeathSoundEventId());
            json.addProperty("hasBounceParticles", p.getBounceParticles() != null);
            json.addProperty("hasHitParticles", p.getHitParticles() != null);
            json.addProperty("hasMissParticles", p.getMissParticles() != null);
            json.addProperty("hasDeathParticles", p.getDeathParticles() != null);
            json.addProperty("hasExplosionConfig", p.getExplosionConfig() != null);
            out.add(entry.getKey(), json);
        }
        return out;
    }

    private static JsonObject dumpProjectileConfigs() {
        final Map<String, ProjectileConfig> assets =
            new TreeMap<>(ProjectileConfig.getAssetMap().getAssetMap());
        final JsonObject out = new JsonObject();
        for (Map.Entry<String, ProjectileConfig> entry : assets.entrySet()) {
            final ProjectileConfig config = entry.getValue();
            final JsonObject json = new JsonObject();
            json.addProperty("id", config.getId());
            json.addProperty("model", field(config, "model", String.class));
            json.addProperty("useModelScale", field(config, "useModelScale", Boolean.class));
            json.addProperty("launchForce", config.getLaunchForce());
            final var offset = config.getSpawnOffset();
            json.addProperty("spawnOffsetX", offset.x);
            json.addProperty("spawnOffsetY", offset.y);
            json.addProperty("spawnOffsetZ", offset.z);
            json.addProperty("rotateSpawnOffsetByPitch", config.isRotateSpawnOffsetByPitch());
            json.addProperty("rotateSpawnOffsetByYaw", config.isRotateSpawnOffsetByYaw());
            final PhysicsConfig physics = config.getPhysicsConfig();
            json.addProperty("physicsType", physics instanceof GroundFollowPhysicsConfig ? "GroundFollow" : "Standard");
            json.add("physics", dumpStandardPhysics(physics));
            out.add(entry.getKey(), json);
        }
        return out;
    }

    private static JsonObject dumpStandardPhysics(PhysicsConfig physics) {
        final JsonObject json = new JsonObject();
        if (!(physics instanceof StandardPhysicsConfig standard)) {
            return json;
        }
        json.addProperty("density", field(standard, "density", Number.class).doubleValue());
        json.addProperty("gravity", standard.getGravity());
        json.addProperty("bounciness", standard.getBounciness());
        json.addProperty("bounceCount", standard.getBounceCount());
        json.addProperty("bounceLimit", standard.getBounceLimit());
        json.addProperty("sticksVertically", standard.isSticksVertically());
        json.addProperty("computeYaw", field(standard, "computeYaw", Boolean.class));
        json.addProperty("computePitch", field(standard, "computePitch", Boolean.class));
        json.addProperty("rotationMode", field(standard, "rotationMode", Enum.class).name());
        json.addProperty("moveOutOfSolidSpeed", field(standard, "moveOutOfSolidSpeed", Number.class).doubleValue());
        json.addProperty("terminalVelocityAir", field(standard, "terminalVelocityAir", Number.class).doubleValue());
        json.addProperty("densityAir", field(standard, "densityAir", Number.class).doubleValue());
        json.addProperty("terminalVelocityWater", field(standard, "terminalVelocityWater", Number.class).doubleValue());
        json.addProperty("densityWater", field(standard, "densityWater", Number.class).doubleValue());
        json.addProperty("hitWaterImpulseLoss", standard.getHitWaterImpulseLoss());
        json.addProperty("rotationForce", field(standard, "rotationForce", Number.class).doubleValue());
        json.addProperty("speedRotationFactor", field(standard, "speedRotationFactor", Number.class).floatValue());
        json.addProperty("swimmingDampingFactor", standard.getSwimmingDampingFactor());
        json.addProperty("allowRolling", standard.isAllowRolling());
        json.addProperty("rollingFrictionFactor", standard.getRollingFrictionFactor());
        json.addProperty("rollingSpeed", field(standard, "rollingSpeed", Number.class).floatValue());
        if (physics instanceof GroundFollowPhysicsConfig ground) {
            json.addProperty("maxClimb", field(ground, "maxClimb", Number.class).intValue());
            json.addProperty("maxDrop", field(ground, "maxDrop", Number.class).intValue());
        }
        return json;
    }

    private static JsonObject resistanceMap(Map<DamageCause, ResistanceModifier[]> map) {
        final JsonObject out = new JsonObject();
        if (map == null) {
            return out;
        }
        final List<Map.Entry<DamageCause, ResistanceModifier[]>> entries = new ArrayList<>(map.entrySet());
        entries.sort((a, b) -> a.getKey().getId().compareTo(b.getKey().getId()));
        for (Map.Entry<DamageCause, ResistanceModifier[]> entry : entries) {
            final JsonArray modifiers = new JsonArray();
            for (ResistanceModifier modifier : entry.getValue()) {
                final JsonObject value = new JsonObject();
                value.addProperty("calculationType", modifier.getCalculationType().name());
                value.addProperty("amount", modifier.getAmount());
                modifiers.add(value);
            }
            out.add(entry.getKey().getId(), modifiers);
        }
        return out;
    }

    private static String statName(int index) {
        final EntityStatType stat = EntityStatType.getAssetMap().getAsset(index);
        return stat != null ? stat.getId() : "#" + index;
    }

    private static <T> T field(Object target, String name, Class<T> type) {
        try {
            Class<?> declaring = target.getClass();
            while (declaring != null) {
                try {
                    final Field field = declaring.getDeclaredField(name);
                    field.setAccessible(true);
                    return type.cast(field.get(target));
                } catch (NoSuchFieldException ignored) {
                    declaring = declaring.getSuperclass();
                }
            }
            throw new NoSuchFieldException(name);
        } catch (ReflectiveOperationException error) {
            throw new IllegalStateException("cannot read " + target.getClass().getSimpleName() + "." + name, error);
        }
    }
}
