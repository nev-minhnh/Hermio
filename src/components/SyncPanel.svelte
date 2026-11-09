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

  <div class="accounts">
    {#each store.accounts as a (a.id)}
      {@const err = store.errors[a.id]}
      <div class="acc" class:bad={!!err && !store.syncing[a.id]}>
        <span class="dot" style:background={a.color || "var(--accent)"}></span>
        <div class="grow ell">
          <div class="ell"><b>{a.name || a.email}</b> <span class="muted small">{a.email} · {a.protocol.toUpperCase()}</span></div>
          {#if store.syncing[a.id]}
            <div class="small accent" in:fade={{ duration: 120 }}><Icon name="sync" size={12} class="spin" /> {t("Syncing…")}</div>
          {:else if err}
            <div class="small errtxt selectable" in:slide={{ duration: 160 }}>{err}</div>
          {:else}
            <div class="small muted">{a.enabled ? t("Synced {when}", { when: ago(a.last_sync) }) : t("Paused")}</div>
          {/if}
        </div>
        <button class="icon-btn" title={t("Sync this mailbox")} onclick={() => store.syncNow(a.id)} disabled={!!store.syncing[a.id] || !a.enabled}>
          <Icon name="sync" size={14} class={store.syncing[a.id] ? "spin" : ""} />
        </button>
      </div>
    {/each}
  </div>

  <div class="logbar">
    <select class="input" bind:value={account}>
      <option value={0}>{t("All mailboxes")}</option>
      {#each store.accounts as a (a.id)}<option value={a.id}>{a.email}</option>{/each}
    </select>
    <label class="small chk"><input type="checkbox" bind:checked={onlyProblems} /> {t("Problems only")}</label>
    <span class="grow"></span>
    <button class="icon-btn" title={t("Copy log")} onclick={copy}><Icon name="copy" size={15} /></button>
    <button class="icon-btn" title={t("Open log file")} onclick={() => api.syncLogOpen().catch((e) => store.error(String(e)))}><Icon name="open" size={15} /></button>
    <button class="icon-btn" title={t("Clear")} onclick={clear}><Icon name="trash" size={15} /></button>
  </div>
  <div class="log selectable" bind:this={box} onscroll={onScroll}>
    {#each shown as l, i (l.ts + ":" + i)}
      <div class="line {l.level}" in:fade={{ duration: 140 }}>
        <span class="tm">{time(l.ts)}</span>
        <span class="lv">{l.level === "ok" ? "✓" : l.level === "error" ? "✕" : l.level === "warn" ? "!" : "·"}</span>
        <span class="msg">{#if !account}<span class="who">{l.account}</span>{/if}{l.message}</span>
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
    width: min(560px, calc(100vw - 28px));
    max-height: min(640px, calc(100vh - 70px));
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
    display: flex;
    flex-direction: column;
    max-height: 210px;
    overflow-y: auto;
    padding: 4px 6px;
    border-bottom: 1px solid var(--border);
  }
  .acc {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 6px;
    border-radius: 6px;
    transition: background 160ms var(--ease);
  }
  .acc.bad {
    background: color-mix(in srgb, #c4314b 7%, transparent);
  }
  .acc .errtxt {
    white-space: normal;
    color: #c4314b;
    line-height: 1.35;
  }
  .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
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
    grid-template-columns: auto 14px 1fr;
    gap: 6px;
    padding: 1px 0;
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
  .who {
    color: var(--text-3);
    margin-right: 6px;
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
