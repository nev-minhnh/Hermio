<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import { fly } from "svelte/transition";
  import Icon from "../lib/Icon.svelte";
  import { store } from "../lib/store.svelte";

  // Hide by itself after a while; it never steals focus or blocks the window.
  $effect(() => {
    if (!store.suggestPopup) return;
    const h = setTimeout(() => (store.suggestPopup = 0), 20000);
    return () => clearTimeout(h);
  });
</script>

{#if store.suggestPopup > 0}
  <div class="pop card" role="status" aria-live="polite" transition:fly={{ y: -12, duration: 200 }}>
    <div class="ico"><Icon name="sparkle" size={18} /></div>
    <div class="grow">
      <b>{t("Group suggestions are ready")}</b>
      <div class="muted small">{t("The NPU is awake again and sorted {n} new emails that arrived while it slept.", { n: store.suggestPopup })}</div>
      <div class="acts">
        <button class="btn primary" onclick={() => store.reviewSuggestions()}>{t("Review")}</button>
        <button class="btn subtle" onclick={() => (store.suggestPopup = 0)}>{t("Later")}</button>
      </div>
    </div>
    <button class="icon-btn" title={t("Close")} onclick={() => (store.suggestPopup = 0)}><Icon name="close" size={14} /></button>
  </div>
{/if}

<style>
  .pop {
    position: fixed;
    right: 18px;
    top: 54px;
    z-index: 900;
    width: min(360px, calc(100vw - 36px));
    display: flex;
    gap: 12px;
    padding: 12px 10px 12px 14px;
    box-shadow: var(--shadow-lg, 0 12px 32px rgba(0, 0, 0, 0.18));
  }
  .ico {
    width: 34px;
    height: 34px;
    flex: none;
    border-radius: 10px;
    display: grid;
    place-items: center;
    color: white;
    background: var(--brand-grad);
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .acts {
    display: flex;
    gap: 8px;
    margin-top: 10px;
  }
  .small {
    line-height: 1.45;
    margin-top: 2px;
  }
</style>
