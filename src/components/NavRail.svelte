<script lang="ts">
  import { scale } from "svelte/transition";
  import { t } from "../lib/i18n.svelte";
  import Icon from "../lib/Icon.svelte";
  import { store } from "../lib/store.svelte";

  const items = [
    { id: "mail", icon: "mail", label: t("Mail") },
    { id: "calendar", icon: "calendar", label: t("Calendar") },
  ] as const;
</script>

<nav class="rail">
  {#each items as it}
    <button class="item" class:on={store.route === it.id} title={it.label} onclick={() => (store.route = it.id)}>
      <Icon name={it.icon} size={22} />
      <span>{it.label}</span>
      {#if it.id === "mail" && store.counts.inbox > 0}
        <i class="dot" transition:scale={{ duration: 180, start: 0.5 }}>{store.counts.inbox > 99 ? "99+" : store.counts.inbox}</i>
      {/if}
    </button>
  {/each}
  <div class="spacer"></div>
  <button class="item" class:on={store.route === "settings"} title={t("Settings")} onclick={() => (store.route = "settings")}>
    <Icon name="settings" size={22} />
    <span>{t("Settings")}</span>
  </button>
</nav>

<style>
  .rail {
    width: 64px;
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    padding: 4px 0 12px;
  }
  .item {
    position: relative;
    width: 56px;
    height: 54px;
    border: none;
    border-radius: var(--radius-l);
    background: transparent;
    color: var(--text-2);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
    font-size: 0.68rem;
    font-weight: 600;
    letter-spacing: 0.01em;
    transition: background var(--dur) var(--ease), color var(--dur), transform 90ms var(--ease);
  }
  .item:hover {
    background: var(--hover);
    color: var(--text);
  }
  .item:active {
    transform: scale(0.95);
  }
  .item.on {
    background: var(--selected);
    color: var(--text-accent);
  }
  .item.on::before {
    content: "";
    position: absolute;
    left: -4px;
    top: 16px;
    bottom: 16px;
    width: 3px;
    border-radius: 2px;
    background: var(--accent);
    animation: bar-in 260ms var(--ease-out);
  }
  @keyframes bar-in {
    from {
      transform: scaleY(0.2);
      opacity: 0;
    }
  }
  .dot {
    position: absolute;
    top: 5px;
    right: 7px;
    min-width: 17px;
    height: 17px;
    padding: 0 4px;
    border-radius: 9px;
    background: var(--brand-grad);
    color: white;
    font-style: normal;
    font-size: 0.66rem;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 2px solid var(--bg-app);
  }
  .spacer {
    flex: 1;
  }
</style>
