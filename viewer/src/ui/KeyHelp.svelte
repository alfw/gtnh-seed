<script>
  // `?`: every keyboard shortcut in one place
  let { onclose } = $props();
  const MAP = [
    ['+ / −', 'zoom in / out (or wheel, pinch, the − + buttons)'],
    ['0', 'centre on 0, 0'],
    ['double-click', 'zoom in there'],
    ['right-click', 'menu: /tp, marker, 3D view, search centre, measure, radius'],
    ['P', 'place a marker (shift-click keeps placing)'],
    ['M', 'measure (shift-click adds points)'],
    ['R', 'radius circle'],
    ['V', '3D view: click where to stand'],
    ['N', 'jump to the Nether / Overworld at the same spot'],
    ['C', 'copy /tp of the selected marker'],
    ['Del', 'delete the selected marker'],
    ['[ / ]', 'previous / next search hit'],
    ['Esc', 'close popup or menu, clear the tool'],
    ['?', 'this help'],
  ];
  const VIEW3D = [
    ['drag', 'look around'],
    ['W A S D', 'move'],
    ['space / shift', 'up / down (flying)'],
    ['F', 'walk / fly'],
    ['B', 'biome colours'],
    ['H', 'height colours'],
    ['X', 'x-ray (caves, structures, ore boxes)'],
    ['M', 'minimap'],
    ['L', 'layers panel'],
    ['Esc', 'back to the map'],
  ];
</script>

<div class="keyhelp-back" role="presentation" onclick={onclose}>
  <div class="keyhelp" role="dialog" aria-label="Keyboard shortcuts" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.key === 'Escape' && onclose()}>
    <div class="popup-head"><strong>Keyboard shortcuts</strong><button class="small" onclick={onclose} title="Close">×</button></div>
    <div class="kh-cols">
      <div>
        <div class="dim tiny">Map</div>
        {#each MAP as [k, d]}<div class="kh-row"><kbd>{k}</kbd><span>{d}</span></div>{/each}
      </div>
      <div>
        <div class="dim tiny">3D view</div>
        {#each VIEW3D as [k, d]}<div class="kh-row"><kbd>{k}</kbd><span>{d}</span></div>{/each}
      </div>
    </div>
  </div>
</div>
