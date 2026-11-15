package rwg2dump;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.ObjectInputStream;
import java.io.ObjectOutputStream;
import java.lang.reflect.Field;
import java.lang.reflect.Modifier;
import java.util.ArrayList;
import java.util.List;
import java.util.Random;
import java.util.concurrent.atomic.AtomicLong;

import cpw.mods.fml.common.eventhandler.EventPriority;
import cpw.mods.fml.common.eventhandler.SubscribeEvent;
import net.minecraftforge.event.terraingen.DecorateBiomeEvent;
import net.minecraftforge.event.terraingen.PopulateChunkEvent;

/**
 * v2.16: observes populate / decorate events of the overworld while {@link #active} and records, in order, the
 * chunk, nesting depth and the event's Random state (HIGHEST priority = before any other listener drew; LOWEST =
 * after them). Registered on both Forge buses (Pre/Post on EVENT_BUS, Populate/Decorate on TERRAIN_GEN_BUS).
 *
 * Record: {@code [seq, kind, cx, cz, depth, type, rand]}; kind: popPre, popPreLate (LOWEST), popPost, populate,
 * decPre, decPost, decorate; cx/cz = chunk (decorate events carry block coords: converted with >> 4); type = the
 * Populate/Decorate EventType or ""; rand = state string (see {@link #state}).
 */
public final class Hooks216 {

    static volatile boolean active;
    /** v2.18: LOWEST-priority twins of Populate / decorate Pre / Post, and the firing stack of CUSTOM / ICE / ANIMALS
     * populate events and of every decPre / decPost (who fires them, who may reseed the shared Random). */
    static volatile boolean trace;
    static Object world; // only this world's events are recorded
    static final List<String> log = new ArrayList<String>();
    static int seq, depth;
    static String stateMethod = "none";

    static void reset(Object w) {
        synchronized (log) {
            log.clear();
        }
        world = w;
        depth = 0;
    }

    static List<String> drain() {
        synchronized (log) {
            List<String> r = new ArrayList<String>(log);
            log.clear();
            return r;
        }
    }

    static Object get(Object e, String f) {
        try {
            return e.getClass().getField(f).get(e);
        } catch (Throwable t) {
            return null;
        }
    }

    static int geti(Object e, String f) {
        Object o = get(e, f);
        return o instanceof Integer ? (Integer) o : Integer.MIN_VALUE;
    }

    static void rec(Object e, String kind, boolean blockCoords) {
        if (!active) return;
        try {
            Object w = get(e, "world");
            if (world != null && w != world) return;
            int cx = geti(e, "chunkX"), cz = geti(e, "chunkZ");
            if (blockCoords && cx != Integer.MIN_VALUE) {
                cx >>= 4;
                cz >>= 4;
            }
            Object type = get(e, "type");
            String st = state(get(e, "rand"));
            String ty = type == null ? "" : String.valueOf(type);
            String line = "[" + (seq++) + ",\"" + kind + "\"," + cx + "," + cz + "," + depth + ",\""
                    + ty + "\"," + (st == null ? "null" : "\"" + st + "\"");
            if (trace && !kind.endsWith("Late") && (kind.startsWith("dec") && !kind.equals("decorate")
                    || kind.equals("populate") && (ty.equals("CUSTOM") || ty.equals("ICE") || ty.equals("ANIMALS"))))
                line += "," + stack();
            line += "]";
            synchronized (log) {
                if (log.size() < 400000) log.add(line);
            }
        } catch (Throwable ignored) {}
    }

    /**
     * Random state: {@code "s:<hex>"} = the internal 48-bit `seed` (scrambled, as `Random.seed` holds it), read by
     * reflection; {@code "o:<int>,<int>"} = first two `nextInt()` of a serialised copy (state recoverable by a 2^16
     * search) when reflection is closed; then {@code "|f:<field>=<state>"} for every `Random`-typed field of a
     * subclass (BOP's RandomForcedPositiveOwned keeps the shared Random in one). The original is never drawn from.
     */
    static String state(Object r) {
        if (!(r instanceof Random)) return r == null ? null : "?" + r.getClass().getName();
        StringBuilder sb = new StringBuilder(one((Random) r));
        if (r.getClass() != Random.class) {
            sb.append("|c:").append(r.getClass().getName());
            for (Class<?> k = r.getClass(); k != null && k != Random.class; k = k.getSuperclass())
                for (Field f : k.getDeclaredFields()) {
                    if (Modifier.isStatic(f.getModifiers())) continue;
                    try {
                        f.setAccessible(true);
                        Object v = f.get(r);
                        if (v instanceof Random && v != r) sb.append("|f:").append(f.getName()).append('=').append(one((Random) v));
                    } catch (Throwable ignored) {}
                }
        }
        return sb.toString();
    }

    static Field seedField;
    static boolean seedFieldTried;

    static String one(Random r) {
        if (!seedFieldTried) {
            seedFieldTried = true;
            try {
                Field f = Random.class.getDeclaredField("seed");
                f.setAccessible(true);
                seedField = f;
                stateMethod = "reflection";
            } catch (Throwable t) {
                stateMethod = "serialization (" + t.getClass().getSimpleName() + ")";
            }
        }
        if (seedField != null)
            try {
                return "s:" + Long.toHexString(((AtomicLong) seedField.get(r)).get());
            } catch (Throwable ignored) {}
        try {
            ByteArrayOutputStream bo = new ByteArrayOutputStream();
            ObjectOutputStream oo = new ObjectOutputStream(bo);
            // a plain Random with the same state: copy via the serialised form of java.util.Random only
            oo.writeObject(r);
            oo.close();
            Object c = new ObjectInputStream(new ByteArrayInputStream(bo.toByteArray())).readObject();
            Random cr = (Random) c;
            return "o:" + cr.nextInt() + "," + cr.nextInt();
        } catch (Throwable t) {
            return "x:" + t.getClass().getSimpleName();
        }
    }

    /** JSON array of the frames that fired the event (event-bus / reflection / own frames skipped), at most 16. */
    static String stack() {
        StringBuilder sb = new StringBuilder("[");
        int n = 0;
        for (StackTraceElement f : Thread.currentThread().getStackTrace()) {
            String c = f.getClassName();
            if (c.startsWith("java.lang.Thread") || c.startsWith("rwg2dump.") || c.startsWith("cpw.mods.fml.common.eventhandler")
                    || c.startsWith("sun.reflect") || c.startsWith("jdk.internal") || c.startsWith("java.lang.reflect")
                    || c.contains("ASMEventHandler"))
                continue;
            if (n > 0) sb.append(',');
            sb.append('"').append(c).append('.').append(f.getMethodName()).append(':').append(f.getLineNumber()).append('"');
            if (++n >= 16) break;
        }
        return sb.append(']').toString();
    }

    // ------------------------------------------------------------------------------ listeners (public for ASM)

    @SubscribeEvent(priority = EventPriority.HIGHEST)
    public void popPre(PopulateChunkEvent.Pre e) {
        if (active && (world == null || get(e, "world") == world)) depth++;
        rec(e, "popPre", false);
    }

    @SubscribeEvent(priority = EventPriority.LOWEST)
    public void popPreLate(PopulateChunkEvent.Pre e) {
        rec(e, "popPreLate", false);
    }

    @SubscribeEvent(priority = EventPriority.HIGHEST)
    public void popPost(PopulateChunkEvent.Post e) {
        rec(e, "popPost", false);
        if (active && (world == null || get(e, "world") == world)) depth--;
    }

    @SubscribeEvent(priority = EventPriority.HIGHEST)
    public void populate(PopulateChunkEvent.Populate e) {
        rec(e, "populate", false);
    }

    @SubscribeEvent(priority = EventPriority.LOWEST)
    public void populateLate(PopulateChunkEvent.Populate e) {
        if (trace) rec(e, "populateLate", false);
    }

    @SubscribeEvent(priority = EventPriority.LOWEST)
    public void popPostLate(PopulateChunkEvent.Post e) {
        if (trace) rec(e, "popPostLate", false);
    }

    @SubscribeEvent(priority = EventPriority.LOWEST)
    public void decPreLate(DecorateBiomeEvent.Pre e) {
        if (trace) rec(e, "decPreLate", true);
    }

    @SubscribeEvent(priority = EventPriority.HIGHEST)
    public void decPre(DecorateBiomeEvent.Pre e) {
        rec(e, "decPre", true);
    }

    @SubscribeEvent(priority = EventPriority.HIGHEST)
    public void decPost(DecorateBiomeEvent.Post e) {
        rec(e, "decPost", true);
    }

    @SubscribeEvent(priority = EventPriority.HIGHEST)
    public void decorate(DecorateBiomeEvent.Decorate e) {
        rec(e, "decorate", true);
    }

    /** Registers one instance on both buses; returns null or the error. */
    static String register(ClassLoader cl) {
        try {
            Class<?> mf = Class.forName("net.minecraftforge.common.MinecraftForge", true, cl);
            Object h = new Hooks216();
            List<String> done = new ArrayList<String>();
            for (String bus : new String[] { "EVENT_BUS", "TERRAIN_GEN_BUS" }) {
                Object b = mf.getField(bus).get(null);
                b.getClass().getMethod("register", Object.class).invoke(b, h);
                done.add(bus);
            }
            instance = h;
            return null;
        } catch (Throwable t) {
            return Extras.err(t);
        }
    }

    static Object instance;

    static void unregister(ClassLoader cl) {
        active = false;
        if (instance == null) return;
        try {
            Class<?> mf = Class.forName("net.minecraftforge.common.MinecraftForge", true, cl);
            for (String bus : new String[] { "EVENT_BUS", "TERRAIN_GEN_BUS" }) {
                Object b = mf.getField(bus).get(null);
                b.getClass().getMethod("unregister", Object.class).invoke(b, instance);
            }
        } catch (Throwable ignored) {}
        instance = null;
    }
}
