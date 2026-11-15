package rwg2dump;

import java.io.File;
import java.lang.reflect.Array;
import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.Collection;
import java.util.IdentityHashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Random;

/**
 * rwg2dump v2.1 additions. Each section is independent and never fatal: failures are written into its JSON.
 *
 * - chunk noise (any thread, no world): ChunkGeneratorRealistic(cmr, seed, continental).getNewNoise per chunk ->
 *   blended terrain height (testHeight) + per-column biome (biomesForGeneration[].baseBiome = the chunk biome array
 *   before the lava-cave marker pass).
 * - world chunks (server thread, loaded world): the world's own generator provideChunk (pre-population blocks):
 *   chunk biome array (incl. lava marker), top block, roguelike ground y, LootGames surface y.
 * - forestry.json: Forestry's own climate classification for every biome + getFloatTemperature samples.
 * - dungeons.json: Roguelike + LootGames configs and ground truth for the loaded world.
 */
final class Extras {

    static final int CHUNK_ORIGIN_HALF = 16; // chunks [-16,16)^2 around origin
    static final int CHUNK_LANDMARK = 12; // 12x12 chunks around a volcano / lava cave
    static final int WORLD_CHUNK_HALF = 6; // world provideChunk: [-6,6)^2
    static final int ROGUE_RANGE = 128; // chunks
    static final int LOOT_RANGE = 128; // chunks
    static final int TC_RANGE = 24; // chunks: [-24, 24]^2 fully populated, then scanned

    private final ClassLoader cl;
    private final Rwg2Dump.Dumper d;

    Extras(ClassLoader cl, Rwg2Dump.Dumper d) {
        this.cl = cl;
        this.d = d;
    }

    Class<?> cls(String n) throws ClassNotFoundException {
        return Class.forName(n, true, cl);
    }

    static String err(Throwable t) {
        while (t.getCause() != null && (t instanceof java.lang.reflect.InvocationTargetException)) t = t.getCause();
        java.io.StringWriter sw = new java.io.StringWriter();
        t.printStackTrace(new java.io.PrintWriter(sw));
        String s = sw.toString();
        return s.length() > 4000 ? s.substring(0, 4000) : s;
    }

    // =========================================================================================== chunk noise

    /** Returns manifest entries (comma-terminated) for the chunk-noise files of one seed/type. */
    String chunkNoise(Object cmr, long seed, boolean continental, List<String> notes) {
        String type = continental ? "RWG2_CONTINENTS" : "RWG2";
        StringBuilder files = new StringBuilder();
        try {
            Class<?> gcls = cls("rwg2.world.ChunkGeneratorRealistic");
            Constructor<?> ctor = gcls.getConstructor(d.cmrClass, long.class, boolean.class);
            Object gen = ctor.newInstance(cmr, seed, continental);
            Method getNewNoise = gcls.getMethod("getNewNoise", d.cmrClass, int.class, int.class,
                    Array.newInstance(d.rbbClass, 0).getClass());
            List<int[]> regions = new ArrayList<int[]>();
            regions.add(new int[] { -CHUNK_ORIGIN_HALF, -CHUNK_ORIGIN_HALF, 2 * CHUNK_ORIGIN_HALF, 2 * CHUNK_ORIGIN_HALF });
            List<String> names = new ArrayList<String>();
            names.add("origin");
            if (continental) {
                int[] v = findLandmark(cmr, "getVolcanoCoordinates");
                if (v != null) {
                    regions.add(new int[] { (v[0] >> 4) - CHUNK_LANDMARK / 2, (v[1] >> 4) - CHUNK_LANDMARK / 2,
                            CHUNK_LANDMARK, CHUNK_LANDMARK });
                    names.add("volcano");
                }
                int[] l = findLandmark(cmr, "getLavaCaveCoordinates");
                if (l != null) {
                    regions.add(new int[] { (l[0] >> 4) - CHUNK_LANDMARK / 2, (l[1] >> 4) - CHUNK_LANDMARK / 2,
                            CHUNK_LANDMARK, CHUNK_LANDMARK });
                    names.add("lava");
                }
            }
            for (int r = 0; r < regions.size(); r++) {
                int[] g = regions.get(r);
                int n = g[2] * g[3] * 256;
                ByteBuffer bio = Rwg2Dump.le(n * 2), hgt = Rwg2Dump.le(n * 4);
                Object biomes = Array.newInstance(d.rbbClass, 256);
                for (int cz = 0; cz < g[3]; cz++)
                    for (int cx = 0; cx < g[2]; cx++) {
                        int wx = (g[0] + cx) * 16, wz = (g[1] + cz) * 16;
                        float[] h = (float[]) getNewNoise.invoke(gen, cmr, wx, wz, biomes);
                        for (int lz = 0; lz < 16; lz++)
                            for (int lx = 0; lx < 16; lx++) {
                                Object rb = Array.get(biomes, lz * 16 + lx);
                                Object base = rb == null ? null : d.rbbBase.get(rb);
                                bio.putShort((short) Math.min(0xffff, d.genId(base)));
                                hgt.putFloat(h[lx * 16 + lz]);
                            }
                    }
                String file = String.format("chunks_%s_%s_%s.bin", Rwg2Dump.seedTag(seed), type, names.get(r));
                Rwg2Dump.writeBuffers(new File(d.out, file), bio, hgt);
                files.append(d.manifestEntry(file, seed, type, "chunks:" + names.get(r), g[0] * 16, g[1] * 16,
                        g[2], g[3], 16,
                        "chunk-major (cz,cx row-major over w x h chunks), 256 columns each idx=lz*16+lx: "
                                + "[u16 biomesForGeneration.baseBiome id x N][f32 getNewNoise testHeight x N]"));
            }
        } catch (Throwable t) {
            notes.add("chunkNoise " + seed + " " + type + ": " + err(t));
            Rwg2Dump.log("chunkNoise failed: " + t);
        }
        return files.toString();
    }

    /** First accepted landmark centre (world coords) on a 128-block scan around the origin, or null. */
    int[] findLandmark(Object cmr, String method) throws Exception {
        Field cf = d.cmrClass.getDeclaredField("continents");
        cf.setAccessible(true);
        Object cont = cf.get(cmr);
        boolean volcano = method.startsWith("getVolcano");
        Method key = cont.getClass().getMethod(volcano ? "getVolcanoSeedKey" : "getLavaCaveSeedKey", int.class, int.class);
        Method center = cont.getClass().getMethod(volcano ? "getVolcanoCenterCoordinates" : "getLavaCaveCenterCoordinates",
                int.class, int.class);
        Method gate = d.cmrClass.getMethod(method, int.class, int.class);
        for (int r = 0; r <= 8192; r += 128)
            for (int z = -r; z <= r; z += 128)
                for (int x = -r; x <= r; x += 128) {
                    if (Math.max(Math.abs(x), Math.abs(z)) != r) continue;
                    if ((Long) key.invoke(cont, x, z) == Long.MIN_VALUE) continue;
                    long c = (Long) center.invoke(cont, x, z);
                    int cx = (int) (c >> 32), cz = (int) c;
                    if ((Long) gate.invoke(cmr, cx, cz) != Long.MIN_VALUE) return new int[] { cx, cz };
                }
        return null;
    }

    /** Restore global statics the extra generator constructors touched (CanyonColor is seeded per world). */
    void restoreWorldStatics(Long worldSeed) {
        if (worldSeed == null) return;
        try {
            cls("rwg2.util.CanyonColor").getMethod("init", long.class).invoke(null, worldSeed);
        } catch (Throwable t) {
            Rwg2Dump.log("WARN: CanyonColor restore failed: " + t);
        }
    }

    // =========================================================================================== world helpers

    Object world() throws Exception {
        Class<?> dm = cls("net.minecraftforge.common.DimensionManager");
        return dm.getMethod("getWorld", int.class).invoke(null, 0);
    }

    long worldSeedOf(Object world) throws Exception {
        for (String n : new String[] { "getSeed", "func_72905_C" })
            try {
                return (Long) cls("net.minecraft.world.World").getMethod(n).invoke(world);
            } catch (NoSuchMethodException ignored) {}
        throw new IllegalStateException("getSeed not found");
    }

    Object worldGenerator(Object world) throws Exception {
        Object cps = Structures.fieldByTypeName(world, "ChunkProviderServer");
        Class<?> icp = cls("net.minecraft.world.chunk.IChunkProvider");
        for (Class<?> k = cps.getClass(); k != null; k = k.getSuperclass())
            for (Field f : k.getDeclaredFields())
                if (!Modifier.isStatic(f.getModifiers()) && f.getType() == icp) {
                    f.setAccessible(true);
                    return f.get(cps);
                }
        throw new IllegalStateException("world generator not found");
    }

    /** Block / material access by signature (production names are SRG). */
    final class Blocks {
        final Class<?> blockC, materialC, chunkC, worldC;
        final Method chunkGetBlock, worldGetBlock, getMaterial, fromName, uid;
        final Object air, water, wood, cactus, snow, grass, gourd, leaves, plants, lava, portal;
        final Map<Object, Integer> ids = new IdentityHashMap<Object, Integer>();
        final List<String> names = new ArrayList<String>();

        Blocks() throws Exception {
            blockC = cls("net.minecraft.block.Block");
            materialC = cls("net.minecraft.block.material.Material");
            chunkC = cls("net.minecraft.world.chunk.Chunk");
            worldC = cls("net.minecraft.world.World");
            chunkGetBlock = sig(chunkC, blockC, int.class, int.class, int.class);
            worldGetBlock = sig(worldC, blockC, int.class, int.class, int.class);
            getMaterial = sig(blockC, materialC);
            Method fn = null;
            for (Method m : blockC.getDeclaredMethods())
                if (Modifier.isStatic(m.getModifiers()) && m.getReturnType() == blockC
                        && m.getParameterTypes().length == 1 && m.getParameterTypes()[0] == String.class)
                    fn = m;
            fromName = fn;
            Class<?> gr = cls("cpw.mods.fml.common.registry.GameRegistry");
            uid = gr.getMethod("findUniqueIdentifierFor", blockC);
            air = mat("minecraft:air");
            water = mat("minecraft:water");
            wood = mat("minecraft:log");
            cactus = mat("minecraft:cactus");
            snow = mat("minecraft:snow_layer");
            grass = mat("minecraft:grass");
            gourd = mat("minecraft:pumpkin");
            leaves = mat("minecraft:leaves");
            plants = mat("minecraft:yellow_flower");
            lava = mat("minecraft:lava");
            portal = mat("minecraft:portal");
        }

        Object mat(String name) throws Exception {
            return getMaterial.invoke(fromName.invoke(null, name));
        }

        int id(Object block) {
            Integer i = ids.get(block);
            if (i == null) {
                String n;
                try {
                    n = String.valueOf(uid.invoke(null, block));
                } catch (Throwable t) {
                    n = String.valueOf(block);
                }
                i = names.size();
                names.add(n);
                ids.put(block, i);
            }
            return i;
        }

        /** Roguelike WorldEditor.validGroundBlock. */
        boolean rogueGround(Object block) throws Exception {
            Object m = getMaterial.invoke(block);
            return m != air && m != wood && m != water && m != cactus && m != snow && m != grass && m != gourd
                    && m != leaves && m != plants;
        }

        /** LootGames StructureGenerator.isValidSurfaceBlock. */
        boolean lootSurface(Object block) throws Exception {
            Object m = getMaterial.invoke(block);
            return m != wood && m != water && m != cactus && m != snow && m != grass && m != leaves && m != plants
                    && m != air && m != lava && m != portal;
        }
    }

    static Method sig(Class<?> c, Class<?> ret, Class<?>... params) {
        for (Class<?> k = c; k != null; k = k.getSuperclass())
            for (Method m : k.getDeclaredMethods())
                if (!Modifier.isStatic(m.getModifiers()) && m.getReturnType() == ret
                        && java.util.Arrays.equals(m.getParameterTypes(), params)) {
                    m.setAccessible(true);
                    return m;
                }
        throw new IllegalStateException("no method " + ret.getSimpleName() + " " + c.getSimpleName() + "("
                + java.util.Arrays.toString(params) + ")");
    }

    // =========================================================================================== chunk loading

    /** Force-loads (generates + populates) chunks so worldgen block probes see real terrain. */
    final class Loader {
        final Object cps;
        final Method load, exists;
        final Field onRequest;
        final List<String> errors = new ArrayList<String>();

        Loader(Object world) throws Exception {
            cps = Structures.fieldByTypeName(world, "ChunkProviderServer");
            Method l = null, e = null;
            for (String n : new String[] { "loadChunk", "func_73158_c" })
                try {
                    l = cps.getClass().getMethod(n, int.class, int.class);
                    break;
                } catch (NoSuchMethodException ignored) {}
            for (String n : new String[] { "chunkExists", "func_73149_a" })
                try {
                    e = cps.getClass().getMethod(n, int.class, int.class);
                    break;
                } catch (NoSuchMethodException ignored) {}
            load = l;
            exists = e;
            onRequest = Rwg2Dump.field(cps.getClass(), "loadChunkOnProvideRequest", "field_73250_a");
        }

        String onRequest() {
            try {
                return onRequest == null ? "unknown" : String.valueOf(onRequest.get(cps));
            } catch (Throwable t) {
                return String.valueOf(t);
            }
        }

        boolean exists(int cx, int cz) {
            try {
                return exists != null && (Boolean) exists.invoke(cps, cx, cz);
            } catch (Throwable t) {
                return false;
            }
        }

        /** Loads every chunk overlapping [x0,x1]x[z0,z1]; returns how many were not loaded before. */
        int area(int x0, int z0, int x1, int z1) {
            int fresh = 0;
            for (int cz = x0 == x1 && z0 == z1 ? z0 >> 4 : z0 >> 4; cz <= z1 >> 4; cz++)
                for (int cx = x0 >> 4; cx <= x1 >> 4; cx++) {
                    if (!exists(cx, cz)) fresh++;
                    try {
                        load.invoke(cps, cx, cz);
                    } catch (Throwable t) {
                        if (errors.size() < 5) errors.add("chunk " + cx + "," + cz + ": " + err(t));
                    }
                }
            return fresh;
        }
    }

    // =========================================================================================== thaumcraft

    /**
     * Server thread. Force-generates (and populates) chunks [-TC_RANGE, TC_RANGE]^2 of the loaded world, then lists
     * every Thaumcraft tile entity in them (aura nodes with type/modifier/aspects, eldritch obelisk/altar/cap,
     * mound chests are vanilla chests and are listed when inside TC structures' bounding chunks via `chests`).
     */
    String thaumcraft() {
        Rwg2Dump.Json j = new Rwg2Dump.Json();
        j.open('{');
        try {
            Object world = world();
            Loader loader = new Loader(world);
            int r = TC_RANGE;
            long t0 = System.currentTimeMillis();
            // populate needs the +x/+z neighbours loaded too
            int fresh = loader.area(-r * 16, -r * 16, (r + 2) * 16 - 1, (r + 2) * 16 - 1);
            j.key("rangeChunks").val(r);
            j.key("freshChunks").val(fresh);
            j.key("millis").val(System.currentTimeMillis() - t0);
            Class<?> chunkC = cls("net.minecraft.world.chunk.Chunk");
            Method getChunk = null; // getChunkFromChunkCoords (not ...FromBlockCoords, same signature)
            for (String nm : new String[] { "getChunkFromChunkCoords", "func_72964_e" })
                try {
                    getChunk = cls("net.minecraft.world.World").getMethod(nm, int.class, int.class);
                    break;
                } catch (NoSuchMethodException ignored) {}
            if (getChunk == null) throw new IllegalStateException("getChunkFromChunkCoords not found");
            Field teMap = null;
            for (Field f : chunkC.getDeclaredFields())
                if (Map.class.isAssignableFrom(f.getType()) && !Modifier.isStatic(f.getModifiers())) {
                    f.setAccessible(true);
                    teMap = f; // chunkTileEntityMap
                }
            Class<?> teC = cls("net.minecraft.tileentity.TileEntity");
            Field fx = Rwg2Dump.field(teC, "xCoord", "field_145851_c"), fy = Rwg2Dump.field(teC, "yCoord", "field_145848_d"),
                    fz = Rwg2Dump.field(teC, "zCoord", "field_145849_e");
            j.key("tiles").open('[');
            int n = 0;
            for (int cz = -r; cz <= r; cz++)
                for (int cx = -r; cx <= r; cx++) {
                    Object chunk = getChunk.invoke(world, cx, cz);
                    Map<?, ?> m = (Map<?, ?>) teMap.get(chunk);
                    for (Object te : new ArrayList<Object>(m.values())) {
                        String cn = te.getClass().getName();
                        boolean tc = cn.startsWith("thaumcraft.");
                        boolean chest = cn.endsWith("TileEntityChest") || cn.endsWith("TileEntitySpawner")
                                || cn.endsWith("TileEntityMobSpawner");
                        if (!tc && !chest) continue;
                        j.open('{');
                        j.key("class").str(cn);
                        j.key("x").val(fx.getInt(te));
                        j.key("y").val(fy.getInt(te));
                        j.key("z").val(fz.getInt(te));
                        if (cn.endsWith("TileNode") || cn.contains("Node")) {
                            for (String g : new String[] { "getNodeType", "getNodeModifier" })
                                try {
                                    j.key(g.substring(7).toLowerCase()).str(String.valueOf(te.getClass().getMethod(g).invoke(te)));
                                } catch (Throwable ignored) {}
                            try {
                                Object al = te.getClass().getMethod("getAspects").invoke(te);
                                Object[] as = (Object[]) al.getClass().getMethod("getAspects").invoke(al);
                                StringBuilder sb = new StringBuilder();
                                for (Object a : as) {
                                    if (a == null) continue;
                                    Object tag = a.getClass().getMethod("getTag").invoke(a);
                                    Object amt = al.getClass().getMethod("getAmount", a.getClass()).invoke(al, a);
                                    sb.append(tag).append(':').append(amt).append(' ');
                                }
                                j.key("aspects").str(sb.toString().trim());
                            } catch (Throwable ignored) {}
                        }
                        j.close('}');
                        n++;
                    }
                }
            j.close(']');
            j.key("count").val(n);
            j.key("totalMillis").val(System.currentTimeMillis() - t0);
        } catch (Throwable t) {
            j.key("error").str(err(t));
        }
        j.close('}');
        return j.toString();
    }

    // =========================================================================================== gregtech

    static Object call(Object o, String m) throws Exception {
        Method mm = null;
        for (Class<?> k = o.getClass(); k != null && mm == null; k = k.getSuperclass())
            for (Method x : k.getDeclaredMethods())
                if (x.getName().equals(m) && x.getParameterTypes().length == 0) mm = x;
        if (mm == null) mm = o.getClass().getMethod(m);
        mm.setAccessible(true);
        return mm.invoke(o);
    }

    static Object fieldOf(Object o, String name) throws Exception {
        for (Class<?> k = o.getClass(); k != null; k = k.getSuperclass())
            try {
                Field f = k.getDeclaredField(name);
                f.setAccessible(true);
                return f.get(o);
            } catch (NoSuchFieldException ignored) {}
        throw new NoSuchFieldException(name);
    }

    static void material(Rwg2Dump.Json j, String key, Object m) {
        j.key(key).open('{');
        if (m != null) {
            try {
                j.key("id").raw(String.valueOf(call(m, "getId")));
            } catch (Throwable t) {
                j.key("idError").str(String.valueOf(t));
            }
            try {
                j.key("name").str(String.valueOf(call(m, "getInternalName")));
            } catch (Throwable t) {
                j.key("name").str(String.valueOf(m));
            }
            j.key("class").str(m.getClass().getName());
            try {
                j.key("local").str(String.valueOf(m.getClass().getMethod("getLocalizedName").invoke(m)));
            } catch (Throwable ignored) {}
        }
        j.close('}');
    }

    /** GregTech ore veins: live vein list (WorldgenGTOreLayer.sList) + config + the vein cache of the loaded world. */
    String gregtech() {
        Rwg2Dump.Json j = new Rwg2Dump.Json();
        j.open('{');
        try {
            Class<?> layerC = cls("gregtech.common.WorldgenGTOreLayer");
            Class<?> genC = cls("gregtech.common.GTWorldgenerator");
            try {
                Object gt = cls("gregtech.common.config.Gregtech").getField("general").get(null);
                for (String f : new String[] { "oreveinPercentage", "oreveinAttempts", "oreveinMaxPlacementAttempts" })
                    j.key(f).raw(String.valueOf(fieldOf(gt, f)));
            } catch (Throwable t) {
                j.key("configError").str(String.valueOf(t));
            }
            try {
                j.key("pattern").str(String.valueOf(genC.getMethod("getOregenPattern").invoke(null)));
                j.key("patternSource").str(String.valueOf(genC.getMethod("getOregenPatternSource").invoke(null)));
            } catch (Throwable t) {
                j.key("patternError").str(String.valueOf(t));
            }
            // in-game (lang-file) vein names, as NEI / VisualProspecting / JourneyMap show them
            try {
                Rwg2Dump.Json mn = new Rwg2Dump.Json();
                mn.open('{');
                for (Object e : (Object[]) cls("gregtech.api.enums.OreMixes").getMethod("values").invoke(null)) {
                    Object b = e.getClass().getField("oreMixBuilder").get(e);
                    mn.key(String.valueOf(fieldOf(b, "oreMixName"))).str(String.valueOf(b.getClass().getMethod("getLocalizedName").invoke(b)));
                }
                mn.close('}');
                j.key("mixNames").raw(mn.toString());
            } catch (Throwable t) {
                j.key("mixNamesError").str(String.valueOf(t));
            }
            List<?> list = (List<?>) layerC.getField("sList").get(null);
            j.key("veins").open('[');
            for (Object l : list) {
                j.open('{');
                j.key("name").str(String.valueOf(fieldOf(l, "mWorldGenName")));
                j.key("weight").raw(String.valueOf(fieldOf(l, "mWeight")));
                j.key("density").raw(String.valueOf(fieldOf(l, "mDensity")));
                j.key("size").raw(String.valueOf(fieldOf(l, "mSize")));
                for (String dim : new String[] { "Overworld" }) {
                    j.key("minY").raw(String.valueOf(l.getClass().getMethod("getMinY", String.class).invoke(l, dim)));
                    j.key("maxY").raw(String.valueOf(l.getClass().getMethod("getMaxY", String.class).invoke(l, dim)));
                    j.key("overworld").raw(String.valueOf(l.getClass().getMethod("canGenerateIn", String.class).invoke(l, dim)));
                }
                j.key("stoneSpecific").raw(String.valueOf(call(l, "isStoneSpecific")));
                try {
                    Object stone = cls("gregtech.api.enums.StoneCategory").getField("Stone").get(null);
                    Method cg = null;
                    for (Method m : l.getClass().getMethods())
                        if (m.getName().equals("canGenerateIn") && m.getParameterTypes().length == 1
                                && !m.getParameterTypes()[0].equals(String.class)) cg = m;
                    j.key("stone").raw(String.valueOf(cg.invoke(l, stone)));
                } catch (Throwable t) {
                    j.key("stoneError").str(String.valueOf(t));
                }
                try {
                    j.key("restrictBiome").str(String.valueOf(fieldOf(l, "mRestrictBiome")));
                } catch (Throwable ignored) {}
                material(j, "primary", fieldOf(l, "mPrimary"));
                material(j, "secondary", fieldOf(l, "mSecondary"));
                material(j, "between", fieldOf(l, "mBetween"));
                material(j, "sporadic", fieldOf(l, "mSporadic"));
                j.close('}');
            }
            j.close(']');
            // vein cache of the loaded world (filled while chunks were generated above)
            try {
            Object map = genC.getField("validOreveins").get(null);
            Object set = map.getClass().getMethod("long2ObjectEntrySet").invoke(map);
            Rwg2Dump.Json cj = new Rwg2Dump.Json();
            cj.open('[');
            int n = 0;
            // public interface methods (the entry class itself is not accessible)
            Class<?> entC = cls("it.unimi.dsi.fastutil.longs.Long2ObjectMap$Entry");
            Method gk = entC.getMethod("getLongKey"), gv = Map.Entry.class.getMethod("getValue");
            for (Object e : (Iterable<?>) set) {
                long key = (Long) gk.invoke(e);
                Object v = gv.invoke(e);
                cj.open('{');
                cj.key("oreveinSeed").str(Long.toString(key));
                Object layer = call(v, "layer");
                cj.key("layer").str(String.valueOf(fieldOf(layer, "mWorldGenName")));
                cj.key("placementSeed").str(String.valueOf(call(v, "placementSeed")));
                Object pl = call(v, "placement");
                if (pl != null) {
                    for (String f : new String[] { "veinMinY", "veinWestX", "veinEastX", "veinNorthZ", "veinSouthZ" })
                        cj.key(f).raw(String.valueOf(call(pl, f)));
                }
                cj.close('}');
                n++;
            }
            cj.close(']');
            j.key("cache").raw(cj.toString());
            j.key("cacheSize").val(n);
            } catch (Throwable t) {
                j.key("cacheError").str(String.valueOf(t));
            }
            try {
                j.key("pendingTasks").raw(String.valueOf(((List<?>) Rwg2Dump.field(genC, "PENDING_TASKS").get(null)).size()));
            } catch (Throwable ignored) {}
        } catch (Throwable t) {
            j.key("error").str(err(t));
        }
        j.close('}');
        return j.toString();
    }

    /**
     * Server thread, after thaumcraft() populated [-TC_RANGE, TC_RANGE]^2. Scans every block of those chunks for
     * GT ore blocks (gregtech GTBlockOre; meta: mat = meta % 1000, +8000 natural, +16000 small, stone = (meta % 8000) / 1000)
     * and writes per chunk: material id -> count/minY/maxY (normal ores), plus small-ore totals.
     */
    String gtScan() {
        Rwg2Dump.Json j = new Rwg2Dump.Json();
        j.open('{');
        try {
            Object world = world();
            int r = TC_RANGE;
            long t0 = System.currentTimeMillis();
            Class<?> oreC = cls("gregtech.common.blocks.GTBlockOre");
            Class<?> chunkC = cls("net.minecraft.world.chunk.Chunk");
            Method getChunk = null;
            for (String nm : new String[] { "getChunkFromChunkCoords", "func_72964_e" })
                try {
                    getChunk = cls("net.minecraft.world.World").getMethod(nm, int.class, int.class);
                    break;
                } catch (NoSuchMethodException ignored) {}
            Method gb = null, gm = null, gs = null;
            for (String nm : new String[] { "getBlock", "func_150810_a" })
                try {
                    gb = chunkC.getMethod(nm, int.class, int.class, int.class);
                    break;
                } catch (NoSuchMethodException ignored) {}
            for (String nm : new String[] { "getBlockMetadata", "func_76628_c" })
                try {
                    gm = chunkC.getMethod(nm, int.class, int.class, int.class);
                    break;
                } catch (NoSuchMethodException ignored) {}
            for (String nm : new String[] { "getBlockStorageArray", "func_76587_i" })
                try {
                    gs = chunkC.getMethod(nm);
                    break;
                } catch (NoSuchMethodException ignored) {}
            if (getChunk == null || gb == null || gm == null) throw new IllegalStateException("chunk methods not found");
            // 2.10 census: every non-stone solid block below y 128 by name and 16-block layer, and the full GT
            // placement-test column (chunk x+7, z+9; WorldgenGTOreLayer's NO_OVERLAP check) of every chunk
            Blocks bk = new Blocks();
            Object stoneB = bk.fromName.invoke(null, "minecraft:stone");
            java.util.TreeMap<String, int[]> census = new java.util.TreeMap<String, int[]>();
            StringBuilder cols = new StringBuilder();
            j.key("rangeChunks").val(r);
            j.key("chunks").open('[');
            long ores = 0;
            for (int cz = -r; cz <= r; cz++)
                for (int cx = -r; cx <= r; cx++) {
                    Object chunk = getChunk.invoke(world, cx, cz);
                    Object[] secs = gs != null ? (Object[]) gs.invoke(chunk) : null;
                    // key: mat | small << 16 ; value: count, minY, maxY
                    java.util.TreeMap<Integer, int[]> h = new java.util.TreeMap<Integer, int[]>();
                    for (int sy = 0; sy < 16; sy++) {
                        if (secs != null && (sy >= secs.length || secs[sy] == null)) continue;
                        for (int y = sy * 16; y < sy * 16 + 16; y++)
                            for (int lz = 0; lz < 16; lz++)
                                for (int lx = 0; lx < 16; lx++) {
                                    Object b = gb.invoke(chunk, lx, y, lz);
                                    if (!oreC.isInstance(b)) {
                                        if (y < 128 && b != stoneB) {
                                            Object mt = bk.getMaterial.invoke(b);
                                            if (mt != bk.air && mt != bk.water && mt != bk.lava) {
                                                String nm = bk.names.size() > bk.id(b) ? bk.names.get(bk.id(b)) : String.valueOf(b);
                                                int[] c = census.get(nm);
                                                if (c == null) census.put(nm, c = new int[8]);
                                                c[y >> 4]++;
                                            }
                                        }
                                        continue;
                                    }
                                    int meta = (Integer) gm.invoke(chunk, lx, y, lz);
                                    int key = (meta % 1000) | (meta >= 16000 ? 1 << 16 : 0);
                                    int[] v = h.get(key);
                                    if (v == null) h.put(key, v = new int[] { 0, 999, -1 });
                                    v[0]++;
                                    v[1] = Math.min(v[1], y);
                                    v[2] = Math.max(v[2], y);
                                    ores++;
                                }
                    }
                    // the placement-test column, y 0..127: block id (into censusBlocks) + ':' + meta for GT ores
                    cols.append(cols.length() == 0 ? "" : ",").append("{\"cx\":").append(cx).append(",\"cz\":").append(cz).append(",\"ids\":\"");
                    for (int y = 0; y < 128; y++) {
                        Object b = gb.invoke(chunk, 7, y, 9);
                        cols.append(y == 0 ? "" : " ").append(b == stoneB ? "s" : String.valueOf(bk.id(b)));
                        if (oreC.isInstance(b)) cols.append(':').append(gm.invoke(chunk, 7, y, 9));
                    }
                    cols.append("\"}");
                    if (h.isEmpty()) continue;
                    j.open('{');
                    j.key("cx").val(cx);
                    j.key("cz").val(cz);
                    j.key("ores").open('[');
                    for (Map.Entry<Integer, int[]> e : h.entrySet()) {
                        int[] v = e.getValue();
                        j.raw("[" + (e.getKey() & 0xffff) + "," + (e.getKey() >> 16) + "," + v[0] + "," + v[1] + "," + v[2] + "]");
                    }
                    j.close(']');
                    j.close('}');
                }
            j.close(']');
            j.key("format").str("ores: [materialId, small(0/1), count, minY, maxY]");
            j.key("oreBlocks").raw(Long.toString(ores));
            j.key("census").open('{');
            for (Map.Entry<String, int[]> e : census.entrySet()) {
                StringBuilder sb = new StringBuilder("[");
                for (int k = 0; k < 8; k++) sb.append(k == 0 ? "" : ",").append(e.getValue()[k]);
                j.key(e.getKey()).raw(sb.append(']').toString());
            }
            j.close('}');
            j.key("censusFormat").str("non-stone, non-air/water/lava, non-GT-ore blocks below y128 in the scanned chunks: counts per 16-block layer (y 0-15, 16-31, ...)");
            j.key("testColumns").raw("[" + cols + "]");
            j.key("testColumnFormat").str("per chunk: blocks at (cx*16+7, y, cz*16+9) for y 0..127; s = minecraft:stone, else an index into censusBlocks; ':meta' on GT ores");
            j.key("censusBlocks").open('[');
            for (String n : bk.names) j.str(n);
            j.close(']');
            j.key("millis").val(System.currentTimeMillis() - t0);
        } catch (Throwable t) {
            j.key("error").str(err(t));
        }
        j.close('}');
        return j.toString();
    }

    // =========================================================================================== spawn (v2.6)
    //
    // Paste into Extras (uses cls(), world(), worldGenerator(), Blocks, sig(), err()). Call on the server thread with
    // the world loaded; write the result to spawn.json. Validates crates/rwg2-biomes/src/spawn.rs:
    //  - "spawn": WorldInfo spawn X/Y/Z (what createSpawnPosition chose when the world was created)
    //  - "walk": the vanilla walk replayed from new Random(seed) (positions are exact seed maths), per step:
    //      pre = getTopBlock semantics on a FRESH provideChunk (pre-population, what the original search saw for
    //            chunks that were not populated yet), with y
    //      now = world.getTopBlock as the world stands (populated chunks: tallgrass/trees/snow), + populated flag
    //  - "preStop": first step whose pre top is minecraft:grass (should equal the spawn unless population
    //    intervened during the original search)
    // Never loads/populates world chunks itself: "now" is only read for chunks that already exist.

    static final int SPAWN_WALK_STEPS = 64;

    static Method named(Class<?> c, Class<?>[] params, String... names) {
        for (String n : names)
            for (Class<?> k = c; k != null; k = k.getSuperclass())
                try {
                    Method m = k.getDeclaredMethod(n, params);
                    m.setAccessible(true);
                    return m;
                } catch (NoSuchMethodException ignored) {}
        return null;
    }

    String spawn() {
        StringBuilder j = new StringBuilder("{");
        try {
            Object world = world();
            Class<?> worldC = cls("net.minecraft.world.World");
            Class<?> chunkC = cls("net.minecraft.world.chunk.Chunk");
            Class<?> blockC = cls("net.minecraft.block.Block");
            Blocks b = new Blocks();

            long seed = (Long) named(worldC, new Class<?>[0], "getSeed", "func_72905_C").invoke(world);
            Object info = named(worldC, new Class<?>[0], "getWorldInfo", "func_72912_H").invoke(world);
            Class<?> wi = info.getClass();
            int sx = (Integer) named(wi, new Class<?>[0], "getSpawnX", "func_76079_c").invoke(info);
            int sy = (Integer) named(wi, new Class<?>[0], "getSpawnY", "func_76075_d").invoke(info);
            int sz = (Integer) named(wi, new Class<?>[0], "getSpawnZ", "func_76074_e").invoke(info);
            String type = "?";
            try {
                Object wt = named(wi, new Class<?>[0], "getTerrainType", "func_76067_t").invoke(info);
                type = String.valueOf(named(wt.getClass(), new Class<?>[0], "getWorldTypeName", "func_77127_a").invoke(wt));
            } catch (Throwable t) {
                type = String.valueOf(t);
            }
            j.append("\"seed\":\"").append(seed).append("\",\"worldType\":\"").append(type).append("\",");
            j.append("\"spawn\":[").append(sx).append(',').append(sy).append(',').append(sz).append("],");

            Method getTop = named(worldC, new Class<?>[] { int.class, int.class }, "getTopBlock", "func_147474_b");
            // the spawn predicate as the live provider implements it (v2.7: the 2.6 walk stopped on a non-grass top)
            Object prov = null;
            Method canSpawn = null;
            try {
            prov = Rwg2Dump.field(worldC, "provider", "field_73011_w").get(world);
            canSpawn = named(prov.getClass(), new Class<?>[] { int.class, int.class }, "canCoordinateBeSpawn", "func_76566_a");
            j.append("\"provider\":\"").append(prov.getClass().getName()).append("\",");
            if (canSpawn != null) j.append("\"canSpawnDeclaredIn\":\"").append(canSpawn.getDeclaringClass().getName()).append("\",");
            Object cm = Rwg2Dump.field(prov.getClass(), "worldChunkMgr", "field_76578_c").get(prov);
            j.append("\"chunkManager\":\"").append(cm.getClass().getName()).append("\",");
            try {
                Object list = named(cm.getClass(), new Class<?>[0], "getBiomesToSpawnIn", "func_76932_a").invoke(cm);
                j.append("\"spawnBiomes\":").append(((java.util.List<?>) list).size()).append(',');
            } catch (Throwable t) {
                j.append("\"spawnBiomesError\":\"").append(String.valueOf(t).replace("\"", "'")).append("\",");
            }
            } catch (Throwable t) {
                j.append("\"providerError\":\"").append(String.valueOf(t).replace("\"", "'")).append("\",");
            }
            Object gen = worldGenerator(world);
            Method provide = named(gen.getClass(), new Class<?>[] { int.class, int.class }, "provideChunk", "func_73154_d");
            Object cps = Structures.fieldByTypeName(world, "ChunkProviderServer");
            Method exists = named(cps.getClass(), new Class<?>[] { int.class, int.class }, "chunkExists", "func_73149_a");
            Method getChunk = named(worldC, new Class<?>[] { int.class, int.class }, "getChunkFromChunkCoords", "func_72964_e");
            Field populated = Rwg2Dump.field(chunkC, "isTerrainPopulated", "field_76646_k");
            Method cGet = b.chunkGetBlock;

            Random r = new Random(seed);
            int x = 0, z = 0, preStop = -1;
            Map<Long, Object> fresh = new LinkedHashMap<Long, Object>();
            j.append("\"walk\":[");
            for (int i = 0; i <= SPAWN_WALK_STEPS; i++) {
                if (i > 0) {
                    x += r.nextInt(64) - r.nextInt(64);
                    z += r.nextInt(64) - r.nextInt(64);
                    j.append(',');
                }
                int cx = x >> 4, cz = z >> 4, lx = x & 15, lz = z & 15;
                long key = ((long) cx << 32) ^ (cz & 0xffffffffL);
                Object c = fresh.get(key);
                if (c == null) {
                    c = provide.invoke(gen, cx, cz); // generator only, not added to the world
                    fresh.put(key, c);
                }
                // World.getTopBlock: y = 63; while (!air(y + 1)) y++
                int y = 63;
                while (y < 255 && b.getMaterial.invoke(cGet.invoke(c, lx, y + 1, lz)) != b.air) y++;
                Object pre = cGet.invoke(c, lx, y, lz);
                String preName = String.valueOf(b.uid.invoke(null, pre));
                if (preStop < 0 && "minecraft:grass".equals(preName)) preStop = i;
                j.append("{\"i\":").append(i).append(",\"x\":").append(x).append(",\"z\":").append(z)
                        .append(",\"pre\":\"").append(preName).append("\",\"preY\":").append(y);
                boolean ex = exists != null && (Boolean) exists.invoke(cps, cx, cz);
                if (ex) {
                    Object now = getTop.invoke(world, x, z);
                    Object wc = getChunk.invoke(world, cx, cz);
                    j.append(",\"now\":\"").append(b.uid.invoke(null, now)).append('"');
                    if (canSpawn != null) {
                        try {
                            j.append(",\"canSpawnNow\":").append(canSpawn.invoke(prov, x, z));
                        } catch (Throwable t) {
                            j.append(",\"canSpawnNow\":null");
                        }
                    }
                    if (populated != null) j.append(",\"populated\":").append(populated.get(wc));
                }
                j.append('}');
            }
            j.append("],\"preStop\":").append(preStop);
        } catch (Throwable t) {
            if (j.length() > 1 && j.charAt(j.length() - 1) != ',' && j.charAt(j.length() - 1) != '{') j.append(',');
            j.append("\"error\":\"").append(err(t).replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n")
                    .replace("\t", " ")).append('"');
        }
        return j.append('}').toString();
    }


    // ---- paste into tools/rwg2dump/src/rwg2dump/Extras.java; write the result as gtfluids.json ----
    // Ground truth for crates/rwg2-biomes/src/gtfluids.rs. Server thread, any time after GT preInit
    // (getPristineAmount needs no loaded chunks). Reflection only; no GT/Guava/commons classes referenced statically.
    //   "table": the live overworld GTUODimension, fluids in HashBiMap iteration order (what getRandomFluid walks)
    //            -> drop into data/gtnh-2.9.0/gtfluids.json with "order": "hashbimap"
    //   "chunks": [cx, cz, registry|null, pristineAmount] for chunks [-24, 24]^2 of dim 0
    // Do NOT call UndergroundOil.undergroundOil(...) here: coefficient >= 0 with floor(amount*k/5000) <= 0 zeroes the
    // chunk, and the -1 read path zeroes chunks with amount <= 5000 — both mutate the saved store.
    static final int UO_RANGE = 24;

    String gtFluids() {
        Rwg2Dump.Json j = new Rwg2Dump.Json();
        j.open('{');
        try {
            Object world = world();
            long seed = 0;
            for (String nm : new String[] { "getSeed", "func_72905_C" })
                try {
                    seed = (Long) cls("net.minecraft.world.World").getMethod(nm).invoke(world);
                    break;
                } catch (NoSuchMethodException ignored) {}
            j.key("seed").val(seed);
            j.key("dim").val(0);
            j.key("rangeChunks").val(UO_RANGE);
            // Guava actually in use (HashBiMap order differs across Guava versions: 17 = bucket order, 23+ = insertion)
            try {
                Class<?> hb = cls("com.google.common.collect.HashBiMap");
                Package p = hb.getPackage();
                j.key("guavaVersion").str(String.valueOf(p == null ? null : p.getImplementationVersion()));
                j.key("guavaJar").str(String.valueOf(hb.getProtectionDomain().getCodeSource().getLocation()));
            } catch (Throwable t) {
                j.key("guavaError").str(err(t));
            }
            Object proxy = cls("gregtech.GTMod").getField("proxy").get(null);
            Object uoList = proxy.getClass().getField("mUndergroundOil").get(proxy);
            try {
                j.key("blackList").str(java.util.Arrays.toString((int[]) fieldOf(uoList, "blackList")));
                j.key("dimensionKeys").str(String.valueOf(
                    ((Map<?, ?>) uoList.getClass().getMethod("getDimensionList").invoke(uoList)).keySet()));
            } catch (Throwable t) {
                j.key("listError").str(err(t));
            }
            Object dim = uoList.getClass().getMethod("GetDimension", int.class).invoke(uoList, 0);
            j.key("table").open('{');
            if (dim != null) {
                j.key("dimension").str(String.valueOf(fieldOf(dim, "Dimension")));
                j.key("maxChance").raw(String.valueOf(fieldOf(dim, "maxChance")));
                Map<?, ?> fl = (Map<?, ?>) dim.getClass().getMethod("getFluids").invoke(dim);
                j.key("order").str("hashbimap");
                j.key("fluids").open('[');
                for (Map.Entry<?, ?> e : fl.entrySet()) { // iteration order is the point
                    Object f = e.getValue();
                    j.open('{');
                    j.key("key").str(String.valueOf(e.getKey()));
                    j.key("registry").str(String.valueOf(fieldOf(f, "Registry")));
                    j.key("minAmount").raw(String.valueOf(fieldOf(f, "MinAmount")));
                    j.key("maxAmount").raw(String.valueOf(fieldOf(f, "MaxAmount")));
                    j.key("chance").raw(String.valueOf(fieldOf(f, "Chance")));
                    j.key("decreasePerOperation").raw(String.valueOf(fieldOf(f, "DecreasePerOperationAmount")));
                    j.key("name").str(String.valueOf(dim.getClass().getMethod("getUOFluidKey", f.getClass()).invoke(dim, f)));
                    try {
                        Object fluid = f.getClass().getMethod("getFluid").invoke(f);
                        j.key("registered").raw(String.valueOf(fluid != null));
                        if (fluid != null)
                            j.key("displayName").str(String.valueOf(fluid.getClass().getMethod("getLocalizedName").invoke(fluid)));
                    } catch (Throwable ignored) {}
                    j.close('}');
                }
                j.close(']');
            }
            j.close('}');
            // pristine amounts: UndergroundOil.getPristineAmount(World, int, int) -> Pair<GTUOFluid, Integer> | null
            Class<?> uo = cls("gregtech.common.UndergroundOil");
            Method pristine = null;
            for (Method m : uo.getMethods())
                if (m.getName().equals("getPristineAmount") && m.getParameterTypes().length == 3) pristine = m;
            if (pristine == null) throw new IllegalStateException("getPristineAmount not found");
            j.key("chunks").open('[');
            for (int cz = -UO_RANGE; cz <= UO_RANGE; cz++)
                for (int cx = -UO_RANGE; cx <= UO_RANGE; cx++) {
                    Object pair = pristine.invoke(null, world, cx, cz);
                    String reg = null;
                    int amount = 0;
                    if (pair != null) {
                        reg = String.valueOf(fieldOf(call(pair, "getLeft"), "Registry"));
                        amount = (Integer) call(pair, "getRight");
                    }
                    j.raw("[" + cx + "," + cz + "," + (reg == null ? "null" : "\"" + reg + "\"") + "," + amount + "]");
                }
            j.close(']');
        } catch (Throwable t) {
            j.key("error").str(err(t));
        }
        j.close('}');
        return j.toString();
    }


    // rwg2dump addition (proposed, v2.8): villages.json — VillageNames MapGenVillageVN candidates + starts over a
    // large area of the LOADED world (seed of the world), for validating villages.rs at scale.
    // Style: reflection only, like Structures.java / Extras.java. Run on the server thread. Does not load chunks:
    // canSpawnStructureAtCoords reads world.getBiomeGenForCoords (unloaded → WorldChunkManager.getBiomeGenAt), and
    // the VN Start constructor only reads world.getWorldChunkManager(). Nothing is put into structureMap.
    //
    // Output: {"worldSeed":"..","spacing":{"max":g,"min":h},"rangeRegions":R,
    //          "candidates":[{"chunkX","chunkZ","biome":id,"biomeName":"..","canSpawn":bool,
    //                          "sizeable":bool?,"components":n?,"nonRoad":n?,"starter":"class"?}]}

    String villages(Object chunkProvider /* ChunkGeneratorRealistic */, Object world, long seed) {
        Rwg2Dump.Json j = new Rwg2Dump.Json();
        j.open('{');
        try {
            Object mg = Structures.readField(chunkProvider, "villageGenerator");
            Class<?> mgc = mg.getClass(); // astrotibs.villagenames.village.MapGenVillageVN
            // spacing fields (VN: field_82665_g = max, field_82666_h = min)
            Field fg = mgc.getDeclaredField("field_82665_g"); fg.setAccessible(true);
            Field fh = mgc.getDeclaredField("field_82666_h"); fh.setAccessible(true);
            int g = fg.getInt(mg), h = fh.getInt(mg);
            j.key("worldSeed").val(Long.toString(seed));
            j.key("generatorClass").val(mgc.getName());
            j.key("spacing").open('{'); j.key("max").val(g); j.key("min").val(h); j.close('}');

            // canSpawnStructureAtCoords: MCP name or SRG func_75047_a, (int,int) -> boolean, declared in the VN class
            Method canSpawn = null, getStart = null;
            for (Method m : mgc.getDeclaredMethods()) {
                Class<?>[] p = m.getParameterTypes();
                if (p.length == 2 && p[0] == int.class && p[1] == int.class) {
                    if (m.getReturnType() == boolean.class) canSpawn = m;
                    else if (m.getReturnType().getSimpleName().equals("StructureStart")) getStart = m;
                }
            }
            canSpawn.setAccessible(true);
            getStart.setAccessible(true);
            // MapGenBase fields: rand (java.util.Random), worldObj (World): set worldObj like func_151539_a does
            Class<?> mgb = mgc;
            while (!mgb.getSimpleName().equals("MapGenBase")) mgb = mgb.getSuperclass();
            Field fRand = null, fWorld = null;
            for (Field f : mgb.getDeclaredFields()) {
                if (f.getType() == Random.class) fRand = f;
                else if (f.getType().isInstance(world)) fWorld = f;
            }
            fRand.setAccessible(true); fWorld.setAccessible(true);
            fWorld.set(mg, world);
            Random rand = (Random) fRand.get(mg);
            Object wcm = invokeAny(world, "getWorldChunkManager", "func_72959_q");
            Method getBiomeGenAt = null;
            for (Method m : wcm.getClass().getMethods())
                if (m.getParameterTypes().length == 2 && m.getParameterTypes()[0] == int.class
                        && m.getReturnType().getSimpleName().equals("BiomeGenBase")) getBiomeGenAt = m;

            final int R = 24; // regions: chunks [-R*g, R*g) → ±1200 chunks for g = 50 (2304 regions)
            j.key("rangeRegions").val(R);
            j.key("candidates").open('[');
            for (int rz = -R; rz < R; rz++)
                for (int rx = -R; rx < R; rx++) {
                    // reproduce the candidate exactly like canSpawn does, then call canSpawn on it (and on nothing else)
                    Random r = new Random((long) rx * 341873128712L + (long) rz * 132897987541L + seed + 10387312L);
                    int cx = rx * g + r.nextInt(g - h), cz = rz * g + r.nextInt(g - h);
                    boolean ok = (Boolean) canSpawn.invoke(mg, cx, cz);
                    Object b = getBiomeGenAt.invoke(wcm, cx * 16 + 8, cz * 16 + 8);
                    j.open('{');
                    j.key("chunkX").val(cx); j.key("chunkZ").val(cz);
                    Object bid = fieldAny(b, "biomeID", "field_76756_M");
                    j.key("biome").val(bid instanceof Integer ? (Integer) bid : -1);
                    j.key("biomeName").val(String.valueOf(fieldAny(b, "biomeName", "field_76791_y")));
                    j.key("canSpawn").val(ok);
                    // also sanity: the chunk next to the candidate must be refused
                    j.key("neighbourRefused").val(!(Boolean) canSpawn.invoke(mg, cx + 1, cz));
                    if (ok) {
                        try {
                            // MapGenBase.func_151539_a seeding + MapGenStructure.func_151538_a's rand.nextInt()
                            rand.setSeed(seed);
                            long l = rand.nextLong(), i1 = rand.nextLong();
                            rand.setSeed((long) cx * l ^ (long) cz * i1 ^ seed);
                            rand.nextInt();
                            Object start = getStart.invoke(mg, cx, cz);
                            List<?> comps = (List<?>) Structures.fieldByTypeName(start, "LinkedList");
                            int nonRoad = 0;
                            for (Object c : comps) {
                                Class<?> k = c.getClass(); boolean road = false;
                                for (; k != null; k = k.getSuperclass())
                                    if (k.getName().endsWith("StructureVillagePieces$Road")) road = true;
                                if (!road) nonRoad++;
                            }
                            j.key("components").val(comps.size());
                            j.key("nonRoad").val(nonRoad);
                            j.key("starter").val(comps.isEmpty() ? "" : comps.get(0).getClass().getName());
                            j.key("sizeable").val((Boolean) invokeAny(start, "isSizeableStructure", "func_75069_d"));
                        } catch (Throwable t) {
                            j.key("startError").val(Extras.err(t));
                        }
                    }
                    j.close('}');
                }
            j.close(']');
        } catch (Throwable t) {
            j.key("error").val(Extras.err(t));
        }
        j.close('}');
        return j.toString();
    }
    /** Calls a no-arg method by MCP or SRG name (dev vs obfuscated runtime). */
    static Object invokeAny(Object o, String... names) throws Exception {
        for (String n : names)
            for (Class<?> k = o.getClass(); k != null; k = k.getSuperclass())
                try {
                    Method m = k.getDeclaredMethod(n);
                    m.setAccessible(true);
                    return m.invoke(o);
                } catch (NoSuchMethodException e) {
                    // next
                }
        throw new NoSuchMethodException(o.getClass().getName() + "." + String.join("|", names));
    }

    static Object fieldAny(Object o, String... names) {
        for (String n : names) {
            Object v = Structures.readField(o, n);
            if (v != null) return v;
        }
        return null;
    }

    // Notes for wiring: MCP names above (getWorldChunkManager, isSizeableStructure, biomeID, biomeName) need the
    // SRG fallbacks the dumper already uses elsewhere (func_72959_q, func_75069_d, field_76756_M, field_76791_y).
    // StructureStart.components is a LinkedList field (field_75075_a). Cost: 2304 regions × (1 canSpawn + 1 start) —
    // a VN start builds ~40–70 pieces, ~1 ms each ⇒ a few seconds. Use a throwaway world.


    // ===================================================================================== thaumcraft nodes (2.8)
    // Drop-in for rwg2dump Extras (same helpers as thaumcraft(): world(), Loader, cls(), Rwg2Dump.Json/field).
    // Why: GTNH's Gadomancy (enableAdditionalNodeTypes=true) swaps Thaumcraft's blockAiry for its BlockNode, whose
    // node tile entity is makeo.gadomancy.common.blocks.tiles.TileExtendedNode. thaumcraft() only kept classes
    // starting with "thaumcraft.", so every wild / eldritch-ring / hilltop / mound node was dropped. This lists
    // every TileNode subclass, plus the aspect registry the node generator indexes.
    //
    // Output: { "aspects": [tag, ...] (Aspect.aspects registration order),
    //           "basic": n, "complex": n (ThaumcraftWorldGenerator lists, filled at the first node creation),
    //           "nodes": [{class, x, y, z, type, modifier, aspects}], "count": n }
    String thaumcraftNodes() {
        Rwg2Dump.Json j = new Rwg2Dump.Json();
        j.open('{');
        try {
            Object world = world();
            Class<?> aspectC = cls("thaumcraft.api.aspects.Aspect");
            java.util.Map<?, ?> reg = (java.util.Map<?, ?>) aspectC.getField("aspects").get(null);
            j.key("aspects").open('[');
            for (Object tag : new ArrayList<Object>(reg.keySet())) j.str(String.valueOf(tag));
            j.close(']');
            Class<?> genC = cls("thaumcraft.common.lib.world.ThaumcraftWorldGenerator");
            for (String f : new String[] { "basicAspects", "complexAspects" }) {
                Field ff = genC.getDeclaredField(f);
                ff.setAccessible(true);
                j.key(f.startsWith("basic") ? "basic" : "complex").val(((java.util.List<?>) ff.get(null)).size());
            }
            Class<?> tileNodeC = cls("thaumcraft.common.tiles.TileNode");
            int r = TC_RANGE;
            Loader loader = new Loader(world);
            loader.area(-r * 16, -r * 16, (r + 2) * 16 - 1, (r + 2) * 16 - 1); // no-op when thaumcraft() ran first
            Method getChunk = null;
            for (String nm : new String[] { "getChunkFromChunkCoords", "func_72964_e" })
                try {
                    getChunk = cls("net.minecraft.world.World").getMethod(nm, int.class, int.class);
                    break;
                } catch (NoSuchMethodException ignored) {}
            Class<?> chunkC = cls("net.minecraft.world.chunk.Chunk");
            Field teMap = null;
            for (Field f : chunkC.getDeclaredFields())
                if (Map.class.isAssignableFrom(f.getType()) && !Modifier.isStatic(f.getModifiers())) {
                    f.setAccessible(true);
                    teMap = f;
                }
            Class<?> teC = cls("net.minecraft.tileentity.TileEntity");
            Field fx = Rwg2Dump.field(teC, "xCoord", "field_145851_c"), fy = Rwg2Dump.field(teC, "yCoord", "field_145848_d"),
                    fz = Rwg2Dump.field(teC, "zCoord", "field_145849_e");
            j.key("nodes").open('[');
            int n = 0;
            for (int cz = -r; cz <= r; cz++)
                for (int cx = -r; cx <= r; cx++) {
                    Map<?, ?> m = (Map<?, ?>) teMap.get(getChunk.invoke(world, cx, cz));
                    for (Object te : new ArrayList<Object>(m.values())) {
                        if (!tileNodeC.isInstance(te)) continue; // TileNode, TileExtendedNode, TileNodeEnergized...
                        j.open('{');
                        j.key("class").str(te.getClass().getName());
                        j.key("x").val(fx.getInt(te));
                        j.key("y").val(fy.getInt(te));
                        j.key("z").val(fz.getInt(te));
                        j.key("type").str(String.valueOf(tileNodeC.getMethod("getNodeType").invoke(te)));
                        j.key("modifier").str(String.valueOf(tileNodeC.getMethod("getNodeModifier").invoke(te)));
                        Object al = tileNodeC.getMethod("getAspects").invoke(te);
                        StringBuilder sb = new StringBuilder();
                        for (Object a : (Object[]) al.getClass().getMethod("getAspects").invoke(al)) {
                            if (a == null) continue;
                            sb.append(aspectC.getMethod("getTag").invoke(a)).append(':')
                                    .append(al.getClass().getMethod("getAmount", aspectC).invoke(al, a)).append(' ');
                        }
                        j.key("aspects").str(sb.toString().trim());
                        j.close('}');
                        n++;
                    }
                }
            j.close(']');
            j.key("count").val(n);
        } catch (Throwable t) {
            j.key("error").str(err(t));
        }
        j.close('}');
        return j.toString();
    }
    // Register next to thaumcraft() in the dumper, output file "thaumcraft-nodes.json". Expected on seed 1337 /
    // 92900795 (chunks ±24): ~65 / ~67 wild nodes (TileExtendedNode, NORMAL mostly, ~1/3 in caves at y 11–40),
    // one ring node at altar y + 1 per ring, plus the TileNode silverwood/totem nodes already in thaumcraft.json.
    // "complex" should print 63 and aspects[48..69] gives the addon names (Forbidden Magic's 7 at 49..55).


    // =========================================================================================== other dimensions (v2.8)
    //
    // Paste into Extras (uses cls(), err(), Loader, call(), fieldOf(), material(), Rwg2Dump.Json/field/writeBuffers,
    // d.out). Call on the SERVER THREAD with the world loaded (it initialises dims -1 and 7 if they are not loaded:
    // DimensionManager.initDimension) and write the result to dims.json. Validates crates/rwg2-biomes/src/twilight.rs
    // and nether.rs. Sections (each independent, failures recorded in place):
    //  - "dims": per dim (-1, TwilightForestMod.dimensionID): provider / chunk manager / chunk generator classes,
    //    World.getSeed() (TF: provider.getSeed(), honours TwilightForestSeed), getDimensionName,
    //    GT DimensionDef.getDimensionName(world) + def flags, the MapGen classes held by the chunk generator.
    //  - "tfConfig": TwilightForestMod statics (dimensionID, twilightForestSeed, oldMapGen, maxBiomeIDs) + biome ids.
    //  - "tfBiomes": per seed (loaded TF seed + `seeds`), a TFWorldChunkManager(seed, DEFAULT) built directly
    //    (seed-only, no world): getBiomeGenAt(arr, x, z, w, h, false) windows (voronoi = zoomedBiomes) and
    //    getBiomesForGeneration windows (1:4 river-mix layer) -> tfbiomes_<seed>_<name>.bin (u16 LE row-major).
    //  - "tfFeatures": loaded TF world only: for regions [-R, R]^2 getNearestCenterXYZ(region*16) +
    //    generateFeatureFor1Point7(centre chunk) + getFeatureDirectlyAt(centre chunk) (id, name, structure flag) and the
    //    biome at the region centre; then MapGenTFMajorFeature starts (box, pieces) over +-TF_STRUCT_CHUNKS.
    //  - "nether": canSpawnStructureAtCoords over [-N, N]^2 chunks on the live MapGenNetherBridge (worldObj set to the
    //    nether world) + structure starts over +-NETHER_STRUCT_CHUNKS (box incl. setRandomHeight y, pieces) + a coarse
    //    biome histogram of the live nether chunk manager + BOPBiomeManager.netherBiomes (id, weight, name, in order).
    //  - "netherBiomes": like tfBiomes, with a WorldChunkManagerBOPHell(seed, DEFAULT) built directly (expected
    //    provider: biomesoplenty.common.world.WorldProviderBOPHell) -> netherbiomes_<seed>_<name>.bin.
    //  - "gt": the live WorldgenQuery filter per GT dimension name ("Nether", "Twilight Forest", "Overworld" for
    //    reference): sList entries with matches(), getMinY/getMaxY(dim), weight, size, density, materials; DimensionDef
    //    flags (oreVeinChance, respectsOreVeinHeights, generatesOre).
    //  - "gtScan": optional real ore blocks per chunk in force-populated [-GT_SCAN, GT_SCAN]^2 of dims -1 and TF (same
    //    format as gtores.json). Populating writes region files: throwaway world only.
    // Writes structure .dat files and region files for dims -1 / 7: run on a throwaway world copy.
    // Needs Structures.ints(..) and Structures.fieldOfType(..) package-private (drop `private`), plus imports
    // java.lang.reflect.Constructor (already there) and java.nio.ByteBuffer (already there).
    // Hook: in the dump command, after gregtech(): write "dims.json" = extras.dims(seeds) (seeds = manifest seeds).

    static final int TF_REGION_R = 8;            // TF feature regions [-8, 8]^2 (17x17 regions = 272x272 chunks)
    static final int TF_STRUCT_CHUNKS = 64;      // MapGenTFMajorFeature scan radius (chunks)
    static final int NETHER_CHECK_CHUNKS = 256;  // canSpawnStructureAtCoords census radius (chunks)
    static final int NETHER_STRUCT_CHUNKS = 96;  // MapGenNetherBridge structure scan radius (chunks)
    static final int GT_SCAN = 12;               // chunks, per dim; 0 disables gtScan

    String dims(long[] seeds) {
        Rwg2Dump.Json j = new Rwg2Dump.Json();
        j.open('{');
        Class<?> dm;
        int tfDim = 7;
        try {
            dm = cls("net.minecraftforge.common.DimensionManager");
        } catch (Throwable t) {
            j.key("error").str(err(t));
            j.close('}');
            return j.toString();
        }
        // ---- TF config statics
        try {
            Class<?> tfm = cls("twilightforest.TwilightForestMod");
            j.key("tfConfig").open('{');
            for (String f : new String[] { "dimensionID", "dimensionProviderID", "twilightForestSeed", "oldMapGen", "maxBiomeIDs" })
                try {
                    Object v = tfm.getField(f).get(null);
                    if (v instanceof String) j.key(f).str((String) v);
                    else j.key(f).raw(String.valueOf(v));
                    if (f.equals("dimensionID")) tfDim = (Integer) v;
                } catch (Throwable t) {
                    j.key(f + "Error").str(String.valueOf(t));
                }
            j.key("biomes").open('{');
            Class<?> tfb = cls("twilightforest.biomes.TFBiomeBase");
            Class<?> bgb = cls("net.minecraft.world.biome.BiomeGenBase");
            Field bid = Rwg2Dump.field(bgb, "biomeID", "field_76756_M");
            for (Field f : tfb.getFields())
                if (Modifier.isStatic(f.getModifiers()) && bgb.isAssignableFrom(f.getType())) {
                    Object b = f.get(null);
                    j.key(f.getName()).raw(b == null ? "null" : String.valueOf(bid.getInt(b)));
                }
            j.close('}');
            j.close('}');
        } catch (Throwable t) {
            j.key("tfConfigError").str(err(t));
        }
        // ---- per-dim provider info (initialises the dims)
        Object nether = null, tf = null, end = null;
        j.key("dims").open('{');
        for (int dim : new int[] { -1, tfDim, 1 }) {
            j.key(String.valueOf(dim)).open('{');
            try {
                Object w = dm.getMethod("getWorld", int.class).invoke(null, dim);
                if (w == null) {
                    dm.getMethod("initDimension", int.class).invoke(null, dim);
                    w = dm.getMethod("getWorld", int.class).invoke(null, dim);
                    j.key("initialised").raw("true");
                }
                if (w == null) throw new IllegalStateException("world for dim " + dim + " not available");
                if (dim == -1) nether = w;
                else if (dim == 1) end = w;
                else tf = w;
                j.key("providerType").raw(String.valueOf(dm.getMethod("getProviderType", int.class).invoke(null, dim)));
                Class<?> worldC = cls("net.minecraft.world.World");
                Object prov = Rwg2Dump.field(worldC, "provider", "field_73011_w").get(w);
                j.key("provider").str(prov.getClass().getName());
                j.key("dimensionId").raw(String.valueOf(Rwg2Dump.field(prov.getClass(), "dimensionId", "field_76574_g").get(prov)));
                j.key("dimensionName").str(String.valueOf(named(prov.getClass(), new Class<?>[0], "getDimensionName", "func_80007_l").invoke(prov)));
                j.key("worldSeed").str(String.valueOf(named(worldC, new Class<?>[0], "getSeed", "func_72905_C").invoke(w)));
                Object cm = Rwg2Dump.field(prov.getClass(), "worldChunkMgr", "field_76578_c").get(prov);
                j.key("chunkManager").str(cm.getClass().getName());
                Object gen = worldGenerator(w);
                j.key("chunkGenerator").str(gen.getClass().getName());
                // MapGenBase instances held by the generator (fortress / TF feature / caves ...)
                Class<?> mgb = cls("net.minecraft.world.gen.MapGenBase");
                j.key("mapGens").open('{');
                for (Class<?> k = gen.getClass(); k != null && k != Object.class; k = k.getSuperclass())
                    for (Field f : k.getDeclaredFields())
                        if (!Modifier.isStatic(f.getModifiers()) && mgb.isAssignableFrom(f.getType())) {
                            f.setAccessible(true);
                            Object mg = f.get(gen);
                            j.key(f.getName()).str(mg == null ? null : mg.getClass().getName());
                        }
                j.close('}');
                try {
                    Class<?> dd = cls("galacticgreg.api.enums.DimensionDef");
                    String gtName = String.valueOf(dd.getMethod("getDimensionName", worldC).invoke(null, w));
                    j.key("gtDimensionName").str(gtName);
                    Object def = dd.getMethod("getDefForWorld", worldC).invoke(null, w);
                    j.key("gtDef").str(String.valueOf(def));
                    if (def != null) for (String g : new String[] { "getOreVeinChance", "respectsOreVeinHeights", "generatesOre" })
                        try {
                            j.key(g).raw(String.valueOf(def.getClass().getMethod(g).invoke(def)));
                        } catch (Throwable ignored) {}
                } catch (Throwable t) {
                    j.key("gtError").str(String.valueOf(t));
                }
            } catch (Throwable t) {
                j.key("error").str(err(t));
            }
            j.close('}');
        }
        j.close('}');

        // ---- TF / Nether biome grids (seed-only chunk managers)
        for (String[] cmSpec : new String[][] { { "tfBiomes", "twilightforest.world.TFWorldChunkManager", "tfbiomes" },
                { "netherBiomes", "biomesoplenty.common.world.WorldChunkManagerBOPHell", "netherbiomes" } })
        try {
            Class<?> cmC = cls(cmSpec[1]);
            Class<?> wtC = cls("net.minecraft.world.WorldType");
            Object wtDefault = Rwg2Dump.field(wtC, "DEFAULT", "field_77137_b").get(null);
            Constructor<?> ctor = cmC.getConstructor(long.class, wtC);
            Class<?> bgb = cls("net.minecraft.world.biome.BiomeGenBase");
            Field bid = Rwg2Dump.field(bgb, "biomeID", "field_76756_M");
            Method zoomed = sig(cmC, Array.newInstance(bgb, 0).getClass(), Array.newInstance(bgb, 0).getClass(), int.class,
                    int.class, int.class, int.class, boolean.class);
            Method forGen = named(cmC, new Class<?>[] { Array.newInstance(bgb, 0).getClass(), int.class, int.class, int.class, int.class },
                    "getBiomesForGeneration", "func_76937_a");
            List<Long> all = new ArrayList<Long>();
            Object own = cmSpec[0].equals("tfBiomes") ? tf : nether;
            if (own != null) all.add((Long) named(cls("net.minecraft.world.World"), new Class<?>[0], "getSeed", "func_72905_C").invoke(own));
            for (long s : seeds) if (!all.contains(s)) all.add(s);
            // name, x, z, w, h, quarter(1:4 layer)
            Object[][] windows = { { "origin", -256, -256, 512, 512, false }, { "far", 123456, -98765, 256, 256, false },
                    { "q_origin", -128, -128, 256, 256, true }, { "q_wide", -1024, -1024, 512, 512, true } };
            j.key(cmSpec[0]).open('[');
            for (long s : all) {
                Object cm = ctor.newInstance(s, wtDefault);
                for (Object[] w : windows) {
                    int x = (Integer) w[1], z = (Integer) w[2], ww = (Integer) w[3], hh = (Integer) w[4];
                    boolean q = (Boolean) w[5];
                    ByteBuffer buf = Rwg2Dump.le(ww * hh * 2);
                    // in 64x64 tiles (IntCache-friendly), row-major output
                    short[] grid = new short[ww * hh];
                    for (int tz = 0; tz < hh; tz += 64)
                        for (int tx = 0; tx < ww; tx += 64) {
                            int tw = Math.min(64, ww - tx), th = Math.min(64, hh - tz);
                            Object arr = q ? forGen.invoke(cm, null, x + tx, z + tz, tw, th)
                                    : zoomed.invoke(cm, null, x + tx, z + tz, tw, th, false);
                            for (int i = 0; i < tw * th; i++) {
                                Object b = Array.get(arr, i);
                                grid[(tz + i / tw) * ww + tx + i % tw] = (short) (b == null ? -1 : bid.getInt(b));
                            }
                        }
                    for (short v : grid) buf.putShort(v);
                    String file = String.format("%s_%s_%s.bin", cmSpec[2], Rwg2Dump.seedTag(s), w[0]);
                    Rwg2Dump.writeBuffers(new File(d.out, file), buf);
                    j.open('{');
                    j.key("seed").str(Long.toString(s));
                    j.key("file").str(file);
                    j.key("layer").str(q ? "getBiomesForGeneration (1:4, x/z in quarter cells)" : "getBiomeGenAt(..., false) (blocks)");
                    j.key("x").val(x);
                    j.key("z").val(z);
                    j.key("w").val(ww);
                    j.key("h").val(hh);
                    j.close('}');
                }
            }
            j.close(']');
        } catch (Throwable t) {
            j.key(cmSpec[0] + "Error").str(err(t));
        }

        // ---- TF features (loaded TF world)
        if (tf != null) try {
            Class<?> worldC = cls("net.minecraft.world.World");
            Class<?> feat = cls("twilightforest.TFFeature");
            Method nearest = feat.getMethod("getNearestCenterXYZ", int.class, int.class, worldC);
            Method gen17 = feat.getMethod("generateFeatureFor1Point7", int.class, int.class, worldC);
            Method directly = feat.getMethod("getFeatureDirectlyAt", int.class, int.class, worldC);
            Method biomeAt = named(worldC, new Class<?>[] { int.class, int.class }, "getBiomeGenForCoords", "func_72807_a");
            Field bid = Rwg2Dump.field(cls("net.minecraft.world.biome.BiomeGenBase"), "biomeID", "field_76756_M");
            j.key("tfFeatures").open('{');
            j.key("regions").open('[');
            for (int rz = -TF_REGION_R; rz <= TF_REGION_R; rz++)
                for (int rx = -TF_REGION_R; rx <= TF_REGION_R; rx++) {
                    Object cc = nearest.invoke(null, rx * 16, rz * 16, tf);
                    int[] xyz = Structures.ints(cc, 3); // ChunkCoordinates posX, posY, posZ
                    int cx = xyz[0] >> 4, cz = xyz[2] >> 4;
                    Object f = gen17.invoke(null, cx, cz, tf);
                    Object fd = directly.invoke(null, cx, cz, tf);
                    int rcx = Math.round(cx / 16F) * 16, rcz = Math.round(cz / 16F) * 16;
                    Object b = biomeAt.invoke(tf, (rcx << 4) + 8, (rcz << 4) + 8);
                    j.raw("[" + rx + "," + rz + "," + xyz[0] + "," + xyz[2] + "," + fieldOf(f, "featureID") + ","
                            + fieldOf(fd, "featureID") + "," + fieldOf(f, "isStructureEnabled") + "," + bid.getInt(b) + "]");
                }
            j.close(']');
            j.key("format").str("regions: [rx, rz, centreX, centreZ, generateFeatureFor1Point7 id, getFeatureDirectlyAt id, isStructureEnabled, biome at region centre]");
            // structure starts of the live MapGenTFMajorFeature
            Object gen = worldGenerator(tf);
            Object mg = fieldByTypeNameEnd(gen, "MapGenTFMajorFeature");
            if (mg != null) {
                scanStarts(gen, tf, mg, TF_STRUCT_CHUNKS);
                writeStartsGeneric(j, mg);
            } else j.key("majorFeature").str("not found");
            j.close('}');
        } catch (Throwable t) {
            j.key("tfFeaturesError").str(err(t));
        }

        // ---- Nether fortresses + biome histogram
        if (nether != null) try {
            j.key("nether").open('{');
            Object gen = worldGenerator(nether);
            Object mg = fieldByTypeNameEnd(gen, "MapGenNetherBridge");
            if (mg == null) throw new IllegalStateException("no MapGenNetherBridge in " + gen.getClass().getName());
            j.key("bridgeClass").str(mg.getClass().getName());
            Field wo = Rwg2Dump.field(cls("net.minecraft.world.gen.MapGenBase"), "worldObj", "field_75039_c");
            if (wo.get(mg) == null) wo.set(mg, nether);
            Method can = named(mg.getClass(), new Class<?>[] { int.class, int.class }, "canSpawnStructureAtCoords", "func_75047_a");
            j.key("checkRadiusChunks").val(NETHER_CHECK_CHUNKS);
            j.key("starts").open('[');
            int n = 0;
            for (int cz = -NETHER_CHECK_CHUNKS; cz < NETHER_CHECK_CHUNKS; cz++)
                for (int cx = -NETHER_CHECK_CHUNKS; cx < NETHER_CHECK_CHUNKS; cx++)
                    if ((Boolean) can.invoke(mg, cx, cz)) {
                        j.raw("[" + cx + "," + cz + "]");
                        n++;
                    }
            j.close(']');
            j.key("count").val(n);
            scanStarts(gen, nether, mg, NETHER_STRUCT_CHUNKS);
            writeStartsGeneric(j, mg);
            // biome histogram of the live chunk manager (coarse grid)
            Class<?> worldC = cls("net.minecraft.world.World");
            Object prov = Rwg2Dump.field(worldC, "provider", "field_73011_w").get(nether);
            Object cm = Rwg2Dump.field(prov.getClass(), "worldChunkMgr", "field_76578_c").get(prov);
            Method at = named(cm.getClass(), new Class<?>[] { int.class, int.class }, "getBiomeGenAt", "func_76935_a");
            Field bid = Rwg2Dump.field(cls("net.minecraft.world.biome.BiomeGenBase"), "biomeID", "field_76756_M");
            java.util.TreeMap<Integer, Integer> hist = new java.util.TreeMap<Integer, Integer>();
            for (int z = -8192; z < 8192; z += 64)
                for (int x = -8192; x < 8192; x += 64) {
                    Object b = at.invoke(cm, x, z);
                    int id = b == null ? -1 : bid.getInt(b);
                    Integer c = hist.get(id);
                    hist.put(id, c == null ? 1 : c + 1);
                }
            j.key("biomeHistogram").str(hist.toString());
            try {
                List<?> nb = (List<?>) cls("biomesoplenty.common.world.BOPBiomeManager").getField("netherBiomes").get(null);
                Field bid2 = Rwg2Dump.field(cls("net.minecraft.world.biome.BiomeGenBase"), "biomeID", "field_76756_M");
                Field bname = Rwg2Dump.field(cls("net.minecraft.world.biome.BiomeGenBase"), "biomeName", "field_76791_y");
                Field wt = Rwg2Dump.field(cls("net.minecraft.util.WeightedRandom$Item"), "itemWeight", "field_76292_a");
                j.key("bopNetherBiomes").open('[');
                for (Object e : nb) {
                    Object b = e.getClass().getField("biome").get(e);
                    j.raw("[" + bid2.getInt(b) + "," + wt.getInt(e) + ",\"" + bname.get(b) + "\"]");
                }
                j.close(']');
            } catch (Throwable ignored) {}
            j.close('}');
        } catch (Throwable t) {
            j.key("netherError").str(err(t));
        }

        // ---- GT vein lists per dimension name (the real WorldgenQuery filter)
        try {
            Class<?> layerC = cls("gregtech.common.WorldgenGTOreLayer");
            Class<?> qC = cls("gregtech.common.worldgen.WorldgenQuery");
            List<?> list = (List<?>) layerC.getField("sList").get(null);
            j.key("gt").open('{');
            for (String dim : new String[] { "Nether", "Twilight Forest", "Overworld" }) {
                Object q = qC.getMethod("veins").invoke(null);
                q = qC.getMethod("inDimension", String.class).invoke(q, dim);
                Method matches = null;
                for (Method m : qC.getMethods()) if (m.getName().equals("matches") && m.getParameterTypes().length == 1) matches = m;
                j.key(dim).open('[');
                for (Object l : list) {
                    if (!(Boolean) matches.invoke(q, l)) continue;
                    j.open('{');
                    j.key("name").str(String.valueOf(fieldOf(l, "mWorldGenName")));
                    j.key("weight").raw(String.valueOf(fieldOf(l, "mWeight")));
                    j.key("density").raw(String.valueOf(fieldOf(l, "mDensity")));
                    j.key("size").raw(String.valueOf(fieldOf(l, "mSize")));
                    j.key("minY").raw(String.valueOf(l.getClass().getMethod("getMinY", String.class).invoke(l, dim)));
                    j.key("maxY").raw(String.valueOf(l.getClass().getMethod("getMaxY", String.class).invoke(l, dim)));
                    material(j, "primary", fieldOf(l, "mPrimary"));
                    material(j, "secondary", fieldOf(l, "mSecondary"));
                    material(j, "between", fieldOf(l, "mBetween"));
                    material(j, "sporadic", fieldOf(l, "mSporadic"));
                    j.close('}');
                }
                j.close(']');
                try {
                    Object def = cls("galacticgreg.api.enums.DimensionDef").getMethod("getDefByName", String.class).invoke(null, dim);
                    j.key(dim + ":def").open('{');
                    if (def != null) for (String g : new String[] { "getOreVeinChance", "respectsOreVeinHeights", "generatesOre" })
                        try {
                            j.key(g).raw(String.valueOf(def.getClass().getMethod(g).invoke(def)));
                        } catch (Throwable ignored) {}
                    j.close('}');
                } catch (Throwable ignored) {}
            }
            j.close('}');
        } catch (Throwable t) {
            j.key("gtError").str(err(t));
        }

        // ---- real GT ore blocks in force-populated chunks of dims -1 / TF
        if (GT_SCAN > 0) {
            j.key("gtScan").open('{');
            for (Object w : new Object[] { nether, tf, end }) {
                if (w == null) continue;
                try {
                    Object prov = Rwg2Dump.field(cls("net.minecraft.world.World"), "provider", "field_73011_w").get(w);
                    int dim = (Integer) Rwg2Dump.field(prov.getClass(), "dimensionId", "field_76574_g").get(prov);
                    j.key(String.valueOf(dim)).raw(gtScanWorld(w, GT_SCAN));
                    j.key(dim + ":terrain").raw(dimTerrain(w, dim, GT_SCAN));
                } catch (Throwable t) {
                    j.key("error").str(err(t));
                }
            }
            j.close('}');
        }
        // ---- 2.11: the End (Hardcore Ender Expansion in GTNH): coarse pre-population island map from the world's own
        // generator, chunks [-END_COARSE, END_COARSE]^2 every END_STEP chunks: top Y + top block at local (8, 8)
        if (end != null) {
            j.key("endCoarse").open('{');
            try {
                Blocks b = new Blocks();
                Object gen = worldGenerator(end);
                Method provide = null;
                for (String n : new String[] { "provideChunk", "func_73154_d" })
                    try {
                        provide = gen.getClass().getMethod(n, int.class, int.class);
                        break;
                    } catch (NoSuchMethodException ignored) {}
                j.key("generator").str(gen.getClass().getName());
                j.key("chunkMin").val(-END_COARSE);
                j.key("step").val(END_STEP);
                j.key("format").str("rows z, cols x: topY, topBlockIndex per sampled chunk at local (8, 8); -1 = void");
                j.key("rows").open('[');
                for (int cz = -END_COARSE; cz <= END_COARSE; cz += END_STEP) {
                    StringBuilder row = new StringBuilder("[");
                    for (int cx = -END_COARSE; cx <= END_COARSE; cx += END_STEP) {
                        Object chunk = provide.invoke(gen, cx, cz);
                        int top = -1;
                        Object topB = null;
                        for (int y = 255; y >= 0; y--) {
                            Object blk = b.chunkGetBlock.invoke(chunk, 8, y, 8);
                            if (b.getMaterial.invoke(blk) != b.air) {
                                top = y;
                                topB = blk;
                                break;
                            }
                        }
                        if (row.length() > 1) row.append(',');
                        row.append(top).append(',').append(topB == null ? -1 : b.id(topB));
                    }
                    j.raw(row.append(']').toString());
                }
                j.close(']');
                j.key("blocks").open('[');
                for (String sname : b.names) j.str(sname);
                j.close(']');
            } catch (Throwable t) {
                j.key("error").str(err(t));
            }
            j.close('}');
        }
        // ---- 2.12: HEE islands + towers from HEE's own classes (placement is seed + chunk + dragon kills only)
        if (end != null) {
            j.key("endStructures").open('{');
            try {
                long seed = (Long) named(cls("net.minecraft.world.World"), new Class<?>[0], "getSeed", "func_72905_C").invoke(end);
                Object gen = worldGenerator(end);
                Object islandGen = fieldByTypeNameEnd(gen, "MapGenIsland");
                Object towerGen = fieldByTypeNameEnd(gen, "MapGenTower");
                Field wo = Rwg2Dump.field(cls("net.minecraft.world.gen.MapGenBase"), "worldObj", "field_75039_c");
                wo.set(islandGen, end);
                wo.set(towerGen, end);
                Method canI = named(islandGen.getClass(), new Class<?>[] { int.class, int.class }, "canSpawnStructureAtCoords", "func_75047_a");
                Method canT = named(towerGen.getClass(), new Class<?>[] { int.class, int.class }, "canSpawnStructureAtCoords", "func_75047_a");
                int kills = -1;
                try {
                    Class<?> ds = cls("chylex.hee.system.savedata.types.DragonSavefile");
                    Object sf = cls("chylex.hee.system.savedata.WorldDataHandler").getMethod("get", Class.class).invoke(null, ds);
                    kills = (Integer) ds.getMethod("getDragonDeathAmount").invoke(sf);
                } catch (Throwable t) {
                    j.key("killsError").str(err(t));
                }
                j.key("worldSeed").str(String.valueOf(seed));
                j.key("dragonKills").val(kills);
                j.key("rangeChunks").val(END_STRUCT);
                Method biomeAt = cls("chylex.hee.world.structure.island.util.IslandSpawnChecker").getMethod("getIslandBiomeAt", int.class, int.class, long.class, int.class);
                Class<?> ibb = cls("chylex.hee.world.structure.island.biome.IslandBiomeBase");
                Method pick = ibb.getMethod("pickRandomBiome", java.util.Random.class);
                Field bid = ibb.getField("biomeID");
                Method genData = ibb.getMethod("generateData", java.util.Random.class);
                StringBuilder isl = new StringBuilder(), isl2 = new StringBuilder(), starts = new StringBuilder(), tw = new StringBuilder();
                for (int cz = -END_STRUCT; cz <= END_STRUCT; cz++)
                    for (int cx = -END_STRUCT; cx <= END_STRUCT; cx++) {
                        byte b0 = (Byte) biomeAt.invoke(null, cx, cz, seed, 1 + Math.max(kills, 0));
                        if (b0 != -1) {
                            int x = cx * 16, z = cz * 16;
                            java.util.Random r = new java.util.Random(((x / 9) * 238504L + (z / 9) * 10058432215L) ^ seed);
                            int bottomY = 6 + r.nextInt(25);
                            Object biome = pick.invoke(null, r);
                            Object data = genData.invoke(biome, r);
                            Object content = data.getClass().getField("content").get(data);
                            String devs = String.valueOf(data.getClass().getMethod("getDeviationsAsString").invoke(data));
                            if (isl.length() > 0) isl.append(',');
                            isl.append('[').append(cx).append(',').append(cz).append(',').append(b0).append(',').append(bottomY).append(',')
                                    .append(bid.get(biome)).append(',').append(content.getClass().getField("id").get(content)).append(",\"")
                                    .append(devs).append("\"]");
                        }
                        byte b1 = (Byte) biomeAt.invoke(null, cx, cz, seed, 2 + Math.max(kills, 0));
                        if (b1 != -1) {
                            if (isl2.length() > 0) isl2.append(',');
                            isl2.append('[').append(cx).append(',').append(cz).append(',').append(b1).append(']');
                        }
                        if ((Boolean) canI.invoke(islandGen, cx, cz)) {
                            if (starts.length() > 0) starts.append(',');
                            starts.append('[').append(cx).append(',').append(cz).append(']');
                        }
                        if ((Boolean) canT.invoke(towerGen, cx, cz)) {
                            if (tw.length() > 0) tw.append(',');
                            tw.append('[').append(cx).append(',').append(cz).append(']');
                        }
                    }
                j.key("format").str("islands: [cx, cz, getIslandBiomeAt, bottomY, biomeID, contentVariationId, deviations] (consistentRand via HEE pickRandomBiome/generateData); islandsNextKill: [cx, cz, biome] with one more dragon kill; islandStarts / towerStarts: MapGenIsland / MapGenTower canSpawnStructureAtCoords");
                j.key("islands").raw("[" + isl + "]");
                j.key("islandsNextKill").raw("[" + isl2 + "]");
                j.key("islandStarts").raw("[" + starts + "]");
                j.key("towerStarts").raw("[" + tw + "]");
            } catch (Throwable t) {
                j.key("error").str(err(t));
            }
            j.close('}');
        }
        // ---- 2.13: generated End samples: the nearest HEE island (post-population tops over its 208×208 area), the
        // two nearest Draconic Evolution comets (block census at the predicted core), DE config, both seed views
        if (end != null) {
            j.key("endGenerated").open('{');
            try {
                Blocks b = new Blocks();
                Loader ld = new Loader(end);
                Class<?> worldC = cls("net.minecraft.world.World");
                long provSeed = (Long) named(worldC, new Class<?>[0], "getSeed", "func_72905_C").invoke(end);
                Object info = named(worldC, new Class<?>[0], "getWorldInfo", "func_72912_H").invoke(end);
                long rawSeed = (Long) named(info.getClass(), new Class<?>[0], "getSeed", "func_76063_b").invoke(info);
                Method chunkAt = named(worldC, new Class<?>[] { int.class, int.class }, "getChunkFromChunkCoords", "func_72964_e");
                j.key("worldGetSeed").str(String.valueOf(provSeed));
                j.key("worldInfoSeed").str(String.valueOf(rawSeed));
                int rarity = 10000;
                try {
                    Class<?> ch = cls("com.brandon3055.draconicevolution.common.handler.ConfigHandler");
                    j.key("deConfig").open('{');
                    for (String f : new String[] { "cometRarity", "generateEnderComets", "generateChaosIslands", "chaosIslandSeparation", "oreWeightEnd", "disableOreSpawnEnd" }) {
                        Object v = ch.getField(f).get(null);
                        j.key(f).raw(String.valueOf(v));
                        if (f.equals("cometRarity")) rarity = (Integer) v;
                    }
                    j.close('}');
                } catch (Throwable t) {
                    j.key("deConfigError").str(err(t));
                }
                // nearest island start (HEE's own checker)
                Method biomeAt = cls("chylex.hee.world.structure.island.util.IslandSpawnChecker").getMethod("getIslandBiomeAt", int.class, int.class, long.class, int.class);
                int kills = 0;
                try {
                    Class<?> ds = cls("chylex.hee.system.savedata.types.DragonSavefile");
                    Object sf = cls("chylex.hee.system.savedata.WorldDataHandler").getMethod("get", Class.class).invoke(null, ds);
                    kills = (Integer) ds.getMethod("getDragonDeathAmount").invoke(sf);
                } catch (Throwable ignored) {}
                int bx = 0, bz = 0;
                long best = Long.MAX_VALUE;
                final List<int[]> allStarts = new ArrayList<int[]>();
                for (int cz = -160; cz <= 160; cz++)
                    for (int cx = -160; cx <= 160; cx++)
                        if ((Byte) biomeAt.invoke(null, cx, cz, rawSeed, 1 + kills) != -1) {
                            long d = (long) cx * cx + (long) cz * cz;
                            allStarts.add(new int[] { cx, cz });
                            if (d < best) {
                                best = d;
                                bx = cx;
                                bz = cz;
                            }
                        }
                if (best != Long.MAX_VALUE) {
                    j.key("island").open('{');
                    j.key("startChunk").raw("[" + bx + "," + bz + "]");
                    int fresh = ld.area((bx - 1) * 16, (bz - 1) * 16, (bx + 15) * 16 + 15, (bz + 15) * 16 + 15);
                    j.key("freshChunks").val(fresh);
                    int x0 = bx * 16 + 8, z0 = bz * 16 + 8;
                    j.key("x0").val(x0);
                    j.key("z0").val(z0);
                    j.key("format").str("tops / topBlocks: 208 rows (z) × 208 (x) from (x0, z0): highest non-air y (-1 void) and its block index (blocks)");
                    StringBuilder tops = new StringBuilder("["), tb = new StringBuilder("[");
                    for (int z = z0; z < z0 + 208; z++)
                        for (int x = x0; x < x0 + 208; x++) {
                            Object chunk = chunkAt.invoke(end, x >> 4, z >> 4);
                            int top = -1;
                            Object blk = null;
                            for (int y = 255; y >= 0; y--) {
                                Object bl = b.chunkGetBlock.invoke(chunk, x & 15, y, z & 15);
                                if (b.getMaterial.invoke(bl) != b.air) {
                                    top = y;
                                    blk = bl;
                                    break;
                                }
                            }
                            if (tops.length() > 1) {
                                tops.append(',');
                                tb.append(',');
                            }
                            tops.append(top);
                            tb.append(blk == null ? -1 : b.id(blk));
                        }
                    j.key("tops").raw(tops.append(']').toString());
                    j.key("topBlocks").raw(tb.append(']').toString());
                    j.close('}');
                }
                // DE comets: FML per-chunk Random (GameRegistry.generateWorld: Random(world.getSeed()), xSeed/zSeed =
                // nextLong() >> 2 + 1L, reseeded (xSeed·cx + zSeed·cz) ^ seed before every generator)
                java.util.Random fr = new java.util.Random(provSeed);
                long xs = fr.nextLong() >> 2 + 1L, zs = fr.nextLong() >> 2 + 1L;
                List<int[]> comets = new ArrayList<int[]>();
                for (int cz = -400; cz <= 400; cz++)
                    for (int cx = -400; cx <= 400; cx++) {
                        int x = cx * 16, z = cz * 16;
                        if (!(Math.sqrt(x * x + z * z) > 200)) continue;
                        java.util.Random q = new java.util.Random((xs * cx + zs * cz) ^ provSeed);
                        int x1 = x + q.nextInt(16), y = 20 + q.nextInt(170), z1 = z + q.nextInt(16);
                        if (q.nextInt(Math.max(1, rarity)) == 0) {
                            double rot = q.nextInt();
                            q.nextInt(50);
                            q.nextInt(40);
                            int size = 2 + q.nextInt(8);
                            comets.add(new int[] { x1, y, z1, size, cx, cz });
                        }
                    }
                java.util.Collections.sort(comets, new java.util.Comparator<int[]>() {
                    public int compare(int[] a, int[] c) {
                        return Long.compare((long) a[0] * a[0] + (long) a[2] * a[2], (long) c[0] * c[0] + (long) c[2] * c[2]);
                    }
                });
                // 2.13.1 diagnostics: why the island may be missing
                j.key("diag").open('{');
                try {
                    Object islandGen = fieldByTypeNameEnd(worldGenerator(end), "MapGenIsland");
                    Object towerGen = fieldByTypeNameEnd(worldGenerator(end), "MapGenTower");
                    Class<?> mgs = cls("net.minecraft.world.gen.structure.MapGenStructure");
                    Field sm = Rwg2Dump.field(mgs, "structureMap", "field_75053_d");
                    Map<?, ?> im = (Map<?, ?>) sm.get(islandGen), tm = (Map<?, ?>) sm.get(towerGen);
                    j.key("islandStructureMapSize").val(im.size());
                    j.key("towerStructureMapSize").val(tm.size());
                    long key = (long) bx & 4294967295L | ((long) bz & 4294967295L) << 32;
                    j.key("islandStartInMap").str(String.valueOf(im.containsKey(key)));
                    StringBuilder keys = new StringBuilder();
                    int n = 0;
                    for (Object k : im.keySet()) {
                        if (n++ >= 20) break;
                        long l = (Long) k;
                        keys.append('[').append((int) l).append(',').append((int) (l >> 32)).append(']');
                    }
                    j.key("islandMapKeys").str(keys.toString());
                } catch (Throwable t) {
                    j.key("structureMapError").str(err(t));
                }
                try {
                    // who listens to ChunkProviderEvent.ReplaceBiomeBlocks, and what a test event returns
                    Class<?> ev = cls("net.minecraftforge.event.terraingen.ChunkProviderEvent$ReplaceBiomeBlocks");
                    Object bus = cls("net.minecraftforge.common.MinecraftForge").getField("EVENT_BUS").get(null);
                    Field busId = Rwg2Dump.field(bus.getClass(), "busID");
                    int id = busId.getInt(bus);
                    Object proto = null;
                    for (java.lang.reflect.Constructor<?> c : ev.getConstructors())
                        if (c.getParameterTypes().length == 7) {
                            Object[] a = new Object[7];
                            Class<?>[] pt = c.getParameterTypes();
                            for (int q = 0; q < 7; q++) {
                                if (pt[q] == int.class) a[q] = 4000;
                                else if (pt[q].isArray() && pt[q].getComponentType() == byte.class) a[q] = new byte[32768];
                                else if (pt[q].isArray()) a[q] = java.lang.reflect.Array.newInstance(pt[q].getComponentType(), pt[q].getComponentType().getSimpleName().contains("Biome") ? 256 : 32768);
                                else if (pt[q].isInstance(end)) a[q] = end;
                                else if (pt[q].isInstance(worldGenerator(end))) a[q] = worldGenerator(end);
                            }
                            proto = c.newInstance(a);
                        }
                    Object ll = ev.getMethod("getListenerList").invoke(proto);
                    Object[] ls = (Object[]) ll.getClass().getMethod("getListeners", int.class).invoke(ll, id);
                    j.key("replaceBiomeBlocksListeners").open('[');
                    for (Object l : ls) j.str(String.valueOf(l));
                    j.close(']');
                    boolean posted = (Boolean) bus.getClass().getMethod("post", cls("cpw.mods.fml.common.eventhandler.Event")).invoke(bus, proto);
                    j.key("testEventCanceled").str(String.valueOf(posted));
                    j.key("testEventResult").str(String.valueOf(ev.getMethod("getResult").invoke(proto)));
                } catch (Throwable t) {
                    j.key("eventError").str(err(t));
                }
                try {
                    // full-depth census of the island area (clouds hide the tops)
                    Map<String, Integer> census = new java.util.TreeMap<String, Integer>();
                    int x0 = bx * 16 + 8, z0 = bz * 16 + 8;
                    for (int x = x0; x < x0 + 208; x += 2)
                        for (int z = z0; z < z0 + 208; z += 2) {
                            Object chunk = chunkAt.invoke(end, x >> 4, z >> 4);
                            for (int y = 0; y < 256; y++) {
                                Object bl = b.chunkGetBlock.invoke(chunk, x & 15, y, z & 15);
                                if (b.getMaterial.invoke(bl) == b.air) continue;
                                String nm = b.names.get(b.id(bl));
                                Integer v = census.get(nm);
                                census.put(nm, v == null ? 1 : v + 1);
                            }
                        }
                    j.key("islandAreaCensusEvery2").open('{');
                    for (Map.Entry<String, Integer> e : census.entrySet()) j.key(e.getKey()).val(e.getValue());
                    j.close('}');
                } catch (Throwable t) {
                    j.key("censusError").str(err(t));
                }
                // 2.13.2: the island start object itself, then one forced generateStructuresInChunk (exceptions kept)
                int[] freshStart = null;
                try {
                    Object islandGen = fieldByTypeNameEnd(worldGenerator(end), "MapGenIsland");
                    j.key("probeNearest").open('{');
                    islandProbe(j, end, islandGen, bx, bz, b, ld, false);
                    j.close('}');
                    java.util.Collections.sort(allStarts, new java.util.Comparator<int[]>() {
                        public int compare(int[] a, int[] c) {
                            return Long.compare((long) a[0] * a[0] + (long) a[1] * a[1], (long) c[0] * c[0] + (long) c[1] * c[1]);
                        }
                    });
                    for (int[] st : allStarts)
                        if (!ld.exists(st[0] + 6, st[1] + 6) && !ld.exists(st[0], st[1])) {
                            j.key("probeFresh").open('{');
                            islandProbe(j, end, islandGen, st[0], st[1], b, ld, true);
                            j.close('}');
                            freshStart = st;
                            break;
                        }
                } catch (Throwable t) {
                    j.key("probeError").str(err(t));
                }
                j.close('}');
                // 2.14: block-level checks (fresh island scan + GT ores + draconium, a fresh tower, main-island draconium)
                j.key("checks214").open('{');
                try {
                    checks214(j, end, b, ld, freshStart, chunkAt);
                } catch (Throwable t) {
                    j.key("error").str(err(t));
                }
                j.close('}');
                j.key("cometCandidates").val(comets.size());
                j.key("comets").open('[');
                for (int k = 0; k < Math.min(2, comets.size()); k++) {
                    int[] c = comets.get(k);
                    ld.area(c[0] - 32, c[2] - 32, c[0] + 32, c[2] + 32);
                    Map<String, Integer> census = new java.util.TreeMap<String, Integer>();
                    int r = c[3];
                    for (int x = c[0] - r; x <= c[0] + r; x++)
                        for (int z = c[2] - r; z <= c[2] + r; z++)
                            for (int y = c[1] - r; y <= c[1] + r; y++) {
                                Object bl = b.chunkGetBlock.invoke(chunkAt.invoke(end, x >> 4, z >> 4), x & 15, y, z & 15);
                                if (b.getMaterial.invoke(bl) == b.air) continue;
                                b.id(bl);
                                String n = b.names.get(b.id(bl));
                                Integer v = census.get(n);
                                census.put(n, v == null ? 1 : v + 1);
                            }
                    StringBuilder cs = new StringBuilder("{");
                    for (Map.Entry<String, Integer> e : census.entrySet()) {
                        if (cs.length() > 1) cs.append(',');
                        cs.append('"').append(e.getKey()).append("\":").append(e.getValue());
                    }
                    j.raw("{\"x\":" + c[0] + ",\"y\":" + c[1] + ",\"z\":" + c[2] + ",\"size\":" + c[3] + ",\"chunk\":[" + c[4] + "," + c[5] + "],\"censusInCube\":" + cs.append('}') + "}");
                }
                j.close(']');
                j.key("blocks").open('[');
                for (String sname : b.names) j.str(sname);
                j.close(']');
            } catch (Throwable t) {
                j.key("error").str(err(t));
            }
            j.close('}');
        }
        j.close('}');
        return j.toString();
    }

    static final int END_COARSE = 256; // chunks
    static final int END_STRUCT = 220; // chunks
    static final int END_STEP = 8;

    // =========================================================================================== dungeon loot (2.11)
    /**
     * Vanilla dungeon chests draw their loot from the chunk's shared populate Random, so the number of draws decides
     * every later blob in that chunk. Records the live ChestGenHooks("dungeonChest") table (content class, item,
     * min/max stack, weight; count range) and probes the draw sequence: for PROBES seeds a counting Random runs
     * getItems + getCount + WeightedRandomChestContent.generateChestContents into a fresh TileEntityChest; the number of
     * next() calls and the filled slots are written.
     */
    static final int PROBES = 400;

    static final class CountingRandom extends java.util.Random {
        int calls;

        CountingRandom(long seed) {
            super(seed);
        }

        @Override
        protected int next(int bits) {
            calls++;
            return super.next(bits);
        }
    }

    String dungeonLoot() {
        Rwg2Dump.Json j = new Rwg2Dump.Json();
        j.open('{');
        lootTable(j, "dungeonChest", PROBES);
        j.close('}');
        return j.toString();
    }

    /** Forge 1.7.10 ChestGenHooks categories (2.17: mineshaft corridor chests draw from the populate Random). */
    static final String[] LOOT_CATEGORIES = { "mineshaftCorridor", "strongholdCorridor", "strongholdLibrary",
            "strongholdCrossing", "villageBlacksmith", "pyramidDesertyChest", "pyramidJungleChest",
            "pyramidJungleDispenser", "bonusChest", "dungeonChest" };

    /**
     * v2.17 chestloot.json: every ChestGenHooks category's live table + draw probes (same format as dungeonloot.json,
     * one object per category), plus the class of each MapGenStructure.structureMap of the overworld generator
     * (iteration order of overlapping mineshaft starts).
     */
    String chestLoot() {
        Rwg2Dump.Json j = new Rwg2Dump.Json();
        j.open('{');
        j.key("rwg2dumpVersion").str(Rwg2Dump.VERSION);
        j.key("categories").open('{');
        for (String c : LOOT_CATEGORIES) {
            j.key(c).open('{');
            lootTable(j, c, c.equals("mineshaftCorridor") ? PROBES : 100);
            j.close('}');
        }
        j.close('}');
        try {
            Object w = world();
            Object gen = worldGenerator(w);
            Class<?> mgs = cls("net.minecraft.world.gen.structure.MapGenStructure");
            Field sm = Rwg2Dump.field(mgs, "structureMap", "field_75053_d");
            j.key("generator").str(gen.getClass().getName());
            j.key("structureMaps").open('{');
            for (Class<?> c = gen.getClass(); c != null && c != Object.class; c = c.getSuperclass())
                for (Field f : c.getDeclaredFields()) {
                    if (java.lang.reflect.Modifier.isStatic(f.getModifiers())) continue;
                    f.setAccessible(true);
                    Object v = f.get(gen);
                    if (v != null && mgs.isInstance(v)) {
                        Object m = sm.get(v);
                        j.key(f.getName()).str(v.getClass().getName() + " map " + (m == null ? "null" : m.getClass().getName()) + " size " + (m instanceof Map ? ((Map<?, ?>) m).size() : -1));
                    }
                }
            j.close('}');
        } catch (Throwable t) {
            j.key("structureMapsError").str(err(t));
        }
        j.close('}');
        return j.toString();
    }

    void lootTable(Rwg2Dump.Json j, String category, int probes) {
        try {
            Class<?> hooks = cls("net.minecraftforge.common.ChestGenHooks");
            Class<?> wrcc = cls("net.minecraft.util.WeightedRandomChestContent");
            Class<?> invC = cls("net.minecraft.inventory.IInventory");
            Object info = hooks.getMethod("getInfo", String.class).invoke(null, category);
            j.key("countMin").raw(String.valueOf(hooks.getMethod("getMin").invoke(info)));
            j.key("countMax").raw(String.valueOf(hooks.getMethod("getMax").invoke(info)));
            Method getItems = hooks.getMethod("getItems", java.util.Random.class);
            Method getCount = hooks.getMethod("getCount", java.util.Random.class);
            Object[] items = (Object[]) getItems.invoke(info, new java.util.Random(1));
            Field fStack = Rwg2Dump.field(wrcc, "theItemId", "field_76297_b");
            Field fMin = Rwg2Dump.field(wrcc, "theMinimumChanceToGenerateItem", "field_76295_d");
            Field fMax = Rwg2Dump.field(wrcc, "theMaximumChanceToGenerateItem", "field_76296_e");
            Field fW = Rwg2Dump.field(cls("net.minecraft.util.WeightedRandom$Item"), "itemWeight", "field_76292_a");
            // 2.11.1: display name, registry id (music discs share one unlocalized name), max stack size
            Class<?> isC = cls("net.minecraft.item.ItemStack");
            Method dName = named(isC, new Class<?>[0], "getDisplayName", "func_82833_r");
            Method mStack = named(isC, new Class<?>[0], "getMaxStackSize", "func_77976_d");
            Method gItem = named(isC, new Class<?>[0], "getItem", "func_77973_b");
            Object reg = null;
            Method regName = null;
            try {
                reg = Rwg2Dump.field(cls("net.minecraft.item.Item"), "itemRegistry", "field_150901_e").get(null);
                regName = named(reg.getClass(), new Class<?>[] { Object.class }, "getNameForObject", "func_148750_c");
            } catch (Throwable ignored) {}
            j.key("items").open('[');
            for (Object it : items) {
                j.open('{');
                j.key("class").str(it.getClass().getName());
                Object st = fStack.get(it);
                j.key("stack").str(String.valueOf(st));
                try {
                    j.key("name").str(String.valueOf(dName.invoke(st)));
                } catch (Throwable t) {
                    j.key("nameError").str(err(t));
                }
                try {
                    j.key("maxStack").raw(String.valueOf(mStack.invoke(st)));
                } catch (Throwable ignored) {}
                try {
                    j.key("id").str(String.valueOf(regName.invoke(reg, gItem.invoke(st))));
                } catch (Throwable ignored) {}
                j.key("min").raw(String.valueOf(fMin.get(it)));
                j.key("max").raw(String.valueOf(fMax.get(it)));
                j.key("weight").raw(String.valueOf(fW.get(it)));
                j.close('}');
            }
            j.close(']');
            Method gen = null;
            for (Method m : wrcc.getMethods())
                if (Modifier.isStatic(m.getModifiers()) && m.getParameterTypes().length == 4
                        && m.getParameterTypes()[0] == java.util.Random.class && m.getParameterTypes()[2] == invC)
                    gen = m;
            Class<?> chestC = cls("net.minecraft.tileentity.TileEntityChest");
            Method getSlot = named(invC, new Class<?>[] { int.class }, "getStackInSlot", "func_70301_a");
            j.key("probeFormat").str("[seed, nextCalls, items, count, slot:stack ...] for new Random(seed): getItems, getCount, generateChestContents");
            j.key("probes").open('[');
            for (int i = 0; i < probes; i++) {
                long seed = 0x9E3779B97F4A7C15L * (i + 1);
                CountingRandom r = new CountingRandom(seed);
                Object[] its = (Object[]) getItems.invoke(info, r);
                int cnt = (Integer) getCount.invoke(info, r);
                Object chest = chestC.newInstance();
                Object arr = java.lang.reflect.Array.newInstance(wrcc, its.length);
                for (int k = 0; k < its.length; k++) java.lang.reflect.Array.set(arr, k, its[k]);
                gen.invoke(null, r, arr, chest, cnt);
                StringBuilder sb = new StringBuilder("[\"" + seed + "\"," + r.calls + "," + its.length + "," + cnt);
                for (int k = 0; k < 27; k++) {
                    Object st = getSlot.invoke(chest, k);
                    if (st != null) sb.append(",\"").append(k).append(':').append(String.valueOf(st).replace("\"", "'")).append('"');
                }
                j.raw(sb.append(']').toString());
            }
            j.close(']');
        } catch (Throwable t) {
            j.key("error").str(err(t));
        }
    }

    /** First non-static field whose type's simple name ends with `suffix` (MapGen instances by type). */
    /** 2.13.2 diagnostics for one HEE island start at chunk (sx, sz). */
    void islandProbe(Rwg2Dump.Json j, Object end, Object islandGen, int sx, int sz, Blocks b, Loader ld, boolean loadArea) throws Exception {
        Class<?> worldC = cls("net.minecraft.world.World");
        Class<?> chunkC = cls("net.minecraft.world.chunk.Chunk");
        Method chunkAt = named(worldC, new Class<?>[] { int.class, int.class }, "getChunkFromChunkCoords", "func_72964_e");
        j.key("startChunk").raw("[" + sx + "," + sz + "]");
        try {
            Class<?> bg = cls("chylex.hee.world.biome.BiomeGenHardcoreEnd");
            j.key("overrideWorldGen").str(String.valueOf(bg.getField("overrideWorldGen").get(null)));
        } catch (Throwable t) {
            j.key("overrideWorldGenError").str(err(t));
        }
        if (loadArea) {
            ld.errors.clear();
            int fresh = ld.area((sx - 1) * 16, (sz - 1) * 16, (sx + 15) * 16 + 15, (sz + 15) * 16 + 15);
            j.key("freshChunks").val(fresh);
            j.key("loadErrors").open('[');
            for (String e : ld.errors) j.str(e);
            j.close(']');
        }
        Field populated = Rwg2Dump.field(chunkC, "isTerrainPopulated", "field_76646_k");
        int pop = 0, loaded = 0;
        for (int cz = sz; cz <= sz + 13; cz++)
            for (int cx = sx; cx <= sx + 13; cx++)
                if (ld.exists(cx, cz)) {
                    loaded++;
                    if (populated != null && (Boolean) populated.get(chunkAt.invoke(end, cx, cz))) pop++;
                }
        j.key("areaChunksLoaded").val(loaded);
        j.key("areaChunksPopulated").val(pop);
        Class<?> mgs = cls("net.minecraft.world.gen.structure.MapGenStructure");
        Field sm = Rwg2Dump.field(mgs, "structureMap", "field_75053_d");
        Map<?, ?> im = (Map<?, ?>) sm.get(islandGen);
        long key = (long) sx & 4294967295L | ((long) sz & 4294967295L) << 32;
        Object start = im.get(key);
        j.key("startInMap").val(start != null);
        j.key("censusBefore").raw(islandCensus(end, sx, sz, b, chunkAt));
        if (start != null) {
            j.key("startBefore").open('{');
            describeStart(j, start);
            j.close('}');
        }
        Method gsc = named(mgs, new Class<?>[] { worldC, java.util.Random.class, int.class, int.class }, "generateStructuresInChunk", "func_75051_a");
        try {
            Object r = gsc.invoke(islandGen, end, new java.util.Random(1L), sx + 6, sz + 6);
            j.key("forcedResult").str(String.valueOf(r));
        } catch (Throwable t) {
            j.key("forcedError").str(err(t));
        }
        Object start2 = im.get(key);
        if (start2 != null) {
            j.key("startAfter").open('{');
            describeStart(j, start2);
            j.close('}');
        }
        j.key("censusAfter").raw(islandCensus(end, sx, sz, b, chunkAt));
    }

    /** 2.14: non-air runs of one column, "y:len:blockIndex" joined by ' ' (bottom up). */
    static String columnRuns(Blocks b, Object chunk, int lx, int lz) throws Exception {
        StringBuilder sb = new StringBuilder();
        int y0 = -1, id0 = -1;
        for (int y = 0; y <= 256; y++) {
            int id = -1;
            if (y < 256) {
                Object bl = b.chunkGetBlock.invoke(chunk, lx, y, lz);
                if (b.getMaterial.invoke(bl) != b.air) id = b.id(bl);
            }
            if (id != id0) {
                if (id0 >= 0) {
                    if (sb.length() > 0) sb.append(' ');
                    sb.append(y0).append(':').append(y - y0).append(':').append(id0);
                }
                y0 = y;
                id0 = id;
            }
        }
        return sb.toString();
    }

    /** 2.14: rows (z) of column-run strings over [x0, x0+w) × [z0, z0+h), see columnRuns. */
    static void scanRuns(Rwg2Dump.Json j, Object world, Blocks b, Method chunkAt, int x0, int z0, int w, int h) throws Exception {
        j.key("x0").val(x0);
        j.key("z0").val(z0);
        j.key("w").val(w);
        j.key("h").val(h);
        j.key("rows").open('[');
        for (int z = z0; z < z0 + h; z++) {
            j.open('[');
            for (int x = x0; x < x0 + w; x++) j.str(columnRuns(b, chunkAt.invoke(world, x >> 4, z >> 4), x & 15, z & 15));
            j.close(']');
        }
        j.close(']');
    }

    /** 2.14: GT ore blocks per chunk over a chunk rect (gtScan format) and every block whose name contains "draconium". */
    void oresRect(Rwg2Dump.Json j, Object world, Blocks b, Method chunkAt, int cx0, int cz0, int cx1, int cz1, int maxY) throws Exception {
        Class<?> oreC = null;
        try {
            oreC = cls("gregtech.common.blocks.GTBlockOre");
        } catch (Throwable ignored) {}
        Method gm = named(b.chunkC, new Class<?>[] { int.class, int.class, int.class }, "getBlockMetadata", "func_76628_c");
        StringBuilder drac = new StringBuilder();
        j.key("chunkRect").raw("[" + cx0 + "," + cz0 + "," + cx1 + "," + cz1 + "]");
        j.key("gtChunks").open('[');
        for (int cz = cz0; cz <= cz1; cz++)
            for (int cx = cx0; cx <= cx1; cx++) {
                Object chunk = chunkAt.invoke(world, cx, cz);
                java.util.TreeMap<Integer, int[]> h = new java.util.TreeMap<Integer, int[]>();
                for (int y = 0; y <= maxY; y++)
                    for (int lz = 0; lz < 16; lz++)
                        for (int lx = 0; lx < 16; lx++) {
                            Object bl = b.chunkGetBlock.invoke(chunk, lx, y, lz);
                            if (oreC != null && oreC.isInstance(bl)) {
                                int meta = (Integer) gm.invoke(chunk, lx, y, lz);
                                int key = (meta % 1000) | (meta >= 16000 ? 1 << 16 : 0);
                                int[] v = h.get(key);
                                if (v == null) h.put(key, v = new int[] { 0, 999, -1 });
                                v[0]++;
                                v[1] = Math.min(v[1], y);
                                v[2] = Math.max(v[2], y);
                            } else if (b.getMaterial.invoke(bl) != b.air && b.names.get(b.id(bl)).toLowerCase().contains("draconium")) {
                                if (drac.length() > 0) drac.append(',');
                                drac.append('[').append(cx * 16 + lx).append(',').append(y).append(',').append(cz * 16 + lz).append(']');
                            }
                        }
                if (h.isEmpty()) continue;
                StringBuilder o = new StringBuilder();
                for (Map.Entry<Integer, int[]> e : h.entrySet()) {
                    int[] v = e.getValue();
                    if (o.length() > 0) o.append(',');
                    o.append("[" + (e.getKey() & 0xffff) + "," + (e.getKey() >> 16) + "," + v[0] + "," + v[1] + "," + v[2] + "]");
                }
                j.raw("{\"cx\":" + cx + ",\"cz\":" + cz + ",\"ores\":[" + o + "]}");
            }
        j.close(']');
        j.key("draconium").raw("[" + drac + "]");
    }

    void checks214(Rwg2Dump.Json j, Object end, Blocks b, Loader ld, int[] freshStart, Method chunkAt) throws Exception {
        j.key("format").str("scan rows: per column \"y:len:blockIndex\" runs (blocks = endGenerated.blocks); gtChunks: [materialId, small, count, minY, maxY]; draconium: [x, y, z]");
        // FML world generators in run order (each gets its own reseeded Random)
        try {
            Class<?> gr = cls("cpw.mods.fml.common.registry.GameRegistry");
            Field sl = gr.getDeclaredField("sortedGeneratorList");
            sl.setAccessible(true);
            Object l = sl.get(null);
            j.key("fmlGenerators").open('[');
            if (l != null) for (Object g : (java.util.Collection<?>) l) j.str(g.getClass().getName());
            j.close(']');
        } catch (Throwable t) {
            j.key("fmlGeneratorsError").str(err(t));
        }
        try {
            Class<?> wg = cls("com.brandon3055.draconicevolution.common.world.DraconicWorldGenerator");
            j.key("deGen").str("chaos=" + wg.getField("chaosIslandsEnabled").get(null) + " comets=" + wg.getField("cometsEnabled").get(null)
                    + " endOre=" + wg.getField("oreEnabledInEnd").get(null));
        } catch (Throwable t) {
            j.key("deGenError").str(err(t));
        }
        // main island: chunks [-12, 11]^2 force-populated (population needs the +1 neighbours)
        try {
            int fresh = ld.area(-12 * 16, -12 * 16, 12 * 16 + 15, 12 * 16 + 15);
            j.key("main").open('{');
            j.key("freshChunks").val(fresh);
            oresRect(j, end, b, chunkAt, -12, -12, 11, 11, 90);
            j.close('}');
        } catch (Throwable t) {
            j.key("mainError").str(err(t));
        }
        // the fresh island probeFresh generated: full block scan + ores
        if (freshStart != null)
            try {
                int sx = freshStart[0], sz = freshStart[1];
                j.key("island").open('{');
                j.key("startChunk").raw("[" + sx + "," + sz + "]");
                oresRect(j, end, b, chunkAt, sx, sz, sx + 13, sz + 13, 255);
                j.key("scan").open('{');
                scanRuns(j, end, b, chunkAt, sx * 16 + 8, sz * 16 + 8, 208, 208);
                j.close('}');
                j.close('}');
            } catch (Throwable t) {
                j.key("islandError").str(err(t));
            }
        // the nearest tower whose area is not generated yet
        try {
            Object towerGen = fieldByTypeNameEnd(worldGenerator(end), "MapGenTower");
            Field wo = Rwg2Dump.field(cls("net.minecraft.world.gen.MapGenBase"), "worldObj", "field_75039_c");
            wo.set(towerGen, end);
            Method canT = named(towerGen.getClass(), new Class<?>[] { int.class, int.class }, "canSpawnStructureAtCoords", "func_75047_a");
            List<int[]> ts = new ArrayList<int[]>();
            for (int cz = -100; cz <= 100; cz++)
                for (int cx = -100; cx <= 100; cx++)
                    if ((Boolean) canT.invoke(towerGen, cx, cz)) ts.add(new int[] { cx, cz });
            java.util.Collections.sort(ts, new java.util.Comparator<int[]>() {
                public int compare(int[] a, int[] c) {
                    return Long.compare((long) a[0] * a[0] + (long) a[1] * a[1], (long) c[0] * c[0] + (long) c[1] * c[1]);
                }
            });
            j.key("towerCandidates").val(ts.size());
            for (int[] t : ts) {
                boolean any = false;
                for (int dz = -1; dz <= 3 && !any; dz++)
                    for (int dx = -1; dx <= 3 && !any; dx++) any = ld.exists(t[0] + dx, t[1] + dz);
                if (any) continue;
                int mx = t[0] * 16, mz = t[1] * 16;
                j.key("tower").open('{');
                j.key("startChunk").raw("[" + t[0] + "," + t[1] + "]");
                ld.errors.clear();
                j.key("freshChunks").val(ld.area(mx - 40, mz - 40, mx + 72, mz + 72));
                j.key("loadErrors").open('[');
                for (String e : ld.errors) j.str(e);
                j.close(']');
                Class<?> mgs = cls("net.minecraft.world.gen.structure.MapGenStructure");
                Map<?, ?> tm = (Map<?, ?>) Rwg2Dump.field(mgs, "structureMap", "field_75053_d").get(towerGen);
                Object start = tm.get((long) t[0] & 4294967295L | ((long) t[1] & 4294967295L) << 32);
                if (start != null) {
                    j.key("start").open('{');
                    describeStart(j, start);
                    j.close('}');
                }
                oresRect(j, end, b, chunkAt, t[0] - 1, t[1] - 1, t[0] + 2, t[1] + 2, 255);
                j.key("scan").open('{');
                scanRuns(j, end, b, chunkAt, mx - 12, mz - 12, 56, 56);
                j.close('}');
                j.close('}');
                break;
            }
        } catch (Throwable t) {
            j.key("towerError").str(err(t));
        }
    }

    /** Full-depth block census every 4th column of the island area (start·16 + 8, 208²). */
    String islandCensus(Object end, int sx, int sz, Blocks b, Method chunkAt) throws Exception {
        Map<String, Integer> census = new java.util.TreeMap<String, Integer>();
        int x0 = sx * 16 + 8, z0 = sz * 16 + 8;
        for (int x = x0; x < x0 + 208; x += 4)
            for (int z = z0; z < z0 + 208; z += 4) {
                Object chunk = chunkAt.invoke(end, x >> 4, z >> 4);
                for (int y = 0; y < 256; y++) {
                    Object bl = b.chunkGetBlock.invoke(chunk, x & 15, y, z & 15);
                    if (b.getMaterial.invoke(bl) == b.air) continue;
                    String nm = b.names.get(b.id(bl));
                    Integer v = census.get(nm);
                    census.put(nm, v == null ? 1 : v + 1);
                }
            }
        StringBuilder cs = new StringBuilder("{");
        for (Map.Entry<String, Integer> e : census.entrySet()) {
            if (cs.length() > 1) cs.append(',');
            cs.append('"').append(e.getKey()).append("\":").append(e.getValue());
        }
        return cs.append('}').toString();
    }

    void describeStart(Rwg2Dump.Json j, Object start) throws Exception {
        j.key("class").str(start.getClass().getName());
        Class<?> ss = cls("net.minecraft.world.gen.structure.StructureStart");
        Field bbF = Rwg2Dump.field(ss, "boundingBox", "field_75074_b");
        Field compF = Rwg2Dump.field(ss, "components", "field_75075_a");
        j.key("bb").str(String.valueOf(bbF == null ? null : bbF.get(start)));
        Method siz = named(start.getClass(), new Class<?>[0], "isSizeableStructure", "func_75069_d");
        if (siz != null) j.key("sizeable").str(String.valueOf(siz.invoke(start)));
        java.util.Collection<?> comps = compF == null ? null : (java.util.Collection<?>) compF.get(start);
        j.key("components").open('[');
        if (comps != null)
            for (Object c : comps) {
                j.open('{');
                j.key("class").str(c.getClass().getName());
                Class<?> sc = cls("net.minecraft.world.gen.structure.StructureComponent");
                Field cbb = Rwg2Dump.field(sc, "boundingBox", "field_74887_e");
                Field cbm = Rwg2Dump.field(sc, "coordBaseMode", "field_74885_f");
                j.key("bb").str(String.valueOf(cbb == null ? null : cbb.get(c)));
                j.key("coordBaseMode").str(String.valueOf(cbm == null ? null : cbm.get(c)));
                j.key("fields").str(simpleFields(c));
                for (Class<?> k = c.getClass(); k != null; k = k.getSuperclass())
                    for (Field f : k.getDeclaredFields())
                        if (f.getType().getName().endsWith("LargeStructureWorld")) {
                            f.setAccessible(true);
                            Object w = f.get(c);
                            Field ch = w.getClass().getDeclaredField("chunks");
                            ch.setAccessible(true);
                            Object[][] arr = (Object[][]) ch.get(w);
                            int n = 0, setUp = 0, genXZ = 0;
                            StringBuilder centre = new StringBuilder();
                            for (int ax = 0; ax < arr.length; ax++)
                                for (int az = 0; az < arr[ax].length; az++) {
                                    Object lc = arr[ax][az];
                                    if (lc == null) continue;
                                    n++;
                                    Field mn = Rwg2Dump.field(lc.getClass(), "minBlockY");
                                    Field mx = Rwg2Dump.field(lc.getClass(), "maxBlockY");
                                    Field ag = Rwg2Dump.field(lc.getClass(), "alreadyGeneratedXZ");
                                    int a = mn == null ? -1 : mn.getInt(lc), m = mx == null ? -1 : mx.getInt(lc);
                                    if (m > 0) setUp++;
                                    Object g = ag == null ? null : ag.get(lc);
                                    int gs = 0;
                                    try {
                                        gs = (Integer) g.getClass().getMethod("size").invoke(g);
                                    } catch (Throwable ignored) {}
                                    genXZ += gs;
                                    if (ax == 6 && az == 6) centre.append("minY ").append(a).append(" maxY ").append(m).append(" genXZ ").append(gs);
                                }
                            j.key("largeWorld").str(n + " chunks, " + setUp + " with blocks, genXZ total " + genXZ + "; centre " + centre);
                        }
                j.close('}');
            }
        j.close(']');
    }

    static String simpleFields(Object o) {
        StringBuilder sb = new StringBuilder();
        for (Class<?> k = o.getClass(); k != null && !k.getName().startsWith("java."); k = k.getSuperclass())
            for (Field f : k.getDeclaredFields()) {
                if (Modifier.isStatic(f.getModifiers())) continue;
                Class<?> t = f.getType();
                if (!(t.isPrimitive() || t == String.class || Number.class.isAssignableFrom(t) || t == Boolean.class || t.isEnum()
                        || t.getName().contains("IslandBiome") || t.getName().contains("BiomeData"))) continue;
                try {
                    f.setAccessible(true);
                    sb.append(k.getSimpleName()).append('.').append(f.getName()).append('=').append(f.get(o)).append("; ");
                } catch (Throwable ignored) {}
            }
        return sb.toString();
    }

    static Object fieldByTypeNameEnd(Object o, String suffix) throws Exception {
        for (Class<?> k = o.getClass(); k != null && k != Object.class; k = k.getSuperclass())
            for (Field f : k.getDeclaredFields())
                if (!Modifier.isStatic(f.getModifiers()) && f.getType().getSimpleName().endsWith(suffix)) {
                    f.setAccessible(true);
                    return f.get(o);
                }
        // field declared as a supertype (e.g. MapGenStructure): match the instance class
        for (Class<?> k = o.getClass(); k != null && k != Object.class; k = k.getSuperclass())
            for (Field f : k.getDeclaredFields())
                if (!Modifier.isStatic(f.getModifiers()) && !f.getType().isPrimitive()) {
                    f.setAccessible(true);
                    Object v = f.get(o);
                    if (v != null && v.getClass().getSimpleName().endsWith(suffix)) return v;
                }
        return null;
    }

    /** MapGenBase.func_151539_a(provider, world, cx, cz, null) over +-r chunks (17x17 per call), like Structures.scan. */
    void scanStarts(Object provider, Object world, Object mapGen, int r) throws Exception {
        Class<?> icp = cls("net.minecraft.world.chunk.IChunkProvider");
        Method gen = null;
        for (Class<?> k = mapGen.getClass(); k != null && gen == null; k = k.getSuperclass())
            for (Method m : k.getDeclaredMethods()) {
                Class<?>[] p = m.getParameterTypes();
                if (p.length == 5 && p[0] == icp && p[2] == int.class && p[3] == int.class && p[4].isArray()
                        && m.getReturnType() == void.class) {
                    m.setAccessible(true);
                    gen = m;
                    break;
                }
            }
        if (gen == null) throw new IllegalStateException("MapGenBase generate(provider, world, x, z, blocks) not found");
        for (int cz = -r; cz <= r; cz += 17)
            for (int cx = -r; cx <= r; cx += 17) gen.invoke(mapGen, provider, world, cx, cz, null);
    }

    /** structureMap entries: chunk, start class, bounding box, pieces (class + box). */
    void writeStartsGeneric(Rwg2Dump.Json j, Object mg) throws Exception {
        Map<?, ?> map = null;
        for (Class<?> k = mg.getClass(); k != null && map == null; k = k.getSuperclass())
            for (Field f : k.getDeclaredFields())
                if (!Modifier.isStatic(f.getModifiers()) && Map.class.isAssignableFrom(f.getType())) {
                    f.setAccessible(true);
                    map = (Map<?, ?>) f.get(mg);
                    break;
                }
        j.key("structures").open('[');
        if (map != null) for (Map.Entry<?, ?> e : new ArrayList<Map.Entry<?, ?>>((Collection) map.entrySet())) {
            long key = (Long) e.getKey();
            Object start = e.getValue();
            j.open('{');
            j.key("chunkX").val((int) key);
            j.key("chunkZ").val((int) (key >>> 32));
            j.key("class").str(start == null ? null : start.getClass().getName());
            if (start != null) {
                Object bb = Structures.fieldByTypeName(start, "StructureBoundingBox");
                if (bb != null) {
                    int[] b = Structures.ints(bb, 6);
                    j.key("box").raw("[" + b[0] + "," + b[1] + "," + b[2] + "," + b[3] + "," + b[4] + "," + b[5] + "]");
                }
                Object comps = Structures.fieldOfType(start, java.util.LinkedList.class);
                if (comps instanceof Collection) {
                    j.key("pieces").open('[');
                    for (Object piece : (Collection<?>) comps) {
                        Object pb = Structures.fieldByTypeName(piece, "StructureBoundingBox");
                        String box = "";
                        if (pb != null) {
                            int[] b = Structures.ints(pb, 6);
                            box = "," + b[0] + "," + b[1] + "," + b[2] + "," + b[3] + "," + b[4] + "," + b[5];
                        }
                        j.raw("[\"" + piece.getClass().getSimpleName() + "\"" + box + "]");
                    }
                    j.close(']');
                }
            }
            j.close('}');
        }
        j.close(']');
    }

    /** gtScan() for any loaded WorldServer: force-populate [-r, r]^2 and list GT ore blocks per chunk. */
    /**
     * 2.9: real terrain of a dimension for the Nether / Twilight Forest terrain ports. Pre-population columns from the
     * world's own generator (`provideChunk`, like worldChunks) and post-population air masks + Thaumcraft node tiles
     * from the loaded chunks (call after gtScanWorld loaded them). Chunks [-r, r]^2.
     */
    String dimTerrain(Object world, int dim, int r) {
        Rwg2Dump.Json j = new Rwg2Dump.Json();
        j.open('{');
        try {
            Blocks b = new Blocks();
            Object gen = worldGenerator(world);
            Method provide = null;
            for (String n : new String[] { "provideChunk", "func_73154_d" })
                try {
                    provide = gen.getClass().getMethod(n, int.class, int.class);
                    break;
                } catch (NoSuchMethodException ignored) {}
            Method getChunk = named(cls("net.minecraft.world.World"), new Class<?>[] { int.class, int.class }, "getChunkFromChunkCoords", "func_72964_e");
            int w = 2 * r + 1, n = w * w * 256;
            ByteBuffer preTop = Rwg2Dump.le(n), preTopBlk = Rwg2Dump.le(n * 2), preSolid = Rwg2Dump.le(n * 32),
                    postTop = Rwg2Dump.le(n), postTopBlk = Rwg2Dump.le(n * 2), postAir = Rwg2Dump.le(n * 32);
            long t0 = System.currentTimeMillis();
            for (int pass = 0; pass < 2; pass++)
                for (int cz = -r; cz <= r; cz++)
                    for (int cx = -r; cx <= r; cx++) {
                        Object chunk = pass == 0 ? provide.invoke(gen, cx, cz) : getChunk.invoke(world, cx, cz);
                        for (int lz = 0; lz < 16; lz++)
                            for (int lx = 0; lx < 16; lx++) {
                                byte[] mask = new byte[32];
                                int top = -1;
                                Object topB = null;
                                for (int y = 255; y >= 0; y--) {
                                    Object blk = b.chunkGetBlock.invoke(chunk, lx, y, lz);
                                    Object mat = b.getMaterial.invoke(blk);
                                    boolean isAir = mat == b.air;
                                    if (!isAir && top < 0) {
                                        top = y;
                                        topB = blk;
                                    }
                                    boolean bit = pass == 0 ? !isAir && mat != b.water && mat != b.lava : isAir;
                                    if (bit) mask[y >> 3] |= (byte) (1 << (y & 7));
                                }
                                if (pass == 0) {
                                    preTop.put((byte) top);
                                    preTopBlk.putShort((short) (topB == null ? -1 : b.id(topB)));
                                    preSolid.put(mask);
                                } else {
                                    postTop.put((byte) top);
                                    postTopBlk.putShort((short) (topB == null ? -1 : b.id(topB)));
                                    postAir.put(mask);
                                }
                            }
                    }
            Long seed = new Structures(cl).worldSeed();
            String file = "dimterrain_" + (dim < 0 ? "m" + (-dim) : String.valueOf(dim)) + ".bin";
            Rwg2Dump.writeBuffers(new File(d.out, file), preTop, preTopBlk, preSolid, postTop, postTopBlk, postAir);
            j.key("file").str(file);
            j.key("generator").str(gen.getClass().getName());
            j.key("worldSeed").str(seed == null ? null : Long.toString(seed));
            j.key("chunkMin").val(-r);
            j.key("chunkMax").val(r);
            j.key("layout").str("chunk-major (cz,cx row-major from chunkMin), 256 columns each idx=lz*16+lx, N columns: "
                    + "[u8 preTop (highest non-air, provideChunk = before population) x N][u16 preTopBlock x N]"
                    + "[32 B pre solid mask (non-air, non-water, non-lava), bit y = byte y>>3 bit y&7 x N]"
                    + "[u8 postTop (populated world) x N][u16 postTopBlock x N][32 B post AIR mask (material air) x N]; y = -1 if none");
            j.key("blocks").open('[');
            for (String sname : b.names) j.str(sname);
            j.close(']');
            j.key("millis").val(System.currentTimeMillis() - t0);
            // Thaumcraft node tiles in the loaded chunks of this dim
            try {
                Class<?> aspectC = cls("thaumcraft.api.aspects.Aspect");
                Class<?> tileNodeC = cls("thaumcraft.common.tiles.TileNode");
                Class<?> chunkC = cls("net.minecraft.world.chunk.Chunk");
                Field teMap = null;
                for (Field f : chunkC.getDeclaredFields())
                    if (Map.class.isAssignableFrom(f.getType()) && !Modifier.isStatic(f.getModifiers())) {
                        f.setAccessible(true);
                        teMap = f;
                    }
                Class<?> teC = cls("net.minecraft.tileentity.TileEntity");
                Field fx = Rwg2Dump.field(teC, "xCoord", "field_145851_c"), fy = Rwg2Dump.field(teC, "yCoord", "field_145848_d"),
                        fz = Rwg2Dump.field(teC, "zCoord", "field_145849_e");
                j.key("tcNodes").open('[');
                java.util.TreeMap<String, Integer> other = new java.util.TreeMap<String, Integer>();
                for (int cz = -r; cz <= r; cz++)
                    for (int cx = -r; cx <= r; cx++) {
                        Map<?, ?> m = (Map<?, ?>) teMap.get(getChunk.invoke(world, cx, cz));
                        for (Object te : new ArrayList<Object>(m.values())) {
                            String cn = te.getClass().getName();
                            if (!tileNodeC.isInstance(te)) {
                                if (cn.contains("thaumcraft") || cn.contains("gadomancy")) other.put(cn, other.containsKey(cn) ? other.get(cn) + 1 : 1);
                                continue;
                            }
                            j.open('{');
                            j.key("class").str(cn);
                            j.key("x").val(fx.getInt(te));
                            j.key("y").val(fy.getInt(te));
                            j.key("z").val(fz.getInt(te));
                            j.key("type").str(String.valueOf(tileNodeC.getMethod("getNodeType").invoke(te)));
                            j.key("modifier").str(String.valueOf(tileNodeC.getMethod("getNodeModifier").invoke(te)));
                            Object al = tileNodeC.getMethod("getAspects").invoke(te);
                            StringBuilder sb = new StringBuilder();
                            for (Object a : (Object[]) al.getClass().getMethod("getAspects").invoke(al)) {
                                if (a == null) continue;
                                sb.append(aspectC.getMethod("getTag").invoke(a)).append(':')
                                        .append(al.getClass().getMethod("getAmount", aspectC).invoke(al, a)).append(' ');
                            }
                            j.key("aspects").str(sb.toString().trim());
                            j.close('}');
                        }
                    }
                j.close(']');
                j.key("tcOtherTiles").open('{');
                for (Map.Entry<String, Integer> e : other.entrySet()) j.key(e.getKey()).val(e.getValue());
                j.close('}');
                Class<?> genC = cls("thaumcraft.common.lib.world.ThaumcraftWorldGenerator");
                Field bl = genC.getDeclaredField("dimensionBlacklist");
                bl.setAccessible(true);
                j.key("tcDimBlacklist").str(String.valueOf(bl.get(null)));
            } catch (Throwable t) {
                j.key("tcError").str(err(t));
            }
        } catch (Throwable t) {
            j.key("error").str(err(t));
        }
        j.close('}');
        return j.toString();
    }

    String gtScanWorld(Object world, int r) {
        Rwg2Dump.Json j = new Rwg2Dump.Json();
        j.open('{');
        try {
            Loader loader = new Loader(world);
            long t0 = System.currentTimeMillis();
            j.key("freshChunks").val(loader.area(-r * 16, -r * 16, (r + 2) * 16 - 1, (r + 2) * 16 - 1));
            Class<?> oreC = cls("gregtech.common.blocks.GTBlockOre");
            Class<?> chunkC = cls("net.minecraft.world.chunk.Chunk");
            Method getChunk = named(cls("net.minecraft.world.World"), new Class<?>[] { int.class, int.class }, "getChunkFromChunkCoords", "func_72964_e");
            Method gb = named(chunkC, new Class<?>[] { int.class, int.class, int.class }, "getBlock", "func_150810_a");
            Method gm = named(chunkC, new Class<?>[] { int.class, int.class, int.class }, "getBlockMetadata", "func_76628_c");
            j.key("rangeChunks").val(r);
            j.key("chunks").open('[');
            for (int cz = -r; cz <= r; cz++)
                for (int cx = -r; cx <= r; cx++) {
                    Object chunk = getChunk.invoke(world, cx, cz);
                    java.util.TreeMap<Integer, int[]> h = new java.util.TreeMap<Integer, int[]>();
                    for (int y = 0; y < 256; y++)
                        for (int lz = 0; lz < 16; lz++)
                            for (int lx = 0; lx < 16; lx++) {
                                Object b = gb.invoke(chunk, lx, y, lz);
                                if (!oreC.isInstance(b)) continue;
                                int meta = (Integer) gm.invoke(chunk, lx, y, lz);
                                int key = (meta % 1000) | (meta >= 16000 ? 1 << 16 : 0);
                                int[] v = h.get(key);
                                if (v == null) h.put(key, v = new int[] { 0, 999, -1 });
                                v[0]++;
                                v[1] = Math.min(v[1], y);
                                v[2] = Math.max(v[2], y);
                            }
                    if (h.isEmpty()) continue;
                    j.open('{');
                    j.key("cx").val(cx);
                    j.key("cz").val(cz);
                    j.key("ores").open('[');
                    for (Map.Entry<Integer, int[]> e : h.entrySet()) {
                        int[] v = e.getValue();
                        j.raw("[" + (e.getKey() & 0xffff) + "," + (e.getKey() >> 16) + "," + v[0] + "," + v[1] + "," + v[2] + "]");
                    }
                    j.close(']');
                    j.close('}');
                }
            j.close(']');
            j.key("format").str("ores: [materialId, small(0/1), count, minY, maxY]");
            j.key("millis").val(System.currentTimeMillis() - t0);
        } catch (Throwable t) {
            j.key("error").str(err(t));
        }
        j.close('}');
        return j.toString();
    }


    // =========================================================================================== world chunks

    /** Server thread. Pre-population chunks from the world's own generator. */
    String worldChunks(List<String> notes) {
        Rwg2Dump.Json j = new Rwg2Dump.Json();
        j.open('{');
        try {
            Object world = world();
            Object gen = worldGenerator(world);
            Blocks b = new Blocks();
            Method provide = null;
            for (String n : new String[] { "provideChunk", "func_73154_d" })
                try {
                    provide = gen.getClass().getMethod(n, int.class, int.class);
                    break;
                } catch (NoSuchMethodException ignored) {}
            if (provide == null) throw new IllegalStateException("provideChunk not found");
            Method shortArr = null, byteArr = null;
            for (Method m : b.chunkC.getMethods()) {
                if (m.getParameterTypes().length != 0) continue;
                if (m.getName().equals("getBiomeShortArray") && m.getReturnType() == short[].class) shortArr = m;
                if (m.getReturnType() == byte[].class && (m.getName().equals("getBiomeArray")
                        || m.getName().equals("func_76605_m"))) byteArr = m;
            }
            int half = WORLD_CHUNK_HALF, w = 2 * half;
            int n = w * w * 256;
            ByteBuffer bio = Rwg2Dump.le(n * 2), top = Rwg2Dump.le(n), topBlk = Rwg2Dump.le(n * 2),
                    ground = Rwg2Dump.le(n), loot = Rwg2Dump.le(n);
            for (int cz = -half; cz < half; cz++)
                for (int cx = -half; cx < half; cx++) {
                    Object chunk = provide.invoke(gen, cx, cz);
                    short[] sa = shortArr != null ? (short[]) shortArr.invoke(chunk) : null;
                    byte[] ba = sa == null && byteArr != null ? (byte[]) byteArr.invoke(chunk) : null;
                    for (int lz = 0; lz < 16; lz++)
                        for (int lx = 0; lx < 16; lx++) {
                            int idx = lz * 16 + lx;
                            bio.putShort(sa != null ? sa[idx] : ba != null ? (short) (ba[idx] & 0xff) : (short) -1);
                            int t = -1, g = -1, l = -1;
                            Object tb = null;
                            for (int y = 255; y >= 0 && (t < 0 || g < 0 || l < 0); y--) {
                                Object blk = b.chunkGetBlock.invoke(chunk, lx, y, lz);
                                if (t < 0 && b.getMaterial.invoke(blk) != b.air) {
                                    t = y;
                                    tb = blk;
                                }
                                if (g < 0 && b.rogueGround(blk)) g = y;
                                if (l < 0 && b.lootSurface(blk)) l = y;
                            }
                            top.put((byte) t);
                            topBlk.putShort((short) (tb == null ? -1 : b.id(tb)));
                            ground.put((byte) g);
                            loot.put((byte) l);
                        }
                }
            Long seed = new Structures(cl).worldSeed();
            String file = "worldchunks_" + (seed == null ? "unknown" : Rwg2Dump.seedTag(seed)) + ".bin";
            Rwg2Dump.writeBuffers(new File(d.out, file), bio, top, topBlk, ground, loot);
            j.key("file").str(file);
            j.key("generator").str(gen.getClass().getName());
            j.key("worldSeed").str(seed == null ? null : Long.toString(seed));
            j.key("chunkX0").val(-half);
            j.key("chunkZ0").val(-half);
            j.key("chunksW").val(w);
            j.key("chunksH").val(w);
            j.key("biomeArray").str(shortArr != null ? "short (EndlessIDs)" : byteArr != null ? "byte" : "none");
            j.key("layout").str("chunk-major (cz,cx row-major), 256 columns each idx=lz*16+lx: [u16 chunk biome x N]"
                    + "[u8 topY (highest non-air) x N][u16 topBlock index x N][u8 roguelike validGround y x N]"
                    + "[u8 lootgames validSurface y x N]; y = -1 (0xff) if none. provideChunk = before population.");
            j.key("blocks").open('[');
            for (String s : b.names) j.str(s);
            j.close(']');
        } catch (Throwable t) {
            j.key("error").str(err(t));
            notes.add("worldChunks: " + t);
        }
        j.close('}');
        return j.toString();
    }


    // =========================================================================================== 2.15 block areas

    static final int B215_R = 3; // chunks [c-3, c+3]^2 per area (7x7)

    /**
     * 2.15: full block columns (id + metadata) of a few areas, before population (generator provideChunk) and after
     * (force-populated world chunks). Verifies the surface port's metadata (stained clay, red sand, coarse dirt,
     * podzol, filler layers), the population snow / ice pass, and the Nether / Twilight Forest surface names (the
     * 2.9 dump only kept the top block, which is the bedrock roof in the Nether). Writes blocks215_<area>.bin.gz.
     */
    String blocks215() {
        Rwg2Dump.Json j = new Rwg2Dump.Json();
        j.open('{');
        Blocks b = null;
        try {
            b = new Blocks();
            j.key("layout").str("gzip; chunk-major (cz, cx row-major from c-r), per chunk 256 columns idx=lz*16+lx, per column y 0..255: "
                    + "[pre: u16 block index x 65536 per chunk][pre: u8 meta ...][post: u16 block index ...][post: u8 meta ...]; "
                    + "pre = generator provideChunk (before population), post = loaded world chunk after force-populating [c-r-1, c+r+1]");
            Class<?> dm = cls("net.minecraftforge.common.DimensionManager");
            Object ow = world();
            j.key("worldSeed").str(Long.toString(worldSeedOf(ow)));
            try {
                Object wi = named(cls("net.minecraft.world.World"), new Class<?>[0], "getWorldInfo", "func_72912_H").invoke(ow);
                Object wt = named(wi.getClass(), new Class<?>[0], "getTerrainType", "func_76067_t").invoke(wi);
                j.key("worldType").str(String.valueOf(named(wt.getClass(), new Class<?>[0], "getWorldTypeName", "func_77127_a").invoke(wt)));
            } catch (Throwable t) {
                j.key("worldTypeError").str(err(t));
            }
            int[][] picks = pickAreas215(j, ow);
            // 2.15.2: the area list opens after the header keys (2.15 / 2.15.1 wrote them inside the array)
            j.key("areas").open('[');
            String[] names = { "origin", "cold", "red" };
            for (int i = 0; i < 3; i++)
                if (picks[i] != null) area215(j, ow, 0, names[i], picks[i][0] >> 4, picks[i][1] >> 4, b);
            Object nether = dm.getMethod("getWorld", int.class).invoke(null, -1);
            if (nether != null) area215(j, nether, -1, "nether", 0, 0, b);
            for (int dim : new int[] { 7 }) {
                Object tf = dm.getMethod("getWorld", int.class).invoke(null, dim);
                if (tf != null) area215(j, tf, dim, "tf", 0, 0, b);
            }
            j.close(']');
        } catch (Throwable t) {
            j.key("error").str(err(t));
        }
        if (b != null) {
            j.key("blocks").open('[');
            for (String n : b.names) j.str(n);
            j.close(']');
        }
        j.close('}');
        return j.toString();
    }

    /** Area centres in the overworld: origin, nearest cold spot (5 samples temp < 0.15), nearest RWG2 mesa / canyon / red desert. */
    int[][] pickAreas215(Rwg2Dump.Json j, Object world) throws Exception {
        int[][] r = new int[3][];
        r[0] = new int[] { 0, 0 };
        Method wcm = named(cls("net.minecraft.world.World"), new Class<?>[0], "getWorldChunkManager", "func_72959_q");
        Object cm = wcm.invoke(world);
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
        j.key("chunkManager").str(cm.getClass().getName());
        for (int ring = 0; ring <= 125 && (r[1] == null || r[2] == null); ring++)
            for (int a = -ring; a <= ring; a++)
                for (int k = 0; k < 4; k++) {
                    int gx = k == 0 ? a : k == 1 ? ring : k == 2 ? -a : -ring, gz = k == 0 ? -ring : k == 1 ? a : k == 2 ? ring : -a;
                    if (Math.max(Math.abs(gx), Math.abs(gz)) != ring) continue;
                    int x = gx * 64, z = gz * 64;
                    if (r[1] == null) {
                        boolean cold = true;
                        for (int[] o : new int[][] { { 0, 0 }, { 48, 0 }, { -48, 0 }, { 0, 48 }, { 0, -48 } })
                            if (d.bgbTemp.getFloat(gen.invoke(cm, x + o[0], z + o[1])) >= 0.15f) cold = false;
                        if (cold) r[1] = new int[] { x, z };
                    }
                    if (r[2] == null && d.cmrClass.isInstance(cm)) {
                        // red sand + stained clay painters (RWG2 red/mesa family)
                        String sn = d.getBiomeDataAt.invoke(cm, x, z).getClass().getSimpleName();
                        if (sn.equals("RealisticBiomeMesa") || sn.equals("RealisticBiomeCanyon") || sn.equals("RealisticBiomeMesaPlains")
                                || sn.equals("RealisticBiomeRedDesertMountains")) r[2] = new int[] { x, z };
                    }
                }
        j.key("picked").raw("[" + (r[1] == null ? "null" : "[" + r[1][0] + "," + r[1][1] + "]") + ","
                + (r[2] == null ? "null" : "[" + r[2][0] + "," + r[2][1] + "]") + "]");
        return r;
    }

    void area215(Rwg2Dump.Json j, Object world, int dim, String name, int ccx, int ccz, Blocks b) {
        j.open('{');
        j.key("name").str(name);
        j.key("dim").val(dim);
        int r = B215_R;
        j.key("chunkX0").val(ccx - r);
        j.key("chunkZ0").val(ccz - r);
        j.key("chunks").val(2 * r + 1);
        long t0 = System.currentTimeMillis();
        try {
            Object gen = worldGenerator(world);
            j.key("generator").str(gen.getClass().getName());
            Method provide = null;
            for (String n : new String[] { "provideChunk", "func_73154_d" })
                try {
                    provide = gen.getClass().getMethod(n, int.class, int.class);
                    break;
                } catch (NoSuchMethodException ignored) {}
            Method getChunk = named(cls("net.minecraft.world.World"), new Class<?>[] { int.class, int.class }, "getChunkFromChunkCoords", "func_72964_e");
            Method meta = named(b.chunkC, new Class<?>[] { int.class, int.class, int.class }, "getBlockMetadata", "func_76628_c");
            Loader ld = new Loader(world);
            int fresh = ld.area((ccx - r - 1) * 16, (ccz - r - 1) * 16, (ccx + r + 1) * 16 + 15, (ccz + r + 1) * 16 + 15);
            j.key("freshChunks").val(fresh);
            int w = 2 * r + 1, n = w * w * 65536;
            byte[] out = new byte[n * 6];
            int o = 0;
            for (int pass = 0; pass < 2; pass++) {
                int ido = o, mo = o + n * 2;
                for (int cz = ccz - r; cz <= ccz + r; cz++)
                    for (int cx = ccx - r; cx <= ccx + r; cx++) {
                        Object chunk = pass == 0 ? provide.invoke(gen, cx, cz) : getChunk.invoke(world, cx, cz);
                        for (int lz = 0; lz < 16; lz++)
                            for (int lx = 0; lx < 16; lx++)
                                for (int y = 0; y < 256; y++) {
                                    int id = b.id(b.chunkGetBlock.invoke(chunk, lx, y, lz));
                                    out[ido++] = (byte) id;
                                    out[ido++] = (byte) (id >> 8);
                                    out[mo++] = (byte) (int) (Integer) meta.invoke(chunk, lx, y, lz);
                                }
                    }
                o += n * 3;
            }
            String file = "blocks215_" + name + ".bin.gz";
            java.util.zip.GZIPOutputStream gz = new java.util.zip.GZIPOutputStream(new java.io.FileOutputStream(new File(d.out, file)), 1 << 16);
            try {
                gz.write(out);
            } finally {
                gz.close();
            }
            j.key("file").str(file);
        } catch (Throwable t) {
            j.key("error").str(err(t));
        }
        j.key("millis").val(System.currentTimeMillis() - t0);
        j.close('}');
    }

    // =========================================================================================== forestry

    String forestry() {
        Rwg2Dump.Json j = new Rwg2Dump.Json();
        j.open('{');
        try {
            Class<?> bgb = d.bgbClass;
            Object[] all = null;
            for (Method m : bgb.getDeclaredMethods())
                if (Modifier.isStatic(m.getModifiers()) && m.getParameterTypes().length == 0
                        && m.getReturnType().isArray() && m.getReturnType().getComponentType() == bgb) {
                    m.setAccessible(true);
                    all = (Object[]) m.invoke(null);
                }
            if (all == null) throw new IllegalStateException("getBiomeGenArray not found");
            Method floatTemp = null;
            for (Method m : bgb.getDeclaredMethods())
                if (!Modifier.isStatic(m.getModifiers()) && m.getReturnType() == float.class
                        && java.util.Arrays.equals(m.getParameterTypes(),
                                new Class<?>[] { int.class, int.class, int.class })) {
                    m.setAccessible(true);
                    floatTemp = m;
                }
            Class<?> et = null, eh = null, bh = null;
            Object climateManager = null;
            try {
                et = cls("forestry.api.core.EnumTemperature");
                eh = cls("forestry.api.core.EnumHumidity");
                bh = cls("forestry.api.core.BiomeHelper");
                climateManager = cls("forestry.api.core.ForestryAPI").getField("climateManager").get(null);
            } catch (Throwable t) {
                j.key("forestryError").str(String.valueOf(t));
            }
            j.key("climateManager").str(climateManager == null ? null : climateManager.getClass().getName());
            j.key("floatTemperatureMethod").str(floatTemp == null ? null : floatTemp.getName());
            Class<?> bd = cls("net.minecraftforge.common.BiomeDictionary");
            Method types = bd.getMethod("getTypesForBiome", bgb);
            j.key("biomes").open('[');
            for (Object b : all) {
                if (b == null) continue;
                j.open('{');
                j.key("id").val(d.bgbId.getInt(b));
                j.key("name").str(String.valueOf(d.bgbName.get(b)));
                j.key("class").str(b.getClass().getName());
                float temp = d.bgbTemp.getFloat(b), rain = d.bgbRain.getFloat(b);
                j.key("temperature").raw(Rwg2Dump.floatJson(temp));
                j.key("rainfall").raw(Rwg2Dump.floatJson(rain));
                j.key("dictionary").open('[');
                try {
                    for (Object ty : (Object[]) types.invoke(null, b)) j.str(String.valueOf(ty));
                } catch (Throwable ignored) {}
                j.close(']');
                if (floatTemp != null) {
                    Method mm = b.getClass().getMethod(floatTemp.getName(), int.class, int.class, int.class);
                    j.key("floatTemperatureDeclaredBy").str(mm.getDeclaringClass().getName());
                }
                if (et != null) {
                    try {
                        j.key("forestryTemperature").str(String.valueOf(et.getMethod("getFromBiome", bgb).invoke(null, b)));
                        j.key("forestryHumidity").str(String.valueOf(eh.getMethod("getFromValue", float.class).invoke(null, rain)));
                        j.key("hellish").raw(String.valueOf(bh.getMethod("isBiomeHellish", bgb).invoke(null, b)));
                    } catch (Throwable t) {
                        j.key("forestryError").str(String.valueOf(t));
                    }
                }
                j.close('}');
            }
            j.close(']');
            // Height-dependent temperature samples (vanilla static temperature noise, Random(1234)).
            if (floatTemp != null) {
                Object plains = all[1];
                j.key("temperatureSamples").open('{');
                j.key("biomeId").val(d.bgbId.getInt(plains));
                j.key("note").str("[x, y, z, float bits hex]");
                j.key("samples").open('[');
                Random r = new Random(42);
                int[] ys = { 0, 63, 64, 65, 80, 100, 128, 200, 255 };
                for (int i = 0; i < 600; i++) {
                    int x = r.nextInt(200001) - 100000, z = r.nextInt(200001) - 100000;
                    if (i < 64) {
                        x = (i % 8) * 3 - 12;
                        z = (i / 8) * 5 - 20;
                    }
                    int y = ys[i % ys.length];
                    float v = (Float) floatTemp.invoke(plains, x, y, z);
                    j.raw("[" + x + "," + y + "," + z + ",\"" + Integer.toHexString(Float.floatToRawIntBits(v)) + "\"]");
                }
                j.close(']');
                j.close('}');
            }
        } catch (Throwable t) {
            j.key("error").str(err(t));
        }
        j.close('}');
        return j.toString();
    }

    // =========================================================================================== dungeons

    /** Server thread: loads/generates chunks of the loaded world (throwaway world!). */
    String dungeons() {
        Rwg2Dump.Json j = new Rwg2Dump.Json();
        j.open('{');
        j.key("configFiles").open('{');
        try {
            collectConfigs(new File("config"), j, 0);
        } catch (Throwable t) {
            j.key("error").str(String.valueOf(t));
        }
        j.close('}');
        Object world = null;
        Long seed = null;
        try {
            world = world();
            seed = new Structures(cl).worldSeed();
        } catch (Throwable ignored) {}
        j.key("worldSeed").str(seed == null ? null : Long.toString(seed));
        j.key("lootgames").open('{');
        try {
            lootgames(j, world);
        } catch (Throwable t) {
            j.key("error").str(err(t));
        }
        j.close('}');
        j.key("roguelike").open('{');
        try {
            roguelike(j, world, seed);
        } catch (Throwable t) {
            j.key("error").str(err(t));
        }
        j.close('}');
        j.close('}');
        return j.toString();
    }

    void collectConfigs(File dir, Rwg2Dump.Json j, int depth) {
        File[] fs = dir.listFiles();
        if (fs == null || depth > 3) return;
        for (File f : fs) {
            String n = f.getPath().toLowerCase().replace('\\', '/');
            if (f.isDirectory()) {
                collectConfigs(f, j, depth + 1);
                continue;
            }
            boolean want = n.contains("rogue") || n.contains("lootgames") || n.endsWith("thaumcraft.cfg") || n.contains("forestry/") && n.contains("climate")
                    || n.endsWith("forestry/common.cfg")
                    // 2.9.1: everything tools/pack-tool checks across pack updates
                    || n.endsWith("newvillages/village_gen.cfg") || n.endsWith("defaultworldgenerator.cfg")
                    || n.endsWith("twilightforest.cfg") || n.contains("hodgepodge") && n.endsWith(".cfg")
                    || n.endsWith("gregtech/worldgeneration.cfg") || n.endsWith("gregtech/undergroundfluids.cfg")
                    || n.endsWith("etfuturum/world.cfg") || n.endsWith("rwg2.cfg");
            if (!want || f.length() > 256 * 1024) continue;
            j.key(f.getPath().replace('\\', '/')).str(Rwg2Dump.readIfExists(f));
        }
    }

    void lootgames(Rwg2Dump.Json j, Object world) throws Exception {
        try {
            Object general = cls("ru.timeconqueror.lootgames.common.config.LGConfigs").getField("GENERAL").get(null);
            Object wg = Rwg2Dump.field(general.getClass(), "worldGen").get(general);
            j.key("disableDungeonGen").raw(String.valueOf(Rwg2Dump.field(wg.getClass(), "disableDungeonGen").get(wg)));
            j.key("dimRhombs").str(String.valueOf(Rwg2Dump.field(wg.getClass(), "dimRhombs").get(wg)));
        } catch (Throwable t) {
            j.key("configError").str(String.valueOf(t));
        }
        if (world == null) return;
        Class<?> wgc = cls("eu.usrv.legacylootgames.worldgen.LootGamesWorldGen");
        Object gen = wgc.getConstructor().newInstance();
        Method can = null;
        for (Method m : wgc.getDeclaredMethods())
            if (m.getName().equals("canSpawnInChunk_v3")) {
                m.setAccessible(true);
                can = m;
            }
        Class<?> sgc = cls("eu.usrv.legacylootgames.StructureGenerator");
        Method setSurface = sgc.getDeclaredMethod("setSurfaceLevel", int.class, int.class);
        setSurface.setAccessible(true);
        Field fWorld = sgc.getDeclaredField("_mWorldObj"), fSurface = sgc.getDeclaredField("_mSurface");
        fWorld.setAccessible(true);
        fSurface.setAccessible(true);
        Loader loader = null;
        try {
            loader = new Loader(world);
            j.key("loadChunkOnProvideRequest").str(loader.onRequest());
        } catch (Throwable t) {
            j.key("loaderError").str(String.valueOf(t));
        }
        j.key("rangeChunks").val(LOOT_RANGE);
        j.key("dungeons").open('[');
        for (int cz = -LOOT_RANGE; cz <= LOOT_RANGE; cz++)
            for (int cx = -LOOT_RANGE; cx <= LOOT_RANGE; cx++) {
                if (!(Boolean) can.invoke(gen, cx, cz, world)) continue;
                int x = (cx << 4) + 8, z = (cz << 4) + 8;
                j.open('{');
                j.key("chunkX").val(cx);
                j.key("chunkZ").val(cz);
                j.key("x").val(x);
                j.key("z").val(z);
                if (Math.abs(cx) <= 48 && Math.abs(cz) <= 48) {
                    try {
                        int fresh = loader == null ? -1 : loader.area(x - 26, z - 26, x + 26, z + 26);
                        j.key("freshChunks").val(fresh);
                        Object sg = sgc.getConstructor().newInstance();
                        fWorld.set(sg, world);
                        boolean ok = (Boolean) setSurface.invoke(sg, x, z);
                        j.key("surfaceFound").raw(String.valueOf(ok));
                        j.key("surfaceY").val(fSurface.getInt(sg));
                    } catch (Throwable t) {
                        j.key("surfaceError").str(String.valueOf(t));
                    }
                }
                j.close('}');
            }
        j.close(']');
    }

    void roguelike(Rwg2Dump.Json j, Object world, Long seed) throws Exception {
        Class<?> rc = cls("greymerk.roguelike.config.RogueConfig");
        j.key("config").open('{');
        for (Object e : rc.getEnumConstants()) {
            j.key(String.valueOf(e)).open('{');
            for (String g : new String[] { "getBoolean", "getInt", "getDouble", "getIntList" }) {
                String v;
                try {
                    v = String.valueOf(rc.getMethod(g, rc).invoke(null, e));
                } catch (Throwable ignored) {
                    continue;
                }
                j.key(g).str(v);
            }
            j.close('}');
        }
        j.close('}');
        if (world == null || seed == null) return;
        Class<?> edC = cls("greymerk.roguelike.worldgen.WorldEditor");
        Class<?> iedC = cls("greymerk.roguelike.worldgen.IWorldEditor");
        Object editor = null;
        for (Constructor<?> c : edC.getConstructors())
            if (c.getParameterTypes().length == 1) editor = c.newInstance(world);
        Class<?> dc = cls("greymerk.roguelike.dungeon.Dungeon");
        Object dungeon = dc.getConstructor(iedC).newInstance(editor);
        Method canSpawn = dc.getMethod("canSpawnInChunk", int.class, int.class, iedC);
        Method nearby = dc.getMethod("getNearbyCoord", Random.class, int.class, int.class, int.class, int.class);
        Method valid = dc.getMethod("validLocation", Random.class, int.class, int.class);
        Class<?> coordC = cls("greymerk.roguelike.worldgen.Coord");
        Method gx = coordC.getMethod("getX"), gz = coordC.getMethod("getZ");
        Blocks b = null;
        try {
            b = new Blocks();
        } catch (Throwable ignored) {}
        Loader loader = null;
        try {
            loader = new Loader(world);
            j.key("loadChunkOnProvideRequest").str(loader.onRequest());
        } catch (Throwable t) {
            j.key("loaderError").str(String.valueOf(t));
        }
        // GameRegistry.generateWorld: one Random reseeded to the chunk seed before each IWorldGenerator.
        Random fml = new Random(seed);
        long xSeed = fml.nextLong() >> 2 + 1L, zSeed = fml.nextLong() >> 2 + 1L;
        j.key("rangeChunks").val(ROGUE_RANGE);
        j.key("anchors").open('[');
        for (int cz = -ROGUE_RANGE; cz <= ROGUE_RANGE; cz++)
            for (int cx = -ROGUE_RANGE; cx <= ROGUE_RANGE; cx++) {
                if (!(Boolean) canSpawn.invoke(null, cx, cz, editor)) continue;
                j.open('{');
                j.key("chunkX").val(cx);
                j.key("chunkZ").val(cz);
                if (Math.abs(cx) <= 96 && Math.abs(cz) <= 96) {
                    Random rand = new Random((xSeed * cx + zSeed * cz) ^ seed);
                    int x0 = cx * 16 + 4, z0 = cz * 16 + 4;
                    j.key("attempts").open('[');
                    try {
                        for (int i = 0; i < 50; i++) {
                            Object c = nearby.invoke(null, rand, x0, z0, 40, 100);
                            int x = (Integer) gx.invoke(c), z = (Integer) gz.invoke(c);
                            boolean before = loader != null && loader.exists(x >> 4, z >> 4);
                            boolean asIs = (Boolean) valid.invoke(dungeon, rand, x, z);
                            if (loader != null) loader.area(x - 21, z - 21, x + 21, z + 21);
                            boolean ok = (Boolean) valid.invoke(dungeon, rand, x, z);
                            int gy = -1;
                            if (b != null && ok) {
                                for (int y = 100; y >= 0; y--)
                                    if (b.rogueGround(b.worldGetBlock.invoke(world, x, y, z))) {
                                        gy = y;
                                        break;
                                    }
                            }
                            j.raw("[" + x + "," + z + "," + ok + "," + gy + "," + asIs + "," + before + "]");
                            if (ok) break;
                        }
                    } catch (Throwable t) {
                        j.str("error: " + t);
                    }
                    j.close(']');
                }
                j.close('}');
            }
        j.close(']');
    }
}
