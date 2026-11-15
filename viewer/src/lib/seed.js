// Minecraft 1.7.10 seed semantics: a string that parses as a long is used as-is, anything else is
// String.hashCode() (a signed 32-bit int widened to long).

const I64_MIN = -(1n << 63n);
const I64_MAX = (1n << 63n) - 1n;

export function javaStringHash(s) {
  let h = 0;
  for (let i = 0; i < s.length; i++) h = (Math.imul(31, h) + s.charCodeAt(i)) | 0;
  return h;
}

/** @returns {{ seed: bigint, kind: 'number' | 'text' } | null} */
export function parseSeed(text) {
  const t = text.trim();
  if (t === '') return null;
  if (/^[+-]?\d+$/.test(t)) {
    const v = BigInt(t);
    if (v >= I64_MIN && v <= I64_MAX) return { seed: v, kind: 'number' };
  }
  return { seed: BigInt(javaStringHash(t)), kind: 'text' };
}

export function randomSeed() {
  const a = new BigUint64Array(1);
  crypto.getRandomValues(a);
  return BigInt.asIntN(64, a[0]);
}
