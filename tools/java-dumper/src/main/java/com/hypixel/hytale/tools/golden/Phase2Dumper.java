package com.hypixel.hytale.tools.golden;

import com.google.gson.JsonObject;
import com.hypixel.hytale.procedurallib.NoiseFunction;
import com.hypixel.hytale.procedurallib.file.FileIO;
import com.hypixel.hytale.procedurallib.json.CoordinateRandomizerJsonLoader;
import com.hypixel.hytale.procedurallib.json.JsonLoader;
import com.hypixel.hytale.procedurallib.json.SeedResource;
import com.hypixel.hytale.procedurallib.json.SeedString;
import com.hypixel.hytale.procedurallib.logic.ConstantNoise;
import com.hypixel.hytale.procedurallib.logic.GridNoise;
import com.hypixel.hytale.procedurallib.logic.PointNoise;
import com.hypixel.hytale.procedurallib.logic.SimplexNoise;
import com.hypixel.hytale.procedurallib.property.BlendNoiseProperty;
import com.hypixel.hytale.procedurallib.property.DistortedNoiseProperty;
import com.hypixel.hytale.procedurallib.property.FractalNoiseProperty;
import com.hypixel.hytale.procedurallib.property.NoiseProperty;
import com.hypixel.hytale.procedurallib.property.NormalizeNoiseProperty;
import com.hypixel.hytale.procedurallib.property.OffsetNoiseProperty;
import com.hypixel.hytale.procedurallib.property.ScaleNoiseProperty;
import com.hypixel.hytale.procedurallib.property.SingleNoiseProperty;
import com.hypixel.hytale.procedurallib.property.SumNoiseProperty;
import com.hypixel.hytale.procedurallib.random.CoordinateRandomizer;
import com.hypixel.hytale.procedurallib.random.ICoordinateRandomizer;
import com.hypixel.hytale.server.worldgen.climate.ClimateColor;
import com.hypixel.hytale.server.worldgen.climate.ClimateGraph;
import com.hypixel.hytale.server.worldgen.climate.ClimateNoise;
import com.hypixel.hytale.server.worldgen.climate.ClimatePoint;
import com.hypixel.hytale.server.worldgen.climate.ClimateType;
import com.hypixel.hytale.server.worldgen.loader.climate.ClimateGraphJsonLoader;
import com.hypixel.hytale.server.worldgen.loader.climate.ClimateNoiseJsonLoader;
import com.hypixel.hytale.server.worldgen.zoom.FuzzyZoom;
import com.hypixel.hytale.server.worldgen.zoom.PixelProvider;
import com.hypixel.hytale.server.worldgen.zone.Zone;

import org.joml.Vector2i;

import java.awt.image.BufferedImage;
import java.io.IOException;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.file.Path;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Deque;
import java.util.List;

/**
 * Phase-2 golden dumps: the world mask ({@code zoom}) and the procedural
 * climate mask ({@code climate}) of the Default world.
 *
 * <p>The Default world ({@code testdata/Default}) uses the <em>procedural</em>
 * climate mask: {@code World.json} points at {@code Mask.json}, which carries a
 * {@code Randomizer} (the legacy {@code Mask.png} image mask no longer exists).
 * Both dumps therefore load the real configuration through the real Java
 * loaders ({@code ClimateNoiseJsonLoader}, {@code ClimateGraphJsonLoader},
 * {@code CoordinateRandomizerJsonLoader}) and dump
 *
 * <ul>
 *   <li>the resolved construction of every noise property (including the
 *       integer seed offsets the loaders derived from the seed strings), so a
 *       Rust port can rebuild the exact same graph, and</li>
 *   <li>the sampled results of the ported code paths.</li>
 * </ul>
 *
 * <p>Every floating-point value is written as {@link Double#toHexString(double)}
 * so a Rust port can compare bit patterns without decimal-rounding ambiguity.
 */
final class Phase2Dumper {

    /** Fixed world seeds (runtime seed int + seed string) used by both dumps. */
    static final int[] WORLD_SEEDS = {0, 1, 42, 1234567};

    /** Rasterised continent mask size in pixels (per side). */
    static final int MASK_IMAGE_SIZE = 128;
    /** World blocks covered by one mask pixel (= {@code zoomX}/{@code zoomY}). */
    static final double MASK_BLOCKS_PER_PIXEL = 32.0;
    /** Mask pixel colour for land. */
    static final int MASK_LAND_COLOR = 0x00ff00;
    /** Mask pixel colour for ocean. */
    static final int MASK_OCEAN_COLOR = 0x0000ff;

    /** World coordinates sampled by the zoom dump (special values). */
    static final int[] ZOOM_SPECIAL_COORDS = {-1000, -333, 0, 333, 1000};
    /** Size of the square block grid around the origin sampled by the zoom dump. */
    static final int ZOOM_GRID_SIZE = 16;
    /** Distance between two grid samples (blocks). */
    static final double ZOOM_GRID_STEP = 32.0;
    /** First grid sample (blocks). */
    static final double ZOOM_GRID_START = -240.0;

    /** Size of the climate sample grid (per side). */
    static final int CLIMATE_GRID_SIZE = 16;
    /** First climate sample (blocks). */
    static final double CLIMATE_GRID_START = -2048.0;
    /** Distance between two climate samples (blocks). */
    static final double CLIMATE_GRID_STEP = 256.0;

    private Phase2Dumper() {
    }

    // ------------------------------------------------------------------
    // Zoom (mask) dump
    // ------------------------------------------------------------------

    static String dumpZoom(Path worldDir) throws IOException {
        FileIO.setDefaultRoot(worldDir);
        final JsonObject maskJson = loadMaskJson(worldDir);

        final JsonWriter w = new JsonWriter();
        w.beginObject();
        w.name("class").value("com.hypixel.hytale.server.worldgen.zoom.FuzzyZoom");
        w.name("format").value("floating-point values are Java hex strings (Double.toHexString); "
            + "pixel values are 24-bit RGB integers");
        w.name("source").value("testdata/Default - the Default world uses the procedural climate mask "
            + "(World.json -> Mask.json); the legacy Mask.png image mask no longer exists, so the "
            + "continent mask is rasterised from the real Noise.Continent configuration");
        w.name("seedsNote").value("the world seed int is used as the runtime seed and its decimal "
            + "representation is used as the seed string (the loaders derive the noise seed offsets "
            + "from that string)");

        // --- construction ------------------------------------------------
        w.name("construction");
        w.beginObject();
        w.name("mask");
        w.beginObject();
        w.name("imageWidth").value(MASK_IMAGE_SIZE);
        w.name("imageHeight").value(MASK_IMAGE_SIZE);
        w.name("blocksPerPixel").value(MASK_BLOCKS_PER_PIXEL);
        w.name("offsetX").value(0);
        w.name("offsetY").value(0);
        w.name("originX").value(-(MASK_IMAGE_SIZE / 2) * MASK_BLOCKS_PER_PIXEL);
        w.name("originY").value(-(MASK_IMAGE_SIZE / 2) * MASK_BLOCKS_PER_PIXEL);
        w.name("landThresholdNote").value("pixels are the land/ocean classification of the real "
            + "continent noise at the Noise.Thresholds.Land threshold");
        w.name("landColor").value(MASK_LAND_COLOR);
        w.name("oceanColor").value(MASK_OCEAN_COLOR);
        w.name("randomizerJson").rawObject(maskJson.getAsJsonObject("Randomizer").toString());
        w.name("pixelsNote").value("rasterised mask, one entry per seed, in PixelProvider array order "
            + "(index = x * height + y)");
        w.name("pixels");
        w.beginObject();
        for (int seed : WORLD_SEEDS) {
            final SeedString<SeedResource> seedString = seedString(seed);
            final ClimateNoise climateNoise = loadClimateNoise(seedString, worldDir, maskJson);
            w.name(Integer.toString(seed)).rawArray(pixels(continentProvider(climateNoise, seed)));
        }
        w.endObject();
        w.endObject();

        w.name("randomizerConstruction");
        w.beginObject();
        w.name("note").value("the Mask.json Randomizer section is loaded by "
            + "CoordinateRandomizerJsonLoader; the generator noise is "
            + "ScaleNoiseProperty(SingleNoiseProperty(seedOffset, SIMPLEX), scale)");
        for (int seed : WORLD_SEEDS) {
            final SeedString<SeedResource> seedString = seedString(seed);
            final ICoordinateRandomizer randomizer =
                new CoordinateRandomizerJsonLoader<>(seedString, worldDir, maskJson.get("Randomizer")).load();
            w.name(Integer.toString(seed)).rawObject(describeRandomizer(randomizer));
        }
        w.endObject();
        w.endObject(); // construction

        // --- sample grid -------------------------------------------------
        w.name("grid");
        w.beginObject();
        w.name("note").value("the world coordinates sampled for every seed; the mask is sampled at the "
            + "warped coordinates (rx, rz) exactly like ZonePatternGenerator.generate does");
        w.name("coordinates").rawArray(sampleCoordinates());
        w.endObject();

        // --- per-seed samples --------------------------------------------
        w.name("seeds");
        w.beginObject();
        for (int seed : WORLD_SEEDS) {
            final SeedString<SeedResource> seedString = seedString(seed);
            final ClimateNoise climateNoise = loadClimateNoise(seedString, worldDir, maskJson);
            final ICoordinateRandomizer randomizer =
                new CoordinateRandomizerJsonLoader<>(seedString, worldDir, maskJson.get("Randomizer")).load();
            final FuzzyZoom zoom = new FuzzyZoom(randomizer, continentProvider(climateNoise, seed),
                MASK_BLOCKS_PER_PIXEL, MASK_BLOCKS_PER_PIXEL, 0, 0);

            w.name(Integer.toString(seed));
            w.beginObject();

            final StringBuilder getX = new StringBuilder();
            final StringBuilder getY = new StringBuilder();
            final StringBuilder mask = new StringBuilder();
            final StringBuilder distance = new StringBuilder();
            final StringBuilder distanceRaw = new StringBuilder();
            final StringBuilder inBounds = new StringBuilder();

            for (double[] c : sampleCoordinateList()) {
                final double x = c[0];
                final double z = c[1];
                final double rx = zoom.getX(seed, x, z);
                final double rz = zoom.getY(seed, x, z);
                final int pixel = zoom.generate(rx, rz) & 0xFFFFFF;
                final boolean bounds = zoom.inBounds(rx, rz);
                final double raw = zoom.distance(rx, rz);

                append(getX, JsonWriter.quoteString(hex(rx)));
                append(getY, JsonWriter.quoteString(hex(rz)));
                append(mask, Integer.toString(pixel));
                // ZonePatternGenerator only asks for the distance when the
                // warped coordinate is inside the mask.
                append(distance, JsonWriter.quoteString(bounds ? hex(raw) : hex(Double.POSITIVE_INFINITY)));
                append(distanceRaw, JsonWriter.quoteString(hex(raw)));
                append(inBounds, Boolean.toString(bounds));
            }

            w.name("getX").rawArray(getX.toString());
            w.name("getY").rawArray(getY.toString());
            w.name("mask").rawArray(mask.toString());
            w.name("distance").rawArray(distance.toString());
            w.name("distanceRaw").rawArray(distanceRaw.toString());
            w.name("inBounds").rawArray(inBounds.toString());
            w.name("uniqueZoneCandidates");
            w.rawArray(uniqueZoneCandidates(zoom));
            w.endObject();
        }
        w.endObject();

        w.endObject();
        return w.toString();
    }

    // ------------------------------------------------------------------
    // Climate dump
    // ------------------------------------------------------------------

    static String dumpClimate(Path worldDir) throws IOException {
        FileIO.setDefaultRoot(worldDir);
        final JsonObject maskJson = loadMaskJson(worldDir);

        // The climate graph does not depend on the seed.
        final SeedString<SeedResource> graphSeed = seedString(WORLD_SEEDS[0]);
        final ClimateGraph graph =
            new ClimateGraphJsonLoader<>(graphSeed, worldDir, maskJson.get("Climate")).load();

        final JsonWriter w = new JsonWriter();
        w.beginObject();
        w.name("class").value("com.hypixel.hytale.server.worldgen.climate.ClimateGraph / ClimateNoise");
        w.name("format").value("floating-point values are Java hex strings (Double.toHexString); "
            + "the table/fade maps are flat row-major arrays (index = y * width + x)");
        w.name("source").value("testdata/Default - Mask.json (procedural climate mask)");

        // --- graph construction ------------------------------------------
        w.name("construction");
        w.beginObject();
        w.name("graph");
        w.beginObject();
        w.name("resolution").value(ClimateGraph.RESOLUTION);
        w.name("fadeMode").value(graph.fadeMode().name());
        w.name("fadeRadius").value(graph.fadeRadius());
        w.name("fadeDistance").value(graph.fadeDistance());
        final ClimateType[] parents = graph.getParents();
        final StringBuilder climates = new StringBuilder();
        for (int i = 0; i < parents.length; i++) {
            if (i > 0) climates.append(',');
            climates.append(describeClimateType(parents[i]));
        }
        w.name("climates").rawArray(climates.toString());
        w.endObject();
        w.endObject(); // construction

        // --- graph table + fade ------------------------------------------
        w.name("graph");
        w.beginObject();
        w.name("tableNote").value("climate type ids (the table index of the climate type tree)");
        final StringBuilder tableOut = new StringBuilder();
        for (int y = 0; y < graph.getTable().height; y++) {
            for (int x = 0; x < graph.getTable().width; x++) {
                if (y > 0 || x > 0) tableOut.append(',');
                tableOut.append(graph.getTable().at(x, y));
            }
        }
        w.name("table").rawArray(tableOut.toString());

        w.name("fadeNote").value("raw distance-transform values (fade * fadeDistance is the scaled value)");
        final StringBuilder fadeOut = new StringBuilder();
        for (int y = 0; y < graph.getFade().height; y++) {
            for (int x = 0; x < graph.getFade().width; x++) {
                if (y > 0 || x > 0) fadeOut.append(',');
                fadeOut.append(JsonWriter.quoteString(hex(graph.getFade().at(x, y))));
            }
        }
        w.name("fade").rawArray(fadeOut.toString());
        w.endObject();

        // --- noise samples -------------------------------------------------
        w.name("noise");
        w.beginObject();
        w.name("grid");
        w.beginObject();
        w.name("size").value(CLIMATE_GRID_SIZE);
        w.name("start").value(CLIMATE_GRID_START);
        w.name("step").value(CLIMATE_GRID_STEP);
        w.name("layout").value("values[z][x]");
        w.endObject();

        w.name("seeds");
        w.beginObject();
        for (int seed : WORLD_SEEDS) {
            final SeedString<SeedResource> seedString = seedString(seed);
            final ClimateNoise noise = loadClimateNoise(seedString, worldDir, maskJson);
            final ClimateNoise.Buffer buffer = new ClimateNoise.Buffer();

            w.name(Integer.toString(seed));
            w.beginObject();

            w.name("construction");
            w.beginObject();
            w.name("grid");
            w.beginObject();
            w.name("seedOffset").value(noise.grid.seedOffset);
            w.name("scale").value(noise.grid.scale);
            w.name("gridKind").value("DIRECT");
            w.name("evaluator").value("EUCLIDEAN");
            w.endObject();
            w.name("thresholds");
            w.beginObject();
            w.name("land").value(noise.thresholds.land);
            w.name("island").value(noise.thresholds.island);
            w.name("beachSize").value(noise.thresholds.beachSize);
            w.name("shallowOceanSize").value(noise.thresholds.shallowOceanSize);
            w.endObject();
            w.name("continent").rawObject(describeNoiseProperty(noise.continent));
            w.name("temperature").rawObject(describeNoiseProperty(noise.temperature));
            w.name("intensity").rawObject(describeNoiseProperty(noise.intensity));
            w.endObject();

            final StringBuilder ids = new StringBuilder();
            final StringBuilder continent = new StringBuilder();
            final StringBuilder temperature = new StringBuilder();
            final StringBuilder intensity = new StringBuilder();
            final StringBuilder fade = new StringBuilder();

            for (int j = 0; j < CLIMATE_GRID_SIZE; j++) {
                for (int i = 0; i < CLIMATE_GRID_SIZE; i++) {
                    final double x = CLIMATE_GRID_START + i * CLIMATE_GRID_STEP;
                    final double y = CLIMATE_GRID_START + j * CLIMATE_GRID_STEP;
                    final int id = noise.generate(seed, x, y, buffer, graph);
                    if (j > 0 || i > 0) {
                        ids.append(',');
                        continent.append(',');
                        temperature.append(',');
                        intensity.append(',');
                        fade.append(',');
                    }
                    ids.append(id);
                    continent.append(JsonWriter.quoteString(hex(buffer.continent)));
                    temperature.append(JsonWriter.quoteString(hex(buffer.temperature)));
                    intensity.append(JsonWriter.quoteString(hex(buffer.intensity)));
                    fade.append(JsonWriter.quoteString(hex(buffer.fade)));
                }
            }

            w.name("id").rawArray(ids.toString());
            w.name("continent").rawArray(continent.toString());
            w.name("temperature").rawArray(temperature.toString());
            w.name("intensity").rawArray(intensity.toString());
            w.name("fade").rawArray(fade.toString());
            w.endObject();
        }
        w.endObject(); // seeds

        w.endObject(); // noise

        w.endObject(); // root
        return w.toString();
    }

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    private static JsonObject loadMaskJson(Path worldDir) throws IOException {
        return FileIO.load(worldDir.resolve("Mask.json"), JsonLoader.JSON_OBJ_LOADER);
    }

    private static SeedString<SeedResource> seedString(int seed) {
        return new SeedString<>(Integer.toString(seed), new SeedResource() {
        });
    }

    private static ClimateNoise loadClimateNoise(SeedString<SeedResource> seedString, Path worldDir,
                                                  JsonObject maskJson) {
        return new ClimateNoiseJsonLoader<>(seedString, worldDir, maskJson.get("Noise")).load();
    }

    /**
     * Rasterises the real continent noise into a land/ocean mask image.
     */
    private static PixelProvider continentProvider(ClimateNoise climateNoise, int seed) {
        final BufferedImage image = new BufferedImage(MASK_IMAGE_SIZE, MASK_IMAGE_SIZE, BufferedImage.TYPE_INT_RGB);
        final double origin = -(MASK_IMAGE_SIZE / 2) * MASK_BLOCKS_PER_PIXEL;
        final double land = climateNoise.thresholds.land;

        for (int px = 0; px < MASK_IMAGE_SIZE; px++) {
            for (int py = 0; py < MASK_IMAGE_SIZE; py++) {
                final double wx = origin + px * MASK_BLOCKS_PER_PIXEL;
                final double wy = origin + py * MASK_BLOCKS_PER_PIXEL;
                final double value = climateNoise.continent.get(seed, wx, wy);
                image.setRGB(px, py, value >= land ? MASK_LAND_COLOR : MASK_OCEAN_COLOR);
            }
        }

        return new PixelProvider(image);
    }

    /** The rasterised mask pixels in {@code PixelProvider} array order. */
    private static String pixels(PixelProvider provider) {
        final StringBuilder out = new StringBuilder();
        final int[] pixels = provider.getPixels();
        for (int i = 0; i < pixels.length; i++) {
            if (i > 0) out.append(',');
            out.append(pixels[i] & 0xFFFFFF);
        }
        return out.toString();
    }

    /**
     * Runs {@code ExactZoom.generateUniqueZoneCandidates} over the rasterised
     * mask.
     *
     * <p>The scanned zone is only used through {@code radius()},
     * {@code padding()} and {@code matchesParent(color)}; the {@code Zone} handle
     * itself is never dereferenced by the scan, so the dumper passes {@code null}
     * for it (constructing a real {@code Zone} would need the prefab store from
     * the CoreServer, which is out of scope for the dumper).
     */
    private static String uniqueZoneCandidates(FuzzyZoom zoom) {
        final Zone.UniqueEntry[] entries = {
            new Zone.UniqueEntry(null, MASK_LAND_COLOR, new int[]{MASK_LAND_COLOR}, 4, 2),
            new Zone.UniqueEntry(null, MASK_OCEAN_COLOR, new int[]{MASK_OCEAN_COLOR}, 2, 0)
        };

        final Zone.UniqueCandidate[] candidates = zoom.generateUniqueZoneCandidates(entries, 64);

        final StringBuilder out = new StringBuilder();
        for (int i = 0; i < candidates.length; i++) {
            if (i > 0) out.append(',');
            final Zone.UniqueEntry entry = candidates[i].zone();
            out.append("{\"color\":").append(entry.color());
            out.append(",\"parent\":[");
            for (int p = 0; p < entry.parent().length; p++) {
                if (p > 0) out.append(',');
                out.append(entry.parent()[p]);
            }
            out.append(']');
            out.append(",\"radius\":").append(entry.radius());
            out.append(",\"padding\":").append(entry.padding());
            out.append(",\"positions\":[");
            final Vector2i[] positions = candidates[i].positions();
            for (int p = 0; p < positions.length; p++) {
                if (p > 0) out.append(',');
                out.append('[').append(positions[p].x).append(',').append(positions[p].y).append(']');
            }
            out.append("]}");
        }
        return out.toString();
    }

    private static List<double[]> sampleCoordinateList() {
        final List<double[]> coords = new ArrayList<>();
        for (double x : ZOOM_SPECIAL_COORDS) {
            for (double z : ZOOM_SPECIAL_COORDS) {
                coords.add(new double[]{x, z});
            }
        }
        for (int i = 0; i < ZOOM_GRID_SIZE; i++) {
            final double x = ZOOM_GRID_START + i * ZOOM_GRID_STEP;
            for (int j = 0; j < ZOOM_GRID_SIZE; j++) {
                final double z = ZOOM_GRID_START + j * ZOOM_GRID_STEP;
                coords.add(new double[]{x, z});
            }
        }
        return coords;
    }

    private static String sampleCoordinates() {
        final StringBuilder out = new StringBuilder();
        boolean first = true;
        for (double[] c : sampleCoordinateList()) {
            if (!first) out.append(',');
            first = false;
            out.append('[').append(JsonWriter.quoteString(hex(c[0]))).append(',')
                .append(JsonWriter.quoteString(hex(c[1]))).append(']');
        }
        return out.toString();
    }

    private static void append(StringBuilder sb, String value) {
        if (sb.length() > 0) sb.append(',');
        sb.append(value);
    }

    private static String hex(double value) {
        return Double.toHexString(value);
    }

    // ------------------------------------------------------------------
    // Construction description (reflection based)
    // ------------------------------------------------------------------

    private static String describeClimateType(ClimateType type) {
        final StringBuilder sb = new StringBuilder();
        sb.append('{');
        sb.append("\"name\":").append(JsonWriter.quoteString(type.name));
        sb.append(",\"color\":").append(describeClimateColor(type.color));
        sb.append(",\"island\":").append(describeClimateColor(type.island));
        sb.append(",\"points\":[");
        for (int i = 0; i < type.points.length; i++) {
            if (i > 0) sb.append(',');
            final ClimatePoint point = type.points[i];
            sb.append("{\"temperature\":").append(JsonWriter.quoteString(hex(point.temperature)));
            sb.append(",\"intensity\":").append(JsonWriter.quoteString(hex(point.intensity)));
            sb.append(",\"modifier\":").append(JsonWriter.quoteString(hex(point.modifier))).append('}');
        }
        sb.append(']');
        sb.append(",\"children\":[");
        for (int i = 0; i < type.children.length; i++) {
            if (i > 0) sb.append(',');
            sb.append(describeClimateType(type.children[i]));
        }
        sb.append("]}");
        return sb.toString();
    }

    private static String describeClimateColor(ClimateColor color) {
        return "{\"land\":" + color.land
            + ",\"shore\":" + color.shore
            + ",\"ocean\":" + color.ocean
            + ",\"shallowOcean\":" + color.shallowOcean + '}';
    }

    private static String describeRandomizer(ICoordinateRandomizer randomizer) {
        if (!(randomizer instanceof CoordinateRandomizer coordinateRandomizer)) {
            return "{\"kind\":\"" + JsonWriter.quoteString(randomizer.getClass().getSimpleName()) + "\"}";
        }

        return "{\"kind\":\"COORDINATE_RANDOMIZER\""
            + ",\"x\":" + describeGenerators(coordinateRandomizer.getXNoise())
            + ",\"y\":" + describeGenerators(coordinateRandomizer.getYNoise())
            + ",\"z\":" + describeGenerators(coordinateRandomizer.getZNoise())
            + '}';
    }

    private static String describeGenerators(CoordinateRandomizer.AmplitudeNoiseProperty[] generators) {
        final StringBuilder out = new StringBuilder("[");
        for (int i = 0; i < generators.length; i++) {
            if (i > 0) out.append(',');
            out.append("{\"amplitude\":").append(JsonWriter.quoteString(hex(generators[i].getAmplitude())));
            out.append(",\"property\":").append(describeNoiseProperty(generators[i].getProperty()));
            out.append('}');
        }
        return out.append(']').toString();
    }

    private static String describeNoiseProperty(NoiseProperty property) {
        final StringBuilder sb = new StringBuilder();
        if (property instanceof SingleNoiseProperty single) {
            sb.append("{\"kind\":\"SINGLE\",\"seedOffset\":").append(single.getSeedOffset());
            sb.append(",\"function\":").append(describeNoiseFunction(single.getFunction())).append('}');
        } else if (property instanceof FractalNoiseProperty fractal) {
            sb.append("{\"kind\":\"FRACTAL\",\"seedOffset\":").append(fractal.getSeedOffset());
            sb.append(",\"mode\":").append(JsonWriter.quoteString(fractalMode(fractal)));
            sb.append(",\"octaves\":").append(fractal.getOctaves());
            sb.append(",\"lacunarity\":").append(JsonWriter.quoteString(hex(fractal.getLacunarity())));
            sb.append(",\"persistence\":").append(JsonWriter.quoteString(hex(fractal.getPersistence())));
            sb.append(",\"function\":").append(describeNoiseFunction(fractal.getFunction())).append('}');
        } else if (property instanceof NormalizeNoiseProperty normalize) {
            sb.append("{\"kind\":\"NORMALIZE\"");
            sb.append(",\"min\":").append(JsonWriter.quoteString(hex(normalize.getMin())));
            sb.append(",\"range\":").append(JsonWriter.quoteString(hex(normalize.getRange())));
            sb.append(",\"noise\":").append(describeNoiseProperty(normalize.getNoiseProperty())).append('}');
        } else if (property instanceof ScaleNoiseProperty scale) {
            sb.append("{\"kind\":\"SCALE\"");
            sb.append(",\"scaleX\":").append(JsonWriter.quoteString(hex(scale.getScaleX())));
            sb.append(",\"scaleY\":").append(JsonWriter.quoteString(hex(scale.getScaleY())));
            sb.append(",\"scaleZ\":").append(JsonWriter.quoteString(hex(scale.getScaleZ())));
            sb.append(",\"noise\":").append(describeNoiseProperty(scale.getNoiseProperty())).append('}');
        } else if (property instanceof OffsetNoiseProperty offset) {
            sb.append("{\"kind\":\"OFFSET\"");
            sb.append(",\"offsetX\":").append(JsonWriter.quoteString(hex(offset.getOffsetX())));
            sb.append(",\"offsetY\":").append(JsonWriter.quoteString(hex(offset.getOffsetY())));
            sb.append(",\"offsetZ\":").append(JsonWriter.quoteString(hex(offset.getOffsetZ())));
            sb.append(",\"noise\":").append(describeNoiseProperty(offset.getNoiseProperty())).append('}');
        } else if (property instanceof DistortedNoiseProperty distorted) {
            sb.append("{\"kind\":\"DISTORTED\"");
            sb.append(",\"noise\":").append(describeNoiseProperty(distorted.getNoiseProperty()));
            sb.append(",\"randomizer\":").append(describeRandomizer(distorted.getRandomizer())).append('}');
        } else if (property instanceof BlendNoiseProperty blend) {
            sb.append("{\"kind\":\"BLEND\"");
            sb.append(",\"alpha\":").append(describeNoiseProperty((NoiseProperty) field(blend, "alpha")));
            final NoiseProperty[] noises = (NoiseProperty[]) field(blend, "noises");
            sb.append(",\"noises\":[");
            for (int i = 0; i < noises.length; i++) {
                if (i > 0) sb.append(',');
                sb.append(describeNoiseProperty(noises[i]));
            }
            sb.append(']');
            final double[] thresholds = (double[]) field(blend, "thresholds");
            sb.append(",\"thresholds\":[");
            for (int i = 0; i < thresholds.length; i++) {
                if (i > 0) sb.append(',');
                sb.append(JsonWriter.quoteString(hex(thresholds[i])));
            }
            sb.append("]}");
        } else if (property instanceof SumNoiseProperty sum) {
            sb.append("{\"kind\":\"SUM\",\"entries\":[");
            final SumNoiseProperty.Entry[] entries = sum.getEntries();
            for (int i = 0; i < entries.length; i++) {
                if (i > 0) sb.append(',');
                sb.append("{\"factor\":").append(JsonWriter.quoteString(hex(entries[i].getFactor())));
                sb.append(",\"noise\":").append(describeNoiseProperty(entries[i].getNoiseProperty())).append('}');
            }
            sb.append("]}");
        } else {
            throw new IllegalStateException("Unsupported noise property: " + property.getClass().getName());
        }
        return sb.toString();
    }

    private static String describeNoiseFunction(NoiseFunction function) {
        final StringBuilder sb = new StringBuilder();
        if (function instanceof SimplexNoise) {
            sb.append("{\"kind\":\"SIMPLEX\"}");
        } else if (function instanceof ConstantNoise constant) {
            sb.append("{\"kind\":\"CONSTANT\",\"value\":").append(JsonWriter.quoteString(hex(constant.getValue()))).append('}');
        } else if (function instanceof PointNoise point) {
            sb.append("{\"kind\":\"POINT\"");
            sb.append(",\"x\":").append(JsonWriter.quoteString(hex((double) field(point, "x"))));
            sb.append(",\"y\":").append(JsonWriter.quoteString(hex((double) field(point, "y"))));
            sb.append(",\"z\":").append(JsonWriter.quoteString(hex((double) field(point, "z"))));
            sb.append(",\"innerRadius2\":").append(JsonWriter.quoteString(hex((double) field(point, "innerRadius2"))));
            sb.append(",\"outerRadius2\":").append(JsonWriter.quoteString(hex((double) field(point, "outerRadius2"))));
            sb.append('}');
        } else if (function instanceof GridNoise grid) {
            sb.append("{\"kind\":\"GRID\"");
            sb.append(",\"thicknessX\":").append(JsonWriter.quoteString(hex((double) field(grid, "thicknessX"))));
            sb.append(",\"thicknessY\":").append(JsonWriter.quoteString(hex((double) field(grid, "thicknessY"))));
            sb.append(",\"thicknessZ\":").append(JsonWriter.quoteString(hex((double) field(grid, "thicknessZ"))));
            sb.append('}');
        } else {
            throw new IllegalStateException("Unsupported noise function: " + function.getClass().getName());
        }
        return sb.toString();
    }

    /** Java {@code FractalNoiseProperty.FractalMode} of a fractal property. */
    private static String fractalMode(FractalNoiseProperty fractal) {
        final Object function = field(fractal, "fractalFunction");
        try {
            // The enum constants use anonymous classes, so the mode is resolved
            // by identity against FractalMode#getFunction().
            final Method getter = FractalNoiseProperty.FractalMode.class.getMethod("getFunction");
            for (FractalNoiseProperty.FractalMode mode : FractalNoiseProperty.FractalMode.values()) {
                if (getter.invoke(mode) == function) {
                    return mode.name();
                }
            }
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException("Cannot resolve the fractal mode", e);
        }
        throw new IllegalStateException("Unknown fractal mode: " + function.getClass().getName());
    }

    /** Reads a (possibly inherited) field through reflection. */
    private static Object field(Object target, String name) {
        Class<?> clazz = target.getClass();
        while (clazz != null) {
            try {
                final Field field = clazz.getDeclaredField(name);
                field.setAccessible(true);
                return field.get(target);
            } catch (NoSuchFieldException ignored) {
                clazz = clazz.getSuperclass();
            } catch (IllegalAccessException e) {
                throw new IllegalStateException("Cannot read field " + name, e);
            }
        }
        throw new IllegalStateException("No field '" + name + "' on " + target.getClass().getName());
    }

    // ------------------------------------------------------------------
    // JSON writer
    // ------------------------------------------------------------------

    /**
     * Minimal JSON writer used by the Phase-2 dumps.
     *
     * <p>Objects are pretty-printed, arrays are written compactly on a single
     * line. Compared to Gson's pretty printer this keeps the bulk arrays (the
     * 512x512 climate table / fade map and the rasterised mask) small enough
     * for the golden files to stay in the low megabytes.
     */
    static final class JsonWriter {
        private final StringBuilder out = new StringBuilder(1 << 20);
        private final Deque<Boolean> hasMembers = new ArrayDeque<>();
        private int depth;
        /** True when the next value closes a name() member position. */
        private boolean expectValue;

        JsonWriter() {
            hasMembers.push(Boolean.FALSE);
        }

        JsonWriter beginObject() {
            openChild();
            out.append('{');
            depth++;
            hasMembers.push(Boolean.FALSE);
            return this;
        }

        JsonWriter beginArray() {
            openChild();
            out.append('[');
            depth++;
            hasMembers.push(Boolean.FALSE);
            return this;
        }

        JsonWriter endArray() {
            depth--;
            hasMembers.pop();
            out.append('\n').append("  ".repeat(depth)).append(']');
            return this;
        }

        JsonWriter endObject() {
            depth--;
            hasMembers.pop();
            out.append('\n').append("  ".repeat(depth)).append('}');
            return this;
        }

        JsonWriter name(String name) {
            final boolean has = hasMembers.pop();
            if (has) {
                out.append(',');
            }
            hasMembers.push(Boolean.TRUE);
            out.append('\n').append("  ".repeat(depth));
            quote(name);
            out.append(": ");
            expectValue = true;
            return this;
        }

        /** Emits the element/member separator for a standalone value. */
        private void openChild() {
            if (expectValue) {
                expectValue = false;
                return;
            }
            final boolean has = hasMembers.pop();
            if (has) {
                out.append(',');
            }
            hasMembers.push(Boolean.TRUE);
        }

        JsonWriter value(String value) {
            expectValue = false;
            if (value == null) {
                out.append("null");
                return this;
            }
            quote(value);
            return this;
        }

        JsonWriter value(int value) {
            expectValue = false;
            out.append(value);
            return this;
        }

        JsonWriter value(long value) {
            expectValue = false;
            out.append(value);
            return this;
        }

        JsonWriter value(boolean value) {
            expectValue = false;
            out.append(value);
            return this;
        }

        /** Floating-point values are written as Java hex strings. */
        JsonWriter value(double value) {
            expectValue = false;
            quote(Double.toHexString(value));
            return this;
        }

        /** Writes a pre-formatted array (compact, single line). */
        JsonWriter rawArray(String content) {
            openChild();
            out.append('[').append(content).append(']');
            return this;
        }

        /** Writes a pre-formatted object (compact, single line). */
        JsonWriter rawObject(String content) {
            openChild();
            out.append(content);
            return this;
        }

        private void quote(String value) {
            out.append('"');
            for (int i = 0; i < value.length(); i++) {
                final char c = value.charAt(i);
                switch (c) {
                    case '"' -> out.append("\\\"");
                    case '\\' -> out.append("\\\\");
                    case '\n' -> out.append("\\n");
                    case '\r' -> out.append("\\r");
                    case '\t' -> out.append("\\t");
                    case '\b' -> out.append("\\b");
                    case '\f' -> out.append("\\f");
                    default -> {
                        if (c < 0x20) {
                            out.append(String.format("\\u%04x", (int) c));
                        } else {
                            out.append(c);
                        }
                    }
                }
            }
            out.append('"');
        }

        /** Quotes a string (used by the compact description helpers). */
        static String quoteString(String value) {
            final StringBuilder sb = new StringBuilder(value.length() + 2);
            sb.append('"');
            for (int i = 0; i < value.length(); i++) {
                final char c = value.charAt(i);
                switch (c) {
                    case '"' -> sb.append("\\\"");
                    case '\\' -> sb.append("\\\\");
                    case '\n' -> sb.append("\\n");
                    case '\r' -> sb.append("\\r");
                    case '\t' -> sb.append("\\t");
                    default -> sb.append(c);
                }
            }
            sb.append('"');
            return sb.toString();
        }

        @Override
        public String toString() {
            return out.toString() + "\n";
        }
    }
}
