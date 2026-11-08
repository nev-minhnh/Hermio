<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import type { Snippet } from "svelte";
  import Icon from "../lib/Icon.svelte";
  import { fade, scale } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  let { title, width = 520, onclose, children, footer }: { title: string; width?: number; onclose: () => void; children: Snippet; footer?: Snippet } = $props();
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<div class="backdrop" transition:fade|global={{ duration: 150 }} onpointerdown={(e) => e.target === e.currentTarget && onclose()} role="presentation">
  <div class="dialog" transition:scale|global={{ start: 0.95, duration: 180, easing: cubicOut }} style:width="{width}px" role="dialog" aria-modal="true" aria-label={title}>
    <div class="head">
      <h3>{title}</h3>
      <button class="icon-btn" onclick={onclose} title={t("Close")}><Icon name="close" size={16} /></button>
    </div>
    <div class="content">{@render children()}</div>
    {#if footer}<div class="foot">{@render footer()}</div>{/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed; inset: 0; z-index: 1500;
    background: rgba(0, 0, 0, 0.32);
    display: flex; align-items: center; justify-content: center;
  }
  .dialog {
    max-width: calc(100vw - 40px);
    max-height: calc(100vh - 60px);
    display: flex; flex-direction: column;
    background: var(--surface);
    border-radius: 12px;
    border: 1px solid var(--border);
    box-shadow: var(--shadow-l);
  }
  .head { display: flex; align-items: center; padding: 16px 14px 6px 22px; }
  h3 { flex: 1; margin: 0; font-family: var(--font-display); font-size: 1.15rem; font-weight: 600; }
  .content { padding: 8px 22px 16px; overflow: auto; display: flex; flex-direction: column; gap: 12px; }
  .foot { display: flex; justify-content: flex-end; gap: 8px; padding: 12px 22px 18px; }
</style>
