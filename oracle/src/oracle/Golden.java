package oracle;

import java.io.BufferedWriter;
import java.io.FileWriter;
import java.io.IOException;
import java.util.Random;

import rwg2.util.CellNoise;
import rwg2.util.PerlinNoise;

/**
 * Emits golden vectors from the *verbatim* RWG2 sources (compiled alongside this file).
 *
 * Text format, one record per line, all floats/doubles as raw IEEE bits in hex so nothing is lost to formatting:
 *   seed <i64>
 *   n1 <xbits> <resultbits>
 *   n2 <xbits> <ybits> <resultbits>
 *   n3 <xbits> <ybits> <zbits> <resultbits>
 *   imp <xbits> <ybits> <zbits> <resultbits:64>
 *   rnd <kind> <value>            (java.util.Random stream checks)
 */
public final class Golden {

    static final long[] SEEDS = { 0L, 1L, -1L, 42L, Long.MIN_VALUE, Long.MAX_VALUE, 0x123456789ABCDEFL,
            -4172144997902289642L, 0xBB67AE8584CAA73BL, 0x510E527FADE682D1L, 0x6A09E667F3BCC909L,
            0xBB67AE8584CAA73BL ^ 42L };

    public static void main(String[] args) throws IOException {
        String out = args.length > 0 ? args[0] : "golden";
        perlin(out + "/perlin.txt");
        random(out + "/java_random.txt");
        cell(out + "/cell.txt");
        poisson(out + "/poisson.txt");
        continental(out + "/continental.txt");
    }

    static void poisson(String path) throws IOException {
        try (BufferedWriter w = new BufferedWriter(new FileWriter(path))) {
            Object[][] cfgs = { { 0x510E527FADE682D1L ^ 42L, 1020D, 4 }, { 42L, 5450D, 8 }, { -1L, 75D, 2 },
                    { Long.MIN_VALUE, 1020D, 4 } };
            double[] o5 = new double[6], o10 = new double[10];
            for (Object[] c : cfgs) {
                long seed = (Long) c[0];
                double md = (Double) c[1];
                int rounds = (Integer) c[2];
                rwg2.util.PoissonPointNoise p = new rwg2.util.PoissonPointNoise(seed, md, rounds);
                Random r = new Random(seed ^ rounds);
                w.write("poisson " + seed + " " + db(md) + " " + rounds + "\n");
                for (int i = 0; i < 1500; i++) {
                    double span = i < 500 ? md * 6 : i < 1000 ? 30000 : 3e7;
                    double x = (r.nextDouble() - .5) * 2 * span, z = (r.nextDouble() - .5) * 2 * span;
                    if (i % 7 == 0) {
                        x = Math.floor(x);
                        z = Math.floor(z);
                    }
                    p.sample(x, z, o5);
                    w.write("s " + db(x) + " " + db(z) + " " + db(o5[0]) + " " + (int) o5[1] + " " + (int) o5[2] + " "
                            + db(o5[3]) + " " + db(o5[4]) + " " + (int) o5[5] + "\n");
                    if (i % 3 == 0) {
                        p.sampleTwo(x, z, o10);
                        StringBuilder sb = new StringBuilder("t " + db(x) + " " + db(z));
                        for (double v : o10) sb.append(' ').append(db(v));
                        w.write(sb.append('\n').toString());
                    }
                    if (i % 5 == 0) {
                        int cx = r.nextInt(2001) - 1000, cz = r.nextInt(2001) - 1000, rd = r.nextInt(rounds);
                        p.getCandidate(cx, cz, rd, o5);
                        w.write("a " + cx + " " + cz + " " + rd + " " + p.isAccepted(cx, cz, rd) + " " + db(o5[0]) + " "
                                + db(o5[1]) + "\n");
                    }
                }
            }
        }
    }

    static String contLine(rwg2.util.ContinentalNoise c, int x, int z) {
        return "c " + x + " " + z + " " + fb(c.getValue(x, z)) + " " + c.isIsland(x, z) + " "
                + c.getIslandSizeTier(x, z) + " " + c.getIslandSeedCoordinates(x, z) + " "
                + c.getIslandCenterCoordinates(x, z) + " " + c.getVolcanoCoordinates(x, z) + " "
                + c.getVolcanoVicinityCoordinates(x, z) + " " + c.getVolcanoSeedKey(x, z) + " "
                + c.isIslandVolcano(x, z) + " " + c.getVolcanoCenterCoordinates(x, z) + " "
                + c.getLavaCaveCoordinates(x, z) + " " + c.getLavaCaveSeedKey(x, z) + " "
                + c.getLavaCaveCenterCoordinates(x, z) + "\n";
    }

    static void continental(String path) throws IOException {
        long[] seeds = { 0L, 42L, -6999513761975938210L, 123456789L, Long.MIN_VALUE, -4172144997902289642L };
        try (BufferedWriter w = new BufferedWriter(new FileWriter(path))) {
            for (long world : seeds) {
                long seed = world ^ 0x6A09E667F3BCC909L;
                rwg2.util.ContinentalNoise c = new rwg2.util.ContinentalNoise(seed);
                w.write("cont " + seed + "\n");
                Random r = new Random(world);
                for (int i = 0; i < 2500; i++) {
                    int span = i < 1500 ? 20000 : 3000000;
                    int x = r.nextInt(2 * span + 1) - span, z = r.nextInt(2 * span + 1) - span;
                    w.write(contLine(c, x, z));
                }
                // Coarse scan for features, then dense neighbourhoods around the first hits of each kind.
                int islands = 0, volcanoes = 0, caves = 0, coasts = 0;
                for (int gz = -24000; gz <= 24000; gz += 160) {
                    for (int gx = -24000; gx <= 24000; gx += 160) {
                        boolean isl = c.isIsland(gx, gz);
                        boolean vol = c.getVolcanoSeedKey(gx, gz) != Long.MIN_VALUE;
                        boolean cav = c.getLavaCaveSeedKey(gx, gz) != Long.MIN_VALUE;
                        float v = c.getValue(gx, gz);
                        boolean coast = v > -40f && v < 40f;
                        boolean take = (isl && islands++ < 6) || (vol && volcanoes++ < 6) || (cav && caves++ < 6)
                                || (coast && coasts++ < 4);
                        if (!take) continue;
                        for (int dz = -96; dz <= 96; dz += 12)
                            for (int dx = -96; dx <= 96; dx += 12) w.write(contLine(c, gx + dx, gz + dz));
                    }
                }
                w.write("found " + islands + " " + volcanoes + " " + caves + " " + coasts + "\n");
            }
        }
    }

    /** Doubles shaped like CellNoise call sites (coord / 1400, / 500, / 1250, raw), plus edges. */
    static double[] cellCoords(Random r, int n) {
        double[] scales = { 1D, 500D, 1250D, 1400D, 1600D, 300D };
        double[] edges = { 0D, -0D, 1D, -1D, .5D, -.5D, 1e-300, -1e-300, 2147483647D, -2147483648D, 3e9, -3e9,
                1e15, -1e15, 30000000D / 1250D, -30000000D / 1400D, 12345.999999999, -0.9999999999 };
        double[] v = new double[n];
        int i = 0;
        for (double e : edges) if (i < n) v[i++] = e;
        while (i < n) {
            int kind = r.nextInt(4);
            if (kind == 0) v[i++] = (r.nextInt(60000001) - 30000000 + r.nextGaussian() * 300) / scales[r.nextInt(scales.length)];
            else if (kind == 1) v[i++] = (r.nextInt(40001) - 20000) / scales[r.nextInt(scales.length)];
            else if (kind == 2) v[i++] = (r.nextDouble() - .5) * 10;
            else v[i++] = r.nextGaussian() * 50;
        }
        return v;
    }

    static void cell(String path) throws IOException {
        try (BufferedWriter w = new BufferedWriter(new FileWriter(path))) {
            Random vr = new Random(77);
            for (int i = 0; i < 4000; i++) {
                int x = i < 8 ? new int[] { 0, -1, 1, Integer.MAX_VALUE, Integer.MIN_VALUE, 1326, -1326, 46341 }[i]
                        : vr.nextInt();
                int y = vr.nextInt(), z = i % 3 == 0 ? vr.nextInt(2000) - 1000 : vr.nextInt();
                long s = i % 5 == 0 ? SEEDS[i % SEEDS.length] : vr.nextLong();
                w.write("v2 " + x + " " + z + " " + s + " " + db(CellNoise.valueNoise2D(x, z, s)) + "\n");
                w.write("v3 " + x + " " + y + " " + z + " " + s + " " + db(CellNoise.valueNoise3D(x, y, z, s)) + "\n");
            }
            for (long seed : SEEDS) {
                for (short method = 0; method <= 1; method++) {
                    CellNoise c = new CellNoise(seed, method);
                    Random r = new Random(seed ^ method ^ 0x5851F42D4C957F2DL);
                    double[] a = cellCoords(r, 600), b = cellCoords(r, 600);
                    double[] freqs = { 1D, .5D, 2D, 1D / 1400D };
                    w.write("cell " + seed + " " + method + "\n");
                    for (int u = 0; u <= 1; u++) {
                        c.setUseDistance(u == 1);
                        w.write("usedist " + u + "\n");
                        for (int i = 0; i < a.length; i++) {
                            double f = freqs[i % freqs.length];
                            w.write("noise " + db(a[i]) + " " + db(b[i]) + " " + db(f) + " "
                                    + fb(c.noise(a[i], b[i], f)) + "\n");
                        }
                        for (int i = 0; i < 60; i++) {
                            double f = freqs[i % freqs.length];
                            w.write("noise3 " + db(a[i]) + " " + db(b[i]) + " " + db(a[i + 60]) + " " + db(f) + " "
                                    + db(c.noise(a[i], b[i], a[i + 60], f)) + "\n");
                        }
                    }
                    double[] out = new double[4];
                    for (int i = 0; i < a.length; i++) {
                        c.sampleTwo2D(a[i], b[i], 1D, out);
                        w.write("two " + db(a[i]) + " " + db(b[i]) + " " + db(1D) + " " + db(out[0]) + " "
                                + db(out[1]) + " " + db(out[2]) + " " + db(out[3]) + "\n");
                        double width = new double[] { 50D / 300D, 50D / 1300D, 9D / 1250D, 60D / 1250D, 2D }[i % 5];
                        float depth = i % 2 == 0 ? 1f : 2.5f;
                        w.write("border " + db(a[i]) + " " + db(b[i]) + " " + db(width) + " " + fb(depth) + " "
                                + fb(c.border(a[i], b[i], width, depth)) + "\n");
                        w.write("border2 " + db(a[i]) + " " + db(b[i]) + " " + db(width) + " " + fb(depth) + " "
                                + fb(c.border2(a[i], b[i], width, depth)) + "\n");
                        w.write("junction " + db(a[i]) + " " + db(b[i]) + " " + db(width) + " " + fb(depth) + " "
                                + fb(c.junction(a[i], b[i], width, depth)) + "\n");
                    }
                }
            }
        }
    }

    /** Coordinates shaped like the real call sites: int coords divided by float scales, plus edge cases. */
    static float[] sampleCoords(Random r, int n) {
        float[] scales = { 1f, 8f, 12f, 55f, 180f, 240f, 560f, 1200f };
        float[] v = new float[n];
        int i = 0;
        float[] edges = { 0f, -0f, 1f, -1f, .5f, -.5f, 255f, 256f, 4095.999f, -4096f, -4096.5f, 1e6f, -1e6f,
                3e9f, -3e9f, Float.MIN_VALUE, 1e-30f, 30000000f / 240f, -30000000f / 1200f };
        for (float e : edges) if (i < n) v[i++] = e;
        while (i < n) {
            int kind = r.nextInt(4);
            if (kind == 0) v[i++] = (r.nextInt(60000001) - 30000000) / scales[r.nextInt(scales.length)];
            else if (kind == 1) v[i++] = (r.nextInt(20001) - 10000) / scales[r.nextInt(scales.length)];
            else if (kind == 2) v[i++] = (r.nextFloat() - .5f) * 8f;
            else v[i++] = (float) (r.nextGaussian() * 1000.0);
        }
        return v;
    }

    static String fb(float f) {
        return Integer.toHexString(Float.floatToRawIntBits(f));
    }

    static String db(double d) {
        return Long.toHexString(Double.doubleToRawLongBits(d));
    }

    static void perlin(String path) throws IOException {
        try (BufferedWriter w = new BufferedWriter(new FileWriter(path))) {
            for (long seed : SEEDS) {
                PerlinNoise p = new PerlinNoise(seed);
                Random r = new Random(seed ^ 0x9E3779B97F4A7C15L);
                w.write("seed " + seed + "\n");
                float[] a = sampleCoords(r, 2000), b = sampleCoords(r, 2000), c = sampleCoords(r, 400);
                for (int i = 0; i < a.length; i++) w.write("n1 " + fb(a[i]) + " " + fb(p.noise1(a[i])) + "\n");
                for (int i = 0; i < a.length; i++)
                    w.write("n2 " + fb(a[i]) + " " + fb(b[i]) + " " + fb(p.noise2(a[i], b[i])) + "\n");
                for (int i = 0; i < c.length; i++) w.write(
                        "n3 " + fb(a[i]) + " " + fb(b[i]) + " " + fb(c[i]) + " " + fb(p.noise3(a[i], b[i], c[i])) + "\n");
                for (int i = 0; i < c.length; i++) {
                    double x = a[i] * 1.0001, y = b[i] * .9997, z = c[i];
                    w.write("imp " + db(x) + " " + db(y) + " " + db(z) + " " + db(p.improvedNoise(x, y, z)) + "\n");
                }
            }
        }
    }

    static void random(String path) throws IOException {
        try (BufferedWriter w = new BufferedWriter(new FileWriter(path))) {
            for (long seed : SEEDS) {
                Random r = new Random(seed);
                w.write("seed " + seed + "\n");
                for (int i = 0; i < 64; i++) {
                    w.write("int " + r.nextInt() + "\n");
                    w.write("long " + r.nextLong() + "\n");
                    w.write("double " + db(r.nextDouble()) + "\n");
                    w.write("float " + fb(r.nextFloat()) + "\n");
                    w.write("bool " + r.nextBoolean() + "\n");
                    int bound = 1 + (i * 7919) % 1000;
                    w.write("intb " + bound + " " + r.nextInt(bound) + "\n");
                    w.write("intb 1024 " + r.nextInt(1024) + "\n");
                    w.write("gauss " + db(r.nextGaussian()) + "\n");
                }
            }
        }
    }
}
