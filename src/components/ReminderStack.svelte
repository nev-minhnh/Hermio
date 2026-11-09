<script lang="ts">
  /** Calendar reminders as cards in the window (the same ones Windows shows as notifications). */
  import { onDestroy, onMount } from "svelte";
  import { fly, fade } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { cubicOut } from "svelte/easing";
  import { t, locale } from "../lib/i18n.svelte";
  import Icon from "../lib/Icon.svelte";
  import { api, type Reminder } from "../lib/api";
  import { listen } from "../lib/tauri";
  import { store } from "../lib/store.svelte";

  let now = $state(Date.now() / 1000);
  let skipping = $state<string | null>(null);
  const offs: (() => void)[] = [];
  let timer: ReturnType<typeof setInterval>;

  onMount(async () => {
    offs.push(
      await listen<Reminder>("reminder", (r) => {
        // A later step of the same event replaces its card.
        store.reminders = [r, ...store.reminders.filter((x) => x.event_id !== r.event_id || x.start_ts !== r.start_ts)];
      }),
    );
    offs.push(await listen<{ key: string }>("reminder-skipped", (p) => close(p.key)));
    timer = setInterval(() => {
      now = Date.now() / 1000;
      // Cards go away a few minutes after the event started.
      store.reminders = store.reminders.filter((r) => r.start_ts + 300 > now);
    }, 15000);
  });
  onDestroy(() => {
    offs.forEach((f) => f());
    clearInterval(timer);
  });

  function close(key: string) {
    store.reminders = store.reminders.filter((r) => r.key !== key);
  }
  function lead(r: Reminder) {
    const mins = Math.max(0, Math.round((r.start_ts - now) / 60));
    if (mins <= 0) return t("now");
    const h = Math.floor(mins / 60);
    const m = mins % 60;
    if (!h) return t("in {m} min", { m });
    return m ? t("in {h} h {m} min", { h, m }) : t("in {h} h", { h });
  }
  const tm = (ts: number) => new Date(ts * 1000).toLocaleTimeString(locale(), { hour: "numeric", minute: "2-digit" });
  async function skip(r: Reminder) {
    skipping = r.key;
    try {
      await api.reminderSkip(r.key);
      close(r.key);
      store.success(t("You won't get more reminders for “{title}”.", { title: r.title || t("(No title)") }));
    } catch (e) {
      store.error(t("Couldn't skip the reminders: {err}", { err: String(e) }));
    } finally {
      skipping = null;
    }
  }
  function open(r: Reminder) {
    close(r.key);
    store.showEvent(r.event_id, r.start_ts);
  }
</script>

<div class="stack" aria-live="polite">
  {#each store.reminders as r (r.key)}
    <div class="rem card" style:--c={r.color || "var(--accent)"} in:fly={{ y: 24, duration: 260, easing: cubicOut }} out:fade={{ duration: 160 }} animate:flip={{ duration: 220 }}>
      <div class="top">
        <Icon name="bell" size={15} />
        <b class="title" title={r.title}>{r.title || t("(No title)")}</b>
        <span class="lead">{lead(r)}</span>
        <button class="icon-btn x" title={t("Close")} onclick={() => close(r.key)}><Icon name="close" size={13} /></button>
      </div>
      <div class="muted small line">{tm(r.start_ts)} – {tm(r.end_ts)}{#if r.location}{" · "}{r.location}{/if}</div>
      <div class="acts">
        {#if !r.last}
          <button class="btn subtle" onclick={() => skip(r)} disabled={skipping === r.key}>
            {#if skipping === r.key}<span class="spinner"></span>{:else}<Icon name="close" size={14} />{/if}{t("Skip remaining reminders")}
          </button>
        {/if}
        <span class="grow"></span>
        {#if r.meet_link}<button class="btn" onclick={() => api.openUrl(r.meet_link)}><Icon name="video" size={14} />{t("Join")}</button>{/if}
        <button class="btn primary" onclick={() => open(r)}>{t("Open")}</button>
      </div>
    </div>
  {/each}
</div>

<style>
  .stack {
    position: fixed;
    bottom: 20px;
    right: 18px;
    z-index: 850;
    display: flex;
    flex-direction: column-reverse;
    gap: 10px;
    width: min(360px, calc(100vw - 28px));
    pointer-events: none;
  }
  .rem {
    pointer-events: auto;
    padding: 12px 12px 10px 14px;
    border-left: 4px solid var(--c);
    box-shadow: var(--shadow-l);
  }
  .top {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-accent);
  }
  .title {
    flex: 1;
    min-width: 0;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .lead {
    flex: none;
    font-size: 0.76rem;
    font-weight: 600;
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--text-accent);
  }
  .x {
    width: 24px;
    height: 24px;
  }
  .line {
    margin: 4px 0 8px 23px;
  }
  .acts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  .grow {
    flex: 1;
  }
</style>
