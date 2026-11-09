<script lang="ts">
  let {
    size = $bindable(),
    min = 100,
    max = 1000,
    vertical = false,
    invert = false,
    quiet = false,
    onend,
  }: { size: number; min?: number; max?: number; vertical?: boolean; invert?: boolean; quiet?: boolean; onend?: () => void } = $props();

  let dragging = $state(false);

  function down(e: PointerEvent) {
    e.preventDefault();
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    dragging = true;
    const start = vertical ? e.clientY : e.clientX;
    const startSize = size;
    const move = (ev: PointerEvent) => {
      const d = (vertical ? ev.clientY : ev.clientX) - start;
      size = Math.max(min, Math.min(max, startSize + (invert ? -d : d)));
    };
    const up = () => {
      dragging = false;
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      onend?.();
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
  }
</script>

<div class="splitter" class:vertical class:dragging class:quiet onpointerdown={down} role="separator" aria-orientation={vertical ? "horizontal" : "vertical"}></div>

<style>
  .splitter {
    flex: none;
    width: 8px;
    margin: 0 -3px;
    z-index: 5;
    cursor: col-resize;
    position: relative;
  }
  .splitter::after {
    content: "";
    position: absolute;
    top: 0;
    bottom: 0;
    left: 3.5px;
    width: 1px;
    background: var(--border);
    transition: background var(--dur), width var(--dur), left var(--dur);
  }
  .splitter:hover::after,
  .splitter.dragging::after {
    background: var(--accent);
    width: 2px;
    left: 3px;
  }
  /* Between panes that already have their own edges: no resting line, only a rounded handle
     (inset from the ends) while hovering or dragging. */
  .splitter.quiet::after {
    top: 14px;
    bottom: 14px;
    border-radius: 2px;
    background: transparent;
  }
  .splitter.quiet:hover::after,
  .splitter.quiet.dragging::after {
    background: color-mix(in srgb, var(--accent) 70%, transparent);
  }
  .splitter.vertical {
    width: auto;
    height: 8px;
    margin: -3px 0;
    cursor: row-resize;
  }
  .splitter.vertical::after {
    top: 3.5px;
    bottom: auto;
    left: 0;
    right: 0;
    width: auto;
    height: 1px;
  }
  .splitter.vertical:hover::after,
  .splitter.vertical.dragging::after {
    height: 2px;
    top: 3px;
    width: auto;
    left: 0;
  }
</style>
