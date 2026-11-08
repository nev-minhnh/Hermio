<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import { onMount } from "svelte";
  import Dialog from "./Dialog.svelte";
  import Icon from "../lib/Icon.svelte";
  import { api, type Calendar, type CalEvent } from "../lib/api";
  import { store } from "../lib/store.svelte";

  let { event, onclose, onsaved }: { event: Partial<CalEvent>; onclose: () => void; onsaved?: () => void } = $props();

  let calendars = $state<Calendar[]>([]);
  let title = $state(event.title ?? "");
  let calendarId = $state(event.calendar_id ?? 0);
  let allDay = $state(!!event.all_day);
  let location = $state(event.location ?? "");
  let description = $state(event.description ?? "");
  const pad = (n: number) => String(n).padStart(2, "0");
  const toLocal = (ts: number) => {
    const d = new Date(ts * 1000);
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
  };
  const now = Math.ceil(Date.now() / 1800000) * 1800;
  let start = $state(toLocal(event.start_ts ?? now));
  let end = $state(toLocal(event.end_ts ?? (event.start_ts ?? now) + 3600));
  let saving = $state(false);
  const readOnly = $derived(!!calendars.find((c) => c.id === calendarId && !c.writable));

  onMount(async () => {
    calendars = await api.calendars();
    if (!calendarId) calendarId = calendars.find((c) => c.writable && c.account_id !== null)?.id ?? calendars[0]?.id ?? 1;
  });

  const fromLocal = (s: string, dayOnly: boolean) => {
    const d = new Date(s);
    if (dayOnly) d.setHours(0, 0, 0, 0);
    return Math.floor(d.getTime() / 1000);
  };

  async function save() {
    let s = fromLocal(start, allDay);
    let e = fromLocal(end, allDay);
    if (allDay && e <= s) e = s + 86400;
    if (!allDay && e <= s) e = s + 1800;
    saving = true;
    try {
      await api.saveEvent({
        ...event,
        id: event.id ?? 0,
        calendar_id: calendarId,
        title: title.trim() || t("(No subject)"),
        location,
        description,
        start_ts: s,
        end_ts: e,
        all_day: allDay,
        remote_id: calendarId === event.calendar_id ? (event.remote_id ?? "") : "",
      });
      store.success(event.id ? t("Event updated") : t("Event added"));
      onsaved?.();
      onclose();
    } catch (err) {
      store.error(String(err));
    } finally {
      saving = false;
    }
  }

  async function remove() {
    if (!event.id || !confirm(t("Delete this event?"))) return;
    try {
      await api.deleteEvent(event.id);
      onsaved?.();
      onclose();
    } catch (err) {
      store.error(String(err));
    }
  }
</script>

<Dialog title={event.id ? t("Event") : t("New event")} {onclose} width={560}>
  <!-- svelte-ignore a11y_autofocus -->
  <input class="input title" placeholder={t("Add a title")} bind:value={title} autofocus disabled={readOnly} />
  <div class="row">
    <label class="field grow">
      <span>{t("Calendar")}</span>
      <select class="input" bind:value={calendarId} disabled={!!event.id && !!event.remote_id}>
        {#each calendars.filter((c) => c.writable || c.id === calendarId) as c}
          <option value={c.id}>{c.name}{c.account_id ? ` · ${store.accountById(c.account_id)?.email ?? ""}` : t(" · Local")}</option>
        {/each}
      </select>
    </label>
    <button class="allday" onclick={() => (allDay = !allDay)} disabled={readOnly}>
      <span class="switch" class:on={allDay}></span> {t("All day")}
    </button>
  </div>
  <div class="row">
    <label class="field grow">
      <span><Icon name="clock" size={14} /> {t("Start")}</span>
      {#if allDay}
        <input class="input" type="date" value={start.slice(0, 10)} onchange={(e) => (start = (e.target as HTMLInputElement).value + "T00:00")} disabled={readOnly} />
      {:else}
        <input class="input" type="datetime-local" bind:value={start} disabled={readOnly} />
      {/if}
    </label>
    <label class="field grow">
      <span>{t("End")}</span>
      {#if allDay}
        <input class="input" type="date" value={end.slice(0, 10)} onchange={(e) => (end = (e.target as HTMLInputElement).value + "T00:00")} disabled={readOnly} />
      {:else}
        <input class="input" type="datetime-local" bind:value={end} disabled={readOnly} />
      {/if}
    </label>
  </div>
  <label class="field">
    <span><Icon name="location" size={14} /> {t("Location")}</span>
    <input class="input" bind:value={location} placeholder={t("Add a location or meeting link")} disabled={readOnly} />
  </label>
  {#if event.meet_link}
    <button class="btn" onclick={() => api.openUrl(event.meet_link!)}><Icon name="video" size={16} /> {t("Join meeting")}</button>
  {/if}
  <label class="field">
    <span>{t("Description")}</span>
    <textarea class="input" rows="4" bind:value={description} disabled={readOnly}></textarea>
  </label>
  {#if event.recurring}
    <div class="muted small">{t("This is one occurrence of a recurring event; changes apply to this occurrence only.")}</div>
  {/if}

  {#snippet footer()}
    {#if event.id && !readOnly}
      <button class="btn danger" onclick={remove}><Icon name="trash" size={16} />{t("Delete")}</button>
    {/if}
    <span class="grow"></span>
    <button class="btn" onclick={onclose}>{t("Cancel")}</button>
    {#if !readOnly}
      <button class="btn primary" onclick={save} disabled={saving}>{saving ? t("Saving…") : t("Save")}</button>
    {/if}
  {/snippet}
</Dialog>

<style>
  .title {
    height: 40px;
    font-size: 1.1rem;
    font-weight: 600;
  }
  .allday {
    display: flex;
    align-items: center;
    gap: 8px;
    border: none;
    background: none;
    margin-top: 18px;
    font-weight: 600;
  }
  .field span {
    display: flex;
    align-items: center;
    gap: 4px;
  }
</style>
