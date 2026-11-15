/** Distance in blocks, rounded more the further it is (shared by the map and the lazy street view). */
export function fmtBlocks(d) {
  const r = d < 100 ? 1 : d < 1000 ? 10 : 50;
  return `${(Math.round(d / r) * r).toLocaleString('en-US')} blocks`;
}
