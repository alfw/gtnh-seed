package rwg2dump;

import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.Map;

/**
 * rwg2dump 2.19 additions to the quick probe ({@code /rwg2dump probe}):
 * <ul>
 * <li>{@code spawnLists}: every registered biome's creature spawn list (class, weight, min / max group) and
 * {@code getSpawningChance()}: the draws of {@code SpawnerAnimals.performWorldGenSpawning} (ANIMALS event) depend on
 * it;</li>
 * <li>{@code fortressChests}: Nether chunks [-FORT_R, FORT_R]^2 force-loaded (row-major, so the inner ones are
 * populated), the Nether generator / fortress map-gen classes, every fortress start in its structure map (chunk,
 * box), and every chest tile entity in the populated chunks with its slots (slot, stack string, registry id).</li>
 * </ul>
 */
final class Probe219 {

    static final int FORT_R = 24; // chunks around the Nether origin

    final Extras x;

    Probe219(Extras x) {
        this.x = x;
    }

    void run(Rwg2Dump.Json j) {
        j.key("spawnLists").open('{');
        try {
            spawnLists(j);
        } catch (Throwable t) {
            j.key("error").str(Extras.err(t));
        }
        j.close('}');
        j.key("fortressChests").open('{');
        try {
            fortressChests(j);
        } catch (Throwable t) {
            j.key("error").str(Extras.err(t));
        }
        j.close('}');
    }

    // ============================================================================================ spawn lists

    void spawnLists(Rwg2Dump.Json j) throws Exception {
        Class<?> bgb = x.cls("net.minecraft.world.biome.BiomeGenBase");
        Object[] list = (Object[]) Rwg2Dump.field(bgb, "biomeList", "field_76773_a").get(null);
        Field name = Rwg2Dump.field(bgb, "biomeName", "field_76791_y");
        Field id = Rwg2Dump.field(bgb, "biomeID", "field_76756_M");
        String[][] lists = {{"creatures", "spawnableCreatureList", "field_76762_K"}, {"monsters", "spawnableMonsterList", "field_76761_J"},
                {"water", "spawnableWaterCreatureList", "field_76755_L"}, {"ambient", "spawnableCaveCreatureList", "field_82914_M"}};
        Method chance = Extras.named(bgb, new Class<?>[0], "getSpawningChance", "func_76741_f");
        Class<?> sle = x.cls("net.minecraft.world.biome.BiomeGenBase$SpawnListEntry");
        Field ec = Rwg2Dump.field(sle, "entityClass", "field_76300_b");
        Field mn = Rwg2Dump.field(sle, "minGroupCount", "field_76301_c");
        Field mx = Rwg2Dump.field(sle, "maxGroupCount", "field_76299_d");
        Field wt = Rwg2Dump.field(x.cls("net.minecraft.util.WeightedRandom$Item"), "itemWeight", "field_76292_a");
        j.key("format").str("id -> {name, chance, creatures|monsters|water|ambient: [[class, weight, min, max]]} (-1 = field missing)");
        j.key("biomes").open('{');
        for (Object b : list) {
            if (b == null) continue;
            j.key(String.valueOf(id.get(b))).open('{');
            j.key("name").str(String.valueOf(name.get(b)));
            try {
                j.key("chance").raw(String.valueOf(chance.invoke(b)));
            } catch (Throwable t) {
                j.key("chanceError").str(Extras.err(t));
            }
            for (String[] ls : lists) {
                j.key(ls[0]).open('[');
                try {
                    Field lf = Rwg2Dump.field(bgb, ls[1], ls[2]);
                    Object cl = lf == null ? null : lf.get(b);
                    if (cl instanceof Collection)
                        for (Object e : (Collection<?>) cl) {
                            Class<?> c = (Class<?>) ec.get(e);
                            j.open('[').str(c == null ? "null" : c.getName()).val(num(wt, e)).val(num(mn, e)).val(num(mx, e)).close(']');
                        }
                } catch (Throwable t) {
                    j.str("error: " + t);
                }
                j.close(']');
            }
            j.close('}');
        }
        j.close('}');
    }

    static long num(Field f, Object o) {
        try {
            return f == null ? -1 : ((Number) f.get(o)).longValue();
        } catch (Throwable t) {
            return -1;
        }
    }

    // ============================================================================================ fortress chests

    void fortressChests(Rwg2Dump.Json j) throws Exception {
        Class<?> dm = x.cls("net.minecraftforge.common.DimensionManager");
        Object nether = dm.getMethod("getWorld", int.class).invoke(null, -1);
        if (nether == null) {
            dm.getMethod("initDimension", int.class).invoke(null, -1);
            nether = dm.getMethod("getWorld", int.class).invoke(null, -1);
        }
        if (nether == null) {
            j.key("error").str("nether world not available");
            return;
        }
        j.key("worldSeed").str(Long.toString(x.worldSeedOf(nether)));
        Extras.Loader ld = x.new Loader(nether);
        long t0 = System.currentTimeMillis();
        int fresh = ld.area(-FORT_R * 16, -FORT_R * 16, FORT_R * 16 + 15, FORT_R * 16 + 15);
        j.key("range").val(FORT_R).key("freshChunks").val(fresh).key("loadOrder").str("row-major cz outer, cx inner");
        j.key("loadMillis").val(System.currentTimeMillis() - t0);
        // generator + fortress map-gen + starts
        try {
            Object gen = Structures.fieldByTypeName(nether, "ChunkProviderServer");
            Object inner = Extras.fieldAny(gen, "currentChunkProvider", "field_73246_d");
            j.key("generator").str(inner == null ? "?" : inner.getClass().getName());
            Object mg = null;
            if (inner != null)
                for (Class<?> k = inner.getClass(); k != null && mg == null; k = k.getSuperclass())
                    for (Field f : k.getDeclaredFields()) {
                        f.setAccessible(true);
                        Object v = f.get(inner);
                        if (v != null && v.getClass().getName().contains("NetherBridge")) {
                            mg = v;
                            break;
                        }
                    }
            j.key("mapGen").str(mg == null ? "?" : mg.getClass().getName());
            if (mg != null) {
                Object map = Extras.fieldAny(mg, "structureMap", "field_75053_d");
                j.key("structureMap").str(map == null ? "?" : map.getClass().getName());
                j.key("starts").open('[');
                Collection<?> vals = map instanceof Map ? ((Map<?, ?>) map).values() : null;
                if (vals != null)
                    for (Object st : vals) {
                        Object cx = Extras.fieldAny(st, "field_143024_c"), cz = Extras.fieldAny(st, "field_143023_d");
                        Object bb = Extras.fieldAny(st, "boundingBox", "field_75074_b");
                        String box = "?";
                        if (bb != null) {
                            int[] v = new int[6];
                            String[][] n = { { "minX", "field_78897_a" }, { "minY", "field_78895_b" }, { "minZ", "field_78896_c" },
                                    { "maxX", "field_78893_d" }, { "maxY", "field_78894_e" }, { "maxZ", "field_78892_f" } };
                            for (int i = 0; i < 6; i++) v[i] = (Integer) Extras.fieldAny(bb, n[i][0], n[i][1]);
                            box = "[" + v[0] + "," + v[1] + "," + v[2] + "," + v[3] + "," + v[4] + "," + v[5] + "]";
                        }
                        j.raw("{\"chunk\":[" + cx + "," + cz + "],\"box\":" + box + "}");
                    }
                j.close(']');
            }
        } catch (Throwable t) {
            j.key("startsError").str(Extras.err(t));
        }
        // chests
        Method getChunk = Extras.named(x.cls("net.minecraft.world.World"), new Class<?>[] { int.class, int.class }, "getChunkFromChunkCoords", "func_72964_e");
        Class<?> invC = x.cls("net.minecraft.inventory.IInventory");
        Method size = Extras.named(invC, new Class<?>[0], "getSizeInventory", "func_70302_i_");
        Method slot = Extras.named(invC, new Class<?>[] { int.class }, "getStackInSlot", "func_70301_a");
        Method gItem = Extras.named(x.cls("net.minecraft.item.ItemStack"), new Class<?>[0], "getItem", "func_77973_b");
        Object reg = null;
        Method regName = null;
        try {
            reg = Rwg2Dump.field(x.cls("net.minecraft.item.Item"), "itemRegistry", "field_150901_e").get(null);
            regName = Extras.named(reg.getClass(), new Class<?>[] { Object.class }, "getNameForObject", "func_148750_c");
        } catch (Throwable ignored) {}
        j.key("chests").open('[');
        int n = 0;
        for (int cz = -FORT_R + 1; cz <= FORT_R - 1; cz++)
            for (int cx = -FORT_R + 1; cx <= FORT_R - 1; cx++) {
                Object chunk = getChunk.invoke(nether, cx, cz);
                Object map = Extras.fieldAny(chunk, "chunkTileEntityMap", "field_150816_i");
                if (!(map instanceof Map)) continue;
                for (Object te : new ArrayList<Object>(((Map<?, ?>) map).values())) {
                    if (!invC.isInstance(te)) continue;
                    Object tx = Extras.fieldAny(te, "xCoord", "field_145851_c"), ty = Extras.fieldAny(te, "yCoord", "field_145848_d"),
                            tz = Extras.fieldAny(te, "zCoord", "field_145849_e");
                    j.open('{').key("class").str(te.getClass().getName()).key("pos").raw("[" + tx + "," + ty + "," + tz + "]");
                    j.key("slots").open('[');
                    int sz = (Integer) size.invoke(te);
                    for (int i = 0; i < sz; i++) {
                        Object st = slot.invoke(te, i);
                        if (st == null) continue;
                        String id = "?";
                        try {
                            id = String.valueOf(regName.invoke(reg, gItem.invoke(st)));
                        } catch (Throwable ignored) {}
                        j.open('[').val((long) i).str(String.valueOf(st)).str(id).close(']');
                    }
                    j.close(']').close('}');
                    n++;
                }
            }
        j.close(']');
        j.key("chestCount").val(n);
        if (!ld.errors.isEmpty()) {
            j.key("loadErrors").open('[');
            for (String e : ld.errors) j.str(e);
            j.close(']');
        }
    }
}
