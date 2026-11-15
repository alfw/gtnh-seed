package gnu.trove.map.hash;

/** Oracle shim: only the subset RWG2's util classes use (caches only, no semantic effect). */
public class TLongObjectHashMap<V> {
    private final java.util.HashMap<Long, V> m = new java.util.HashMap<Long, V>();
    public V get(long k) { return m.get(k); }
    public V put(long k, V v) { return m.put(k, v); }
    public int size() { return m.size(); }
    public void clear() { m.clear(); }
}
