<script>
  // Searchable replacement for a long <select>: a button showing the current choice; opening it shows a filter
  // field and the grouped list below it (inline, so it is never clipped by the scrolling sidebar).
  // groups: [{ label, items: [{ value, label, hint? }] }]
  let { value = $bindable(), groups = [], title = '', placeholder = 'type to filter…', class: cls = '', onchange } = $props();

  let open = $state(false);
  let q = $state('');
  let active = $state(0);
  let root, field, list;

  const same = (a, b) => String(a) === String(b);
  let current = $derived(groups.flatMap((g) => g.items).find((it) => same(it.value, value)));
  let shown = $derived.by(() => {
    const words = q.trim().toLowerCase().split(/\s+/).filter(Boolean);
    const hit = (s) => words.every((w) => s.includes(w));
    const out = [];
    for (const g of groups) {
      const glab = (g.label ?? '').toLowerCase();
      const items = words.length ? g.items.filter((it) => hit(`${it.label} ${it.hint ?? ''} ${glab}`.toLowerCase())) : g.items;
      if (items.length) out.push({ label: g.label, items });
    }
    return out;
  });
  let flat = $derived(shown.flatMap((g) => g.items));

  function show() {
    open = true;
    q = '';
    active = Math.max(0, flat.findIndex((it) => same(it.value, value)));
    queueMicrotask(() => {
      field?.focus();
      list?.querySelector('.cur')?.scrollIntoView({ block: 'nearest' });
    });
  }
  function pick(it) {
    open = false;
    if (!same(it.value, value)) {
      value = it.value;
      onchange?.(it.value);
    }
  }
  function key(e) {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      if (!flat.length) return;
      active = (active + (e.key === 'ArrowDown' ? 1 : flat.length - 1)) % flat.length;
      queueMicrotask(() => list?.querySelector('.act')?.scrollIntoView({ block: 'nearest' }));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (flat[active]) pick(flat[active]);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      open = false;
    }
  }
  $effect(() => {
    if (!open) return;
    const away = (e) => { if (root && !root.contains(e.target)) open = false; };
    document.addEventListener('pointerdown', away, true);
    return () => document.removeEventListener('pointerdown', away, true);
  });
  $effect(() => { q; active = 0; });
</script>

<div class="picker {cls}" bind:this={root}>
  <button type="button" class="trigger" class:open {title} onclick={() => (open ? (open = false) : show())} aria-haspopup="listbox" aria-expanded={open}>
    <span class="cur-label">{current?.label ?? '—'}</span>{#if current?.hint}<span class="hint-t">{current.hint}</span>{/if}<span class="caret">▾</span>
  </button>
  {#if open}
    <div class="pop">
      <input bind:this={field} bind:value={q} {placeholder} onkeydown={key} aria-label="Filter" />
      <div class="list" role="listbox" bind:this={list}>
        {#each shown as g (g.label)}
          {#if g.label}<div class="glabel">{g.label}</div>{/if}
          {#each g.items as it (it.value)}
            {@const idx = flat.indexOf(it)}
            <button type="button" role="option" aria-selected={same(it.value, value)} class:cur={same(it.value, value)} class:act={idx === active}
              onpointerenter={() => (active = idx)} onclick={() => pick(it)}>
              <span>{it.label}</span>{#if it.hint}<span class="hint-t">{it.hint}</span>{/if}
            </button>
          {/each}
        {:else}
          <div class="none dim">no match</div>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .picker { position: relative; min-width: 0; }
  .trigger { display: flex; align-items: center; gap: 6px; width: 100%; text-align: left; background: #0e1216; }
  .trigger.open { border-color: var(--accent); }
  .cur-label { flex: 1 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .caret { color: var(--dim); flex: none; }
  .hint-t { color: var(--dim); font-size: 11px; flex: none; }
  .pop { margin-top: 4px; border: 1px solid var(--line); border-radius: 6px; background: #0e1216; padding: 4px; }
  .pop input { width: 100%; margin-bottom: 4px; }
  .list { max-height: 260px; overflow-y: auto; overscroll-behavior: contain; }
  .glabel { color: var(--dim); font-size: 11px; text-transform: uppercase; letter-spacing: .04em; padding: 6px 6px 2px; }
  .list button { display: flex; align-items: baseline; gap: 6px; width: 100%; text-align: left; border: 0; border-radius: 4px; background: none; padding: 5px 8px; }
  .list button span:first-child { flex: 1 1 auto; }
  .list button.act { background: #2a3340; }
  .list button.cur { color: var(--accent); }
  .none { padding: 6px 8px; }
</style>
