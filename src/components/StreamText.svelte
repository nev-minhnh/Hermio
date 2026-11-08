<script lang="ts">
  /**
   * Smooth streaming text. Tokens arrive in bursts; this reveals them at a steady, adaptive pace
   * (catching up quickly when a big chunk lands), renders finished blocks once, fades new blocks
   * in and keeps a soft caret at the end while the answer is being written.
   */
  import { onDestroy, untrack } from "svelte";
  import { escapeHtml } from "../lib/util";
  import { reduced } from "../lib/motion";

  let {
    text,
    streaming = false,
    render,
    onstep,
    class: cls = "",
  }: {
    text: string;
    streaming?: boolean;
    /** Markdown → HTML for one block; plain text when omitted. */
    render?: (s: string) => string;
    /** Called after each reveal step (e.g. to keep the view scrolled to the bottom). */
    onstep?: () => void;
    class?: string;
  } = $props();

  // A message that is already complete when shown (history, re-render) appears at once.
  const animated = untrack(() => streaming) && !reduced();
  let shown = $state(untrack(() => (animated ? 0 : text.length)));
  let raf = 0;
  let last = 0;

  $effect(() => {
    const target = text.length;
    if (!animated) {
      shown = target;
      return;
    }
    if (shown > target) shown = target; // text was replaced
    if (shown < target && !raf) raf = requestAnimationFrame(step);
  });

  function step(now: number) {
    raf = 0;
    const dt = last ? Math.min(64, now - last) : 16;
    last = now;
    const backlog = text.length - shown;
    if (backlog <= 0) {
      last = 0;
      return;
    }
    // ~45 chars/s minimum, and never more than ~0.25 s behind the model.
    const n = Math.max(Math.ceil((dt / 1000) * 45), Math.ceil(backlog * Math.min(1, dt / 250)));
    let next = Math.min(text.length, shown + n);
    // Don't split a surrogate pair (emoji…).
    const c = text.charCodeAt(next - 1);
    if (c >= 0xd800 && c <= 0xdbff && next < text.length) next++;
    shown = next;
    onstep?.();
    if (shown < text.length) raf = requestAnimationFrame(step);
    else last = 0;
  }
  onDestroy(() => cancelAnimationFrame(raf));

  const visible = $derived(shown >= text.length ? text : text.slice(0, shown));

  /** Blocks: one per paragraph/heading, consecutive list items together, fenced code together. */
  function split(src: string): string[] {
    if (!render) return [src];
    const out: string[] = [];
    let list = false;
    let fence = false;
    for (const line of src.split("\n")) {
      const isList = /^\s*([-*•]|\d+[.)])\s+/.test(line);
      if (fence) {
        out[out.length - 1] += "\n" + line;
        if (/^\s*```/.test(line)) fence = false;
        continue;
      }
      if (/^\s*```/.test(line)) {
        out.push(line);
        fence = true;
        list = false;
        continue;
      }
      if (!line.trim()) {
        list = false;
        continue;
      }
      if (isList && list) out[out.length - 1] += "\n" + line;
      else out.push(line);
      list = isList;
    }
    return out;
  }
  const blocks = $derived(split(visible));
  const live = $derived(streaming || shown < text.length);
</script>

<div class="stream {cls}" class:live class:plain={!render}>
  {#each blocks as b, i (i)}
    <div class="sb" class:fresh={animated}>{@html render ? render(b) : escapeHtml(b)}</div>
  {/each}
</div>

<style>
  .stream.plain .sb {
    white-space: pre-wrap;
  }
  .sb.fresh {
    animation: block-in 240ms cubic-bezier(0.2, 0.8, 0.2, 1);
  }
  @keyframes block-in {
    from {
      opacity: 0;
      translate: 0 4px;
    }
  }
  /* Soft caret right after the last character while writing. */
  .live .sb:last-child > :global(p:last-child)::after,
  .live .sb:last-child > :global(h4:last-child)::after,
  .live .sb:last-child > :global(ul:last-child > li:last-child)::after,
  .live.plain .sb:last-child::after,
  .live:empty::after {
    content: "";
    display: inline-block;
    width: 0.5em;
    height: 1em;
    margin-left: 2px;
    vertical-align: -0.15em;
    border-radius: 2px;
    background: currentColor;
    opacity: 0.55;
    animation: caret 1s ease-in-out infinite;
  }
  @keyframes caret {
    50% {
      opacity: 0.12;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .sb.fresh {
      animation: none;
    }
  }
</style>
