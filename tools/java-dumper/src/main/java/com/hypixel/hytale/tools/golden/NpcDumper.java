package com.hypixel.hytale.tools.golden;

import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.hypixel.hytale.server.npc.movement.Steering;
import com.hypixel.hytale.server.npc.util.NPCPhysicsMath;
import com.hypixel.hytale.server.npc.util.expression.ExecutionContext;
import com.hypixel.hytale.server.npc.util.expression.Expression;
import com.hypixel.hytale.server.npc.util.expression.StdLib;
import com.hypixel.hytale.server.npc.util.expression.StdScope;
import com.hypixel.hytale.server.npc.movement.steeringforces.SteeringForceEvade;
import com.hypixel.hytale.server.npc.movement.steeringforces.SteeringForcePursue;
import com.hypixel.hytale.server.npc.movement.steeringforces.SteeringForceRotate;
import it.unimi.dsi.fastutil.ints.Int2FloatOpenHashMap;
import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import org.joml.Vector3d;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * Emits the deterministic NPC navigation/steering oracle.
 *
 * <p>Only pure, RNG-free Java paths are dumped: the package-private
 * {@code SearchDirections} tables for each movement configuration and fixed
 * inputs through the pursue, evade and rotate steering forces. The random wander force and the
 * motion-controller-dependent A* expansion are not part of this fixture.
 *
 * <p>Usage: {@code NpcDumper <output-directory> [world-directory] [repo-root]}
 */
public final class NpcDumper {

    private static final Gson GSON = new GsonBuilder()
        .setPrettyPrinting()
        .disableHtmlEscaping()
        .serializeNulls()
        .create();

    private NpcDumper() {
    }

    public static void main(String[] args) throws Exception {
        if (args.length < 1) {
            System.err.println("usage: NpcDumper <output-directory> [world-directory] [repo-root]");
            System.exit(2);
        }
        final Path outDir = Path.of(args[0]).toAbsolutePath().normalize();

        final JsonObject root = new JsonObject();
        final JsonObject meta = new JsonObject();
        meta.addProperty("producer", NpcDumper.class.getName());
        meta.addProperty("searchDirections", "SearchDirections.create for walk/fly with and without diagonal moves");
        meta.addProperty("steering", "SteeringForcePursue/Evade/Rotate with fixed inputs; no RNG");
        meta.addProperty("perception", "NPCPhysicsMath.isInViewCone with fixed viewer/direction/object inputs");
        meta.addProperty("expressions", "Java Expression/StdScope outputs for the role-builder expression subset");
        meta.addProperty("damageRanges", "DamageCalculator.computeDamageRange for fixed base damage/random modifier cases");
        root.add("meta", meta);
        root.add("searchDirections", searchDirections());
        root.add("steering", steering());
        root.add("perception", perception());
        root.add("expressions", expressions());
        root.add("damageRanges", damageRanges());

        Files.createDirectories(outDir);
        Files.writeString(
            outDir.resolve("npc.json"),
            GSON.toJson(root) + "\n",
            StandardCharsets.UTF_8
        );
        System.out.println("npc dump: ok");
    }

    private static JsonObject searchDirections() throws Exception {
        final JsonObject out = new JsonObject();
        out.add("walkNoDiagonal", directions(true, false, false, true, false));
        out.add("walkDiagonal", directions(true, true, false, true, false));
        out.add("planeXyNoDiagonal", directions(true, false, false, false, true));
        out.add("planeYzNoDiagonal", directions(true, false, true, false, false));
        out.add("flyNoDiagonal", directions(false, false, false, false, false));
        out.add("flyDiagonal", directions(false, true, false, false, false));
        return out;
    }

    private static JsonObject directions(boolean is2D, boolean canMoveDiagonal, boolean projectedX,
                                         boolean projectedY, boolean projectedZ) throws Exception {
        final Class<?> type = Class.forName("com.hypixel.hytale.server.npc.navigation.SearchDirections");
        final Method create = type.getDeclaredMethod("create",
            boolean.class, boolean.class, boolean.class, boolean.class, boolean.class);
        create.setAccessible(true);
        final Object directions = create.invoke(null, is2D, canMoveDiagonal, projectedX, projectedY, projectedZ);
        return dumpDirections(type, directions);
    }

    private static JsonObject dumpDirections(Class<?> type, Object directions) throws Exception {
        final Method directionsAccessor = type.getDeclaredMethod("directions");
        directionsAccessor.setAccessible(true);
        final Method distancesAccessor = type.getDeclaredMethod("distances");
        distancesAccessor.setAccessible(true);
        final Method inverseAccessor = type.getDeclaredMethod("inverseDirections");
        inverseAccessor.setAccessible(true);
        final Method normalsAccessor = type.getDeclaredMethod("normalDirections");
        normalsAccessor.setAccessible(true);
        final Method countAccessor = type.getDeclaredMethod("normalsPerDirection");
        countAccessor.setAccessible(true);

        final Vector3d[] directionArray = (Vector3d[]) directionsAccessor.invoke(directions);
        final double[] distanceArray = (double[]) distancesAccessor.invoke(directions);
        final int[] inverseArray = (int[]) inverseAccessor.invoke(directions);
        final int[] normalArray = (int[]) normalsAccessor.invoke(directions);
        final int count = (int) countAccessor.invoke(directions);

        final JsonObject out = new JsonObject();
        final JsonArray steps = new JsonArray();
        for (Vector3d direction : directionArray) {
            final JsonArray step = new JsonArray();
            step.add(direction.x);
            step.add(direction.y);
            step.add(direction.z);
            steps.add(step);
        }
        out.add("directions", steps);

        final JsonArray distances = new JsonArray();
        for (double distance : distanceArray) {
            distances.add(distance);
        }
        out.add("distances", distances);

        final JsonArray inverse = new JsonArray();
        for (int index : inverseArray) {
            inverse.add(index);
        }
        out.add("inverseDirections", inverse);

        final JsonArray normals = new JsonArray();
        if (normalArray != null) {
            for (int index : normalArray) {
                normals.add(index);
            }
        }
        out.add("normalDirections", normalArray == null ? null : normals);
        out.addProperty("normalsPerDirection", count);
        return out;
    }

    private static JsonObject steering() {
        final JsonObject out = new JsonObject();

        final JsonArray pursue = new JsonArray();
        pursue.add(pursueCase(new Vector3d(0, 0, 0), new Vector3d(30, 0, 0), 5, 20, 3));
        pursue.add(pursueCase(new Vector3d(0, 0, 0), new Vector3d(10, 0, 0), 5, 20, 3));
        pursue.add(pursueCase(new Vector3d(0, 0, 0), new Vector3d(3, 0, 0), 5, 20, 3));
        pursue.add(pursueCase(new Vector3d(0, 0, 0), new Vector3d(0, 0, 0), 5, 20, 3));
        out.add("pursue", pursue);

        final JsonArray evade = new JsonArray();
        evade.add(evadeCase(new Vector3d(0, 0, 0), new Vector3d(10, 0, 0), 5, 20, 3, 0.0f, false));
        evade.add(evadeCase(new Vector3d(0, 0, 0), new Vector3d(2, 0, 0), 5, 20, 3, 0.0f, false));
        evade.add(evadeCase(new Vector3d(0, 0, 0), new Vector3d(30, 0, 0), 5, 20, 3, 0.0f, false));
        evade.add(evadeCase(new Vector3d(0, 0, 0), new Vector3d(0, 0, 0), 5, 20, 3, 0.5f, false));
        evade.add(evadeCase(new Vector3d(0, 0, 0), new Vector3d(10, 0, 0), 5, 20, 3, 0.5f, true));
        out.add("evade", evade);

        final JsonArray rotate = new JsonArray();
        rotate.add(rotateCase(0.5f, 0.1f, 3.0));
        rotate.add(rotateCase(0.01f, 0.0f, 3.0));
        rotate.add(rotateCase(-0.5f, -0.6f, 3.0));
        out.add("rotate", rotate);

        return out;
    }

    private static JsonArray expressions() {
        final StdScope scope = new StdScope(StdLib.getInstance());
        scope.addVar("a", 2.0);
        scope.addVar("b", 9.0);
        scope.addVar("name", "x");
        scope.addVar("flag", true);
        scope.addVar("values", new double[] {1.0, 2.0});
        scope.addVar("names", new String[] {"x", "y"});
        scope.addVar("flags", new boolean[] {true, false});
        scope.addVar("emptyValues", new double[0]);
        scope.addVar("emptyNames", new String[0]);

        final String[] cases = {
            "a + b * 2",
            "100 - a",
            "(a + b) / 2",
            "max(a, b)",
            "min(a, b)",
            "a > 1 && b < 10",
            "!flag",
            "isEmpty(name)",
            "isEmpty(\"\")",
            "isEmptyStringArray(names)",
            "isEmptyStringArray(emptyNames)",
            "makeRange(a)",
            "[a + 1, b * 2]",
            "values",
            "names",
            "flags",
            "emptyValues",
            "PI",
        };

        final JsonArray out = new JsonArray();
        for (String expressionText : cases) {
            final Expression expression = new Expression();
            final ExecutionContext context = expression.evaluate(expressionText, scope);
            final ExecutionContext.Operand operand = context.top();
            final JsonObject entry = new JsonObject();
            entry.addProperty("expression", expressionText);
            entry.addProperty("type", operand.type.name());
            switch (operand.type) {
                case NUMBER -> entry.addProperty("number", operand.number);
                case BOOLEAN -> entry.addProperty("boolean", operand.bool);
                case STRING -> entry.addProperty("string", operand.string);
                case NUMBER_ARRAY -> entry.add("numbers", array(operand.numberArray));
                case STRING_ARRAY -> {
                    final JsonArray strings = new JsonArray();
                    if (operand.stringArray != null) {
                        for (String value : operand.stringArray) strings.add(value);
                    }
                    entry.add("strings", strings);
                }
                case BOOLEAN_ARRAY -> {
                    final JsonArray booleans = new JsonArray();
                    if (operand.boolArray != null) {
                        for (boolean value : operand.boolArray) booleans.add(value);
                    }
                    entry.add("booleans", booleans);
                }
                default -> {
                }
            }
            out.add(entry);
        }
        return out;
    }

    private static JsonArray damageRanges() throws Exception {
        final JsonArray out = new JsonArray();
        out.add(damageRange("absolute", "ABSOLUTE", new float[] {10f}, 0f, 1.0));
        out.add(damageRange("absoluteRandom", "ABSOLUTE", new float[] {10f}, 0.1f, 1.0));
        out.add(damageRange("dps", "DPS", new float[] {2f, 1f}, 0f, 2.0));
        out.add(damageRange("dpsRandom", "DPS", new float[] {2f, 1f}, 0.25f, 1.5));
        return out;
    }

    private static JsonObject damageRange(String name, String type, float[] baseDamage,
                                          float randomPercentage, double duration) throws Exception {
        final Class<?> clazz = Class.forName(
            "com.hypixel.hytale.server.core.modules.interaction.interaction.config.server.combat.DamageCalculator");
        final Constructor<?> constructor = clazz.getDeclaredConstructor();
        constructor.setAccessible(true);
        final Object calculator = constructor.newInstance();

        final Field typeField = clazz.getDeclaredField("type");
        typeField.setAccessible(true);
        @SuppressWarnings({"unchecked", "rawtypes"})
        final Object typeValue = Enum.valueOf((Class<? extends Enum>) typeField.getType().asSubclass(Enum.class), type);
        typeField.set(calculator, typeValue);

        final Field randomField = clazz.getDeclaredField("randomPercentageModifier");
        randomField.setAccessible(true);
        randomField.setFloat(calculator, randomPercentage);

        final Field baseField = clazz.getDeclaredField("baseDamage");
        baseField.setAccessible(true);
        final Int2FloatOpenHashMap damage = new Int2FloatOpenHashMap();
        for (int index = 0; index < baseDamage.length; index++) {
            damage.put(index + 1, baseDamage[index]);
        }
        baseField.set(calculator, damage);

        final Method compute = clazz.getMethod("computeDamageRange", double.class, float[].class);
        final float[] range = new float[2];
        compute.invoke(calculator, duration, range);

        final JsonObject out = new JsonObject();
        out.addProperty("name", name);
        out.addProperty("type", type);
        final JsonArray base = new JsonArray();
        for (float value : baseDamage) base.add(value);
        out.add("baseDamage", base);
        out.addProperty("randomPercentageModifier", randomPercentage);
        out.addProperty("duration", duration);
        out.addProperty("min", range[0]);
        out.addProperty("max", range[1]);
        return out;
    }

    private static JsonArray array(double[] values) {
        final JsonArray out = new JsonArray();
        if (values != null) {
            for (double value : values) out.add(value);
        }
        return out;
    }

    private static JsonArray perception() {
        final JsonArray out = new JsonArray();
        out.add(viewCone(new Vector3d(0, 0, 0), new Vector3d(1, 0, 0), 0.70710677f, new Vector3d(5, 0, 1)));
        out.add(viewCone(new Vector3d(0, 0, 0), new Vector3d(1, 0, 0), 0.70710677f, new Vector3d(5, 0, 5)));
        out.add(viewCone(new Vector3d(0, 0, 0), new Vector3d(1, 0, 0), 0.70710677f, new Vector3d(-5, 0, 0)));
        out.add(viewCone(new Vector3d(10, 20, 30), new Vector3d(0, 0, -1), 0.5f, new Vector3d(10, 20, 25)));
        return out;
    }

    private static JsonObject viewCone(Vector3d viewer, Vector3d direction, float cosHalfAngle, Vector3d object) {
        final JsonObject out = new JsonObject();
        out.add("viewer", vector(viewer));
        out.add("viewDirection", vector(direction));
        out.addProperty("cosHalfAngle", cosHalfAngle);
        out.add("object", vector(object));
        out.addProperty("result", NPCPhysicsMath.isInViewCone(viewer, direction, cosHalfAngle, object));
        return out;
    }

    private static JsonObject pursueCase(Vector3d self, Vector3d target, double stop, double slowdown, double falloff) {
        final var force = new SteeringForcePursue(stop, slowdown);
        force.setFalloff(falloff);
        force.setComponentSelector(new Vector3d(1, 1, 1));
        force.setPositions(self, target);
        final var steering = new Steering();
        final boolean result = force.compute(steering);
        final JsonObject out = steering("pursue", self, target, result, steering);
        out.addProperty("stopDistance", stop);
        out.addProperty("slowdownDistance", slowdown);
        out.addProperty("falloff", falloff);
        return out;
    }

    private static JsonObject evadeCase(Vector3d self, Vector3d target, double slowdown, double stop, double falloff,
                                        float directionHint, boolean adhere) {
        final var force = new SteeringForceEvade(slowdown, stop);
        force.setFalloff(falloff);
        force.setDirectionHint(directionHint);
        force.setAdhereToDirectionHint(adhere);
        force.setComponentSelector(new Vector3d(1, 1, 1));
        force.setPositions(self, target);
        final var steering = new Steering();
        final boolean result = force.compute(steering);
        final JsonObject out = steering("evade", self, target, result, steering);
        out.addProperty("slowdownDistance", slowdown);
        out.addProperty("stopDistance", stop);
        out.addProperty("falloff", falloff);
        out.addProperty("directionHint", directionHint);
        out.addProperty("adhereToDirectionHint", adhere);
        return out;
    }

    private static JsonObject rotateCase(float desired, float heading, double toleranceDegrees) {
        final var force = new SteeringForceRotate();
        force.setDesiredHeading(desired);
        force.setHeading(heading);
        force.setTolerance(Math.toRadians(toleranceDegrees));
        final var steering = new Steering();
        final boolean result = force.compute(steering);
        final JsonObject out = new JsonObject();
        out.addProperty("kind", "rotate");
        out.addProperty("desiredHeading", desired);
        out.addProperty("heading", heading);
        out.addProperty("toleranceDegrees", toleranceDegrees);
        out.addProperty("result", result);
        out.addProperty("hasYaw", steering.hasYaw());
        out.addProperty("yaw", steering.getYaw());
        return out;
    }

    private static JsonObject steering(String kind, Vector3d self, Vector3d target, boolean result, Steering steering) {
        final JsonObject out = new JsonObject();
        out.addProperty("kind", kind);
        out.add("self", vector(self));
        out.add("target", vector(target));
        out.addProperty("result", result);
        out.addProperty("hasTranslation", steering.hasTranslation());
        out.addProperty("x", steering.getX());
        out.addProperty("y", steering.getY());
        out.addProperty("z", steering.getZ());
        out.addProperty("hasYaw", steering.hasYaw());
        out.addProperty("hasPitch", steering.hasPitch());
        out.addProperty("hasRoll", steering.hasRoll());
        return out;
    }

    private static JsonArray vector(Vector3d vector) {
        final JsonArray out = new JsonArray();
        out.add(vector.x);
        out.add(vector.y);
        out.add(vector.z);
        return out;
    }
}
