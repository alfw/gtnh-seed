package rwg2dump;

import java.io.File;
import java.io.FileOutputStream;
import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.Map;
import java.util.Random;
import java.util.zip.GZIPOutputStream;

/**
 * rwg2dump 2.16 one-run collection (see {@link #SECTIONS}). Server thread. Runs FIRST after structures so its
 * areas are generated in a fresh world (all areas ≥ {@link #MIN_R} blocks from the origin, beyond everything the
 * older sections force-load). Every section is fail-safe (error string instead of data) and time-boxed.
 */
final class Pass216 {

    static final String[][] SECTIONS = {
            { "deco216.json + deco216_<area>.bin.gz", "decoration validation: per area (7x7 chunks) post-population blocks y50..200, heightValue, GT test column, tile entities, node columns, the populate/decorate event log (order, depth, Random states)", "~13 areas x 81 chunk loads: 2-6 min; ~5 MB gz" },
            { "postpop216.json", "A1 Roguelike attempts after population (verdict, ground, 13x13 rows at ground+4 / ground-3, heightValue), A4 LootGames setSurfaceLevel + 21x21 heightValue / top block, A5 spawn walk post-population (heightValue, top, canSpawn)", "1-4 min; < 3 MB" },
            { "nether216.json", "B9 Nether node tiles in [-12,12]^2 + block columns y-6..y+6 (written after the dims section)", "seconds" },
            { "gregtech216.json", "B6 GT validOreveins cache after the deco areas (same reader as gregtech.json)", "seconds" }, };

    static final int MIN_R = 2560; // blocks; older sections load up to ~1700 blocks from the origin
    static final int MAX_R = 24000;
    static final int AREA_R = 3; // 7x7 chunks
    static final int Y0 = 50, Y1 = 200, NY = Y1 - Y0 + 1;
    static final long DECO_BUDGET_MS = 25 * 60_000L, POST_BUDGET_MS = 12 * 60_000L;

    /** Area targets: name, kind (n = getBiomeDataAt RealisticBiome class simple name, g = getBiomeGenAt biome name), matches. */
    static final String[][] TARGETS = {
            { "nativeTemperateForest", "n", "RealisticBiomeWoodHills", "RealisticBiomeWoodMountains" },
            { "nativeTaiga", "n", "RealisticBiomeTaigaHills", "RealisticBiomeTaigaPlains" },
            { "nativeJungle", "n", "RealisticBiomeJungleHills", "RealisticBiomeRedwoodJungle" },
            { "bopGarden", "g", "Garden" },
            { "bopEucalyptus", "g", "Eucalyptus Forest" },
            { "bopChaparral", "g", "Chaparral" },
            { "bopFungiForest", "g", "Fungi Forest" },
            { "bopFrostForest", "g", "Frost Forest" },
            { "bopPrairie", "g", "Prairie" },
            { "bopTundra", "g", "Tundra" },
            { "oceanKelpForest", "g", "Kelp Forest" },
            { "oceanCoralReef", "g", "Coral Reef" },
            { "vanillaIcePlainsSpikes", "g", "Ice Plains Spikes" },
            { "vanillaSunflowerPlains", "g", "Sunflower Plains" }, };

    final Extras x;
    final Rwg2Dump.Dumper d;
    final ClassLoader cl;
    List<int[]> areaCentres = new ArrayList<int[]>(); // chunk centres of the deco areas, for the post-pop sections

    Pass216(Extras x, Rwg2Dump.Dumper d, ClassLoader cl) {
        this.x = x;
        this.d = d;
        this.cl = cl;
    }

    // ============================================================================================ deco

    String deco() {
        Rwg2Dump.Json j = new Rwg2Dump.Json();
        j.open('{');
        long t0 = System.currentTimeMillis();
        try {
            j.key("sections").open('[');
            for (String[] s : SECTIONS) j.raw("[\"" + s[0] + "\",\"" + s[1].replace("\"", "'") + "\",\"" + s[2] + "\"]");
            j.close(']');
            j.key("layout").str("per area gzip: [u16 block index (49 chunks: cz rows, cx inner; 256 columns lz*16+lx; y "
                    + Y0 + ".." + Y1 + ") ][u8 meta same order][u8 heightValue (World.getHeightValue) x 49*256]"
                    + "[u16 GT test column (chunk x+7, z+9) y0..127 x 49][u8 meta same]; block names in 'blocks'");
            j.key("eventFormat").str("[seq, kind, cx, cz, depth, type, rand]; kinds popPre(HIGHEST) popPreLate(LOWEST) populate "
                    + "decPre decorate decPost popPost; rand 's:<hex>' = Random.seed (scrambled state) or 'o:a,b' = first two "
                    + "nextInt() of a copy; '|c:<class>' subclass, '|f:<field>=<state>' Random-typed fields (shared Random of BOP's "
                    + "RandomForcedPositiveOwned)");
            Object w = x.world();
            j.key("worldSeed").str(Long.toString(x.worldSeedOf(w)));
            String reg = Hooks216.register(cl);
            j.key("hooks").str(reg == null ? "registered" : reg);
            Extras.Blocks b = x.new Blocks();
            Extras.Loader ld = x.new Loader(w);
            j.key("loadChunkOnProvideRequest").str(ld.onRequest());
            Object chunkLoader = null;
            Method onDisk = null;
            try {
                chunkLoader = Extras.fieldAny(Structures.fieldByTypeName(w, "ChunkProviderServer"), "currentChunkLoader", "field_73247_e");
                if (chunkLoader != null)
                    onDisk = chunkLoader.getClass().getMethod("chunkExists", x.cls("net.minecraft.world.World"), int.class, int.class);
            } catch (Throwable t) {
                j.key("onDiskError").str(Extras.err(t));
            }
            List<Object[]> picks = pick(j, w);
            j.key("areas").open('[');
            for (Object[] p : picks) {
                if (System.currentTimeMillis() - t0 > DECO_BUDGET_MS) {
                    j.open('{').key("name").str((String) p[0]).key("skipped").str("time budget").close('}');
                    continue;
                }
                area(j, w, b, ld, chunkLoader, onDisk, (String) p[0], (Integer) p[1], (Integer) p[2]);
            }
            j.close(']');
            j.key("randomStateMethod").str(Hooks216.stateMethod);
            j.key("blocks").open('[');
            for (String n : b.names) j.str(n);
            j.close(']');
        } catch (Throwable t) {
            j.key("error").str(Extras.err(t));
        } finally {
            Hooks216.unregister(cl);
        }
        j.key("millis").val(System.currentTimeMillis() - t0);
        j.close('}');
        return j.toString();
    }

    /** Nearest dominated area per target (≥ 65 of 81 samples at 16-block spacing), rings of 64 blocks from MIN_R. */
    List<Object[]> pick(Rwg2Dump.Json j, Object w) throws Exception {
        Method wcm = Extras.named(x.cls("net.minecraft.world.World"), new Class<?>[0], "getWorldChunkManager", "func_72959_q");
        Object cm = wcm.invoke(w);
        Method gen = null;
        for (Class<?> k = cm.getClass(); k != null && gen == null; k = k.getSuperclass())
            for (Method m : k.getDeclaredMethods()) {
                Class<?>[] p = m.getParameterTypes();
                if (p.length == 2 && p[0] == int.class && p[1] == int.class && d.bgbClass.isAssignableFrom(m.getReturnType())) {
                    m.setAccessible(true);
                    gen = m;
                    break;
                }
            }
        boolean cmr = d.cmrClass.isInstance(cm);
        int[][] found = new int[TARGETS.length][];
        int left = TARGETS.length;
        long t0 = System.currentTimeMillis();
        for (int ring = MIN_R / 64; ring <= MAX_R / 64 && left > 0 && System.currentTimeMillis() - t0 < 180_000; ring++)
            for (int a = -ring; a < ring && left > 0; a++)
                for (int k = 0; k < 4 && left > 0; k++) {
                    int gx = k == 0 ? a : k == 1 ? ring : k == 2 ? -a : -ring, gz = k == 0 ? -ring : k == 1 ? a : k == 2 ? ring : -a;
                    int cx = gx * 64, cz = gz * 64;
                    String cn = cmr ? d.getBiomeDataAt.invoke(cm, cx, cz).getClass().getSimpleName() : "";
                    String gn = String.valueOf(d.bgbName.get(gen.invoke(cm, cx, cz)));
                    for (int t = 0; t < TARGETS.length; t++) {
                        if (found[t] != null || !match(TARGETS[t], cn, gn)) continue;
                        int hit = 0;
                        for (int i = -4; i <= 4; i++)
                            for (int q = -4; q <= 4; q++) {
                                int sx = cx + i * 16, sz = cz + q * 16;
                                String c2 = TARGETS[t][1].equals("n") && cmr ? d.getBiomeDataAt.invoke(cm, sx, sz).getClass().getSimpleName() : "";
                                String g2 = TARGETS[t][1].equals("g") ? String.valueOf(d.bgbName.get(gen.invoke(cm, sx, sz))) : "";
                                if (match(TARGETS[t], c2, g2)) hit++;
                            }
                        if (hit < 65) continue;
                        boolean far = true; // areas (+1 ring) must not overlap
                        for (int[] f : found)
                            if (f != null && Math.abs(f[0] - cx) < 256 && Math.abs(f[1] - cz) < 256) far = false;
                        if (!far) continue;
                        found[t] = new int[] { cx, cz, hit };
                        left--;
                    }
                }
        List<Object[]> out = new ArrayList<Object[]>();
        j.key("picked").open('[');
        for (int t = 0; t < TARGETS.length; t++) {
            j.open('{').key("name").str(TARGETS[t][0]);
            if (found[t] != null) {
                j.key("x").val(found[t][0]).key("z").val(found[t][1]).key("samples").val(found[t][2]);
                out.add(new Object[] { TARGETS[t][0], found[t][0] >> 4, found[t][1] >> 4 });
                areaCentres.add(new int[] { found[t][0] >> 4, found[t][1] >> 4 });
            } else j.key("x").raw("null");
            j.close('}');
        }
        j.close(']');
        j.key("pickMillis").val(System.currentTimeMillis() - t0);
        return out;
    }

    static boolean match(String[] t, String cls, String gen) {
        for (int i = 2; i < t.length; i++)
            if (t[1].equals("n") ? t[i].equals(cls) : t[i].equals(gen)) return true;
        return false;
    }

    void area(Rwg2Dump.Json j, Object w, Extras.Blocks b, Extras.Loader ld, Object chunkLoader, Method onDisk, String name, int ccx, int ccz) {
        j.open('{');
        j.key("name").str(name);
        int r = AREA_R, wdt = 2 * r + 1;
        j.key("chunkX0").val(ccx - r).key("chunkZ0").val(ccz - r).key("chunks").val(wdt);
        long t0 = System.currentTimeMillis();
        try {
            Class<?> worldC = x.cls("net.minecraft.world.World");
            // which chunks of the loaded square (area + 1 ring) existed before (on disk / loaded)
            int onDiskN = 0, loadedN = 0;
            for (int cz = ccz - r - 1; cz <= ccz + r + 1; cz++)
                for (int cx = ccx - r - 1; cx <= ccx + r + 1; cx++) {
                    if (ld.exists(cx, cz)) loadedN++;
                    try {
                        if (onDisk != null && (Boolean) onDisk.invoke(chunkLoader, w, cx, cz)) onDiskN++;
                    } catch (Throwable ignored) {}
                }
            j.key("loadedBefore").val(loadedN).key("onDiskBefore").val(onDisk == null ? -1 : onDiskN);
            Hooks216.reset(w);
            Hooks216.active = true;
            // load order: row-major over the area + 1 ring (cz outer, cx inner), recorded as it happens
            int fresh = ld.area((ccx - r - 1) * 16, (ccz - r - 1) * 16, (ccx + r + 1) * 16 + 15, (ccz + r + 1) * 16 + 15);
            Hooks216.active = false;
            j.key("loadOrder").str("row-major cz outer, cx inner over [" + (ccx - r - 1) + "," + (ccx + r + 1) + "]x[" + (ccz - r - 1) + "," + (ccz + r + 1) + "]");
            j.key("freshChunks").val(fresh);
            List<String> ev = Hooks216.drain();
            j.key("events").open('[');
            for (String e : ev) j.raw(e);
            j.close(']');
            // blocks
            Method getChunk = Extras.named(worldC, new Class<?>[] { int.class, int.class }, "getChunkFromChunkCoords", "func_72964_e");
            Method meta = Extras.named(b.chunkC, new Class<?>[] { int.class, int.class, int.class }, "getBlockMetadata", "func_76628_c");
            Method hv = Extras.named(worldC, new Class<?>[] { int.class, int.class }, "getHeightValue", "func_72976_f");
            int nc = wdt * wdt;
            byte[] out = new byte[nc * 256 * NY * 3 + nc * 256 + nc * 128 * 3];
            int ido = 0, mo = nc * 256 * NY * 2, ho = nc * 256 * NY * 3, go = ho + nc * 256, gmo = go + nc * 128 * 2;
            j.key("tiles").open('[');
            List<int[]> nodes = new ArrayList<int[]>();
            for (int cz = ccz - r; cz <= ccz + r; cz++)
                for (int cx = ccx - r; cx <= ccx + r; cx++) {
                    Object chunk = getChunk.invoke(w, cx, cz);
                    for (int lz = 0; lz < 16; lz++)
                        for (int lx = 0; lx < 16; lx++) {
                            for (int y = Y0; y <= Y1; y++) {
                                int id = b.id(b.chunkGetBlock.invoke(chunk, lx, y, lz));
                                out[ido++] = (byte) id;
                                out[ido++] = (byte) (id >> 8);
                                out[mo++] = (byte) (int) (Integer) meta.invoke(chunk, lx, y, lz);
                            }
                            out[ho++] = (byte) (int) (Integer) hv.invoke(w, cx * 16 + lx, cz * 16 + lz);
                        }
                    for (int y = 0; y < 128; y++) {
                        int id = b.id(b.chunkGetBlock.invoke(chunk, 7, y, 9));
                        out[go++] = (byte) id;
                        out[go++] = (byte) (id >> 8);
                        out[gmo++] = (byte) (int) (Integer) meta.invoke(chunk, 7, y, 9);
                    }
                    tiles(j, chunk, nodes);
                }
            j.close(']');
            j.key("nodeColumns").open('[');
            for (int[] n : nodes) nodeColumn(j, w, b, n);
            j.close(']');
            String file = "deco216_" + name + ".bin.gz";
            GZIPOutputStream gz = new GZIPOutputStream(new FileOutputStream(new File(d.out, file)), 1 << 16);
            try {
                gz.write(out);
            } finally {
                gz.close();
            }
            j.key("file").str(file);
        } catch (Throwable t) {
            Hooks216.active = false;
            j.key("error").str(Extras.err(t));
        }
        j.key("millis").val(System.currentTimeMillis() - t0);
        j.close('}');
    }

    /** Tile entities of a chunk: [class, x, y, z]; node-like tiles are queued for a column dump. */
    void tiles(Rwg2Dump.Json j, Object chunk, List<int[]> nodes) {
        try {
            Object map = Extras.fieldAny(chunk, "chunkTileEntityMap", "field_150816_i");
            if (!(map instanceof Map)) return;
            for (Object te : ((Map<?, ?>) map).values()) {
                Object tx = Extras.fieldAny(te, "xCoord", "field_145851_c"), ty = Extras.fieldAny(te, "yCoord", "field_145848_d"),
                        tz = Extras.fieldAny(te, "zCoord", "field_145849_e");
                String cn = te.getClass().getName();
                j.raw("[\"" + cn + "\"," + tx + "," + ty + "," + tz + "]");
                if (cn.contains("Node") && tx instanceof Integer)
                    nodes.add(new int[] { (Integer) tx, (Integer) ty, (Integer) tz });
            }
        } catch (Throwable ignored) {}
    }

    /** [x, y, z, [names y-6 .. y+6]] */
    void nodeColumn(Rwg2Dump.Json j, Object w, Extras.Blocks b, int[] n) {
        StringBuilder s = new StringBuilder("[" + n[0] + "," + n[1] + "," + n[2] + ",[");
        try {
            for (int y = n[1] - 6; y <= n[1] + 6; y++) {
                if (y > n[1] - 6) s.append(',');
                s.append('"').append(y < 0 || y > 255 ? "" : String.valueOf(b.uid.invoke(null, b.worldGetBlock.invoke(w, n[0], y, n[2])))).append('"');
            }
        } catch (Throwable t) {
            s.append("\"error\"");
        }
        j.raw(s.append("]]").toString());
    }

    // ============================================================================================ post-pop

    String postpop() {
        Rwg2Dump.Json j = new Rwg2Dump.Json();
        j.open('{');
        long t0 = System.currentTimeMillis();
        Object w = null;
        Extras.Blocks b = null;
        try {
            w = x.world();
            b = x.new Blocks();
        } catch (Throwable t) {
            j.key("error").str(Extras.err(t));
        }
        if (w != null && b != null) {
            j.key("roguelike").open('{');
            try {
                roguelike(j, w, b, t0);
            } catch (Throwable t) {
                j.key("error").str(Extras.err(t));
            }
            j.close('}');
            j.key("lootgames").open('{');
            try {
                lootgames(j, w, b, t0);
            } catch (Throwable t) {
                j.key("error").str(Extras.err(t));
            }
            j.close('}');
            j.key("spawnWalk").open('{');
            try {
                spawnWalk(j, w, b);
            } catch (Throwable t) {
                j.key("error").str(Extras.err(t));
            }
            j.close('}');
            j.key("blocks").open('[');
            for (String n : b.names) j.str(n);
            j.close(']');
        }
        j.key("millis").val(System.currentTimeMillis() - t0);
        j.close('}');
        return j.toString();
    }

    int[] heightRow(Object w, Method hv, int x0, int z0, int n) throws Exception {
        int[] v = new int[n * n];
        for (int q = 0; q < n; q++)
            for (int i = 0; i < n; i++) v[q * n + i] = (Integer) hv.invoke(w, x0 + i, z0 + q);
        return v;
    }

    static String ints(int[] v) {
        StringBuilder s = new StringBuilder("[");
        for (int i = 0; i < v.length; i++) s.append(i == 0 ? "" : ",").append(v[i]);
        return s.append(']').toString();
    }

    /**
     * A1: anchors (Roguelike `canSpawnInChunk`) within ±25 chunks of each deco area centre (≤ 3 per area, ≤ 24),
     * attempts replayed like the game (Forge per-chunk Random, `getNearbyCoord`, `validLocation` once per attempt
     * after force-loading ±21 blocks). Per attempt: x, z, verdict, ground y (first rogue ground from y 100 down),
     * 13x13 block indices at ground+4 and ground-3, 13x13 heightValue, centred on (x, z).
     */
    void roguelike(Rwg2Dump.Json j, Object w, Extras.Blocks b, long t0) throws Exception {
        Class<?> edC = x.cls("greymerk.roguelike.worldgen.WorldEditor");
        Class<?> iedC = x.cls("greymerk.roguelike.worldgen.IWorldEditor");
        Object editor = null;
        for (Constructor<?> c : edC.getConstructors())
            if (c.getParameterTypes().length == 1) editor = c.newInstance(w);
        Class<?> dc = x.cls("greymerk.roguelike.dungeon.Dungeon");
        Object dungeon = dc.getConstructor(iedC).newInstance(editor);
        Method canSpawn = dc.getMethod("canSpawnInChunk", int.class, int.class, iedC);
        Method nearby = dc.getMethod("getNearbyCoord", Random.class, int.class, int.class, int.class, int.class);
        Method valid = dc.getMethod("validLocation", Random.class, int.class, int.class);
        Class<?> coordC = x.cls("greymerk.roguelike.worldgen.Coord");
        Method gx = coordC.getMethod("getX"), gz = coordC.getMethod("getZ");
        Method hv = Extras.named(x.cls("net.minecraft.world.World"), new Class<?>[] { int.class, int.class }, "getHeightValue", "func_72976_f");
        Extras.Loader ld = x.new Loader(w);
        long seed = x.worldSeedOf(w);
        Random fml = new Random(seed);
        long xSeed = fml.nextLong() >> 2 + 1L, zSeed = fml.nextLong() >> 2 + 1L;
        j.key("rowsFormat").str("attempt: [x, z, valid, groundY, freshChunks, [13x13 ids at ground+4], [13x13 ids at ground-3], [13x13 heightValue]] row-major z then x from (x-6, z-6)");
        j.key("anchors").open('[');
        int total = 0;
        for (int[] c : areaCentres) {
            int here = 0;
            for (int cz = c[1] - 25; cz <= c[1] + 25 && here < 3 && total < 24; cz++)
                for (int cx = c[0] - 25; cx <= c[0] + 25 && here < 3 && total < 24; cx++) {
                    if (!(Boolean) canSpawn.invoke(null, cx, cz, editor)) continue;
                    if (System.currentTimeMillis() - t0 > POST_BUDGET_MS / 2) break;
                    here++;
                    total++;
                    j.open('{').key("chunkX").val(cx).key("chunkZ").val(cz);
                    Random rand = new Random((xSeed * cx + zSeed * cz) ^ seed);
                    j.key("attempts").open('[');
                    for (int i = 0; i < 50; i++) {
                        Object co = nearby.invoke(null, rand, cx * 16 + 4, cz * 16 + 4, 40, 100);
                        int ax = (Integer) gx.invoke(co), az = (Integer) gz.invoke(co);
                        int fresh = ld.area(ax - 21, az - 21, ax + 21, az + 21);
                        boolean ok = (Boolean) valid.invoke(dungeon, rand, ax, az);
                        int gy = -1;
                        for (int y = 100; y >= 0; y--)
                            if (b.rogueGround(b.worldGetBlock.invoke(w, ax, y, az))) {
                                gy = y;
                                break;
                            }
                        StringBuilder s = new StringBuilder("[" + ax + "," + az + "," + ok + "," + gy + "," + fresh);
                        for (int dy : new int[] { 4, -3 }) {
                            int[] ids = new int[169];
                            for (int q = 0; q < 13; q++)
                                for (int p = 0; p < 13; p++)
                                    ids[q * 13 + p] = gy < 0 ? -1 : b.id(b.worldGetBlock.invoke(w, ax - 6 + p, gy + dy, az - 6 + q));
                            s.append(',').append(ints(ids));
                        }
                        s.append(',').append(ints(heightRow(w, hv, ax - 6, az - 6, 13))).append(']');
                        j.raw(s.toString());
                        if (ok) break;
                    }
                    j.close(']');
                    j.close('}');
                }
        }
        j.close(']');
    }

    /**
     * A4: LootGames dungeons (`canSpawnInChunk_v3`) within ±40 chunks of each deco area centre (≤ 2 per area): force-load
     * ±26 blocks, real `setSurfaceLevel` y, then 21x21 heightValue and the block index under it, centred on (x, z).
     */
    void lootgames(Rwg2Dump.Json j, Object w, Extras.Blocks b, long t0) throws Exception {
        Class<?> wgc = x.cls("eu.usrv.legacylootgames.worldgen.LootGamesWorldGen");
        Object gen = wgc.getConstructor().newInstance();
        Method can = null;
        for (Method m : wgc.getDeclaredMethods())
            if (m.getName().equals("canSpawnInChunk_v3")) {
                m.setAccessible(true);
                can = m;
            }
        Class<?> sgc = x.cls("eu.usrv.legacylootgames.StructureGenerator");
        Method setSurface = sgc.getDeclaredMethod("setSurfaceLevel", int.class, int.class);
        setSurface.setAccessible(true);
        Field fWorld = sgc.getDeclaredField("_mWorldObj"), fSurface = sgc.getDeclaredField("_mSurface");
        fWorld.setAccessible(true);
        fSurface.setAccessible(true);
        Method hv = Extras.named(x.cls("net.minecraft.world.World"), new Class<?>[] { int.class, int.class }, "getHeightValue", "func_72976_f");
        Extras.Loader ld = x.new Loader(w);
        j.key("format").str("[cx, cz, x, z, surfaceFound, surfaceY, freshChunks, [21x21 heightValue], [21x21 block index at heightValue-1]] row-major from (x-10, z-10)");
        j.key("dungeons").open('[');
        for (int[] c : areaCentres) {
            int here = 0;
            for (int cz = c[1] - 40; cz <= c[1] + 40 && here < 2; cz++)
                for (int cx = c[0] - 40; cx <= c[0] + 40 && here < 2; cx++) {
                    if (!(Boolean) can.invoke(gen, cx, cz, w)) continue;
                    if (System.currentTimeMillis() - t0 > POST_BUDGET_MS * 3 / 4) break;
                    here++;
                    int px = (cx << 4) + 8, pz = (cz << 4) + 8;
                    int fresh = ld.area(px - 26, pz - 26, px + 26, pz + 26);
                    Object sg = sgc.getConstructor().newInstance();
                    fWorld.set(sg, w);
                    boolean ok = (Boolean) setSurface.invoke(sg, px, pz);
                    int[] h = heightRow(w, hv, px - 10, pz - 10, 21);
                    int[] top = new int[441];
                    for (int q = 0; q < 21; q++)
                        for (int p = 0; p < 21; p++)
                            top[q * 21 + p] = b.id(b.worldGetBlock.invoke(w, px - 10 + p, Math.max(0, h[q * 21 + p] - 1), pz - 10 + q));
                    j.raw("[" + cx + "," + cz + "," + px + "," + pz + "," + ok + "," + fSurface.getInt(sg) + "," + fresh + "," + ints(h) + "," + ints(top) + "]");
                }
        }
        j.close(']');
    }

    /** A5: the vanilla spawn walk (64 steps) on the populated world: heightValue, getTopBlock, canCoordinateBeSpawn. */
    void spawnWalk(Rwg2Dump.Json j, Object w, Extras.Blocks b) throws Exception {
        Class<?> worldC = x.cls("net.minecraft.world.World");
        Method hv = Extras.named(worldC, new Class<?>[] { int.class, int.class }, "getHeightValue", "func_72976_f");
        Method top = Extras.named(worldC, new Class<?>[] { int.class, int.class }, "getTopBlock", "func_147474_b");
        Object prov = Rwg2Dump.field(worldC, "provider", "field_73011_w").get(w);
        Method can = Extras.named(prov.getClass(), new Class<?>[] { int.class, int.class }, "canCoordinateBeSpawn", "func_76566_a");
        Extras.Loader ld = x.new Loader(w);
        j.key("format").str("[i, x, z, loaded, heightValue, topBlockIndex (getTopBlock), canSpawnNow, block index at heightValue-1]");
        j.key("walk").open('[');
        Random r = new Random(x.worldSeedOf(w));
        int px = 0, pz = 0;
        for (int i = 0; i <= 64; i++) {
            if (i > 0) {
                px += r.nextInt(64) - r.nextInt(64);
                pz += r.nextInt(64) - r.nextInt(64);
            }
            boolean loaded = ld.exists(px >> 4, pz >> 4);
            if (!loaded) {
                j.raw("[" + i + "," + px + "," + pz + ",false]");
                continue;
            }
            int h = (Integer) hv.invoke(w, px, pz);
            j.raw("[" + i + "," + px + "," + pz + ",true," + h + "," + b.id(top.invoke(w, px, pz)) + ","
                    + (can == null ? "null" : String.valueOf(can.invoke(prov, px, pz))) + "," + b.id(b.worldGetBlock.invoke(w, px, Math.max(0, h - 1), pz)) + "]");
        }
        j.close(']');
    }

    /** B9 section (after the dims section populated the Nether): nether216.json. */
    String nether() {
        Rwg2Dump.Json j = new Rwg2Dump.Json();
        j.open('{');
        try {
            netherNodes(j, x.new Blocks());
        } catch (Throwable t) {
            j.key("error").str(Extras.err(t));
        }
        j.close('}');
        return j.toString();
    }

    /** B9: Nether node tiles in loaded chunks [-12, 12]^2 (populated by the dims section) + their columns. */
    void netherNodes(Rwg2Dump.Json j, Extras.Blocks b) throws Exception {
        Object nether = x.cls("net.minecraftforge.common.DimensionManager").getMethod("getWorld", int.class).invoke(null, -1);
        if (nether == null) {
            j.key("error").str("nether not loaded");
            return;
        }
        Extras.Loader ld = x.new Loader(nether);
        Method getChunk = Extras.named(x.cls("net.minecraft.world.World"), new Class<?>[] { int.class, int.class }, "getChunkFromChunkCoords", "func_72964_e");
        List<int[]> nodes = new ArrayList<int[]>();
        Rwg2Dump.Json sink = new Rwg2Dump.Json();
        sink.open('[');
        int loaded = 0;
        for (int cz = -12; cz <= 12; cz++)
            for (int cx = -12; cx <= 12; cx++) {
                if (!ld.exists(cx, cz)) continue;
                loaded++;
                tiles(sink, getChunk.invoke(nether, cx, cz), nodes);
            }
        j.key("loadedChunks").val(loaded);
        j.key("columnFormat").str("[x, y, z, [names y-6..y+6]]");
        j.key("columns").open('[');
        for (int[] n : nodes) nodeColumn(j, nether, b, n);
        j.close(']');
    }

    static void writeGz(File f, byte[] data) throws Exception {
        GZIPOutputStream gz = new GZIPOutputStream(new FileOutputStream(f), 1 << 16);
        try {
            gz.write(data);
        } finally {
            gz.close();
        }
    }

    @SuppressWarnings("unused")
    private static int size(Collection<?> c) {
        return c == null ? 0 : c.size();
    }
}
