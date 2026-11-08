<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import Icon from "../lib/Icon.svelte";
  import { store } from "../lib/store.svelte";
  import { fly, fade } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { cubicOut } from "svelte/easing";
</script>

<div class="toasts" aria-live="polite">
  {#each store.toasts as toast (toast.id)}
    <div class="toast {toast.kind}" in:fly={{ y: 16, duration: 220, easing: cubicOut }} out:fade={{ duration: 160 }} animate:flip={{ duration: 200 }}>
      <Icon name={toast.kind === "error" ? "warning" : toast.kind === "success" ? "check" : "mail"} size={16} />
      <span class="txt">{toast.text}</span>
      {#if toast.action}
        <button class="btn subtle" onclick={() => { toast.action?.run(); store.dismiss(toast.id); }}>{toast.action.label}</button>
      {/if}
      <button class="x" onclick={() => store.dismiss(toast.id)} title={t("Close")}><Icon name="close" size={14} /></button>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    bottom: 22px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 2000;
    pointer-events: none;
  }
  .toast {
    pointer-events: auto;
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 280px;
    max-width: 560px;
    padding: 9px 8px 9px 14px;
    border-radius: var(--radius-l);
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-l);
    animation: slide-up 200ms var(--ease);
    font-size: 0.92rem;
  }
  .toast.error :global(.icon) { color: var(--danger); }
  .toast.success :global(.icon) { color: var(--success); }
  .toast.info :global(.icon) { color: var(--text-accent); }
  .txt { flex: 1; user-select: text; word-break: break-word; }
  .x {
    border: none; background: none; display: flex; padding: 4px; border-radius: 4px; color: var(--text-3);
  }
  .x:hover { background: var(--hover); }
</style>
