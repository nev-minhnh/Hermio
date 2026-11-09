<script lang="ts">
  import { scale } from "svelte/transition";
  import { t } from "../lib/i18n.svelte";
  import Icon from "../lib/Icon.svelte";
  import { store } from "../lib/store.svelte";

  const items = [
    { id: "mail", icon: "mail", label: t("Mail") },
    { id: "calendar", icon: "calendar", label: t("Calendar") },
  ] as const;

  /** Hidden: only a thin line at the window's left edge; hovering it slides the rail out. */
  const collapsed = $derived(!!store.settings.rail_collapsed);
  let peek = $state(false);
  let hideTimer: ReturnType<typeof setTimeout> | undefined;
  function enter() {
    clearTimeout(hideTimer);
    if (collapsed) peek = true;
  }
  function leave() {
    clearTimeout(hideTimer);
    hideTimer = setTimeout(() => (peek = false), 280);
  }
  function go(route: "mail" | "calendar" | "settings") {
    store.route = route;
    peek = false;
  }
  function toggle() {
    peek = false;
    store.set("rail_collapsed", !collapsed);
  }
</script>

<div class="railwrap" class:collapsed class:peek>
  {#if collapsed}
    <!-- The thin handle: hover (or focus) brings the rail back for a moment. -->
    <button class="edge" title={t("Show the navigation (Mail, Calendar, Settings)")} onmouseenter={enter} onmouseleave={leave} onfocus={enter} onclick={enter} aria-label={t("Show navigation")}>
      <span></span>
    </button>
  {/if}
  <nav class="rail" onmouseenter={enter} onmouseleave={leave}>
    {#each items as it}
      <button class="item" class:on={store.route === it.id} title={it.label} onclick={() => go(it.id)}>
        <Icon name={it.icon} size={22} />
        <span>{it.label}</span>
        {#if it.id === "mail" && store.counts.inbox > 0}
          <i class="dot" transition:scale={{ duration: 180, start: 0.5 }}>{store.counts.inbox > 99 ? "99+" : store.counts.inbox}</i>
        {/if}
      </button>
    {/each}
    <div class="spacer"></div>
    <button class="item small" title={collapsed ? t("Keep the navigation open") : t("Hide the navigation (a line at the left edge brings it back)")} onclick={toggle}>
      <Icon name={collapsed ? "chevronRight" : "chevronLeft"} size={18} />
      <span>{collapsed ? t("Pin") : t("Hide")}</span>
    </button>
    <button class="item" class:on={store.route === "settings"} title={t("Settings")} onclick={() => go("settings")}>
      <Icon name="settings" size={22} />
      <span>{t("Settings")}</span>
    </button>
  </nav>
</div>

<style>
  .railwrap {
    position: relative;
    flex: none;
    width: 64px;
    transition: width 240ms var(--ease-out);
    z-index: 30;
  }
  .railwrap.collapsed {
    width: 0;
  }
  .rail {
    position: absolute;
    inset: 0 auto 0 0;
    width: 64px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    padding: 4px 0 12px;
    background: var(--bg-app);
    transition:
      transform 240ms var(--ease-out),
      box-shadow 240ms var(--ease-out),
      opacity 200ms var(--ease);
  }
  .collapsed .rail {
    transform: translateX(-100%);
    opacity: 0;
    pointer-events: none;
  }
  .collapsed.peek .rail {
    transform: none;
    opacity: 1;
    pointer-events: auto;
    border-radius: 0 var(--radius-l) var(--radius-l) 0;
    box-shadow: var(--shadow-l);
  }
  /* The line that stands for the hidden rail. */
  .edge {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 10px;
    padding: 0;
    border: none;
    background: transparent;
    cursor: pointer;
    z-index: 1;
  }
  .edge span {
    position: absolute;
    left: 3px;
    top: 50%;
    width: 3px;
    height: 56px;
    margin-top: -28px;
    border-radius: 3px;
    background: var(--border-strong);
    transition: background var(--dur), height 200ms var(--ease-out), margin 200ms var(--ease-out);
  }
  .edge:hover span,
  .edge:focus-visible span {
    background: var(--accent);
    height: 80px;
    margin-top: -40px;
  }
  .item.small {
    height: 44px;
    color: var(--text-3);
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
