package rwg2dump;

import java.io.BufferedReader;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.Collection;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * rwg2dump 2.18 quick probe ({@code /rwg2dump probe}, server thread, a few minutes):
 * <ul>
 * <li>{@code listeners}: every terrain-gen related listener on the Forge buses (owner class + ASM handler string) and
 * the registered {@code IWorldGenerator}s with their GameRegistry weight;</li>
 * <li>{@code oceans}: one Kelp Forest and one Coral Reef area (3x3 chunks + 1 ring) loaded with the 2.16 event hooks
 * plus 2.18 tracing (LOWEST twins of Populate / decorate Pre / Post, the firing stack of CUSTOM / ICE / ANIMALS
 * populate events and of every decPre / decPost): which code reseeds the shared Random before the deferred ocean
 * decorator;</li>
 * <li>{@code carts}: the chunks around each predicted mineshaft chest cart of {@code carts218.txt} (seed
 * 1631302684446519836) are force-loaded (row-major), then every minecart with an inventory in them is listed with
 * its position and slots (stack string, registry id, display name). Other seeds: the same chunks are loaded and
 * whatever carts exist are listed.</li>
 * </ul>
 */
final class Probe218 {

    static final String[][] OCEANS = { { "oceanKelpForest", "g", "Kelp Forest" }, { "oceanCoralReef", "g", "Coral Reef" } };
    static final int CART_R = 2; // chunks loaded around a cart's populating chunk
    static final long CART_BUDGET_MS = 15 * 60_000L;

    final Extras x;
    final ClassLoader cl;

    Probe218(Extras x, ClassLoader cl) {
        this.x = x;
        this.cl = cl;
    }

    String run() {
        Rwg2Dump.Json j = new Rwg2Dump.Json();
        j.open('{');
        long t0 = System.currentTimeMillis();
        j.key("rwg2dumpVersion").str(Rwg2Dump.VERSION);
        Object w = null;
        try {
            w = x.world();
            j.key("worldSeed").str(Long.toString(x.worldSeedOf(w)));
        } catch (Throwable t) {
            j.key("worldError").str(Extras.err(t));
        }
        j.key("listeners").open('{');
        try {
            listeners(j);
        } catch (Throwable t) {
            j.key("error").str(Extras.err(t));
        }
        j.close('}');
        if (w != null) {
            j.key("oceans").open('{');
            try {
                oceans(j, w);
            } catch (Throwable t) {
                j.key("error").str(Extras.err(t));
            }
            j.close('}');
            j.key("carts").open('{');
            try {
                carts(j, w);
            } catch (Throwable t) {
                j.key("error").str(Extras.err(t));
            }
            j.close('}');
        }
        // 2.19: biome spawn lists + Nether fortress chests
        new Probe219(x).run(j);
        j.key("millis").val(System.currentTimeMillis() - t0);
        j.close('}');
        return j.toString();
    }

    // ============================================================================================ listeners

    void listeners(Rwg2Dump.Json j) throws Exception {
        Class<?> mf = x.cls("net.minecraftforge.common.MinecraftForge");
        for (String bus : new String[] { "EVENT_BUS", "TERRAIN_GEN_BUS", "ORE_GEN_BUS" }) {
            j.key(bus).open('[');
            try {
                Object b = mf.getField(bus).get(null);
                Object map = Extras.fieldAny(b, "listeners");
                if (map instanceof Map)
                    for (Map.Entry<?, ?> e : ((Map<?, ?>) map).entrySet()) {
                        List<String> hs = new ArrayList<String>();
                        if (e.getValue() instanceof Collection)
                            for (Object l : (Collection<?>) e.getValue()) {
                                String s = String.valueOf(l);
                                if (s.contains("terraingen") || s.contains("PopulateChunkEvent") || s.contains("DecorateBiomeEvent")
                                        || s.contains("OreGenEvent") || s.contains("ChunkEvent"))
                                    hs.add(s);
                            }
                        if (hs.isEmpty()) continue;
                        j.open('{').key("owner").str(e.getKey().getClass().getName()).key("handlers").open('[');
                        for (String h : hs) j.str(h);
                        j.close(']').close('}');
                    }
                else j.str("no listeners map: " + (map == null ? "null" : map.getClass().getName()));
            } catch (Throwable t) {
                j.str("error: " + Extras.err(t));
            }
            j.close(']');
        }
        j.key("worldGenerators").open('[');
        try {
            Class<?> gr = x.cls("cpw.mods.fml.common.registry.GameRegistry");
            Object idx = null;
            for (String n : new String[] { "worldGeneratorIndex" })
                try {
                    Field f = gr.getDeclaredField(n);
                    f.setAccessible(true);
                    idx = f.get(null);
                } catch (Throwable ignored) {}
            Set<Object> gens = new LinkedHashSet<Object>();
            try {
                Field f = gr.getDeclaredField("worldGenerators");
                f.setAccessible(true);
                Object v = f.get(null);
                if (v instanceof Collection) gens.addAll((Collection<?>) v);
            } catch (Throwable ignored) {}
            for (Object g : gens) {
                Object weight = idx instanceof Map ? ((Map<?, ?>) idx).get(g) : null;
                j.raw("[\"" + g.getClass().getName() + "\"," + (weight == null ? "null" : weight) + "]");
            }
            try {
                Field f = gr.getDeclaredField("sortedGeneratorList");
                f.setAccessible(true);
                Object v = f.get(null);
                if (v instanceof Collection) {
                    StringBuilder sb = new StringBuilder("\"sorted:");
                    for (Object g : (Collection<?>) v) sb.append(' ').append(g.getClass().getName());
                    j.raw(sb.append('"').toString());
                }
            } catch (Throwable ignored) {}
        } catch (Throwable t) {
            j.str("error: " + Extras.err(t));
        }
        j.close(']');
    }

    // ============================================================================================ oceans

    void oceans(Rwg2Dump.Json j, Object w) throws Exception {
        String reg = Hooks216.register(cl);
        j.key("hooks").str(reg == null ? "registered" : reg);
        try {
            Object cm = Extras.named(x.cls("net.minecraft.world.World"), new Class<?>[0], "getWorldChunkManager", "func_72959_q").invoke(w);
            Method gen = null;
            Class<?> bgb = x.cls("net.minecraft.world.biome.BiomeGenBase");
            Field bname = Rwg2Dump.field(bgb, "biomeName", "field_76791_y");
            for (Class<?> k = cm.getClass(); k != null && gen == null; k = k.getSuperclass())
                for (Method m : k.getDeclaredMethods()) {
                    Class<?>[] p = m.getParameterTypes();
                    if (p.length == 2 && p[0] == int.class && p[1] == int.class && bgb.isAssignableFrom(m.getReturnType())) {
                        m.setAccessible(true);
                        gen = m;
                        break;
                    }
                }
            Extras.Loader ld = x.new Loader(w);
            j.key("areas").open('[');
            for (String[] t : OCEANS) {
                int[] c = find(cm, gen, bname, t[2]);
                j.open('{').key("name").str(t[0]);
                if (c == null) {
                    j.key("skipped").str("not found within 12000 blocks").close('}');
                    continue;
                }
                int ccx = c[0] >> 4, ccz = c[1] >> 4, r = 1;
                j.key("chunkX0").val(ccx - r).key("chunkZ0").val(ccz - r).key("chunks").val(2 * r + 1);
                Hooks216.reset(w);
                Hooks216.trace = true;
                Hooks216.active = true;
                int fresh;
                try {
                    fresh = ld.area((ccx - r - 1) * 16, (ccz - r - 1) * 16, (ccx + r + 1) * 16 + 15, (ccz + r + 1) * 16 + 15);
                } finally {
                    Hooks216.active = false;
                    Hooks216.trace = false;
                }
                j.key("freshChunks").val(fresh);
                j.key("loadOrder").str("row-major cz outer, cx inner over the area + 1 ring");
                j.key("events").open('[');
                for (String e : Hooks216.drain()) j.raw(e);
                j.close(']');
                j.close('}');
            }
            j.close(']');
            if (!ld.errors.isEmpty()) {
                j.key("loadErrors").open('[');
                for (String e : ld.errors) j.str(e);
                j.close(']');
            }
            j.key("randomStateMethod").str(Hooks216.stateMethod);
        } finally {
            Hooks216.unregister(cl);
        }
    }

    /** Nearest point (64-block rings from 2560 out) where ≥ 70 of 81 samples at 16 blocks have the biome name. */
    int[] find(Object cm, Method gen, Field bname, String name) throws Exception {
        for (int ring = 40; ring <= 190; ring++)
            for (int a = -ring; a < ring; a++)
                for (int k = 0; k < 4; k++) {
                    int gx = k == 0 ? a : k == 1 ? ring : k == 2 ? -a : -ring, gz = k == 0 ? -ring : k == 1 ? a : k == 2 ? ring : -a;
                    int cx = gx * 64, cz = gz * 64;
                    if (!name.equals(String.valueOf(bname.get(gen.invoke(cm, cx, cz))))) continue;
                    int hit = 0;
                    for (int i = -4; i <= 4; i++)
                        for (int q = -4; q <= 4; q++)
                            if (name.equals(String.valueOf(bname.get(gen.invoke(cm, cx + i * 16, cz + q * 16))))) hit++;
                    if (hit >= 70) return new int[] { cx, cz };
                }
        return null;
    }

    // ============================================================================================ carts

    void carts(Rwg2Dump.Json j, Object w) throws Exception {
        List<int[]> want = new ArrayList<int[]>();
        InputStream in = Probe218.class.getResourceAsStream("/carts218.txt");
        if (in != null) {
            BufferedReader br = new BufferedReader(new InputStreamReader(in, "UTF-8"));
            for (String l; (l = br.readLine()) != null;) {
                l = l.trim();
                if (l.isEmpty() || l.startsWith("#")) continue;
                String[] p = l.split("\\s+");
                want.add(new int[] { Integer.parseInt(p[0]), Integer.parseInt(p[1]), Integer.parseInt(p[2]), Integer.parseInt(p[3]) });
            }
            br.close();
        }
        j.key("predictedFor").str("1631302684446519836");
        j.key("predicted").open('[');
        for (int[] c : want) j.raw("[" + c[0] + "," + c[1] + "," + c[2] + "," + c[3] + "]");
        j.close(']');
        Extras.Loader ld = x.new Loader(w);
        Class<?> worldC = x.cls("net.minecraft.world.World");
        Method getChunk = Extras.named(worldC, new Class<?>[] { int.class, int.class }, "getChunkFromChunkCoords", "func_72964_e");
        Class<?> invC = x.cls("net.minecraft.inventory.IInventory");
        Method size = Extras.named(invC, new Class<?>[0], "getSizeInventory", "func_70302_i_");
        Method slot = Extras.named(invC, new Class<?>[] { int.class }, "getStackInSlot", "func_70301_a");
        Class<?> isC = x.cls("net.minecraft.item.ItemStack");
        Method dName = Extras.named(isC, new Class<?>[0], "getDisplayName", "func_82833_r");
        Method gItem = Extras.named(isC, new Class<?>[0], "getItem", "func_77973_b");
        Object reg = null;
        Method regName = null;
        try {
            reg = Rwg2Dump.field(x.cls("net.minecraft.item.Item"), "itemRegistry", "field_150901_e").get(null);
            regName = Extras.named(reg.getClass(), new Class<?>[] { Object.class }, "getNameForObject", "func_148750_c");
        } catch (Throwable ignored) {}
        Class<?> entC = x.cls("net.minecraft.entity.Entity");
        Field px = Rwg2Dump.field(entC, "posX", "field_70165_t"), py = Rwg2Dump.field(entC, "posY", "field_70163_u"),
                pz = Rwg2Dump.field(entC, "posZ", "field_70161_v");
        Set<Long> scanned = new LinkedHashSet<Long>();
        Set<Object> seen = new java.util.HashSet<Object>();
        long t0 = System.currentTimeMillis();
        int done = 0;
        j.key("found").open('[');
        for (int[] c : want) {
            if (System.currentTimeMillis() - t0 > CART_BUDGET_MS) break;
            int pcx = (c[0] - 8) >> 4, pcz = (c[2] - 8) >> 4;
            ld.area((pcx - CART_R) * 16, (pcz - CART_R) * 16, (pcx + CART_R) * 16 + 15, (pcz + CART_R) * 16 + 15);
            done++;
            for (int cz = pcz - CART_R + 1; cz <= pcz + CART_R - 1; cz++)
                for (int cx = pcx - CART_R + 1; cx <= pcx + CART_R - 1; cx++) {
                    if (!scanned.add(((long) cx << 32) ^ (cz & 0xffffffffL))) continue;
                    Object chunk = getChunk.invoke(w, cx, cz);
                    Object lists = Extras.fieldAny(chunk, "entityLists", "field_76645_j");
                    if (!(lists instanceof Object[])) continue;
                    for (Object l : (Object[]) lists) {
                        if (!(l instanceof Collection)) continue;
                        for (Object e : new ArrayList<Object>((Collection<?>) l)) {
                            if (!invC.isInstance(e) || !seen.add(e)) continue;
                            String cn = e.getClass().getName();
                            if (!cn.contains("Minecart") && !cn.contains("Cart")) continue;
                            j.open('{').key("class").str(cn).key("chunk").raw("[" + cx + "," + cz + "]");
                            j.key("pos").raw("[" + px.get(e) + "," + py.get(e) + "," + pz.get(e) + "]");
                            j.key("slots").open('[');
                            int n = (Integer) size.invoke(e);
                            for (int i = 0; i < n; i++) {
                                Object st = slot.invoke(e, i);
                                if (st == null) continue;
                                String id = "?", dn = "?";
                                try {
                                    id = String.valueOf(regName.invoke(reg, gItem.invoke(st)));
                                } catch (Throwable ignored) {}
                                try {
                                    dn = String.valueOf(dName.invoke(st));
                                } catch (Throwable ignored) {}
                                j.open('[').val((long) i).str(String.valueOf(st)).str(id).str(dn).close(']');
                            }
                            j.close(']').close('}');
                        }
                    }
                }
        }
        j.close(']');
        j.key("targetsLoaded").val(done);
        j.key("chunksScanned").val(scanned.size());
        if (!ld.errors.isEmpty()) {
            j.key("loadErrors").open('[');
            for (String e : ld.errors) j.str(e);
            j.close(']');
        }
        j.key("cartMillis").val(System.currentTimeMillis() - t0);
    }
}
