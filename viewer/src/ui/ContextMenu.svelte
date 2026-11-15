<script>
  // Right-click (desktop) / long-press (phone) menu for a map point. `menu` = { sx, sy, x, z, ground, copied, tp }.
  let { menu, other = null, onaction } = $props();
  const items = $derived([
    ['tp', `📋 ${menu.copied ? 'copied' : menu.tp}`],
    ['mark', '📍 Add marker here'],
    ['view', '👁 3D view from here'],
    ['search', '🔎 Use as search centre'],
    ['measure', '📏 Measure from here'],
    ['radius', '◯ Radius from here'],
    ['centre', '⌖ Centre map here'],
    ...(other ? [['other', `⇄ Open the ${other.name} at ${other.x}, ${other.z}`]] : []),
  ]);
</script>

<div class="ctxmenu" role="menu" style:left="{menu.sx}px" style:top="{menu.sy}px">
  <div class="ctx-head">
    X {menu.x} · Z {menu.z}<span class="dim"> · chunk {menu.x >> 4}, {menu.z >> 4}{menu.ground != null ? ` · Y${Math.floor(menu.ground)}` : ''}</span>
  </div>
  {#each items as [k, label] (k)}
    <button role="menuitem" onclick={() => onaction(k)}>{label}</button>
  {/each}
</div>
