package rwg2dump;

import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.Map;

/**
 * Ground-truth vanilla structures (village, stronghold, mineshaft) for the *loaded* overworld, read from the very
 * generator instances RWG2's ChunkGeneratorRealistic uses (so modded replacements via getModdedMapGen are covered).
 *
 * Starts are produced by MapGenBase.func_151539_a(provider, world, chunkX, chunkZ, null): the same path vanilla's
 * recreateStructures uses. With null blocks it only records StructureStarts (no block changes), but it does write
 * the world's structure .dat files, so run on a throwaway world copy.
 *
 * Reflection only, members matched by type/signature (production names are SRG).
 */
final class Structures {

    private final ClassLoader cl;

    Structures(ClassLoader cl) {
        this.cl = cl;
    }

    /** Seed of the loaded overworld (via RWG2's generator field), or null. */
    Long worldSeed() {
        try {
            Class<?> dm = Class.forName("net.minecraftforge.common.DimensionManager", true, cl);
            Object world = dm.getMethod("getWorld", int.class).invoke(null, 0);
            Object cps = fieldByTypeName(world, "ChunkProviderServer");
            Class<?> icp = Class.forName("net.minecraft.world.chunk.IChunkProvider", true, cl);
            for (Class<?> k = cps.getClass(); k != null; k = k.getSuperclass())
                for (Field f : k.getDeclaredFields())
                    if (!Modifier.isStatic(f.getModifiers()) && f.getType() == icp) {
                        f.setAccessible(true);
                        return (Long) readField(f.get(cps), "worldSeed");
                    }
        } catch (Throwable ignored) {}
        return null;
    }

    String dump() throws Exception {
        Class<?> dm = Class.forName("net.minecraftforge.common.DimensionManager", true, cl);
        Object world = dm.getMethod("getWorld", int.class).invoke(null, 0);
        if (world == null) throw new IllegalStateException("overworld not loaded");
        Object cps = fieldByTypeName(world, "ChunkProviderServer");
        Class<?> iChunkProvider = Class.forName("net.minecraft.world.chunk.IChunkProvider", true, cl);
        Object gen = null;
        for (Class<?> k = cps.getClass(); k != null && gen == null; k = k.getSuperclass())
            for (Field f : k.getDeclaredFields())
                if (!Modifier.isStatic(f.getModifiers()) && f.getType() == iChunkProvider) {
                    f.setAccessible(true);
                    gen = f.get(cps);
                    break;
                }
        if (gen == null) throw new IllegalStateException("current chunk provider not found");

        Rwg2Dump.Json j = new Rwg2Dump.Json();
        j.open('{');
        j.key("chunkGenerator").str(gen.getClass().getName());
        Long seed = (Long) readField(gen, "worldSeed");
        j.key("worldSeed").str(seed == null ? null : Long.toString(seed));
        Object continental = readField(gen, "continental");
        j.key("continental").raw(String.valueOf(continental));
        j.key("scanRadiusChunks").val(Rwg2Dump.STRUCTURE_RANGE_CHUNKS);

        for (String name : new String[] { "villageGenerator", "strongholdGenerator", "mineshaftGenerator" }) {
            Object mg = readField(gen, name);
            j.key(name).open('{');
            if (mg == null) {
                j.key("present").raw("false");
                j.close('}');
                continue;
            }
            j.key("class").str(mg.getClass().getName());
            try {
                scan(gen, world, mg, iChunkProvider);
                if (name.equals("strongholdGenerator")) writeStrongholdRing(j, mg);
                writeStarts(j, mg);
            } catch (Throwable t) {
                j.key("error").str(String.valueOf(t));
            }
            j.close('}');
        }
        j.close('}');
        return j.toString();
    }

    /** Drive the generator over the scan area so every start in range is created (func_151539_a, null blocks). */
    private void scan(Object provider, Object world, Object mapGen, Class<?> iChunkProvider) throws Exception {
        Method gen = null;
        for (Class<?> k = mapGen.getClass(); k != null && gen == null; k = k.getSuperclass())
            for (Method m : k.getDeclaredMethods()) {
                Class<?>[] p = m.getParameterTypes();
                if (p.length == 5 && p[0] == iChunkProvider && p[2] == int.class && p[3] == int.class
                        && p[4].isArray() && m.getReturnType() == void.class) {
                    m.setAccessible(true);
                    gen = m;
                    break;
                }
            }
        if (gen == null) throw new IllegalStateException("MapGenBase generate(provider, world, x, z, blocks) not found");
        int r = Rwg2Dump.STRUCTURE_RANGE_CHUNKS;
        // each call covers the 17x17 chunks around its centre (MapGenBase.range = 8)
        for (int cz = -r; cz <= r; cz += 17)
            for (int cx = -r; cx <= r; cx += 17) gen.invoke(mapGen, provider, world, cx, cz, null);
    }

    private void writeStrongholdRing(Rwg2Dump.Json j, Object mg) throws Exception {
        for (Class<?> k = mg.getClass(); k != null; k = k.getSuperclass())
            for (Field f : k.getDeclaredFields()) {
                if (Modifier.isStatic(f.getModifiers()) || !f.getType().isArray()
                        || !f.getType().getComponentType().getName().endsWith("ChunkCoordIntPair"))
                    continue;
                f.setAccessible(true);
                Object[] arr = (Object[]) f.get(mg);
                j.key("ring").open('[');
                if (arr != null) for (Object p : arr) {
                    if (p == null) continue;
                    int[] xz = ints(p, 2);
                    j.open('{');
                    j.key("chunkX").val(xz[0]);
                    j.key("chunkZ").val(xz[1]);
                    j.close('}');
                }
                j.close(']');
                return;
            }
    }

    private void writeStarts(Rwg2Dump.Json j, Object mg) throws Exception {
        Map<?, ?> map = null;
        for (Class<?> k = mg.getClass(); k != null && map == null; k = k.getSuperclass())
            for (Field f : k.getDeclaredFields())
                if (!Modifier.isStatic(f.getModifiers()) && Map.class.isAssignableFrom(f.getType())) {
                    f.setAccessible(true);
                    map = (Map<?, ?>) f.get(mg);
                    break;
                }
        j.key("starts").open('[');
        if (map != null) for (Map.Entry<?, ?> e : map.entrySet()) {
            long key = (Long) e.getKey();
            Object start = e.getValue();
            j.open('{');
            j.key("chunkX").val((int) key); // ChunkCoordIntPair.chunkXZ2Int: x & 0xffffffff | z << 32
            j.key("chunkZ").val((int) (key >>> 32));
            j.key("class").str(start == null ? null : start.getClass().getName());
            if (start != null) {
                Boolean sizeable = sizeable(start);
                j.key("sizeable").raw(String.valueOf(sizeable));
                List<int[]> boxes = new ArrayList<int[]>();
                Object bb = fieldByTypeName(start, "StructureBoundingBox");
                if (bb != null) boxes.add(ints(bb, 6));
                if (!boxes.isEmpty()) {
                    int[] b = boxes.get(0);
                    j.key("box").raw("[" + b[0] + "," + b[1] + "," + b[2] + "," + b[3] + "," + b[4] + "," + b[5] + "]");
                }
                Object comps = fieldOfType(start, java.util.LinkedList.class);
                j.key("components").val(comps instanceof Collection ? ((Collection<?>) comps).size() : -1);
                if (comps instanceof Collection) {
                    // piece classes (e.g. stronghold portal room) with their boxes
                    j.key("pieces").open('[');
                    for (Object piece : (Collection<?>) comps) {
                        j.open('{');
                        j.key("class").str(piece.getClass().getName());
                        Object pb = fieldByTypeName(piece, "StructureBoundingBox");
                        if (pb != null) {
                            int[] b = ints(pb, 6);
                            j.key("box").raw("[" + b[0] + "," + b[1] + "," + b[2] + "," + b[3] + "," + b[4] + "," + b[5] + "]");
                        }
                        j.close('}');
                    }
                    j.close(']');
                }
            }
            j.close('}');
        }
        j.close(']');
    }

    /** StructureStart.isSizeableStructure(): the first public boolean no-arg method up the hierarchy. */
    private static Boolean sizeable(Object start) {
        for (Class<?> k = start.getClass(); k != null && k != Object.class; k = k.getSuperclass())
            for (Method m : k.getDeclaredMethods())
                if (m.getReturnType() == boolean.class && m.getParameterTypes().length == 0
                        && Modifier.isPublic(m.getModifiers()) && !Modifier.isStatic(m.getModifiers())) {
                    try {
                        return (Boolean) m.invoke(start);
                    } catch (Throwable t) {
                        return null;
                    }
                }
        return null;
    }

    static Object readField(Object o, String name) {
        for (Class<?> k = o.getClass(); k != null; k = k.getSuperclass()) {
            try {
                Field f = k.getDeclaredField(name);
                f.setAccessible(true);
                return f.get(o);
            } catch (NoSuchFieldException e) {
                // keep looking
            } catch (IllegalAccessException e) {
                return null;
            }
        }
        return null;
    }

    static Object fieldByTypeName(Object o, String simpleName) throws IllegalAccessException {
        for (Class<?> k = o.getClass(); k != null; k = k.getSuperclass())
            for (Field f : k.getDeclaredFields())
                if (!Modifier.isStatic(f.getModifiers()) && f.getType().getName().endsWith(simpleName)) {
                    f.setAccessible(true);
                    return f.get(o);
                }
        return null;
    }

    static Object fieldOfType(Object o, Class<?> type) throws IllegalAccessException {
        for (Class<?> k = o.getClass(); k != null; k = k.getSuperclass())
            for (Field f : k.getDeclaredFields())
                if (!Modifier.isStatic(f.getModifiers()) && type.isAssignableFrom(f.getType())) {
                    f.setAccessible(true);
                    return f.get(o);
                }
        return null;
    }

    /** First `n` non-static int fields in declaration order (ChunkCoordIntPair x,z; StructureBoundingBox min/max). */
    static int[] ints(Object o, int n) throws IllegalAccessException {
        int[] out = new int[n];
        int i = 0;
        for (Field f : o.getClass().getDeclaredFields()) {
            if (i >= n) break;
            if (Modifier.isStatic(f.getModifiers()) || f.getType() != int.class) continue;
            f.setAccessible(true);
            out[i++] = f.getInt(o);
        }
        return out;
    }
}
