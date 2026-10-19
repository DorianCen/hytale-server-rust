package com.hypixel.hytale.tools.golden;

import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.hypixel.hytale.math.util.FastRandom;
import com.hypixel.hytale.math.util.HashUtil;
import com.hypixel.hytale.math.util.MathUtil;
import com.hypixel.hytale.procedurallib.NoiseFunction;
import com.hypixel.hytale.procedurallib.NoiseType;
import com.hypixel.hytale.procedurallib.condition.ConstantIntCondition;
import com.hypixel.hytale.procedurallib.condition.IIntCondition;
import com.hypixel.hytale.procedurallib.logic.BranchNoise;
import com.hypixel.hytale.procedurallib.logic.CellNoise;
import com.hypixel.hytale.procedurallib.logic.ConstantNoise;
import com.hypixel.hytale.procedurallib.logic.DistanceNoise;
import com.hypixel.hytale.procedurallib.logic.GridNoise;
import com.hypixel.hytale.procedurallib.logic.MeshNoise;
import com.hypixel.hytale.procedurallib.logic.OldSimplexNoise;
import com.hypixel.hytale.procedurallib.logic.PerlinNoise;
import com.hypixel.hytale.procedurallib.logic.PointNoise;
import com.hypixel.hytale.procedurallib.logic.ResultBuffer;
import com.hypixel.hytale.procedurallib.logic.SimplexNoise;
import com.hypixel.hytale.procedurallib.logic.ValueNoise;
import com.hypixel.hytale.procedurallib.logic.cell.CellDistanceFunction;
import com.hypixel.hytale.procedurallib.logic.cell.DistanceCalculationMode;
import com.hypixel.hytale.procedurallib.logic.cell.GridCellDistanceFunction;
import com.hypixel.hytale.procedurallib.logic.cell.evaluator.BranchEvaluator;
import com.hypixel.hytale.procedurallib.logic.cell.evaluator.DensityPointEvaluator;
import com.hypixel.hytale.procedurallib.logic.cell.evaluator.NormalPointEvaluator;
import com.hypixel.hytale.procedurallib.logic.cell.evaluator.PointEvaluator;
import com.hypixel.hytale.procedurallib.logic.cell.jitter.CellJitter;
import com.hypixel.hytale.procedurallib.property.NoiseFormulaProperty;
import com.hypixel.hytale.procedurallib.supplier.DoubleRange;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;
import java.util.UUID;

/**
 * Emits deterministic golden data from the original Java Hytale world-generation code.
 * <p>
 * Every floating-point value is written as {@link Double#toHexString(double)} /
 * {@link Float#toHexString(float)} so a Rust port can verify bit-exact results without
 * decimal-rounding ambiguity.
 * <p>
 * Usage: {@code GoldenDumper <output-directory> [world-directory]}
 * <p>
 * The optional world directory defaults to {@code <output-directory>/../Default} and
 * holds the Default world assets ({@code World.json}, {@code Mask.json}, ...).
 */
public final class GoldenDumper {
    private static final Gson GSON = new GsonBuilder()
        .setPrettyPrinting()
        .disableHtmlEscaping()
        .serializeNulls()
        .create();

    private static final long[] RANDOM_SEEDS = {0L, 1L, 2L, 3L, 4L, 123456789L};
    private static final int[] NOISE_SEEDS = {1, 42};

    private static final int GRID_2D_SIZE = 32;
    private static final double GRID_2D_START = -8.0;
    private static final double GRID_2D_STEP = 0.5;

    private static final int GRID_3D_SIZE = 8;
    private static final double GRID_3D_START = -3.95;
    private static final double GRID_3D_STEP = 0.5;

    private GoldenDumper() {
    }

    public static void main(String[] args) throws IOException {
        if (args.length != 1) {
            System.err.println("Usage: GoldenDumper <output-directory>");
            System.exit(2);
        }

        Path output = Path.of(args[0]);
        Files.createDirectories(output);

        final Path worldDir = args.length > 1
            ? Path.of(args[1]).toAbsolutePath()
            : output.toAbsolutePath().getParent().resolve("Default");

        write(output.resolve("fastrandom.json"), dumpFastRandom());
        write(output.resolve("hashutil.json"), dumpHashUtil());
        write(output.resolve("mathutil.json"), dumpMathUtil());
        write(output.resolve("noise.json"), dumpNoise());

        // Phase 2: world mask (zoom) + procedural climate mask.
        writeText(output.resolve("zoom.json"), Phase2Dumper.dumpZoom(worldDir));
        writeText(output.resolve("climate.json"), Phase2Dumper.dumpClimate(worldDir));

        System.out.println("Golden files written to " + output.toAbsolutePath());
    }

    private static void write(Path file, JsonObject json) throws IOException {
        Files.writeString(file, GSON.toJson(json) + "\n", StandardCharsets.UTF_8);
    }

    private static void writeText(Path file, String json) throws IOException {
        Files.writeString(file, json, StandardCharsets.UTF_8);
    }

    // ------------------------------------------------------------------
    // FastRandom
    // ------------------------------------------------------------------

    private static JsonObject dumpFastRandom() {
        JsonObject root = new JsonObject();
        root.addProperty("class", "com.hypixel.hytale.math.util.FastRandom");
        root.addProperty("format", "integers are JSON numbers, doubles/floats are Java hex strings");
        root.addProperty("note", "nextInt(bound), nextLong, nextDouble and nextFloat are inherited from java.util.Random and consume the FastRandom LCG via next(bits)");

        JsonObject seeds = new JsonObject();
        for (long seed : RANDOM_SEEDS) {
            FastRandom random = new FastRandom(seed);
            JsonObject entry = new JsonObject();
            entry.addProperty("seed", seed);

            JsonArray ints = new JsonArray();
            for (int i = 0; i < 256; i++) ints.add(random.nextInt());
            entry.add("nextInt_256", ints);

            JsonArray bounded = new JsonArray();
            for (int i = 0; i < 128; i++) bounded.add(random.nextInt(100));
            entry.add("nextInt_bound100_128", bounded);

            JsonArray longs = new JsonArray();
            for (int i = 0; i < 128; i++) longs.add(random.nextLong());
            entry.add("nextLong_128", longs);

            JsonArray doubles = new JsonArray();
            for (int i = 0; i < 128; i++) doubles.add(hex(random.nextDouble()));
            entry.add("nextDouble_128", doubles);

            JsonArray floats = new JsonArray();
            for (int i = 0; i < 128; i++) floats.add(hex(random.nextFloat()));
            entry.add("nextFloat_128", floats);

            seeds.add(Long.toString(seed), entry);
        }
        root.add("seeds", seeds);
        return root;
    }

    // ------------------------------------------------------------------
    // HashUtil
    // ------------------------------------------------------------------

    private static JsonObject dumpHashUtil() {
        JsonObject root = new JsonObject();
        root.addProperty("class", "com.hypixel.hytale.math.util.HashUtil");
        root.addProperty("format", "long results are JSON integers, random() results are Java hex strings");

        long[] values = hashInputs();
        int n = values.length;

        root.add("hash_1", hashUnary(values));
        root.add("hash_2", hashNary(values, 2));
        root.add("hash_3", hashNary(values, 3));
        root.add("hash_4", hashNary(values, 4));

        root.add("rehash_1", rehashUnary(values));
        root.add("rehash_2", rehashNary(values, 2));
        root.add("rehash_3", rehashNary(values, 3));
        root.add("rehash_4", rehashNary(values, 4));

        root.add("random_1", randomUnary(values));
        root.add("random_2", randomNary(values, 2));
        root.add("random_3", randomNary(values, 3));
        root.add("random_4", randomNary(values, 4));

        JsonArray randomInts = new JsonArray();
        int[] bounds = {1, 2, 3, 7, 16, 100, 1000, Integer.MAX_VALUE};
        for (int i = 0; i < n; i++) {
            long a = values[i];
            long b = values[(i + 1) % n];
            long c = values[(i + 3) % n];
            for (int bound : bounds) {
                JsonObject e = new JsonObject();
                e.addProperty("l1", a);
                e.addProperty("l2", b);
                e.addProperty("l3", c);
                e.addProperty("bound", bound);
                e.addProperty("out", HashUtil.randomInt(a, b, c, bound));
                randomInts.add(e);
            }
        }
        root.add("randomInt", randomInts);

        JsonArray uuids = new JsonArray();
        String[] uuidStrings = {
            "00000000-0000-0000-0000-000000000000",
            "00000000-0000-0000-0000-000000000001",
            "123e4567-e89b-12d3-a456-426614174000",
            "ffffffff-ffff-ffff-ffff-ffffffffffff",
            "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
            "9f8e7d6c-5b4a-3928-1706-f5e4d3c2b1a0"
        };
        for (String uuidString : uuidStrings) {
            JsonObject e = new JsonObject();
            e.addProperty("uuid", uuidString);
            e.addProperty("out", HashUtil.hashUuid(UUID.fromString(uuidString)));
            uuids.add(e);
        }
        root.add("hashUuid", uuids);

        return root;
    }

    private static long[] hashInputs() {
        List<Long> values = new ArrayList<>();
        for (int i = -1000; i <= 1000; i += 137) values.add((long) i);
        values.add(-1234567890123456789L);
        values.add(1234567890123456789L);
        values.add(Long.MIN_VALUE);
        values.add(Long.MIN_VALUE + 1);
        values.add(Long.MAX_VALUE);
        values.add((long) Integer.MIN_VALUE);
        values.add((long) Integer.MAX_VALUE);
        values.add(0x0123456789ABCDEFL);
        values.add(0x7FFF_FFFF_FFFF_FFFFL);
        values.add(-0x7FFF_FFFF_FFFF_FFFFL);
        return values.stream().mapToLong(Long::longValue).toArray();
    }

    private static JsonArray hashUnary(long[] values) {
        JsonArray out = new JsonArray();
        for (long value : values) {
            JsonObject e = new JsonObject();
            e.addProperty("in", value);
            e.addProperty("out", HashUtil.hash(value));
            out.add(e);
        }
        return out;
    }

    private static JsonArray rehashUnary(long[] values) {
        JsonArray out = new JsonArray();
        for (long value : values) {
            JsonObject e = new JsonObject();
            e.addProperty("in", value);
            e.addProperty("out", HashUtil.rehash(value));
            out.add(e);
        }
        return out;
    }

    private static JsonArray randomUnary(long[] values) {
        JsonArray out = new JsonArray();
        for (long value : values) {
            JsonObject e = new JsonObject();
            e.addProperty("in", value);
            e.addProperty("out", hex(HashUtil.random(value)));
            out.add(e);
        }
        return out;
    }

    private static JsonArray hashNary(long[] values, int arity) {
        JsonArray out = new JsonArray();
        int n = values.length;
        for (int i = 0; i < n; i++) {
            long[] in = naryInputs(values, i, arity);
            JsonObject e = new JsonObject();
            addInputs(e, in);
            e.addProperty("out", switch (arity) {
                case 2 -> HashUtil.hash(in[0], in[1]);
                case 3 -> HashUtil.hash(in[0], in[1], in[2]);
                case 4 -> HashUtil.hash(in[0], in[1], in[2], in[3]);
                default -> throw new IllegalArgumentException("arity " + arity);
            });
            out.add(e);
        }
        return out;
    }

    private static JsonArray rehashNary(long[] values, int arity) {
        JsonArray out = new JsonArray();
        int n = values.length;
        for (int i = 0; i < n; i++) {
            long[] in = naryInputs(values, i, arity);
            JsonObject e = new JsonObject();
            addInputs(e, in);
            e.addProperty("out", switch (arity) {
                case 2 -> HashUtil.rehash(in[0], in[1]);
                case 3 -> HashUtil.rehash(in[0], in[1], in[2]);
                case 4 -> HashUtil.rehash(in[0], in[1], in[2], in[3]);
                default -> throw new IllegalArgumentException("arity " + arity);
            });
            out.add(e);
        }
        return out;
    }

    private static JsonArray randomNary(long[] values, int arity) {
        JsonArray out = new JsonArray();
        int n = values.length;
        for (int i = 0; i < n; i++) {
            long[] in = naryInputs(values, i, arity);
            JsonObject e = new JsonObject();
            addInputs(e, in);
            e.addProperty("out", hex(switch (arity) {
                case 2 -> HashUtil.random(in[0], in[1]);
                case 3 -> HashUtil.random(in[0], in[1], in[2]);
                case 4 -> HashUtil.random(in[0], in[1], in[2], in[3]);
                default -> throw new IllegalArgumentException("arity " + arity);
            }));
            out.add(e);
        }
        return out;
    }

    private static long[] naryInputs(long[] values, int index, int arity) {
        int n = values.length;
        long[] in = new long[arity];
        for (int j = 0; j < arity; j++) {
            int step = switch (j) {
                case 0 -> 0;
                case 1 -> 1;
                case 2 -> 3;
                default -> 7;
            };
            in[j] = values[(index + step) % n];
        }
        return in;
    }

    private static void addInputs(JsonObject e, long[] in) {
        JsonArray inputs = new JsonArray();
        for (long value : in) inputs.add(value);
        e.add("in", inputs);
    }

    // ------------------------------------------------------------------
    // MathUtil
    // ------------------------------------------------------------------

    private static JsonObject dumpMathUtil() {
        JsonObject root = new JsonObject();
        root.addProperty("class", "com.hypixel.hytale.math.util.MathUtil");
        root.addProperty("format", "floating-point values are Java hex strings; NaN/Infinity appear as such");
        root.addProperty("skipped", "randomInt/randomDouble/randomFloat are intentionally omitted because they are nondeterministic");

        int[] ints = {-1_000_000, -123_456, -1000, -137, -2, -1, 0, 1, 2, 137, 1000, 123_456, 1_000_000,
            Integer.MIN_VALUE, Integer.MIN_VALUE + 1, Integer.MAX_VALUE - 1, Integer.MAX_VALUE};

        double[] doubles = {
            -1.0e12, -1234.75, -1000.0, -123.456, -8.5, -2.5, -1.5, -1.0, -0.5, -1.0e-9, -0.0,
            0.0, 1.0e-9, 0.5, 1.0, 1.5, 2.5, 8.5, 123.456, 1000.0, 1234.75, 1.0e12,
            1.0 / 3.0, Math.PI, Math.E
        };

        float[] floats = {
            -1.0e12f, -1234.75f, -1000.0f, -123.456f, -8.5f, -2.5f, -1.5f, -1.0f, -0.5f, -1.0e-9f, -0.0f,
            0.0f, 1.0e-9f, 0.5f, 1.0f, 1.5f, 2.5f, 8.5f, 123.456f, 1000.0f, 1234.75f, 1.0e12f,
            1.0f / 3.0f, (float) Math.PI, (float) Math.E
        };

        root.add("abs_int", mapInt(ints, MathUtil::abs));
        root.add("floor_double", mapDouble(doubles, MathUtil::floor));
        root.add("ceil_double", mapDouble(doubles, MathUtil::ceil));
        root.add("fastFloor_double", mapLong(doubles, MathUtil::fastFloor));
        root.add("fastFloor_float", mapInt(floats, MathUtil::fastFloor));
        root.add("fastCeil_double", mapLong(doubles, MathUtil::fastCeil));
        root.add("fastCeil_float", mapInt(floats, MathUtil::fastCeil));
        root.add("fastRound_double", mapLong(doubles, MathUtil::fastRound));
        root.add("fastRound_float", mapInt(floats, MathUtil::fastRound));

        JsonArray clampDouble = new JsonArray();
        JsonArray clampFloat = new JsonArray();
        double[][] clampCases = {{-10.0, 0.0, 1.0}, {-0.0, 0.0, 1.0}, {0.5, 0.0, 1.0}, {1.0, 0.0, 1.0}, {10.0, 0.0, 1.0},
            {-5.0, -2.0, 2.0}, {5.0, -2.0, 2.0}, {3.0, 4.0, 2.0}};
        for (double[] c : clampCases) {
            JsonObject e = new JsonObject();
            e.addProperty("v", hex(c[0]));
            e.addProperty("min", hex(c[1]));
            e.addProperty("max", hex(c[2]));
            e.addProperty("out", hex(MathUtil.clamp(c[0], c[1], c[2])));
            clampDouble.add(e);

            JsonObject f = new JsonObject();
            f.addProperty("v", hex((float) c[0]));
            f.addProperty("min", hex((float) c[1]));
            f.addProperty("max", hex((float) c[2]));
            f.addProperty("out", hex(MathUtil.clamp((float) c[0], (float) c[1], (float) c[2])));
            clampFloat.add(f);
        }
        root.add("clamp_double", clampDouble);
        root.add("clamp_float", clampFloat);

        int[] clampInts = {-10, -2, 0, 1, 2, 10, Integer.MIN_VALUE, Integer.MAX_VALUE};
        JsonArray clampInt = new JsonArray();
        for (int v : clampInts) {
            JsonObject e = new JsonObject();
            e.addProperty("v", v);
            e.addProperty("min", -2);
            e.addProperty("max", 2);
            e.addProperty("out", MathUtil.clamp(v, -2, 2));
            clampInt.add(e);
        }
        root.add("clamp_int", clampInt);

        long[] clampLongs = {-10L, -2L, 0L, 1L, 2L, 10L, Long.MIN_VALUE, Long.MAX_VALUE};
        JsonArray clampLong = new JsonArray();
        for (long v : clampLongs) {
            JsonObject e = new JsonObject();
            e.addProperty("v", v);
            e.addProperty("min", -2L);
            e.addProperty("max", 2L);
            e.addProperty("out", MathUtil.clamp(v, -2L, 2L));
            clampLong.add(e);
        }
        root.add("clamp_long", clampLong);

        double[][] lerpCases = {
            {0.0, 10.0, -0.5}, {0.0, 10.0, 0.0}, {0.0, 10.0, 0.25}, {0.0, 10.0, 0.5},
            {0.0, 10.0, 1.0}, {0.0, 10.0, 1.5}, {-5.0, 5.0, 0.3}, {1.0e9, -1.0e9, 0.75}
        };
        root.add("lerp_double", lerpDouble(lerpCases, false));
        root.add("lerpUnclamped_double", lerpDouble(lerpCases, true));

        JsonArray lerpFloat = new JsonArray();
        JsonArray lerpUnclampedFloat = new JsonArray();
        for (double[] c : lerpCases) {
            float a = (float) c[0];
            float b = (float) c[1];
            float t = (float) c[2];
            JsonObject clamped = new JsonObject();
            clamped.addProperty("a", hex(a));
            clamped.addProperty("b", hex(b));
            clamped.addProperty("t", hex(t));
            clamped.addProperty("out", hex(MathUtil.lerp(a, b, t)));
            lerpFloat.add(clamped);

            JsonObject unclamped = new JsonObject();
            unclamped.addProperty("a", hex(a));
            unclamped.addProperty("b", hex(b));
            unclamped.addProperty("t", hex(t));
            unclamped.addProperty("out", hex(MathUtil.lerpUnclamped(a, b, t)));
            lerpUnclampedFloat.add(unclamped);
        }
        root.add("lerp_float", lerpFloat);
        root.add("lerpUnclamped_float", lerpUnclampedFloat);

        JsonArray seededLerp = new JsonArray();
        long[] lerpSeeds = {0L, 1L, 42L, 123456789L, -1L, Long.MIN_VALUE, Long.MAX_VALUE, 0x0123456789ABCDEFL};
        for (long seed : lerpSeeds) {
            JsonObject e = new JsonObject();
            e.addProperty("a", hex(-3.25f));
            e.addProperty("b", hex(7.5f));
            e.addProperty("seed", seed);
            e.addProperty("out", hex(MathUtil.seededLerp(-3.25f, 7.5f, seed)));
            seededLerp.add(e);
        }
        root.add("seededLerp_float", seededLerp);

        JsonArray floorMod = new JsonArray();
        double[][] modCases = {{5.5, 2.0}, {-5.5, 2.0}, {5.5, -2.0}, {-5.5, -2.0}, {0.0, 3.0}, {Math.PI, Math.PI * 2}};
        for (double[] c : modCases) {
            JsonObject e = new JsonObject();
            e.addProperty("x", hex(c[0]));
            e.addProperty("y", hex(c[1]));
            e.addProperty("out", hex(MathUtil.floorMod(c[0], c[1])));
            floorMod.add(e);
        }
        root.add("floorMod_double", floorMod);

        JsonArray compareAngle = new JsonArray();
        double[][] angleCases = {{0.0, Math.PI}, {Math.PI, 0.0}, {-Math.PI, Math.PI}, {1.0, 2.0}, {0.0, Math.PI * 2}, {Math.PI * 2, 0.0}};
        for (double[] c : angleCases) {
            JsonObject e = new JsonObject();
            e.addProperty("a", hex(c[0]));
            e.addProperty("b", hex(c[1]));
            e.addProperty("out", hex(MathUtil.compareAngle(c[0], c[1])));
            compareAngle.add(e);
        }
        root.add("compareAngle_double", compareAngle);

        float[][] shortAngleCases = {{0.0f, 1.5f}, {1.5f, 0.0f}, {(float) Math.PI, (float) -Math.PI}, {0.0f, (float) (Math.PI * 2)}, {1.0f, 7.0f}};
        JsonArray shortAngle = new JsonArray();
        JsonArray lerpAngle = new JsonArray();
        JsonArray wrapAngle = new JsonArray();
        float[] wrapInputs = {0.0f, 1.0f, (float) Math.PI, (float) -Math.PI, (float) (Math.PI * 2), (float) -Math.PI * 2, 10.0f, -10.0f};
        for (float[] c : shortAngleCases) {
            JsonObject e = new JsonObject();
            e.addProperty("a", hex(c[0]));
            e.addProperty("b", hex(c[1]));
            e.addProperty("out", hex(MathUtil.shortAngleDistance(c[0], c[1])));
            shortAngle.add(e);

            JsonObject l = new JsonObject();
            l.addProperty("a", hex(c[0]));
            l.addProperty("b", hex(c[1]));
            l.addProperty("t", hex(0.25f));
            l.addProperty("out", hex(MathUtil.lerpAngle(c[0], c[1], 0.25f)));
            lerpAngle.add(l);
        }
        for (float value : wrapInputs) {
            JsonObject e = new JsonObject();
            e.addProperty("angle", hex(value));
            e.addProperty("out", hex(MathUtil.wrapAngle(value)));
            wrapAngle.add(e);
        }
        root.add("shortAngleDistance_float", shortAngle);
        root.add("lerpAngle_float", lerpAngle);
        root.add("wrapAngle_float", wrapAngle);

        int[] halfBits = {0x0000, 0x0001, 0x03FF, 0x0400, 0x3555, 0x3C00, 0x3C01, 0x7BFF, 0x7C00, 0x7E00, 0x8000, 0xBC00, 0xFC00, 0xFFFF};
        JsonArray halfToFloat = new JsonArray();
        for (int bits : halfBits) {
            JsonObject e = new JsonObject();
            e.addProperty("in", bits);
            e.addProperty("out", hex(MathUtil.halfFloatToFloat(bits)));
            halfToFloat.add(e);
        }
        root.add("halfFloatToFloat", halfToFloat);

        float[] halfInputs = {0.0f, -0.0f, 1.0f, -1.0f, 1.0f / 3.0f, 65504.0f, 65520.0f, 1.0e9f, Float.MIN_VALUE, Float.MAX_VALUE, Float.POSITIVE_INFINITY, Float.NEGATIVE_INFINITY, Float.NaN};
        JsonArray floatToHalf = new JsonArray();
        for (float value : halfInputs) {
            JsonObject e = new JsonObject();
            e.addProperty("in", hex(value));
            e.addProperty("out", MathUtil.halfFloatFromFloat(value));
            floatToHalf.add(e);
        }
        root.add("halfFloatFromFloat", floatToHalf);

        JsonArray byteCount = new JsonArray();
        int[] byteInputs = {-1, 0, 1, 0xFF, 0x100, 0xFFFF, 0x10000, Integer.MAX_VALUE};
        for (int value : byteInputs) {
            JsonObject e = new JsonObject();
            e.addProperty("in", value);
            e.addProperty("out", MathUtil.byteCount(value));
            byteCount.add(e);
        }
        root.add("byteCount", byteCount);

        int[][] packIntCases = {{0, 0}, {1, 2}, {-1, -2}, {123456, -654321}, {Integer.MIN_VALUE, Integer.MAX_VALUE}, {0x7FFF, 0x8000}};
        JsonArray packInt = new JsonArray();
        for (int[] c : packIntCases) {
            int packed = MathUtil.packInt(c[0], c[1]);
            JsonObject e = new JsonObject();
            e.addProperty("x", c[0]);
            e.addProperty("z", c[1]);
            e.addProperty("packed", packed);
            e.addProperty("unpackLeft", MathUtil.unpackLeft(packed));
            e.addProperty("unpackRight", MathUtil.unpackRight(packed));
            packInt.add(e);
        }
        root.add("packInt", packInt);

        int[][] packLongCases = {{0, 0}, {1, 2}, {-1, -2}, {123456, -654321}, {Integer.MIN_VALUE, Integer.MAX_VALUE}, {0x7FFFFFFF, 0x80000000}};
        JsonArray packLong = new JsonArray();
        for (int[] c : packLongCases) {
            long packed = MathUtil.packLong(c[0], c[1]);
            JsonObject e = new JsonObject();
            e.addProperty("left", c[0]);
            e.addProperty("right", c[1]);
            e.addProperty("packed", packed);
            e.addProperty("unpackLeft", MathUtil.unpackLeft(packed));
            e.addProperty("unpackRight", MathUtil.unpackRight(packed));
            packLong.add(e);
        }
        root.add("packLong", packLong);

        JsonArray length2d = new JsonArray();
        JsonArray lengthSquared2d = new JsonArray();
        JsonArray length3d = new JsonArray();
        JsonArray lengthSquared3d = new JsonArray();
        double[][] lengthCases = {{0.0, 0.0}, {1.0, 0.0}, {0.0, 1.0}, {3.0, 4.0}, {-3.0, -4.0}, {1.0e9, -1.0e9}};
        for (double[] c : lengthCases) {
            JsonObject e = new JsonObject();
            e.addProperty("x", hex(c[0]));
            e.addProperty("y", hex(c[1]));
            e.addProperty("out", hex(MathUtil.lengthSquared(c[0], c[1])));
            lengthSquared2d.add(e);

            JsonObject l = new JsonObject();
            l.addProperty("x", hex(c[0]));
            l.addProperty("y", hex(c[1]));
            l.addProperty("out", hex(MathUtil.length(c[0], c[1])));
            length2d.add(l);

            JsonObject e3 = new JsonObject();
            e3.addProperty("x", hex(c[0]));
            e3.addProperty("y", hex(c[1]));
            e3.addProperty("z", hex(c[0] * 0.5));
            e3.addProperty("out", hex(MathUtil.lengthSquared(c[0], c[1], c[0] * 0.5)));
            lengthSquared3d.add(e3);

            JsonObject l3 = new JsonObject();
            l3.addProperty("x", hex(c[0]));
            l3.addProperty("y", hex(c[1]));
            l3.addProperty("z", hex(c[0] * 0.5));
            l3.addProperty("out", hex(MathUtil.length(c[0], c[1], c[0] * 0.5)));
            length3d.add(l3);
        }
        root.add("lengthSquared_2d", lengthSquared2d);
        root.add("length_2d", length2d);
        root.add("lengthSquared_3d", lengthSquared3d);
        root.add("length_3d", length3d);

        JsonArray round = new JsonArray();
        double[][] roundCases = {{1.23456789, 0}, {1.23456789, 2}, {1.23456789, 4}, {-1.235, 2}, {1234.5678, -2}, {-1234.5678, -2}};
        for (double[] c : roundCases) {
            JsonObject e = new JsonObject();
            e.addProperty("d", hex(c[0]));
            e.addProperty("p", (int) c[1]);
            e.addProperty("out", hex(MathUtil.round(c[0], (int) c[1])));
            round.add(e);
        }
        root.add("round_double", round);

        JsonArray within = new JsonArray();
        double[][] withinCases = {{0.0, 0.0, 1.0}, {1.0, 0.0, 1.0}, {1.0001, 0.0, 1.0}, {-0.0001, 0.0, 1.0}, {-5.0, -5.0, 5.0}};
        for (double[] c : withinCases) {
            JsonObject e = new JsonObject();
            e.addProperty("val", hex(c[0]));
            e.addProperty("min", hex(c[1]));
            e.addProperty("max", hex(c[2]));
            e.addProperty("out", MathUtil.within(c[0], c[1], c[2]));
            within.add(e);
        }
        root.add("within_double", within);

        JsonArray clipToZero = new JsonArray();
        JsonArray closeToZero = new JsonArray();
        double[] zeroCases = {0.0, -0.0, 1.0e-17, -1.0e-17, Math.ulp(1.0), Math.ulp(1.0) * 2, 1.0e-8, -1.0e-8, 1.0};
        for (double value : zeroCases) {
            JsonObject e = new JsonObject();
            e.addProperty("v", hex(value));
            e.addProperty("out", hex(MathUtil.clipToZero(value)));
            clipToZero.add(e);

            JsonObject c = new JsonObject();
            c.addProperty("v", hex(value));
            c.addProperty("out", MathUtil.closeToZero(value));
            closeToZero.add(c);
        }
        root.add("clipToZero_double", clipToZero);
        root.add("closeToZero_double", closeToZero);

        JsonArray percentage = new JsonArray();
        int[][] percentageCases = {{0, 1}, {0, 2}, {1, 2}, {1, 100}, {50, 100}, {99, 100}, {100, 100}, {2, 3}};
        for (int[] c : percentageCases) {
            JsonObject e = new JsonObject();
            e.addProperty("index", c[0]);
            e.addProperty("max", c[1]);
            e.addProperty("out", MathUtil.getPercentageOf(c[0], c[1]));
            percentage.add(e);
        }
        root.add("getPercentageOf", percentage);

        JsonArray percent = new JsonArray();
        int[][] percentCases = {{0, 0}, {1, 0}, {1, 4}, {3, 4}, {-1, 4}, {100, 200}};
        for (int[] c : percentCases) {
            JsonObject e = new JsonObject();
            e.addProperty("v", c[0]);
            e.addProperty("total", c[1]);
            e.addProperty("out", hex(MathUtil.percent(c[0], c[1])));
            percent.add(e);
        }
        root.add("percent", percent);

        JsonArray saturatedMultiply = new JsonArray();
        long[][] satCases = {{0L, 0L}, {2L, 3L}, {-2L, 3L}, {-2L, -3L}, {Long.MAX_VALUE, 2L}, {Long.MIN_VALUE, 2L}, {Long.MAX_VALUE, -2L}, {Long.MIN_VALUE, -2L}};
        for (long[] c : satCases) {
            JsonObject e = new JsonObject();
            e.addProperty("a", c[0]);
            e.addProperty("b", c[1]);
            e.addProperty("out", MathUtil.saturatedMultiply(c[0], c[1]));
            saturatedMultiply.add(e);
        }
        root.add("saturatedMultiply", saturatedMultiply);

        JsonArray lineDistance = new JsonArray();
        JsonArray infLineDistance = new JsonArray();
        JsonArray sideOfLine = new JsonArray();
        double[][] lineCases = {
            {0.0, 0.0, 0.0, 0.0, 1.0, 0.0},
            {0.5, 1.0, 0.0, 0.0, 1.0, 0.0},
            {2.0, 0.0, 0.0, 0.0, 1.0, 0.0},
            {-1.0, 0.0, 0.0, 0.0, 1.0, 0.0},
            {3.0, 4.0, 1.0, 1.0, 5.0, 1.0},
            {1.0, -2.0, 0.0, 0.0, 1.0, 1.0}
        };
        for (double[] c : lineCases) {
            JsonObject e = new JsonObject();
            addLineInputs(e, c);
            e.addProperty("out", hex(MathUtil.distanceToLineSq(c[0], c[1], c[2], c[3], c[4], c[5])));
            lineDistance.add(e);

            JsonObject i = new JsonObject();
            addLineInputs(i, c);
            i.addProperty("out", hex(MathUtil.distanceToInfLineSq(c[0], c[1], c[2], c[3], c[4], c[5])));
            infLineDistance.add(i);

            JsonObject s = new JsonObject();
            addLineInputs(s, c);
            s.addProperty("out", MathUtil.sideOfLine(c[0], c[1], c[2], c[3], c[4], c[5]));
            sideOfLine.add(s);
        }
        root.add("distanceToLineSq", lineDistance);
        root.add("distanceToInfLineSq", infLineDistance);
        root.add("sideOfLine", sideOfLine);

        JsonArray mapToRange = new JsonArray();
        float[][] mapCases = {{0.0f, 0.0f, 10.0f, -1.0f, 1.0f}, {5.0f, 0.0f, 10.0f, -1.0f, 1.0f}, {10.0f, 0.0f, 10.0f, -1.0f, 1.0f}, {-5.0f, 0.0f, 10.0f, -1.0f, 1.0f}, {3.0f, 2.0f, 4.0f, 100.0f, 200.0f}};
        for (float[] c : mapCases) {
            JsonObject e = new JsonObject();
            e.addProperty("value", hex(c[0]));
            e.addProperty("valueMin", hex(c[1]));
            e.addProperty("valueMax", hex(c[2]));
            e.addProperty("rangeMin", hex(c[3]));
            e.addProperty("rangeMax", hex(c[4]));
            e.addProperty("out", hex(MathUtil.mapToRange(c[0], c[1], c[2], c[3], c[4])));
            mapToRange.add(e);
        }
        root.add("mapToRange_float", mapToRange);

        JsonArray percentile = new JsonArray();
        long[] sorted = {1L, 2L, 3L, 4L, 5L, 6L, 7L, 8L, 9L, 10L};
        double[] percentiles = {0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0};
        for (double p : percentiles) {
            JsonObject e = new JsonObject();
            JsonArray data = new JsonArray();
            for (long value : sorted) data.add(value);
            e.add("sortedData", data);
            e.addProperty("percentile", hex(p));
            e.addProperty("out", hex(MathUtil.percentile(sorted, p)));
            percentile.add(e);
        }
        root.add("percentile", percentile);

        return root;
    }

    private static JsonArray mapInt(int[] inputs, java.util.function.IntUnaryOperator op) {
        JsonArray out = new JsonArray();
        for (int value : inputs) {
            JsonObject e = new JsonObject();
            e.addProperty("in", value);
            e.addProperty("out", op.applyAsInt(value));
            out.add(e);
        }
        return out;
    }

    private static JsonArray mapInt(float[] inputs, java.util.function.Function<Float, Integer> op) {
        JsonArray out = new JsonArray();
        for (float value : inputs) {
            JsonObject e = new JsonObject();
            e.addProperty("in", hex(value));
            e.addProperty("out", op.apply(value));
            out.add(e);
        }
        return out;
    }

    private static JsonArray mapDouble(double[] inputs, java.util.function.DoubleToIntFunction op) {
        JsonArray out = new JsonArray();
        for (double value : inputs) {
            JsonObject e = new JsonObject();
            e.addProperty("in", hex(value));
            e.addProperty("out", op.applyAsInt(value));
            out.add(e);
        }
        return out;
    }

    private static JsonArray mapLong(double[] inputs, java.util.function.DoubleToLongFunction op) {
        JsonArray out = new JsonArray();
        for (double value : inputs) {
            JsonObject e = new JsonObject();
            e.addProperty("in", hex(value));
            e.addProperty("out", op.applyAsLong(value));
            out.add(e);
        }
        return out;
    }

    private static JsonArray lerpDouble(double[][] cases, boolean unclamped) {
        JsonArray out = new JsonArray();
        for (double[] c : cases) {
            JsonObject e = new JsonObject();
            e.addProperty("a", hex(c[0]));
            e.addProperty("b", hex(c[1]));
            e.addProperty("t", hex(c[2]));
            e.addProperty("out", hex(unclamped ? MathUtil.lerpUnclamped(c[0], c[1], c[2]) : MathUtil.lerp(c[0], c[1], c[2])));
            out.add(e);
        }
        return out;
    }

    private static void addLineInputs(JsonObject e, double[] c) {
        e.addProperty("x", hex(c[0]));
        e.addProperty("y", hex(c[1]));
        e.addProperty("ax", hex(c[2]));
        e.addProperty("ay", hex(c[3]));
        e.addProperty("bx", hex(c[4]));
        e.addProperty("by", hex(c[5]));
    }

    // ------------------------------------------------------------------
    // Noise
    // ------------------------------------------------------------------

    private static JsonObject dumpNoise() {
        JsonObject root = new JsonObject();
        root.addProperty("source", "com.hypixel.hytale.procedurallib.logic");
        root.addProperty("format", "values are row-major Java Double.toHexString strings");
        root.addProperty("seedSemantics", "NoiseFunction.get(seed, offsetSeed, ...) is called with offsetSeed = seed; most noise implementations hash offsetSeed and ignore seed");

        root.add("grid2d", gridMeta(GRID_2D_SIZE, GRID_2D_START, GRID_2D_STEP, "values[y][x]"));
        root.add("grid3d", gridMeta(GRID_3D_SIZE, GRID_3D_START, GRID_3D_STEP, "values[z][y][x]"));

        Map<String, NoiseEntry> noises = createNoises();
        for (NoiseType type : NoiseType.values()) {
            if (!noises.containsKey(type.name())) {
                throw new IllegalStateException("No golden-noise entry for NoiseType." + type);
            }
        }

        JsonObject noise2d = new JsonObject();
        JsonObject noise3d = new JsonObject();
        for (Map.Entry<String, NoiseEntry> entry : noises.entrySet()) {
            NoiseEntry value = entry.getValue();
            JsonObject item = new JsonObject();
            item.addProperty("class", value.className());
            item.add("config", value.config());
            item.add("seeds", dump2d(value.function()));
            noise2d.add(entry.getKey(), item);
        }

        for (Map.Entry<String, NoiseEntry> entry : noises.entrySet()) {
            NoiseEntry value = entry.getValue();
            if (entry.getKey().equals("MESH") || entry.getKey().equals("BRANCH")) {
                continue;
            }
            JsonObject item = new JsonObject();
            item.addProperty("class", value.className());
            item.add("config", value.config());
            item.add("seeds", dump3d(value.function()));
            noise3d.add(entry.getKey(), item);
        }

        root.add("noise2d", noise2d);
        root.add("noise3d", noise3d);
        return root;
    }

    private static JsonObject gridMeta(int size, double start, double step, String layout) {
        JsonObject grid = new JsonObject();
        grid.addProperty("size", size);
        grid.addProperty("start", hex(start));
        grid.addProperty("step", hex(step));
        grid.addProperty("layout", layout);
        return grid;
    }

    private static JsonObject dump2d(NoiseFunction noise) {
        JsonObject seeds = new JsonObject();
        for (int seed : NOISE_SEEDS) {
            JsonArray rows = new JsonArray();
            for (int yi = 0; yi < GRID_2D_SIZE; yi++) {
                JsonArray row = new JsonArray();
                double y = GRID_2D_START + yi * GRID_2D_STEP;
                for (int xi = 0; xi < GRID_2D_SIZE; xi++) {
                    double x = GRID_2D_START + xi * GRID_2D_STEP;
                    double value = noise.get(seed, seed, x, y);
                    requireFinite(value, noise.getClass().getSimpleName() + " 2D seed=" + seed + " x=" + x + " y=" + y);
                    row.add(hex(value));
                }
                rows.add(row);
            }
            seeds.add(Integer.toString(seed), rows);
        }
        return seeds;
    }

    private static JsonObject dump3d(NoiseFunction noise) {
        JsonObject seeds = new JsonObject();
        for (int seed : NOISE_SEEDS) {
            JsonArray zRows = new JsonArray();
            for (int zi = 0; zi < GRID_3D_SIZE; zi++) {
                JsonArray yRows = new JsonArray();
                double z = GRID_3D_START + zi * GRID_3D_STEP;
                for (int yi = 0; yi < GRID_3D_SIZE; yi++) {
                    JsonArray xRow = new JsonArray();
                    double y = GRID_3D_START + yi * GRID_3D_STEP;
                    for (int xi = 0; xi < GRID_3D_SIZE; xi++) {
                        double x = GRID_3D_START + xi * GRID_3D_STEP;
                        double value = noise.get(seed, seed, x, y, z);
                        requireFinite(value, noise.getClass().getSimpleName() + " 3D seed=" + seed + " x=" + x + " y=" + y + " z=" + z);
                        xRow.add(hex(value));
                    }
                    yRows.add(xRow);
                }
                zRows.add(yRows);
            }
            seeds.add(Integer.toString(seed), zRows);
        }
        return seeds;
    }

    private static Map<String, NoiseEntry> createNoises() {
        Map<String, NoiseEntry> noises = new TreeMap<>();

        noises.put("SIMPLEX", new NoiseEntry(SimplexNoise.INSTANCE, "SimplexNoise", config()));
        noises.put("OLD_SIMPLEX", new NoiseEntry(OldSimplexNoise.INSTANCE, "OldSimplexNoise", config()));

        JsonObject valueConfig = config();
        valueConfig.addProperty("interpolationMode", "QUINTIC");
        noises.put("VALUE", new NoiseEntry(new ValueNoise(com.hypixel.hytale.procedurallib.logic.GeneralNoise.InterpolationMode.QUINTIC.getFunction()), "ValueNoise", valueConfig));
        noises.put("PERLIN", new NoiseEntry(new PerlinNoise(com.hypixel.hytale.procedurallib.logic.GeneralNoise.InterpolationMode.QUINTIC.getFunction()), "PerlinNoise", valueConfig.deepCopy()));

        JsonObject constantConfig = config();
        constantConfig.addProperty("value", 0.5);
        noises.put("CONSTANT", new NoiseEntry(new ConstantNoise(0.5), "ConstantNoise", constantConfig));

        JsonObject gridConfig = config();
        gridConfig.addProperty("thicknessX", 0.1);
        gridConfig.addProperty("thicknessY", 0.1);
        gridConfig.addProperty("thicknessZ", 0.1);
        gridConfig.addProperty("note", "the JSON loader has no default thickness; 0.1 is a representative value");
        noises.put("GRID", new NoiseEntry(new GridNoise(0.1, 0.1, 0.1), "GridNoise", gridConfig));

        JsonObject cellConfig = config();
        cellConfig.addProperty("cellType", "SQUARE");
        cellConfig.addProperty("distanceFunction", "GridCellDistanceFunction.DISTANCE_FUNCTION");
        cellConfig.addProperty("pointEvaluator", "NormalPointEvaluator.EUCLIDEAN");
        cellConfig.addProperty("cellMode", "CELL_VALUE");
        cellConfig.addProperty("jitter", 1.0);
        noises.put("CELL", new NoiseEntry(
            new CellNoise(GridCellDistanceFunction.DISTANCE_FUNCTION, NormalPointEvaluator.EUCLIDEAN, CellNoise.CellMode.CELL_VALUE.getFunction(), null),
            "CellNoise",
            cellConfig));

        JsonObject distanceConfig = config();
        distanceConfig.addProperty("cellType", "SQUARE");
        distanceConfig.addProperty("measurementMode", "CENTRE_DISTANCE");
        distanceConfig.addProperty("distanceCalculationMode", "EUCLIDEAN");
        distanceConfig.addProperty("distance2Mode", "SUB");
        distanceConfig.addProperty("jitter", 1.0);
        noises.put("DISTANCE", new NoiseEntry(
            new DumpDistanceNoise(GridCellDistanceFunction.DISTANCE_FUNCTION, NormalPointEvaluator.EUCLIDEAN, DistanceNoise.Distance2Mode.SUB.getFunction()),
            "DistanceNoise(subclass)",
            distanceConfig));

        JsonObject meshConfig = config();
        meshConfig.addProperty("cellType", "SQUARE");
        meshConfig.addProperty("thickness", 0.1);
        meshConfig.addProperty("jitterX", 1.0);
        meshConfig.addProperty("jitterY", 1.0);
        meshConfig.addProperty("density", "always true");
        meshConfig.addProperty("note", "the JSON loader requires a Thickness; 0.1 is representative; MESH maps to the square MeshNoise");
        IIntCondition density = DensityPointEvaluator.getDensityCondition(null);
        noises.put("MESH", new NoiseEntry(new MeshNoise(density, 0.1, 1.0, 1.0), "MeshNoise", meshConfig));

        JsonObject pointConfig = config();
        pointConfig.addProperty("x", 1.5);
        pointConfig.addProperty("y", -2.0);
        pointConfig.addProperty("z", 0.5);
        pointConfig.addProperty("innerRadius", 1.0);
        pointConfig.addProperty("outerRadius", 8.0);
        pointConfig.addProperty("note", "representative non-degenerate radii; the JSON loader defaults are all zero");
        noises.put("POINT", new NoiseEntry(new PointNoise(1.5, -2.0, 0.5, 1.0, 8.0), "PointNoise", pointConfig));

        JsonObject branchConfig = config();
        branchConfig.addProperty("parentType", "SQUARE");
        branchConfig.addProperty("parentDistanceMode", "EUCLIDEAN");
        branchConfig.addProperty("parentDistanceType", "DIV");
        branchConfig.addProperty("parentFormula", "SQRT");
        branchConfig.addProperty("parentValue", 0.0);
        branchConfig.addProperty("parentFade", 1.0);
        branchConfig.addProperty("parentDensity", "always true");
        branchConfig.addProperty("lineType", "SQUARE");
        branchConfig.addProperty("lineDirection", "OUTWARD");
        branchConfig.addProperty("lineScale", 0.1);
        branchConfig.addProperty("lineThickness", 0.1);
        branchConfig.addProperty("jitter", 1.0);
        branchConfig.addProperty("note", "parentFade is 1.0 (the loader default is 0.0, which makes the output constant -1)");
        noises.put("BRANCH", new NoiseEntry(createBranchNoise(), "BranchNoise", branchConfig));

        return noises;
    }

    private static BranchNoise createBranchNoise() {
        CellDistanceFunction parentFunction = GridCellDistanceFunction.DISTANCE_FUNCTION;
        CellJitter jitter = CellJitter.of(1.0, 1.0, 1.0);
        PointEvaluator parentEvaluator = PointEvaluator.of(
            DistanceCalculationMode.EUCLIDEAN.getFunction(), null, null, jitter);
        BranchEvaluator lineEvaluator = new BranchEvaluator(
            parentFunction,
            GridCellDistanceFunction.POINT_FUNCTION,
            BranchEvaluator.Direction.OUTWARD,
            jitter,
            0.1);
        return new BranchNoise(
            parentFunction,
            parentEvaluator,
            0.0,
            new DoubleRange.Constant(1.0),
            ConstantIntCondition.DEFAULT_TRUE,
            DistanceNoise.Distance2Mode.DIV.getFunction(),
            NoiseFormulaProperty.NoiseFormula.SQRT.getFormula(),
            GridCellDistanceFunction.DISTANCE_FUNCTION,
            lineEvaluator,
            0.1,
            new DoubleRange.Constant(0.1));
    }

    private static JsonObject config() {
        return new JsonObject();
    }

    private static void requireFinite(double value, String where) {
        if (Double.isNaN(value) || Double.isInfinite(value)) {
            throw new IllegalStateException("Non-finite golden value at " + where + ": " + value);
        }
    }

    private record NoiseEntry(NoiseFunction function, String className, JsonObject config) {
    }

    /**
     * Concrete DistanceNoise because the JSON loader normally supplies the buffers.
     */
    private static final class DumpDistanceNoise extends DistanceNoise {
        private DumpDistanceNoise(CellDistanceFunction cellDistanceFunction,
                                  PointEvaluator pointEvaluator,
                                  Distance2Function distance2Function) {
            super(cellDistanceFunction, pointEvaluator, distance2Function);
        }

        @Override
        protected ResultBuffer.ResultBuffer2d localBuffer2d() {
            return ResultBuffer.buffer2d;
        }

        @Override
        protected ResultBuffer.ResultBuffer3d localBuffer3d() {
            return ResultBuffer.buffer3d;
        }
    }

    private static String hex(double value) {
        return Double.toHexString(value);
    }

    private static String hex(float value) {
        return Float.toHexString(value);
    }
}
