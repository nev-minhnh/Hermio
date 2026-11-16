<script lang="ts">
  /** Sync status per mailbox and a live log of what the sync did (opened from the title bar). */
  import { onDestroy, onMount, tick } from "svelte";
  import { fly, fade, slide } from "svelte/transition";
  import { t } from "../lib/i18n.svelte";
  import Icon from "../lib/Icon.svelte";
  import { api, type SyncLogLine } from "../lib/api";
  import { listen } from "../lib/tauri";
  import { store } from "../lib/store.svelte";

  let { onclose }: { onclose: () => void } = $props();

  let lines = $state<SyncLogLine[]>([]);
  let onlyProblems = $state(false);
  let account = $state<number | 0>(0);
  let box: HTMLDivElement;
  let pinned = true;
  let unlisten: (() => void) | null = null;

  const shown = $derived(lines.filter((l) => (!onlyProblems || l.level === "warn" || l.level === "error") && (!account || l.account_id === account)));

  onMount(async () => {
    lines = await api.syncLog().catch(() => []);
    await tick();
    scrollEnd();
    unlisten = await listen<SyncLogLine>("sync-log", async (l) => {
      lines = [...lines.slice(-599), l];
      await tick();
      if (pinned) scrollEnd();
    });
  });
  onDestroy(() => unlisten?.());

  function scrollEnd() {
    if (box) box.scrollTop = box.scrollHeight;
  }
  function onScroll() {
    pinned = box.scrollHeight - box.scrollTop - box.clientHeight < 30;
  }
  function time(ms: number) {
    return new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  }
  function ago(sec: number) {
    if (!sec) return t("never");
    const d = Math.max(0, Date.now() / 1000 - sec);
    if (d < 60) return t("just now");
    if (d < 3600) return t("{n} min ago", { n: Math.floor(d / 60) });
    if (d < 86400) return t("{n} h ago", { n: Math.floor(d / 3600) });
    return new Date(sec * 1000).toLocaleString();
  }
  async function copy() {
    const text = shown.map((l) => `${new Date(l.ts).toLocaleString()} [${l.level}] ${l.account} — ${l.message}`).join("\n");
    await navigator.clipboard.writeText(text).catch(() => {});
    store.success(t("Log copied"));
  }
  function clear() {
    api.syncLogClear().catch(() => {});
    lines = [];
  }
  const colorOf = (id: number) => store.accounts.find((a) => a.id === id)?.color || "var(--text-3)";
  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }
</script>

<svelte:window onkeydown={onKey} />
<div class="backdrop" onclick={onclose} aria-hidden="true" transition:fade={{ duration: 120 }}></div>
<div class="panel card" role="dialog" aria-label={t("Sync status")} transition:fly={{ y: -8, duration: 180 }}>
  <div class="head">
    <b>{t("Sync status")}</b>
    <span class="grow"></span>
    <button class="btn subtle" onclick={() => store.syncNow(null)}><Icon name="sync" size={15} class={Object.values(store.syncing).some(Boolean) ? "spin" : ""} />{t("Sync all")}</button>
    <button class="icon-btn" title={t("Close")} onclick={onclose}><Icon name="close" size={15} /></button>
  </div>

  <!-- Mailboxes as compact cards (two per row); click one to show only its log lines. -->
  <div class="accounts">
    {#each store.accounts as a (a.id)}
      {@const err = store.errors[a.id]}
      {@const busy = !!store.syncing[a.id]}
      <div class="acc" class:bad={!!err && !busy} class:on={account === a.id} style:--c={a.color || "var(--accent)"}>
        <button class="pick" title={account === a.id ? t("Show all mailboxes") : t("Show only this mailbox's log")} onclick={() => (account = account === a.id ? 0 : a.id)}>
          <span class="dot"></span>
          <span class="txt">
            <span class="ell em">{a.email}</span>
            {#if busy}
              <span class="small accent"><Icon name="sync" size={11} class="spin" /> {t("Syncing…")}</span>
            {:else if err}
              <span class="small errtxt ell" title={err}>{err}</span>
            {:else}
              <span class="small muted ell">{a.enabled ? t("Synced {when}", { when: ago(a.last_sync) }) : t("Paused")} · {a.protocol.toUpperCase()}</span>
            {/if}
          </span>
        </button>
        <button class="icon-btn sm" title={t("Sync this mailbox")} onclick={() => store.syncNow(a.id)} disabled={busy || !a.enabled}>
          <Icon name="sync" size={14} class={busy ? "spin" : ""} />
        </button>
      </div>
    {/each}
  </div>

  <div class="logbar">
    <span class="small muted">{account ? t("Log of {who}", { who: store.accounts.find((x) => x.id === account)?.email ?? "" }) : t("Log of all mailboxes")}</span>
    {#if account}<button class="lnk small" onclick={() => (account = 0)}>{t("Show all")}</button>{/if}
    <span class="grow"></span>
    <label class="small chk"><input type="checkbox" bind:checked={onlyProblems} /> {t("Problems only")}</label>
    <button class="icon-btn" title={t("Copy log")} onclick={copy}><Icon name="copy" size={15} /></button>
    <button class="icon-btn" title={t("Open log file")} onclick={() => api.syncLogOpen().catch((e) => store.error(String(e)))}><Icon name="open" size={15} /></button>
    <button class="icon-btn" title={t("Clear")} onclick={clear}><Icon name="trash" size={15} /></button>
  </div>
  <div class="log selectable" bind:this={box} onscroll={onScroll}>
    {#each shown as l, i (l.ts + ":" + i)}
      <div class="line {l.level}" class:one={!!account} in:fade={{ duration: 140 }}>
        <span class="tm">{time(l.ts)}</span>
        <span class="lv">{l.level === "ok" ? "✓" : l.level === "error" ? "✕" : l.level === "warn" ? "!" : "·"}</span>
        {#if !account}<span class="who ell" title={l.account} style:--c={colorOf(l.account_id)}>{l.account.split("@")[0]}</span>{/if}
        <span class="msg">{l.message}</span>
      </div>
    {:else}
      <div class="muted small empty">{t("Nothing logged yet. Press Sync all to start.")}</div>
    {/each}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 800;
  }
  .panel {
    position: fixed;
    top: 46px;
    right: 14px;
    z-index: 801;
    width: min(640px, calc(100vw - 28px));
    max-height: min(680px, calc(100vh - 70px));
    display: flex;
    flex-direction: column;
    padding: 0;
    overflow: hidden;
    box-shadow: var(--shadow-lg, 0 16px 40px rgba(0, 0, 0, 0.2));
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .ell {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .accounts {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(250px, 1fr));
    gap: 6px;
    max-height: 190px;
    overflow-y: auto;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
  }
  .acc {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    padding: 2px 4px 2px 2px;
    border-radius: 8px;
    border: 1px solid var(--border);
    transition: background 160ms var(--ease), border-color 160ms var(--ease);
  }
  .acc:hover {
    background: var(--hover);
  }
  .acc.on {
    border-color: var(--c);
    background: color-mix(in srgb, var(--c) 12%, transparent);
  }
  .acc.bad {
    border-color: color-mix(in srgb, #c4314b 45%, transparent);
    background: color-mix(in srgb, #c4314b 7%, transparent);
  }
  .pick {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 6px 6px;
    border: none;
    background: transparent;
    color: inherit;
    text-align: left;
    cursor: pointer;
  }
  .txt {
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: 1.3;
  }
  .em {
    font-weight: 600;
    font-size: 0.86rem;
  }
  .acc .errtxt {
    color: #c4314b;
  }
  .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    flex: none;
    background: var(--c);
  }
  .icon-btn.sm {
    width: 28px;
    height: 28px;
    flex: none;
  }
  .accent {
    color: var(--text-accent);
  }
  .logbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
  }
  .logbar select {
    width: auto;
    max-width: 240px;
    padding: 3px 8px;
    font-size: 0.82rem;
  }
  .lnk {
    border: none;
    background: none;
    padding: 0;
    color: var(--text-accent);
    cursor: pointer;
  }
  .chk {
    display: inline-flex;
    gap: 5px;
    align-items: center;
  }
  .log {
    flex: 1;
    min-height: 160px;
    overflow-y: auto;
    padding: 4px 10px 10px;
    font-family: var(--mono, ui-monospace, "Cascadia Mono", Consolas, monospace);
    font-size: 0.76rem;
    line-height: 1.45;
    background: var(--surface-2, transparent);
  }
  .line {
    display: grid;
    grid-template-columns: auto 14px auto 1fr;
    gap: 6px;
    padding: 1px 0;
  }
  .line.one {
    grid-template-columns: auto 14px 1fr;
  }
  .tm {
    color: var(--text-3);
  }
  .lv {
    text-align: center;
    font-weight: 700;
    color: var(--text-3);
  }
  .msg {
    word-break: break-word;
  }
  /* Who: the mailbox's name before @, in its colour, in a fixed column so messages line up. */
  .who {
    width: 104px;
    color: var(--c);
    font-weight: 600;
  }
  .line.ok .lv {
    color: #107c10;
  }
  .line.warn .lv,
  .line.warn .msg {
    color: #b7791f;
  }
  .line.error .lv,
  .line.error .msg {
    color: #c4314b;
  }
  .empty {
    padding: 16px 0;
    font-family: inherit;
  }
</style>
