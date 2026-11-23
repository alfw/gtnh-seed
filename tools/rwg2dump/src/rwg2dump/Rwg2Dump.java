package rwg2dump;

import java.io.BufferedReader;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.FileReader;
import java.io.IOException;
import java.io.OutputStreamWriter;
import java.io.PrintWriter;
import java.io.StringWriter;
import java.io.Writer;
import java.lang.reflect.Array;
import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.lang.reflect.InvocationHandler;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.lang.reflect.Proxy;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.channels.FileChannel;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.text.SimpleDateFormat;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Date;
import java.util.IdentityHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.concurrent.atomic.AtomicBoolean;

import cpw.mods.fml.common.Mod;
import cpw.mods.fml.common.event.FMLServerStartingEvent;

/**
 * Server-side golden-data exporter for the GTNH seed viewer.
 *
 * Reflection only: no compile-time dependency on Minecraft or RWG2. In 1.7.10 production, class names are MCP but
 * members are SRG (func_/field_), so the command is registered via a dynamic proxy that dispatches on method
 * <em>signatures</em>, and vanilla members are resolved by trying both MCP and SRG names.
 *
 * Usage (server console): {@code rwg2dump [seed ...]} (numeric seeds, or text seeds hashed like vanilla). The loaded
 * world's seed is always included; v2 also writes structures.json (loaded world only) and landmarks.json.
 */
@Mod(modid = "rwg2dump", name = "RWG2 Dump", version = Rwg2Dump.VERSION, dependencies = "required-after:rwg2",
        acceptableRemoteVersions = "*")
public class Rwg2Dump {

    static final String VERSION = "2.19.2";
    static final int FORMAT = 2;

    static final long[] DEFAULT_SEEDS = { 0L, 1L, -1L, 42L, 123456789L, Long.MIN_VALUE, Long.MAX_VALUE,
            -4172144997902289642L };

    /** name, x0, z0, w, h, step. Sample (i,j) is at (x0 + i*step, z0 + j*step); arrays are row-major in j. */
    static final Object[][] BIOME_REGIONS = { { "near", -256, -256, 512, 512, 1 },
            { "wide", -8192, -8192, 512, 512, 32 }, { "far", 1_000_000 - 2048, -1_000_000 - 2048, 256, 256, 16 }, };
    static final Object[] HEIGHT_REGION = { "heights", -2048, -2048, 128, 128, 32 };
    static final int LANDMARK_RANGE = 12288;
    /** Structure scan radius around the origin, in chunks (loaded world only). */
    static final int STRUCTURE_RANGE_CHUNKS = 96;

    private static final AtomicBoolean RUNNING = new AtomicBoolean();

    @Mod.EventHandler
    public void serverStarting(FMLServerStartingEvent event) {
        try {
            ClassLoader cl = Rwg2Dump.class.getClassLoader();
            Class<?> iCommand = Class.forName("net.minecraft.command.ICommand", false, cl);
            Object cmd = Proxy.newProxyInstance(iCommand.getClassLoader(), new Class<?>[] { iCommand },
                    new CommandHandler());
            Method reg = null;
            for (Method m : event.getClass().getMethods())
                if (m.getName().equals("registerServerCommand") && m.getParameterTypes().length == 1) reg = m;
            if (reg == null) throw new IllegalStateException("registerServerCommand not found");
            reg.invoke(event, cmd);
            log("registered /rwg2dump");
        } catch (Throwable t) {
            log("failed to register command: " + t);
            t.printStackTrace();
        }
    }

    static void log(String s) {
        System.out.println("[rwg2dump] " + s);
    }

    // ------------------------------------------------------------------------------------------------ command

    static final class CommandHandler implements InvocationHandler {

        @Override
        public Object invoke(Object proxy, Method m, Object[] a) throws Throwable {
            int n = a == null ? 0 : a.length;
            Class<?> r = m.getReturnType();
            if (m.getDeclaringClass() == Object.class) {
                if (m.getName().equals("equals")) return proxy == a[0];
                if (m.getName().equals("hashCode")) return System.identityHashCode(proxy);
                return "rwg2dump-command";
            }
            if (r == String.class && n == 0) return "rwg2dump";
            if (r == String.class && n == 1) return "/rwg2dump [seed ...]";
            if (List.class.isAssignableFrom(r) && n == 0) return new ArrayList<Object>();
            if (List.class.isAssignableFrom(r) && n == 2) return null;
            if (r == boolean.class && n == 1) return isAllowed(a[0]);
            if (r == boolean.class && n == 2) return false; // isUsernameIndex
            if (r == int.class && n == 1) return compareTo(a[0]);
            if (r == void.class && n == 2) {
                process(a[0], (String[]) a[1]);
                return null;
            }
            throw new UnsupportedOperationException(m.toString());
        }

        private int compareTo(Object other) {
            try {
                for (Method om : other.getClass().getMethods())
                    if (om.getReturnType() == String.class && om.getParameterTypes().length == 0
                            && !om.getName().equals("toString"))
                        return "rwg2dump".compareTo((String) om.invoke(other));
            } catch (Throwable ignored) {}
            return 0;
        }
    }

    /** Console / RCon always; players only in singleplayer (integrated server). */
    static boolean isAllowed(Object sender) {
        String cls = sender.getClass().getName();
        if (cls.endsWith("DedicatedServer") || cls.endsWith("IntegratedServer") || cls.endsWith("MinecraftServer")
                || cls.endsWith("RConConsoleSource"))
            return true;
        try {
            Class.forName("net.minecraft.client.Minecraft", false, Rwg2Dump.class.getClassLoader());
            return cls.endsWith("EntityPlayerMP");
        } catch (ClassNotFoundException e) {
            return false;
        }
    }

    static void tell(Object sender, String msg) {
        log(msg);
        try {
            Class<?> chat = Class.forName("net.minecraft.util.ChatComponentText", false,
                    Rwg2Dump.class.getClassLoader());
            Object comp = chat.getConstructor(String.class).newInstance("[rwg2dump] " + msg);
            for (Method m : sender.getClass().getMethods()) {
                Class<?>[] p = m.getParameterTypes();
                if (m.getReturnType() == void.class && p.length == 1 && p[0].getName().endsWith("IChatComponent")) {
                    m.invoke(sender, comp);
                    return;
                }
            }
        } catch (Throwable ignored) {}
    }

    static void process(Object sender, String[] args) {
        if (!RUNNING.compareAndSet(false, true)) {
            tell(sender, "a dump is already running");
            return;
        }
        try {
            if (args.length == 1 && args[0].equals("loot")) {
                // v2.17 quick mode: only chestloot.json (seconds; no chunks generated)
                File dir = new File("rwg2dump", new SimpleDateFormat("yyyyMMdd-HHmmss").format(new Date()) + "-loot");
                if (!dir.mkdirs()) throw new IOException("cannot create " + dir);
                writeText(new File(dir, "chestloot.json"), new Extras(Rwg2Dump.class.getClassLoader(), null).chestLoot());
                tell(sender, "rwg2dump " + VERSION + " loot -> " + new File(dir, "chestloot.json").getAbsolutePath());
                RUNNING.set(false);
                return;
            }
            if (args.length == 1 && args[0].equals("probe")) {
                // v2.18 quick probe: listeners, traced ocean populates, mineshaft chest carts; v2.19: biome spawn lists, Nether
                // fortress chests (minutes, server thread)
                File dir = new File("rwg2dump", new SimpleDateFormat("yyyyMMdd-HHmmss").format(new Date()) + "-probe");
                if (!dir.mkdirs()) throw new IOException("cannot create " + dir);
                tell(sender, "rwg2dump " + VERSION + " probe: the server freezes for a few minutes, check the log");
                Extras ex = new Extras(Rwg2Dump.class.getClassLoader(), null);
                writeText(new File(dir, "probe219.json"), new Probe218(ex, Rwg2Dump.class.getClassLoader()).run());
                writeText(new File(dir, "chestloot.json"), ex.chestLoot());
                zipDump(dir);
                tell(sender, "rwg2dump " + VERSION + " probe -> " + dir.getAbsolutePath() + " (+ .zip)");
                log("DONE -> " + dir.getAbsolutePath());
                RUNNING.set(false);
                return;
            }
            LinkedHashSet<Long> seeds = new LinkedHashSet<Long>();
            for (String s : args) {
                try {
                    seeds.add(Long.parseLong(s));
                } catch (NumberFormatException e) {
                    seeds.add((long) s.hashCode()); // vanilla text-seed behaviour
                }
            }
            Long world = new Structures(Rwg2Dump.class.getClassLoader()).worldSeed();
            if (world != null) seeds.add(world); // the loaded world first: structures.json is for this seed
            for (long s : DEFAULT_SEEDS) seeds.add(s);

            final Dumper d = new Dumper(new ArrayList<Long>(seeds));
            tell(sender, "rwg2dump " + VERSION + ": server-thread part (structures, world chunks, dungeons) - the "
                    + "server will freeze for a while, check the log");
            d.prepareOnServerThread(); // touches world saved data; must be on the server thread
            tell(sender, "dumping " + seeds.size() + " seeds x 2 world types to " + d.out.getAbsolutePath()
                    + " (runs in background; progress in server log)");
            Thread t = new Thread(new Runnable() {

                @Override
                public void run() {
                    try {
                        d.run();
                        log("DONE -> " + d.out.getAbsolutePath());
                        zipDump(d.out);
                    } catch (Throwable t) {
                        log("FAILED: " + t);
                        d.writeError(t);
                        zipDump(d.out); // partial results are still useful
                    } finally {
                        RUNNING.set(false);
                    }
                }
            }, "rwg2dump");
            t.setDaemon(true);
            t.setPriority(Thread.MIN_PRIORITY);
            t.start();
        } catch (Throwable t) {
            RUNNING.set(false);
            tell(sender, "failed: " + t);
            t.printStackTrace();
        }
    }

    // ------------------------------------------------------------------------------------------------ dumper

    static final class Dumper {

        final List<Long> seeds;
        final File out;
        final ClassLoader cl = Rwg2Dump.class.getClassLoader();

        Class<?> cmrClass, rbbClass, bgbClass, terrainClass;
        String structuresJson = null, worldChunksJson = null, forestryJson = null, dungeonsJson = null, thaumcraftJson = null, gregtechJson = null, gtScanJson = null, spawnJson = null, gtFluidsJson = null, villagesJson = null, tcNodesJson = null, dimsJson = null, dungeonLootJson = null, chestLootJson = null, blocks215Json = null, deco216Json = null, postpop216Json = null, gregtech216Json = null, nether216Json = null;
        Pass216 p216;
        final List<String> notes = new ArrayList<String>();
        Extras extras;
        Constructor<?> cmrCtor;
        Method getBiomeDataAt, getBiomeGenAt, getMetaBiomeAt, getContinentValue, getNoiseAt, getConfiguredBiomes;
        Field rbbId, rbbSub, rbbBase, rbbBeach, rbbRiver;
        Field bgbId, bgbName, bgbColor, bgbTemp, bgbRain;
        String noiseImpl;
        final IdentityHashMap<Object, Integer> genIdCache = new IdentityHashMap<Object, Integer>();

        Dumper(List<Long> seeds) {
            this.seeds = seeds;
            String ts = new SimpleDateFormat("yyyyMMdd-HHmmss").format(new Date());
            out = new File("rwg2dump", ts);
        }

        void prepareOnServerThread() throws Exception {
            if (!out.mkdirs()) throw new IOException("cannot create " + out);
            cmrClass = Class.forName("rwg2.world.ChunkManagerRealistic", true, cl);
            rbbClass = Class.forName("rwg2.biomes.realistic.RealisticBiomeBase", true, cl);
            bgbClass = Class.forName("net.minecraft.world.biome.BiomeGenBase", true, cl);
            cmrCtor = cmrClass.getConstructor(long.class, boolean.class);
            try {
                terrainClass = Class.forName("rwg2.terrain.TerrainBase", true, cl);
            } catch (ClassNotFoundException e) {
                log("WARN: rwg2.terrain.TerrainBase not found");
            }

            getBiomeDataAt = declared(cmrClass, "getBiomeDataAt", int.class, int.class);
            getMetaBiomeAt = declared(cmrClass, "getMetaBiomeAt", int.class, int.class);
            getContinentValue = declared(cmrClass, "getContinentValue", int.class, int.class);
            getNoiseAt = declared(cmrClass, "getNoiseAt", int.class, int.class);
            getConfiguredBiomes = declared(cmrClass, "getConfiguredBiomes");
            // Overrides WorldChunkManager.getBiomeGenAt(II) -> SRG-renamed in production; match by signature.
            for (Method m : cmrClass.getDeclaredMethods()) {
                Class<?>[] p = m.getParameterTypes();
                if (p.length == 2 && p[0] == int.class && p[1] == int.class
                        && bgbClass.isAssignableFrom(m.getReturnType())) {
                    m.setAccessible(true);
                    getBiomeGenAt = m;
                }
            }
            if (getBiomeGenAt == null) throw new IllegalStateException("getBiomeGenAt(II) not found");

            rbbId = field(rbbClass, "biomeID");
            rbbSub = field(rbbClass, "subID");
            rbbBase = field(rbbClass, "baseBiome");
            rbbBeach = field(rbbClass, "beachBiome");
            rbbRiver = field(rbbClass, "riverBiome");
            bgbId = field(bgbClass, "biomeID", "field_76756_M");
            bgbName = field(bgbClass, "biomeName", "field_76791_y");
            bgbColor = field(bgbClass, "color", "field_76790_z");
            bgbTemp = field(bgbClass, "temperature", "field_76750_F");
            bgbRain = field(bgbClass, "rainfall", "field_76751_G");

            // Same call RWG2 makes when constructing the world's own ChunkManagerRealistic.
            Class<?> wsd = Class.forName("rwg2.world.RwgWorldSavedData", true, cl);
            noiseImpl = String.valueOf(wsd.getMethod("getNoiseImplementation").invoke(null));

            // v2: structure ground truth for the *loaded* world. Touches the world's structure generators and
            // structure data, so it runs here on the server thread. Never fatal for the rest of the dump.
            try {
                structuresJson = new Structures(cl).dump();
            } catch (Throwable t) {
                StringWriter sw = new StringWriter();
                t.printStackTrace(new PrintWriter(sw));
                structuresJson = "{\"error\":" + new Json().str(sw.toString()) + "}";
                log("structures failed: " + t);
            }
            // v2.1 (each section is self-contained and never throws)
            extras = new Extras(cl, this);
            long t0 = System.currentTimeMillis();
            // v2.16 first: its areas must be generated in a fresh world (and its event hooks see only them)
            try {
                p216 = new Pass216(extras, this, cl);
                log("v2.16: decoration areas (populate order, post-population blocks; several minutes)");
                deco216Json = p216.deco();
                log("v2.16: post-population checks (Roguelike, LootGames, spawn walk)");
                postpop216Json = p216.postpop();
                log("v2.16: GT vein cache after the decoration areas");
                gregtech216Json = extras.gregtech();
            } catch (Throwable t) {
                deco216Json = "{\"error\":" + new Json().str(String.valueOf(t)) + "}";
            }
            log("v2.1: world chunks");
            worldChunksJson = extras.worldChunks(notes);
            log("v2.1: forestry");
            forestryJson = extras.forestry();
            log("v2.1: dungeons (generates chunks)");
            dungeonsJson = extras.dungeons();
            log("v2.3: thaumcraft (generates ~2500 chunks, takes a few minutes)");
            thaumcraftJson = extras.thaumcraft();
            log("v2.4: gregtech ore veins");
            gregtechJson = extras.gregtech();
            log("v2.5: gregtech ore block scan");
            gtScanJson = extras.gtScan();
            log("v2.6: spawn walk + GT underground fluids");
            spawnJson = extras.spawn();
            gtFluidsJson = extras.gtFluids();
            log("v2.8: villages, Thaumcraft nodes, Nether + Twilight Forest (inits dims -1 and 7)");
            try {
                Object w = extras.world();
                villagesJson = extras.villages(extras.worldGenerator(w), w, extras.worldSeedOf(w));
            } catch (Throwable t) {
                villagesJson = "{\"error\":\"" + String.valueOf(t).replace("\"", "'") + "\"}";
            }
            tcNodesJson = extras.thaumcraftNodes();
            long[] sa = new long[seeds.size()];
            for (int i = 0; i < sa.length; i++) sa[i] = seeds.get(i);
            dimsJson = extras.dims(sa);
            if (p216 != null) nether216Json = p216.nether();
            log("v2.15: block areas (origin, cold, red, Nether, TF; pre + post population, gzip)");
            blocks215Json = extras.blocks215();
            log("v2.11: dungeon chest loot table + draw probes");
            dungeonLootJson = extras.dungeonLoot();
            log("v2.17: all ChestGenHooks tables + structure map classes");
            chestLootJson = extras.chestLoot();
            log(String.format("v2.1 server-thread part took %.1fs", (System.currentTimeMillis() - t0) / 1000.0));
        }

        void run() throws Exception {
            long t0 = System.currentTimeMillis();
            writeEnv();
            Object probe = cmrCtor.newInstance(0L, true);
            writeBiomes(probe);
            if (structuresJson != null) writeText(new File(out, "structures.json"), structuresJson);
            if (worldChunksJson != null) writeText(new File(out, "worldchunks.json"), worldChunksJson);
            if (forestryJson != null) writeText(new File(out, "forestry.json"), forestryJson);
            if (dungeonsJson != null) writeText(new File(out, "dungeons.json"), dungeonsJson);
            if (thaumcraftJson != null) writeText(new File(out, "thaumcraft.json"), thaumcraftJson);
            if (gregtechJson != null) writeText(new File(out, "gregtech.json"), gregtechJson);
            if (gtScanJson != null) writeText(new File(out, "gtores.json"), gtScanJson);
            if (spawnJson != null) writeText(new File(out, "spawn.json"), spawnJson);
            if (gtFluidsJson != null) writeText(new File(out, "gtfluids.json"), gtFluidsJson);
            if (villagesJson != null) writeText(new File(out, "villages.json"), villagesJson);
            if (tcNodesJson != null) writeText(new File(out, "tcnodes.json"), tcNodesJson);
            if (dimsJson != null) writeText(new File(out, "dims.json"), dimsJson);
            if (dungeonLootJson != null) writeText(new File(out, "dungeonloot.json"), dungeonLootJson);
            if (chestLootJson != null) writeText(new File(out, "chestloot.json"), chestLootJson);
            if (blocks215Json != null) writeText(new File(out, "blocks215.json"), blocks215Json);
            if (deco216Json != null) writeText(new File(out, "deco216.json"), deco216Json);
            if (postpop216Json != null) writeText(new File(out, "postpop216.json"), postpop216Json);
            if (nether216Json != null) writeText(new File(out, "nether216.json"), nether216Json);
            if (gregtech216Json != null) writeText(new File(out, "gregtech216.json"), gregtech216Json);
            int chunkSeeds = 0;
            StringBuilder landmarks = new StringBuilder("{\"format\":" + FORMAT + ",\"seeds\":[");

            int total = seeds.size() * 2, done = 0;
            StringBuilder files = new StringBuilder();
            for (long seed : seeds) {
                for (boolean continental : new boolean[] { false, true }) {
                    long ps = System.currentTimeMillis();
                    Object cmr = cmrCtor.newInstance(seed, continental);
                    String type = continental ? "RWG2_CONTINENTS" : "RWG2";
                    for (Object[] r : BIOME_REGIONS) files.append(biomeGrid(cmr, seed, type, r));
                    files.append(heightGrid(cmr, seed, type, HEIGHT_REGION));
                    if (chunkSeeds < 3) files.append(extras.chunkNoise(cmr, seed, continental, notes));
                    if (continental) {
                        try {
                            landmarks.append(landmarks(cmr, seed)).append(',');
                        } catch (Throwable t) {
                            log("landmarks failed for seed " + seed + ": " + t);
                        }
                    }
                    done++;
                    if (continental) chunkSeeds++;
                    long el = System.currentTimeMillis() - t0;
                    log(String.format("%d/%d seed=%d %s %.1fs (eta %.0fs)", done, total, seed, type,
                            (System.currentTimeMillis() - ps) / 1000.0, el / 1000.0 / done * (total - done)));
                }
            }
            if (landmarks.charAt(landmarks.length() - 1) == ',') landmarks.setLength(landmarks.length() - 1);
            writeText(new File(out, "landmarks.json"), landmarks.append("]}").toString());
            extras.restoreWorldStatics(new Structures(cl).worldSeed());
            if (!notes.isEmpty()) {
                StringBuilder sb = new StringBuilder();
                for (String n : notes) sb.append(n).append("\n\n");
                writeText(new File(out, "notes.txt"), sb.toString());
            }
            writeManifest(files.toString(), System.currentTimeMillis() - t0);
        }

        // ---------------------------------------------------------------- grids

        String biomeGrid(Object cmr, long seed, String type, Object[] r) throws Exception {
            String name = (String) r[0];
            int x0 = (Integer) r[1], z0 = (Integer) r[2], w = (Integer) r[3], h = (Integer) r[4],
                    step = (Integer) r[5];
            int n = w * h;
            ByteBuffer rwg = le(n * 2), gen = le(n * 2), meta = le(n), cont = le(n * 4);
            for (int j = 0; j < h; j++) {
                int z = z0 + j * step;
                for (int i = 0; i < w; i++) {
                    int x = x0 + i * step;
                    Object data = getBiomeDataAt.invoke(cmr, x, z);
                    rwg.putShort((short) rbbId.getInt(data));
                    Object g = getBiomeGenAt.invoke(cmr, x, z);
                    gen.putShort((short) Math.min(0xffff, genId(g)));
                    meta.put((byte) ((Integer) getMetaBiomeAt.invoke(cmr, x, z)).intValue());
                    cont.putFloat((Float) getContinentValue.invoke(cmr, x, z));
                }
            }
            String file = String.format("grid_%s_%s_%s.bin", seedTag(seed), type, name);
            writeBuffers(new File(out, file), rwg, gen, meta, cont);
            return manifestEntry(file, seed, type, name, x0, z0, w, h, step,
                    "[u16 rwgId x N][u16 genBiomeId x N][u8 metaBiome x N][f32 continentValue x N]");
        }

        /**
         * Every volcano / lava-cave landmark (raw, before the height gates) within +-LANDMARK_RANGE of the origin,
         * with the heights the gates use. Uses the manager's private ContinentalNoise (landmass offsets are 0).
         */
        String landmarks(Object cmr, long seed) throws Exception {
            Field cf = cmrClass.getDeclaredField("continents");
            cf.setAccessible(true);
            Object cont = cf.get(cmr);
            Class<?> cc = cont.getClass();
            Method vKey = cc.getMethod("getVolcanoSeedKey", int.class, int.class);
            Method vCenter = cc.getMethod("getVolcanoCenterCoordinates", int.class, int.class);
            Method vIsland = cc.getMethod("isIslandVolcano", int.class, int.class);
            Method lKey = cc.getMethod("getLavaCaveSeedKey", int.class, int.class);
            Method lCenter = cc.getMethod("getLavaCaveCenterCoordinates", int.class, int.class);
            Method mVolcano = cmrClass.getMethod("getVolcanoCoordinates", int.class, int.class);
            Method mBase = cmrClass.getMethod("getVolcanoBaseHeight", int.class, int.class);
            Method mLava = cmrClass.getMethod("getLavaCaveCoordinates", int.class, int.class);
            java.util.HashSet<Long> seen = new java.util.HashSet<Long>();
            Json j = new Json();
            j.open('{');
            j.key("seed").str(Long.toString(seed));
            j.key("volcanoes").open('[');
            for (int z = -LANDMARK_RANGE; z <= LANDMARK_RANGE; z += 128)
                for (int x = -LANDMARK_RANGE; x <= LANDMARK_RANGE; x += 128) {
                    long key = (Long) vKey.invoke(cont, x, z);
                    if (key == Long.MIN_VALUE || !seen.add(key)) continue;
                    long c = (Long) vCenter.invoke(cont, x, z);
                    int cx = (int) (c >> 32), cz = (int) c;
                    j.open('{');
                    j.key("key").str(Long.toString(key));
                    j.key("x").val(cx);
                    j.key("z").val(cz);
                    j.key("island").raw(String.valueOf(vIsland.invoke(cont, x, z)));
                    j.key("baseHeight").raw(floatJson((Float) mBase.invoke(cmr, x, z)));
                    j.key("eligible").raw(String.valueOf((Long) mVolcano.invoke(cmr, cx, cz) != Long.MIN_VALUE));
                    j.key("noiseAtCenter").raw(floatJson((Float) getNoiseAt.invoke(cmr, cx, cz)));
                    j.close('}');
                }
            j.close(']');
            seen.clear();
            j.key("lavaCaves").open('[');
            for (int z = -LANDMARK_RANGE; z <= LANDMARK_RANGE; z += 128)
                for (int x = -LANDMARK_RANGE; x <= LANDMARK_RANGE; x += 128) {
                    long key = (Long) lKey.invoke(cont, x, z);
                    if (key == Long.MIN_VALUE || !seen.add(key)) continue;
                    long c = (Long) lCenter.invoke(cont, x, z);
                    int cx = (int) (c >> 32), cz = (int) c;
                    j.open('{');
                    j.key("key").str(Long.toString(key));
                    j.key("x").val(cx);
                    j.key("z").val(cz);
                    j.key("noiseAtCenter").raw(floatJson((Float) getNoiseAt.invoke(cmr, cx, cz)));
                    j.key("eligible").raw(String.valueOf((Long) mLava.invoke(cmr, x, z) != Long.MIN_VALUE));
                    j.key("genIdAtCenter").val(genId(getBiomeGenAt.invoke(cmr, cx, cz)));
                    j.close('}');
                }
            j.close(']');
            j.close('}');
            return j.toString();
        }

        String heightGrid(Object cmr, long seed, String type, Object[] r) throws Exception {
            String name = (String) r[0];
            int x0 = (Integer) r[1], z0 = (Integer) r[2], w = (Integer) r[3], h = (Integer) r[4],
                    step = (Integer) r[5];
            ByteBuffer hb = le(w * h * 4);
            for (int j = 0; j < h; j++)
                for (int i = 0; i < w; i++)
                    hb.putFloat((Float) getNoiseAt.invoke(cmr, x0 + i * step, z0 + j * step));
            String file = String.format("grid_%s_%s_%s.bin", seedTag(seed), type, name);
            writeBuffers(new File(out, file), hb);
            return manifestEntry(file, seed, type, name, x0, z0, w, h, step, "[f32 getNoiseAt x N]");
        }

        int genId(Object biome) throws Exception {
            if (biome == null) return 0xffff;
            Integer id = genIdCache.get(biome);
            if (id == null) {
                id = bgbId != null ? bgbId.getInt(biome) : 0xfffe;
                genIdCache.put(biome, id);
            }
            return id;
        }

        // ---------------------------------------------------------------- biome tables

        void writeBiomes(Object cmr) throws Exception {
            Json j = new Json();
            j.open('{');
            j.key("format").val(FORMAT);

            // Every registered RealisticBiomeBase, by RWG id.
            j.key("realisticBiomes").open('[');
            Field listF = rbbClass.getDeclaredField("biomeList");
            listF.setAccessible(true);
            Object[] all = (Object[]) listF.get(null);
            for (Object b : all) if (b != null) writeRbb(j, b);
            j.close(']');

            // Candidate lists exactly as the provider holds them (order matters: selection indexes into them).
            j.key("providerLists").open('{');
            for (Field f : cmrClass.getDeclaredFields()) {
                if (Modifier.isStatic(f.getModifiers())) continue;
                f.setAccessible(true);
                Object v = f.get(cmr);
                if (v instanceof List) {
                    j.key(f.getName());
                    ids(j, (List<?>) v);
                } else if (v != null && v.getClass().isArray() && List.class.isAssignableFrom(
                        v.getClass().getComponentType())) {
                    j.key(f.getName()).open('[');
                    for (int i = 0; i < Array.getLength(v); i++) ids(j, (List<?>) Array.get(v, i));
                    j.close(']');
                } else if (v instanceof Boolean || v instanceof Number) {
                    j.key(f.getName()).raw(String.valueOf(v));
                }
            }
            j.close('}');

            j.key("configuredBiomes");
            ids(j, (List<?>) getConfiguredBiomes.invoke(cmr));

            // Support statics: single biomes (oceans, volcano island, lava-cave marker).
            Class<?> support = Class.forName("rwg2.support.Support", true, cl);
            j.key("supportStatics").open('{');
            for (Field f : support.getDeclaredFields()) {
                if (!Modifier.isStatic(f.getModifiers())) continue;
                f.setAccessible(true);
                Object v = f.get(null);
                if (v == null) continue;
                if (rbbClass.isInstance(v)) j.key(f.getName()).val(rbbId.getInt(v));
                else if (bgbClass.isInstance(v)) {
                    j.key(f.getName());
                    writeBgb(j, v);
                }
            }
            j.close('}');

            // RWG2 2.0.9 moved the values out of ConfigRWG into per-category GTNHLib config classes. `configRWG`
            // keeps the merged field map (same names as before); `configClasses` records which class held each.
            j.key("configRWG").open('{');
            List<String> found = new ArrayList<String>();
            for (String cn : RWG_CONFIG_CLASSES) {
                Class<?> c;
                try {
                    c = Class.forName("rwg2.config." + cn, true, cl);
                } catch (ClassNotFoundException e) {
                    continue;
                }
                found.add(cn);
                staticFields(j, c);
            }
            j.close('}');
            j.key("configClasses").open('[');
            for (String cn : found) j.str(cn);
            j.close(']');
            j.close('}');
            writeText(new File(out, "biomes.json"), j.toString());
        }

        void writeRbb(Json j, Object b) throws Exception {
            j.open('{');
            j.key("rwgId").val(rbbId.getInt(b));
            j.key("subId").val(rbbSub.getInt(b));
            j.key("class").str(b.getClass().getName());
            Method dn = rbbClass.getMethod("getDisplayName");
            j.key("displayName").str((String) dn.invoke(b));
            j.key("base");
            writeBgb(j, rbbBase.get(b));
            Object beach = rbbBeach.get(b);
            j.key("beachRwgId").raw(beach == null ? "null" : String.valueOf(rbbId.getInt(beach)));
            j.key("river");
            writeBgb(j, rbbRiver.get(b));
            // v2: the height model. `terrain` = the TerrainBase instance the biome delegates rNoise to (if any),
            // `fields` = the biome's own primitive instance fields (e.g. RealisticBiomeOcean.shallow).
            Object terrain = null;
            for (Class<?> k = b.getClass(); k != null && k != Object.class; k = k.getSuperclass())
                for (Field f : k.getDeclaredFields()) {
                    if (Modifier.isStatic(f.getModifiers()) || terrainClass == null
                            || !terrainClass.isAssignableFrom(f.getType()))
                        continue;
                    f.setAccessible(true);
                    if (terrain == null) terrain = f.get(b);
                }
            j.key("terrain");
            if (terrain == null) j.raw("null");
            else reflectObject(j, terrain, terrainClass);
            j.key("fields");
            reflectObject(j, b, rbbClass);
            j.close('}');
        }

        /** {"class":..,"fields":{name:value}} for primitive/String/primitive-array instance fields up to `stop`. */
        void reflectObject(Json j, Object o, Class<?> stop) throws Exception {
            j.open('{');
            j.key("class").str(o.getClass().getName());
            j.key("fields").open('{');
            for (Class<?> k = o.getClass(); k != null && k != Object.class; k = k.getSuperclass()) {
                for (Field f : k.getDeclaredFields()) {
                    if (Modifier.isStatic(f.getModifiers())) continue;
                    Class<?> t = f.getType();
                    f.setAccessible(true);
                    Object v = f.get(o);
                    if (t == float.class) j.key(f.getName()).raw(floatJson((Float) v));
                    else if (t == double.class) j.key(f.getName()).raw(doubleJson((Double) v));
                    else if (t.isPrimitive()) j.key(f.getName()).raw(String.valueOf(v));
                    else if (v instanceof String) j.key(f.getName()).str((String) v);
                    else if (v != null && t.isArray() && t.getComponentType() == float.class) {
                        j.key(f.getName()).open('[');
                        for (float x : (float[]) v) j.raw(floatJson(x));
                        j.close(']');
                    } else if (v != null && t.isArray() && t.getComponentType().isPrimitive()) {
                        j.key(f.getName()).open('[');
                        for (int i = 0; i < Array.getLength(v); i++) j.raw(String.valueOf(Array.get(v, i)));
                        j.close(']');
                    }
                }
                if (k == stop) break;
            }
            j.close('}');
            j.close('}');
        }

        void writeBgb(Json j, Object g) throws Exception {
            if (g == null) {
                j.raw("null");
                return;
            }
            j.open('{');
            j.key("id").val(genId(g));
            j.key("name").str(bgbName != null ? String.valueOf(bgbName.get(g)) : null);
            j.key("color").val(bgbColor != null ? bgbColor.getInt(g) : -1);
            j.key("temperature").raw(bgbTemp != null ? String.valueOf(bgbTemp.getFloat(g)) : "null");
            j.key("rainfall").raw(bgbRain != null ? String.valueOf(bgbRain.getFloat(g)) : "null");
            j.key("class").str(g.getClass().getName());
            j.close('}');
        }

        void ids(Json j, List<?> l) throws Exception {
            j.open('[');
            for (Object o : l) j.raw(o == null ? "null" : String.valueOf(rbbId.getInt(o)));
            j.close(']');
        }

        static final String[] RWG_CONFIG_CLASSES = { "ConfigRWG", "ContinentalConfig", "ClimateBandConfig",
                "SettingsConfig", "MigrationConfig", "LandmarkDecorationsConfig", "BiomeIdConfig", "DebugConfig" };

        void statics(Json j, Class<?> c) throws Exception {
            j.open('{');
            staticFields(j, c);
            j.close('}');
        }

        /** Primitive / String / primitive-array statics of `c` as keys of the object `j` currently has open. */
        void staticFields(Json j, Class<?> c) throws Exception {
            for (Field f : c.getDeclaredFields()) {
                if (!Modifier.isStatic(f.getModifiers())) continue;
                f.setAccessible(true);
                Object v = f.get(null);
                if (v == null) continue;
                Class<?> t = f.getType();
                if (t.isPrimitive() || v instanceof String || v instanceof Number || v instanceof Boolean)
                    j.key(f.getName()).str(String.valueOf(v));
                else if (t.isArray() && t.getComponentType().isPrimitive()) {
                    j.key(f.getName()).open('[');
                    for (int i = 0; i < Array.getLength(v); i++) j.str(String.valueOf(Array.get(v, i)));
                    j.close(']');
                } else if (t.isArray() && t.getComponentType() == String.class) {
                    j.key(f.getName()).open('[');
                    for (Object s : (Object[]) v) j.str((String) s);
                    j.close(']');
                }
            }
        }

        // ---------------------------------------------------------------- env / manifest

        void writeEnv() throws Exception {
            Json j = new Json();
            j.open('{');
            j.key("rwg2dumpVersion").str(VERSION);
            for (String p : new String[] { "java.version", "java.vendor", "java.vm.name", "java.vm.version",
                    "os.name", "os.arch", "os.version" })
                j.key(p).str(System.getProperty(p));
            j.key("availableProcessors").val(Runtime.getRuntime().availableProcessors());
            j.key("cpuModel").str(cpuModel());
            j.key("noiseImplementation").str(noiseImpl);
            j.key("jvmArgs").open('[');
            try {
                Class<?> mf = Class.forName("java.lang.management.ManagementFactory");
                Object rt = mf.getMethod("getRuntimeMXBean").invoke(null);
                for (Object a : (List<?>) rt.getClass().getMethod("getInputArguments").invoke(rt))
                    j.str(String.valueOf(a));
            } catch (Throwable ignored) {}
            j.close(']');
            j.key("mods").open('[');
            try {
                Class<?> loader = Class.forName("cpw.mods.fml.common.Loader", true, cl);
                Object inst = loader.getMethod("instance").invoke(null);
                for (Object mc : (Collection<?>) loader.getMethod("getModList").invoke(inst)) {
                    j.open('{');
                    j.key("id").str(String.valueOf(mc.getClass().getMethod("getModId").invoke(mc)));
                    j.key("version").str(String.valueOf(mc.getClass().getMethod("getVersion").invoke(mc)));
                    Object src = mc.getClass().getMethod("getSource").invoke(mc);
                    j.key("source").str(src instanceof File ? ((File) src).getName() : String.valueOf(src));
                    if (src instanceof File && ((File) src).isFile() && interesting((File) src))
                        j.key("sha256").str(sha256((File) src));
                    j.close('}');
                }
            } catch (Throwable t) {
                j.str("error: " + t);
            }
            j.close(']');
            j.key("rwg2Config").str(readIfExists(new File("config", "rwg2.cfg")));
            j.close('}');
            writeText(new File(out, "env.json"), j.toString());
        }

        static boolean interesting(File f) {
            String n = f.getName().toLowerCase();
            return n.contains("realistic") || n.contains("rwg") || n.contains("biomesoplenty")
                    || n.contains("extrabiomes") || n.contains("thaumcraft") || n.contains("witchery")
                    || n.contains("endlessids");
        }

        void writeManifest(String files, long millis) throws IOException {
            Json j = new Json();
            j.open('{');
            j.key("format").val(FORMAT);
            j.key("endianness").str("little");
            j.key("layout").str("row-major: index = j*w + i; sample at (x0 + i*step, z0 + j*step)");
            j.key("rwgIdNote").str("rwgId = RealisticBiomeBase.biomeID (RWG registration index, see biomes.json)");
            j.key("genIdNote").str("genBiomeId = getBiomeGenAt(x,z) BiomeGenBase id; includes lava-cave marker");
            j.key("millis").val(millis);
            j.key("seeds").open('[');
            for (long s : seeds) j.str(Long.toString(s));
            j.close(']');
            j.key("files").raw("[" + (files.isEmpty() ? "" : files.substring(0, files.length() - 1)) + "]");
            j.close('}');
            writeText(new File(out, "manifest.json"), j.toString());
        }

        String manifestEntry(String file, long seed, String type, String region, int x0, int z0, int w, int h,
                int step, String layout) {
            Json j = new Json();
            j.open('{');
            j.key("file").str(file);
            j.key("seed").str(Long.toString(seed));
            j.key("worldType").str(type);
            j.key("region").str(region);
            j.key("x0").val(x0);
            j.key("z0").val(z0);
            j.key("w").val(w);
            j.key("h").val(h);
            j.key("step").val(step);
            j.key("layout").str(layout);
            j.close('}');
            return j + ",";
        }

        void writeError(Throwable t) {
            try {
                StringWriter sw = new StringWriter();
                t.printStackTrace(new PrintWriter(sw));
                writeText(new File(out, "error.txt"), sw.toString());
            } catch (Throwable ignored) {}
        }
    }

    // ------------------------------------------------------------------------------------------------ helpers

    static Method declared(Class<?> c, String name, Class<?>... p) throws NoSuchMethodException {
        Method m = c.getDeclaredMethod(name, p);
        m.setAccessible(true);
        return m;
    }

    /** First matching field name in the class hierarchy, or null. */
    static Field field(Class<?> c, String... names) {
        for (Class<?> k = c; k != null; k = k.getSuperclass())
            for (String n : names) {
                try {
                    Field f = k.getDeclaredField(n);
                    f.setAccessible(true);
                    return f;
                } catch (NoSuchFieldException ignored) {}
            }
        log("WARN: field not found: " + c.getName() + "." + names[0]);
        return null;
    }

    static ByteBuffer le(int n) {
        return ByteBuffer.allocate(n).order(ByteOrder.LITTLE_ENDIAN);
    }

    static void writeBuffers(File f, ByteBuffer... bufs) throws IOException {
        FileOutputStream fos = new FileOutputStream(f);
        try {
            FileChannel ch = fos.getChannel();
            for (ByteBuffer b : bufs) {
                b.flip();
                while (b.hasRemaining()) ch.write(b);
            }
        } finally {
            fos.close();
        }
    }

    /** 2.15.1: packs the dated dump folder into rwg2dump/<ts>.zip (entries under <ts>/), next to the folder. */
    static void zipDump(File dir) {
        File zip = new File(dir.getParentFile(), dir.getName() + ".zip");
        long t0 = System.currentTimeMillis();
        try {
            java.util.zip.ZipOutputStream z = new java.util.zip.ZipOutputStream(
                    new java.io.BufferedOutputStream(new java.io.FileOutputStream(zip), 1 << 16));
            try {
                File[] files = dir.listFiles();
                if (files != null) {
                    java.util.Arrays.sort(files);
                    byte[] buf = new byte[1 << 16];
                    for (File f : files) {
                        if (!f.isFile()) continue;
                        java.util.zip.ZipEntry e = new java.util.zip.ZipEntry(dir.getName() + "/" + f.getName());
                        e.setTime(f.lastModified());
                        z.putNextEntry(e);
                        java.io.InputStream in = new java.io.FileInputStream(f);
                        try {
                            int n;
                            while ((n = in.read(buf)) > 0) z.write(buf, 0, n);
                        } finally {
                            in.close();
                        }
                        z.closeEntry();
                    }
                }
            } finally {
                z.close();
            }
            log(String.format("ZIP -> %s (%.1f MB, %.1fs)", zip.getAbsolutePath(), zip.length() / 1048576.0,
                    (System.currentTimeMillis() - t0) / 1000.0));
        } catch (Throwable t) {
            log("zip failed: " + t);
        }
    }

    static void writeText(File f, String s) throws IOException {
        Writer w = new OutputStreamWriter(new FileOutputStream(f), StandardCharsets.UTF_8);
        try {
            w.write(s);
        } finally {
            w.close();
        }
    }

    static String readIfExists(File f) {
        if (!f.isFile()) return null;
        try {
            StringBuilder sb = new StringBuilder();
            BufferedReader r = new BufferedReader(new FileReader(f));
            try {
                for (String l; (l = r.readLine()) != null;) sb.append(l).append('\n');
            } finally {
                r.close();
            }
            return sb.toString();
        } catch (IOException e) {
            return "error: " + e;
        }
    }

    static String cpuModel() {
        String s = readIfExists(new File("/proc/cpuinfo"));
        if (s == null) return System.getenv("PROCESSOR_IDENTIFIER");
        for (String l : s.split("\n"))
            if (l.startsWith("model name") || l.startsWith("Model") || l.startsWith("CPU part"))
                return l.substring(l.indexOf(':') + 1).trim();
        return null;
    }

    static String sha256(File f) {
        try {
            MessageDigest md = MessageDigest.getInstance("SHA-256");
            FileInputStream in = new FileInputStream(f);
            try {
                byte[] buf = new byte[1 << 16];
                for (int n; (n = in.read(buf)) > 0;) md.update(buf, 0, n);
            } finally {
                in.close();
            }
            StringBuilder sb = new StringBuilder();
            for (byte b : md.digest()) sb.append(String.format("%02x", b));
            return sb.toString();
        } catch (Exception e) {
            return "error: " + e;
        }
    }

    /** Float as JSON number plus exact bits, so values round-trip losslessly: {"v":..,"bits":"hex"}. */
    static String floatJson(float f) {
        return "{\"v\":" + (Float.isFinite(f) ? Float.toString(f) : "null") + ",\"bits\":\""
                + Integer.toHexString(Float.floatToRawIntBits(f)) + "\"}";
    }

    static String doubleJson(double d) {
        return "{\"v\":" + (Double.isFinite(d) ? Double.toString(d) : "null") + ",\"bits\":\""
                + Long.toHexString(Double.doubleToRawLongBits(d)) + "\"}";
    }

    static String seedTag(long seed) {
        return seed < 0 ? "m" + Long.toString(seed).substring(1) : Long.toString(seed);
    }

    /** Minimal JSON writer; handles commas automatically. */
    static final class Json {

        private final StringBuilder sb = new StringBuilder();
        private boolean needComma;
        /** A key was written but its value threw before being written: emit null so the JSON stays valid. */
        private boolean dangling;
        /** Open brackets, so toString() can close what an exception left open. */
        private final StringBuilder stack = new StringBuilder();

        private void settle() {
            if (dangling) {
                sb.append("null");
                needComma = true;
                dangling = false;
            }
        }

        Json open(char c) {
            dangling = false;
            sep();
            sb.append(c);
            stack.append(c);
            needComma = false;
            return this;
        }

        Json close(char c) {
            settle();
            if (stack.length() > 0) stack.setLength(stack.length() - 1);
            sb.append(c);
            needComma = true;
            return this;
        }

        Json key(String k) {
            settle();
            // a key inside an array (left there by an exception) would be invalid: close back to an object
            while (stack.length() > 0 && stack.charAt(stack.length() - 1) == '[') close(']');
            sep();
            quote(k);
            sb.append(':');
            needComma = false;
            dangling = true;
            return this;
        }

        Json str(String s) {
            dangling = false;
            sep();
            if (s == null) sb.append("null");
            else quote(s);
            needComma = true;
            return this;
        }

        Json val(String v) {
            return str(v);
        }

        Json val(boolean v) {
            return raw(String.valueOf(v));
        }

        Json val(Object v) {
            return v instanceof Number ? raw(String.valueOf(v)) : v instanceof Boolean ? raw(String.valueOf(v)) : str(String.valueOf(v));
        }

        Json val(long v) {
            return raw(Long.toString(v));
        }

        Json raw(String s) {
            dangling = false;
            sep();
            sb.append(s);
            needComma = true;
            return this;
        }

        private void sep() {
            if (needComma) sb.append(',');
        }

        private void quote(String s) {
            sb.append('"');
            for (int i = 0; i < s.length(); i++) {
                char c = s.charAt(i);
                if (c == '"' || c == '\\') sb.append('\\').append(c);
                else if (c < 0x20) sb.append(String.format("\\u%04x", (int) c));
                else sb.append(c);
            }
            sb.append('"');
        }

        @Override
        public String toString() {
            StringBuilder out = new StringBuilder(sb);
            if (dangling) out.append("null");
            for (int i = stack.length() - 1; i >= 0; i--) out.append(stack.charAt(i) == '{' ? '}' : ']');
            return out.toString();
        }
    }
}
