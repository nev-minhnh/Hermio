<script lang="ts" module>
  export type MenuItem =
    | { sep: true }
    | { label: string; icon?: string; run: () => void; danger?: boolean; header?: false; sep?: false; color?: string; disabled?: boolean; checked?: boolean }
    | { header: true; label: string; sep?: false };
</script>

<script lang="ts">
  import { fade } from "svelte/transition";
  import { onMount } from "svelte";
  import Icon from "../lib/Icon.svelte";

  let { x, y, items, onclose }: { x: number; y: number; items: MenuItem[]; onclose: () => void } = $props();
  let el: HTMLDivElement;
  let pos = $state({ left: 0, top: 0 });

  onMount(() => {
    const r = el.getBoundingClientRect();
    pos = {
      left: Math.min(x, window.innerWidth - r.width - 8),
      top: Math.min(y, window.innerHeight - r.height - 8),
    };
    const close = (e: Event) => {
      if (!el.contains(e.target as Node)) onclose();
    };
    const key = (e: KeyboardEvent) => e.key === "Escape" && onclose();
    setTimeout(() => {
      window.addEventListener("pointerdown", close, true);
      window.addEventListener("keydown", key);
      window.addEventListener("blur", onclose);
    });
    return () => {
      window.removeEventListener("pointerdown", close, true);
      window.removeEventListener("keydown", key);
      window.removeEventListener("blur", onclose);
    };
  });
</script>

<div class="menu" out:fade={{ duration: 90 }} bind:this={el} style:left="{pos.left || x}px" style:top="{pos.top || y}px" role="menu">
  {#each items as it}
    {#if it.sep}
      <hr />
    {:else if it.header}
      <div class="label">{it.label}</div>
    {:else}
      <button
        class:danger={it.danger}
        disabled={it.disabled}
        onclick={() => {
          onclose();
          it.run();
        }}
      >
        {#if it.color}<span class="dot" style:background={it.color}></span>{:else if it.icon}<Icon name={it.icon} size={16} />{:else}<span style="width:16px"></span>{/if}
        <span class="lbl">{it.label}</span>
        {#if it.checked}<Icon name="check" size={14} />{/if}
      </button>
    {/if}
  {/each}
</div>

<style>
  .menu {
    animation: pop 140ms var(--ease);
  }
  .dot {
    width: 10px;
    height: 10px;
    margin: 0 3px;
    border-radius: 3px;
    flex: none;
  }
  .lbl {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  button:disabled {
    opacity: 0.55;
  }
  .danger {
    color: var(--danger);
  }
</style>
