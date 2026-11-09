<script lang="ts">
  import { CAT_COLOR } from "../lib/writing";
  import { t } from "../lib/i18n.svelte";
  import { onMount } from "svelte";
  import Icon from "../lib/Icon.svelte";
  import Logo from "../lib/Logo.svelte";
  import { store } from "../lib/store.svelte";
  import { api } from "../lib/api";
  import { win, inTauri } from "../lib/tauri";
  import { debounce } from "../lib/util";
  import { pill } from "../lib/motion";
  import { scale } from "svelte/transition";
  import SyncPanel from "./SyncPanel.svelte";

  let maximized = $state(false);
  let query = $state(store.search);
  let focused = $state(false);
  const anySyncing = $derived(Object.values(store.syncing).some(Boolean));
  const anyError = $derived(store.accounts.some((a) => a.enabled && !store.syncing[a.id] && !!store.errors[a.id]));
  let syncOpen = $state(false);

  onMount(() => {
    win.isMaximized().then((m) => (maximized = m));
    let off: (() => void) | undefined;
    win.onResized(async () => (maximized = await win.isMaximized())).then((f) => (off = f as () => void));
    return () => off?.();
  });

  const run = debounce(() => {
    store.search = query;
    store.route = "mail";
    store.selected = [];
    store.loadMessages(true);
  }, 220);

  function onInput() {
    run();
  }
  function clear() {
    query = "";
    store.search = "";
    store.loadMessages(true);
  }
  function setMode(m: "subject" | "content" | "people") {
    store.set("search_mode", m);
    if (store.search.trim()) store.loadMessages(true);
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<header class="titlebar" data-tauri-drag-region ondblclick={(e) => e.target === e.currentTarget && win.toggleMaximize()}>
  <div class="brand" data-tauri-drag-region>
    <Logo size={20} />
    <span data-tauri-drag-region>Hermio</span>
  </div>

  <div class="search-wrap" data-tauri-drag-region>
    <div class="search" class:focused>
      <Icon name="search" size={16} />
      <input
        id="global-search"
        placeholder={store.settings.search_mode === "subject"
          ? t("Search message subjects")
          : store.settings.search_mode === "people"
            ? t("Search sender names, To and Cc addresses")
            : t("Search content, subjects, senders")}
        title={t("Tip: from:lan  to:minh@monash.edu  cc:“Lan Pham” work in every mode")}
        bind:value={query}
        oninput={onInput}
        onfocus={() => (focused = true)}
        onblur={() => (focused = false)}
        onkeydown={(e) => e.key === "Escape" && clear()}
        spellcheck="false"
      />
      {#if query}
        <button class="clear" onclick={clear} title={t("Clear search")}><Icon name="close" size={14} /></button>
      {/if}
      <div class="mode segmented" title={t("Search mode")} use:pill>
        <button class:on={store.settings.search_mode === "subject"} onclick={() => setMode("subject")}>{t("Subject")}</button>
        <button class:on={store.settings.search_mode === "content"} onclick={() => setMode("content")}>{t("Content")}</button>
        <button class:on={store.settings.search_mode === "people"} onclick={() => setMode("people")}>{t("People")}</button>
      </div>
    </div>
  </div>

  <div class="right" data-tauri-drag-region>
    {#if store.wa.active && store.route === "mail"}
      {@const w = store.wa}
      {@const ringC = w.error ? "#e5484d" : w.worst ? CAT_COLOR[w.worst as keyof typeof CAT_COLOR] : "#e8b931"}
      <button
        class="wa-btn"
        class:busy={w.busy}
        class:on={store.waOpen && !store.aiOpen}
        class:off={!w.enabled}
        style:--rc={ringC}
        style:--p={w.progress ? Math.round((w.progress[0] / Math.max(1, w.progress[1])) * 100) : 28}
        title={w.error ? t("Writing assistant: {err}", { err: w.error }) : !w.enabled ? t("Writing assistant (scanning stopped)") : w.busy ? t("Writing assistant: checking…") : t("Writing assistant: {n} suggestions", { n: w.count })}
        onclick={() => { const open = !(store.waOpen && !store.aiOpen); store.aiOpen = false; store.waOpen = open; }}
        transition:scale={{ start: 0.6, duration: 180 }}
      >
        <span class="wa-face">
          {#if w.error}<b>!</b>
          {:else if w.count && !w.busy}<b>{Math.min(99, w.count)}</b>
          {:else}<Icon name="penCheck" size={15} />{/if}
        </span>
      </button>
    {/if}
    <button class="ai-btn" class:on={store.aiOpen} title={t("AI assistant (Ctrl+J)")} onclick={() => { store.route = "mail"; store.aiOpen = !store.aiOpen; }}>
      <span>✦</span> {t("AI Assistant")}
    </button>
    <button class="icon-btn sync-btn" class:on={syncOpen} title={t("Sync now and show the sync status (F5 syncs without opening)")} onclick={() => { if (!syncOpen) store.syncNow(null); syncOpen = !syncOpen; }}>
      <Icon name="sync" class={anySyncing ? "spin" : ""} />
      {#if anyError}<i class="errdot" transition:scale={{ duration: 160 }}></i>{/if}
    </button>
    {#if syncOpen}<SyncPanel onclose={() => (syncOpen = false)} />{/if}
    {#if inTauri}
      <div class="winctl">
        <button onclick={() => win.minimize()} title={t("Minimize")}><Icon name="minimize" size={16} /></button>
        <button onclick={() => win.toggleMaximize()} title={maximized ? t("Restore") : t("Maximize")}>
          <Icon name={maximized ? "restore" : "maximize"} size={14} />
        </button>
        <button class="close" onclick={() => win.close()} title={t("Close")}><Icon name="close" size={16} /></button>
      </div>
    {/if}
  </div>
</header>

<style>
  .titlebar {
    height: var(--titlebar-h);
    flex: none;
    display: grid;
    grid-template-columns: 1fr minmax(320px, 640px) 1fr;
    align-items: center;
    padding-left: 14px;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 9px;
    font-family: var(--font-display);
    font-weight: 600;
    font-size: 0.95rem;
  }
  .search-wrap {
    display: flex;
    justify-content: center;
  }
  .search {
    width: 100%;
    height: 32px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 3px 0 11px;
    border-radius: var(--radius);
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--text-3);
    transition: box-shadow var(--dur), border-color var(--dur);
  }
  .search.focused {
    border-color: color-mix(in srgb, var(--accent) 55%, transparent);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 16%, transparent);
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: none;
    outline: none;
    background: transparent;
    color: var(--text);
    user-select: text;
  }
  .search input::placeholder {
    color: var(--text-3);
  }
  .clear {
    border: none;
    background: transparent;
    color: var(--text-3);
    display: flex;
    padding: 4px;
    border-radius: 4px;
  }
  .clear:hover {
    background: var(--hover);
  }
  .mode {
    flex: none;
    padding: 1px;
  }
  .mode button {
    height: 24px;
    padding: 0 9px;
  }
  .right {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 4px;
    height: 100%;
  }
  .ai-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 12px;
    border-radius: 15px;
    border: 1px solid color-mix(in srgb, var(--ai) 40%, var(--border));
    background: color-mix(in srgb, var(--ai) 8%, var(--surface));
    font-weight: 600;
    font-size: 0.86rem;
    margin-right: 4px;
    transition: background var(--dur);
  }
  /* The writing assistant's round button: a ring in the colour of the most serious kind of
     suggestion (gold when there are none), a sweeping arc while it checks. */
  .wa-btn {
    position: relative;
    width: 32px;
    height: 32px;
    margin-right: 8px;
    border: none;
    border-radius: 50%;
    padding: 0;
    background: conic-gradient(var(--rc) 0 100%);
    display: grid;
    place-items: center;
    transition: transform 160ms var(--ease), box-shadow 160ms;
    flex: none;
  }
  .wa-btn:hover {
    transform: scale(1.06);
  }
  .wa-btn.on {
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--rc) 30%, transparent);
  }
  .wa-btn.off {
    filter: grayscale(1) opacity(0.65);
  }
  .wa-btn.busy {
    background: conic-gradient(var(--rc) calc(var(--p) * 1%), color-mix(in srgb, var(--rc) 22%, transparent) 0);
    animation: wa-spin 1s linear infinite;
  }
  .wa-btn.busy .wa-face {
    animation: wa-spin 1s linear infinite reverse;
  }
  @keyframes wa-spin {
    to {
      transform: rotate(360deg);
    }
  }
  .wa-face {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    background: #0d1838;
    color: #fff;
    display: grid;
    place-items: center;
    font-size: 12.5px;
  }
  .wa-face :global(.icon) {
    color: var(--rc);
  }
  .ai-btn span {
    color: var(--ai);
  }
  .ai-btn:hover,
  .ai-btn.on {
    background: color-mix(in srgb, var(--ai) 20%, var(--surface));
  }
  .winctl {
    display: flex;
    height: 100%;
    margin-left: 6px;
  }
  .winctl button {
    width: 46px;
    height: 100%;
    border: none;
    background: transparent;
    color: var(--text-2);
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background 100ms;
  }
  .winctl button:hover {
    background: var(--hover);
  }
  .winctl .close:hover {
    background: #c42b1c;
    color: white;
  }
  .sync-btn {
    position: relative;
  }
  .sync-btn.on {
    background: var(--selected);
  }
  .errdot {
    position: absolute;
    top: 5px;
    right: 5px;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #c4314b;
    box-shadow: 0 0 0 2px var(--bg, var(--surface));
  }
</style>
