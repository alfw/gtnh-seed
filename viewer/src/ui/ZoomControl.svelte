<script>
  // − | level · scale | + (bottom right of the map; phone: top right, level only)
  let { zoom, min, max, onzoom } = $props();
  const scale = (z) => (z >= 0 ? `${Math.round(2 ** z * 10) / 10} px/block` : `${Math.round(2 ** -z * 10) / 10} blocks/px`);
</script>

<div class="zoomctl" role="group" aria-label="Zoom">
  <button onclick={() => onzoom(-1)} disabled={zoom <= min} title="Zoom out (−)">−</button>
  <button class="zl" onclick={() => onzoom(-zoom)} title="Zoom level (click: 1 block per pixel)">
    <b>{zoom > 0 ? '+' : ''}{Math.round(zoom * 10) / 10}</b>
    <span>{scale(zoom)}</span>
  </button>
  <button onclick={() => onzoom(1)} disabled={zoom >= max} title="Zoom in (+)">+</button>
</div>
