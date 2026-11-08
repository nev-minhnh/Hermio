<script lang="ts">
  import { t, locale } from "../lib/i18n.svelte";
  import { flip } from "svelte/animate";
  import { fly, slide } from "svelte/transition";
  import Icon from "../lib/Icon.svelte";
  import { store } from "../lib/store.svelte";
  import { api, type OutboxItem } from "../lib/api";

  let editing = $state<number | null>(null);
  let when = $state("");
  let busy = $state<number | null>(null);

  const fmt = (ts: number) =>
    new Date(ts * 1000).toLocaleString(locale(), { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
  function localInput(ts: number) {
    const d = new Date(ts * 1000);
    const p = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`;
  }

  async function act(o: OutboxItem, f: () => Promise<unknown>) {
    busy = o.id;
    try {
      await f();
    } catch (e) {
      store.error(String(e));
    } finally {
      busy = null;
      await store.loadOutbox();
    }
  }
  const sendNow = (o: OutboxItem) => act(o, async () => {
    await api.sendOutboxNow(o.id);
    store.success(t("Message sent"));
  });
  const edit = (o: OutboxItem) => act(o, async () => {
    const msg = await api.cancelOutbox(o.id);
    store.restoreCompose(msg);
  });
  const cancel = (o: OutboxItem) => {
    if (!confirm(t("Cancel this scheduled message? It will be moved back to a draft you can edit."))) return;
    edit(o);
  };
  const reschedule = (o: OutboxItem) => act(o, async () => {
    const ts = Math.floor(new Date(when).getTime() / 1000);
    if (!ts || ts * 1000 < Date.now()) throw new Error(t("Choose a time in the future"));
    await api.rescheduleOutbox(o.id, ts);
    editing = null;
    store.success(t("Rescheduled"));
  });
</script>

<div class="wrap">
  <div class="head">
    <h2>{t("Scheduled")}</h2>
    <span class="muted small">{t("Sent automatically while Hermio is running (also from the tray).")}</span>
  </div>
  <div class="list scroll">
    {#each store.outbox as o (o.id)}
      <div class="item card" class:failed={o.status === "failed"} animate:flip={{ duration: 200 }} in:fly={{ y: 8, duration: 180 }}>
        <div class="when">
          <Icon name={o.status === "failed" ? "warning" : o.status === "sending" ? "send" : "clock"} size={18} />
          <span>{fmt(o.send_at)}</span>
        </div>
        <div class="txt">
          <b class="ell">{o.subject || t("(No subject)")}</b>
          <span class="muted small ell">{t("To")}: {o.recipients} · {store.accountById(o.account_id)?.email ?? ""}</span>
          {#if o.error}<span class="err small">{o.error}</span>{/if}
          {#if editing === o.id}
            <div class="resched" transition:slide={{ duration: 150 }}>
              <input class="input" type="datetime-local" bind:value={when} />
              <button class="btn primary" onclick={() => reschedule(o)}>{t("Save")}</button>
              <button class="btn subtle" onclick={() => (editing = null)}>{t("Cancel")}</button>
            </div>
          {/if}
        </div>
        <div class="acts">
          <button class="btn subtle" disabled={busy === o.id} onclick={() => { editing = o.id; when = localInput(o.send_at); }}><Icon name="calendar" size={15} />{t("Reschedule")}</button>
          <button class="btn subtle" disabled={busy === o.id} onclick={() => edit(o)}><Icon name="compose" size={15} />{t("Edit")}</button>
          <button class="btn subtle" disabled={busy === o.id} onclick={() => cancel(o)}><Icon name="close" size={15} />{t("Cancel")}</button>
          <button class="btn primary" disabled={busy === o.id} onclick={() => sendNow(o)}><Icon name="send" size={15} />{t("Send now")}</button>
        </div>
      </div>
    {:else}
      <div class="empty">
        <Icon name="clock" size={36} />
        <b>{t("Nothing scheduled")}</b>
        <span class="muted small">{t("Use the arrow next to Send to schedule a message.")}</span>
      </div>
    {/each}
  </div>
</div>

<style>
  .wrap {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    animation: fade 180ms var(--ease);
  }
  .head {
    padding: 12px 18px 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  h2 {
    margin: 0;
    font-family: var(--font-display);
    font-size: 1.12rem;
    font-weight: 600;
  }
  .list {
    flex: 1;
    min-height: 0;
    padding: 4px 14px 14px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 10px 12px;
    flex-wrap: wrap;
  }
  .item.failed {
    border-color: color-mix(in srgb, var(--danger) 45%, transparent);
  }
  .when {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 150px;
    font-weight: 600;
    color: var(--text-accent);
    font-size: 0.88rem;
  }
  .failed .when {
    color: var(--danger);
  }
  .txt {
    flex: 1;
    min-width: 200px;
    display: flex;
    flex-direction: column;
  }
  .ell {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .err {
    color: var(--danger);
  }
  .acts {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }
  .resched {
    display: flex;
    gap: 6px;
    margin-top: 6px;
  }
  .empty {
    margin: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    color: var(--text-2);
    padding: 40px;
  }
</style>
