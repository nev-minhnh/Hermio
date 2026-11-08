<script lang="ts">
  import { reduced } from "../lib/motion";
  import { pill } from "../lib/motion";
  import { t, locale } from "../lib/i18n.svelte";
  import { onMount, tick } from "svelte";
  import Icon from "../lib/Icon.svelte";
  import { api, type Calendar, type CalEvent } from "../lib/api";
  import { store } from "../lib/store.svelte";
  import { listen } from "../lib/tauri";
  import { startOfDay } from "../lib/util";
  import EventDialog from "./EventDialog.svelte";

  type Mode = "day" | "week" | "month";
  let mode = $state<Mode>("week");
  let cursor = $state(startOfDay(new Date()));
  // Switching day / week / month or moving to another period: the grid slides in softly.
  let viewEl = $state<HTMLElement>();
  let lastKey = "";
  let lastStart = 0;
  $effect(() => {
    const start = days[0]?.getTime() ?? 0;
    const key = `${mode}`;
    const el = viewEl;
    if (!el || reduced()) return;
    if (lastKey && (key !== lastKey || start !== lastStart)) {
      const dir = key !== lastKey ? 0 : start > lastStart ? 1 : -1;
      el.animate(
        [
          { opacity: 0.35, translate: `${dir * 18}px ${dir ? 0 : 6}px` },
          { opacity: 1, translate: "0 0" },
        ],
        { duration: 240, easing: "cubic-bezier(.2,.8,.2,1)" },
      );
    }
    lastKey = key;
    lastStart = start;
  });
  let mini = $state(new Date(new Date().getFullYear(), new Date().getMonth(), 1));
  let calendars = $state<Calendar[]>([]);
  let events = $state<CalEvent[]>([]);
  let editing = $state<Partial<CalEvent> | null>(null);
  let syncing = $state(false);
  let gridEl = $state<HTMLDivElement>();
  let nowTick = $state(Date.now());

  const HOUR = 52;
  const ws = $derived(store.settings.week_start);
  const dayNames = $derived(ws === 1 ? ["T2", "T3", "T4", "T5", "T6", "T7", "CN"] : ["CN", "T2", "T3", "T4", "T5", "T6", "T7"]);

  function weekStart(d: Date) {
    const x = startOfDay(d);
    const diff = (x.getDay() - ws + 7) % 7;
    x.setDate(x.getDate() - diff);
    return x;
  }
  const addDays = (d: Date, n: number) => {
    const x = new Date(d);
    x.setDate(x.getDate() + n);
    return x;
  };
  const sameDay = (a: Date, b: Date) => a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();

  const days = $derived.by(() => {
    if (mode === "day") return [cursor];
    if (mode === "week") return Array.from({ length: 7 }, (_, i) => addDays(weekStart(cursor), i));
    const first = weekStart(new Date(cursor.getFullYear(), cursor.getMonth(), 1));
    return Array.from({ length: 42 }, (_, i) => addDays(first, i));
  });

  const range = $derived({ from: Math.floor(days[0].getTime() / 1000), to: Math.floor(addDays(days[days.length - 1], 1).getTime() / 1000) });

  const title = $derived.by(() => {
    if (mode === "month") return cursor.toLocaleDateString(locale(), { month: "long", year: "numeric" });
    if (mode === "day") return cursor.toLocaleDateString(locale(), { weekday: "long", day: "numeric", month: "long", year: "numeric" });
    const a = days[0];
    const b = days[6];
    return a.getMonth() === b.getMonth()
      ? `${a.getDate()}–${b.getDate()} ${b.toLocaleDateString(locale(), { month: "long", year: "numeric" })}`
      : `${a.getDate()}/${a.getMonth() + 1} – ${b.getDate()}/${b.getMonth() + 1}/${b.getFullYear()}`;
  });

  async function load() {
    [calendars, events] = await Promise.all([api.calendars(), api.events(range.from, range.to)]);
  }

  $effect(() => {
    range.from;
    range.to;
    load();
  });

  onMount(() => {
    let off: (() => void) | undefined;
    listen("calendar-changed", () => load()).then((f) => (off = f));
    const t = setInterval(() => (nowTick = Date.now()), 60000);
    scrollToMorning();
    return () => {
      off?.();
      clearInterval(t);
    };
  });

  async function scrollToMorning() {
    await tick();
    if (gridEl) gridEl.scrollTop = HOUR * 7 - 10;
  }

  function move(n: number) {
    if (mode === "month") cursor = new Date(cursor.getFullYear(), cursor.getMonth() + n, 1);
    else cursor = addDays(cursor, mode === "week" ? 7 * n : n);
    mini = new Date(cursor.getFullYear(), cursor.getMonth(), 1);
  }
  function today() {
    cursor = startOfDay(new Date());
    mini = new Date(cursor.getFullYear(), cursor.getMonth(), 1);
    scrollToMorning();
  }

  async function syncNow() {
    syncing = true;
    try {
      await api.syncCalendars();
      await load();
    } catch (e) {
      store.error(String(e));
    } finally {
      syncing = false;
    }
  }

  async function toggleCal(c: Calendar) {
    await api.setCalendarVisible(c.id, !c.visible);
    await load();
  }

  const dayEvents = (d: Date) => {
    const s = d.getTime() / 1000;
    const e = s + 86400;
    return events.filter((ev) => ev.end_ts > s && ev.start_ts < e);
  };
  const timed = (d: Date) => dayEvents(d).filter((e) => !e.all_day && e.end_ts - e.start_ts < 86400);
  const allday = (d: Date) => dayEvents(d).filter((e) => e.all_day || e.end_ts - e.start_ts >= 86400);

  /** Lay out overlapping events into columns. */
  function columns(list: CalEvent[]) {
    const sorted = [...list].sort((a, b) => a.start_ts - b.start_ts || b.end_ts - a.end_ts);
    const out: { ev: CalEvent; col: number; cols: number }[] = [];
    let group: { ev: CalEvent; col: number; cols: number }[] = [];
    let groupEnd = 0;
    const flush = () => {
      const n = Math.max(1, ...group.map((g) => g.col + 1));
      group.forEach((g) => (g.cols = n));
      out.push(...group);
      group = [];
    };
    for (const ev of sorted) {
      if (group.length && ev.start_ts >= groupEnd) flush();
      const used = new Set(group.filter((g) => g.ev.end_ts > ev.start_ts).map((g) => g.col));
      let col = 0;
      while (used.has(col)) col++;
      group.push({ ev, col, cols: 1 });
      groupEnd = Math.max(groupEnd, ev.end_ts);
    }
    flush();
    return out;
  }

  function evStyle(ev: CalEvent, d: Date, col: number, cols: number) {
    const dayStart = d.getTime() / 1000;
    const s = Math.max(ev.start_ts, dayStart);
    const e = Math.min(ev.end_ts, dayStart + 86400);
    const top = ((s - dayStart) / 3600) * HOUR;
    const h = Math.max(22, ((e - s) / 3600) * HOUR - 2);
    return `top:${top}px;height:${h}px;left:calc(${(col / cols) * 100}% + 2px);width:calc(${100 / cols}% - 4px);--c:${ev.color}`;
  }

  function newAt(d: Date, y?: number) {
    const base = d.getTime() / 1000;
    let start = base + 9 * 3600;
    if (y !== undefined) start = base + Math.floor(y / (HOUR / 2)) * 1800;
    editing = { start_ts: start, end_ts: start + 3600, all_day: false };
  }

  const time = (ts: number) => new Date(ts * 1000).toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit" });

  const miniDays = $derived.by(() => {
    const first = weekStart(mini);
    return Array.from({ length: 42 }, (_, i) => addDays(first, i));
  });

  const upcoming = $derived(
    events
      .filter((e) => e.end_ts * 1000 > nowTick)
      .sort((a, b) => a.start_ts - b.start_ts)
      .slice(0, 4),
  );
</script>

<div class="cal">
  <aside class="side">
    <button class="btn primary new" onclick={() => newAt(cursor)}><Icon name="plus" size={18} /> {t("New event")}</button>
    <div class="mini">
      <div class="mini-head">
        <b>{mini.toLocaleDateString(locale(), { month: "long", year: "numeric" })}</b>
        <button class="icon-btn s" onclick={() => (mini = new Date(mini.getFullYear(), mini.getMonth() - 1, 1))}><Icon name="chevronUp" size={14} /></button>
        <button class="icon-btn s" onclick={() => (mini = new Date(mini.getFullYear(), mini.getMonth() + 1, 1))}><Icon name="chevronDown" size={14} /></button>
      </div>
      <div class="mini-grid">
        {#each dayNames as n}<span class="dn">{n}</span>{/each}
        {#each miniDays as d}
          <button
            class="md"
            class:out={d.getMonth() !== mini.getMonth()}
            class:today={sameDay(d, new Date())}
            class:cur={days.some((x) => sameDay(x, d)) && mode !== "month"}
            onclick={() => { cursor = d; if (mode === "month") mode = "day"; }}
          >{d.getDate()}</button>
        {/each}
      </div>
    </div>

    <div class="cals scroll">
      <div class="cal-title">
        <span>{t("Calendar")}</span>
        <button class="icon-btn s" title={t("Add Outlook / ICS calendar")} onclick={() => { store.settingsTab = "calendar"; store.route = "settings"; }}><Icon name="plus" size={14} /></button>
      </div>
      {#each calendars as c (c.id)}
        <button class="calrow" onclick={() => toggleCal(c)} title={c.account_id ? store.accountById(c.account_id)?.email : t("Stored on this PC")}>
          <span class="box" style:--c={c.color} class:on={c.visible}>{#if c.visible}<Icon name="check" size={12} />{/if}</span>
          <span class="nm">{c.name}</span>
          {#if c.kind === "ics"}<span class="tag" title={c.last_error || c.url}>{c.last_error ? t("error") : "ICS"}</span>{:else if !c.writable}<span class="muted small">{t("read-only")}</span>{/if}
        </button>
      {/each}
      {#if !store.accounts.some((a) => a.auth === "oauth_google")}
        <p class="muted small hint">{t("Sign in to a Google account (OAuth) to sync Google Calendar. The local calendar is always available.")}</p>
      {/if}
      {#if upcoming.length}
        <div class="up-title">{t("Upcoming")}</div>
        {#each upcoming as e}
          <button class="up" onclick={() => (editing = e)} style:--c={e.color}>
            <b>{e.title}</b>
            <span class="muted small">{new Date(e.start_ts * 1000).toLocaleDateString(locale(), { weekday: "short", day: "numeric", month: "numeric" })}{e.all_day ? "" : " · " + time(e.start_ts)}</span>
          </button>
        {/each}
      {/if}
    </div>
  </aside>

  <section class="main">
    <div class="head">
      <button class="btn" onclick={today}><Icon name="today" size={16} />{t("Today")}</button>
      <button class="icon-btn" onclick={() => move(-1)} title={t("Previous")}><Icon name="chevronLeft" /></button>
      <button class="icon-btn" onclick={() => move(1)} title="Sau"><Icon name="chevronRight" /></button>
      <h2>{title}</h2>
      <span class="grow"></span>
      <button class="icon-btn" onclick={syncNow} title={t("Sync calendars")}><Icon name="sync" class={syncing ? "spin" : ""} /></button>
      <div class="segmented" use:pill>
        <button class:on={mode === "day"} onclick={() => { mode = "day"; scrollToMorning(); }}>{t("Day")}</button>
        <button class:on={mode === "week"} onclick={() => { mode = "week"; scrollToMorning(); }}>{t("Week")}</button>
        <button class:on={mode === "month"} onclick={() => (mode = "month")}>{t("Month")}</button>
      </div>
    </div>

    {#if mode === "month"}
      <div class="month" bind:this={viewEl}>
        {#each dayNames as n}<div class="mh">{n}</div>{/each}
        {#each days as d}
          {@const evs = dayEvents(d)}
          <div class="mcell" class:out={d.getMonth() !== cursor.getMonth()} class:today={sameDay(d, new Date())} ondblclick={() => newAt(d)} role="gridcell" tabindex="-1">
            <button class="mnum" onclick={() => { cursor = d; mode = "day"; scrollToMorning(); }}>{d.getDate() === 1 ? `${d.getDate()} th${d.getMonth() + 1}` : d.getDate()}</button>
            {#each evs.slice(0, 3) as e}
              <button class="mev" class:solid={e.all_day} style:--c={e.color} onclick={() => (editing = e)}>
                {#if !e.all_day}<i></i><span class="t">{time(e.start_ts)}</span>{/if}
                <span class="tt">{e.title}</span>
              </button>
            {/each}
            {#if evs.length > 3}<button class="more" onclick={() => { cursor = d; mode = "day"; }}>+{evs.length - 3} {t("events")}</button>{/if}
          </div>
        {/each}
      </div>
    {:else}
      <div class="week" bind:this={viewEl}>
        <div class="wh">
          <div class="gutter"></div>
          {#each days as d}
            <div class="whd" class:today={sameDay(d, new Date())}>
              <span class="dname">{d.toLocaleDateString(locale(), { weekday: "short" })}</span>
              <span class="dnum">{d.getDate()}</span>
            </div>
          {/each}
        </div>
        <div class="alld">
          <div class="gutter small muted">{t("all day")}</div>
          {#each days as d}
            <div class="adcell">
              {#each allday(d) as e}
                <button class="adev" style:--c={e.color} onclick={() => (editing = e)}>{e.title}</button>
              {/each}
            </div>
          {/each}
        </div>
        <div class="grid scroll" bind:this={gridEl}>
          <div class="hours" style:height="{HOUR * 24}px">
            <div class="gutter">
              {#each Array(24) as _, h}<span class="hl" style:top="{h * HOUR}px">{h === 0 ? "" : `${String(h).padStart(2, "0")}:00`}</span>{/each}
            </div>
            {#each days as d}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <div
                class="col"
                class:today={sameDay(d, new Date())}
                onclick={(e) => e.target === e.currentTarget && newAt(d, e.offsetY)}
                role="gridcell"
                tabindex="-1"
              >
                {#each Array(24) as _, h}<i class="line" style:top="{h * HOUR}px"></i>{/each}
                {#each columns(timed(d)) as { ev, col, cols } (ev.id)}
                  <button class="ev" class:short={ev.end_ts - ev.start_ts < 2700} style={evStyle(ev, d, col, cols)} onclick={() => (editing = ev)}>
                    <b>{ev.title}</b>
                    <span>{time(ev.start_ts)} – {time(ev.end_ts)}{ev.location ? ` · ${ev.location}` : ""}</span>
                  </button>
                {/each}
                {#if sameDay(d, new Date(nowTick))}
                  {@const n = new Date(nowTick)}
                  <div class="now" style:top="{(n.getHours() + n.getMinutes() / 60) * HOUR}px"></div>
                {/if}
              </div>
            {/each}
          </div>
        </div>
      </div>
    {/if}
  </section>
</div>

{#if editing}
  <EventDialog event={editing} onclose={() => (editing = null)} onsaved={load} />
{/if}

<style>
  .cal {
    flex: 1;
    min-width: 0;
    display: flex;
    gap: 8px;
  }
  .side {
    width: 260px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 4px 4px 0;
  }
  .new {
    height: 38px;
    border-radius: var(--radius-l);
  }
  .mini {
    padding: 8px 6px;
  }
  .mini-head {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 4px 6px;
  }
  .mini-head b {
    flex: 1;
    text-transform: capitalize;
    font-size: 0.9rem;
  }
  .icon-btn.s {
    width: 26px;
    height: 26px;
  }
  .mini-grid {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 2px;
    text-align: center;
  }
  .dn {
    font-size: 0.72rem;
    color: var(--text-3);
    font-weight: 600;
    padding-bottom: 2px;
  }
  .md {
    height: 28px;
    border: none;
    border-radius: 50%;
    background: transparent;
    font-size: 0.8rem;
  }
  .md:hover {
    background: var(--hover);
  }
  .md.out {
    color: var(--text-3);
  }
  .md.cur {
    background: var(--selected);
  }
  .md.today {
    background: var(--accent);
    color: white;
    font-weight: 700;
  }
  .cals {
    flex: 1;
    padding: 0 2px 12px;
  }
  .calrow {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 32px;
    padding: 0 8px;
    border: none;
    border-radius: var(--radius);
    background: transparent;
    text-align: left;
  }
  .calrow:hover {
    background: var(--hover);
  }
  .box {
    width: 18px;
    height: 18px;
    border-radius: 4px;
    border: 2px solid var(--c);
    display: flex;
    align-items: center;
    justify-content: center;
    color: white;
    flex: none;
  }
  .box.on {
    background: var(--c);
  }
  .nm {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 0.92rem;
  }
  .cal-title {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 4px 4px 8px;
    font-size: 0.78rem;
    font-weight: 700;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .tag {
    font-size: 0.68rem;
    font-weight: 700;
    padding: 0 5px;
    border-radius: 4px;
    border: 1px solid var(--border-strong);
    color: var(--text-3);
  }
  .hint {
    padding: 4px 8px;
    line-height: 1.5;
  }
  .up-title {
    margin: 14px 8px 4px;
    font-size: 0.78rem;
    font-weight: 700;
    color: var(--text-3);
  }
  .up {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    width: 100%;
    padding: 6px 10px;
    border: none;
    border-left: 3px solid var(--c);
    border-radius: 0 var(--radius) var(--radius) 0;
    background: transparent;
    margin-bottom: 4px;
    text-align: left;
  }
  .up:hover {
    background: var(--hover);
  }
  .up b {
    font-size: 0.88rem;
    font-weight: 600;
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border-radius: var(--radius-l);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-s);
    overflow: hidden;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
  }
  h2 {
    margin: 0 0 0 8px;
    font-family: var(--font-display);
    font-size: 1.15rem;
    font-weight: 600;
    text-transform: capitalize;
  }
  /* month */
  .month {
    flex: 1;
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    grid-template-rows: 30px repeat(6, 1fr);
    min-height: 0;
  }
  .mh {
    font-size: 0.78rem;
    font-weight: 700;
    color: var(--text-3);
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
  }
  .mcell {
    border-right: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
    padding: 4px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-height: 0;
    overflow: hidden;
  }
  .mcell.out {
    background: var(--surface-2);
  }
  .mcell.out .mnum {
    color: var(--text-3);
  }
  .mnum {
    align-self: flex-start;
    border: none;
    background: none;
    font-size: 0.82rem;
    font-weight: 600;
    padding: 2px 6px;
    border-radius: 12px;
  }
  .mnum:hover {
    background: var(--hover);
  }
  .mcell.today .mnum {
    background: var(--accent);
    color: white;
  }
  .mev {
    display: flex;
    align-items: center;
    gap: 5px;
    height: 22px;
    padding: 0 6px;
    border: none;
    border-radius: 4px;
    background: transparent;
    font-size: 0.78rem;
    text-align: left;
    min-width: 0;
  }
  .mev:hover {
    background: var(--hover);
  }
  .mev i {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--c);
    flex: none;
  }
  .mev.solid {
    background: color-mix(in srgb, var(--c) 22%, var(--surface));
    border-left: 3px solid var(--c);
  }
  .mev .t {
    color: var(--text-3);
    flex: none;
  }
  .mev .tt {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .more {
    border: none;
    background: none;
    font-size: 0.76rem;
    color: var(--text-accent);
    text-align: left;
    padding: 0 6px;
    font-weight: 600;
  }
  /* week/day */
  .week {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .wh,
  .alld {
    display: flex;
    border-bottom: 1px solid var(--border);
    padding-right: 10px;
  }
  .gutter {
    width: 58px;
    flex: none;
    position: relative;
  }
  .alld .gutter {
    padding: 6px;
    text-align: right;
  }
  .whd {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 6px 0 8px;
  }
  .dname {
    font-size: 0.76rem;
    color: var(--text-3);
    font-weight: 600;
    text-transform: capitalize;
  }
  .dnum {
    font-size: 1.35rem;
    font-weight: 600;
    width: 38px;
    height: 38px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
  }
  .whd.today .dnum {
    background: var(--accent);
    color: white;
  }
  .whd.today .dname {
    color: var(--text-accent);
  }
  .adcell {
    flex: 1;
    min-height: 28px;
    padding: 2px;
    border-left: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .adev {
    border: none;
    border-radius: 4px;
    background: color-mix(in srgb, var(--c) 24%, var(--surface));
    border-left: 3px solid var(--c);
    font-size: 0.78rem;
    padding: 2px 6px;
    text-align: left;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .grid {
    flex: 1;
    min-height: 0;
  }
  .hours {
    display: flex;
    position: relative;
  }
  .hl {
    position: absolute;
    right: 8px;
    transform: translateY(-50%);
    font-size: 0.72rem;
    color: var(--text-3);
  }
  .col {
    flex: 1;
    position: relative;
    border-left: 1px solid var(--border);
  }
  .col.today {
    background: color-mix(in srgb, var(--accent) 4%, transparent);
  }
  .line {
    position: absolute;
    left: 0;
    right: 0;
    height: 1px;
    background: var(--border);
    pointer-events: none;
  }
  .ev {
    position: absolute;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
    padding: 3px 6px;
    border: none;
    border-left: 3px solid var(--c);
    border-radius: 5px;
    background: color-mix(in srgb, var(--c) 20%, var(--surface));
    font-size: 0.78rem;
    text-align: left;
    overflow: hidden;
    transition: box-shadow var(--dur), transform 100ms;
  }
  .ev:hover {
    box-shadow: var(--shadow);
    z-index: 2;
  }
  .ev b {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100%;
  }
  .ev span {
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100%;
  }
  .ev.short {
    flex-direction: row;
    align-items: center;
    gap: 6px;
    padding-top: 1px;
    padding-bottom: 1px;
  }
  .ev.short span {
    flex: none;
  }
  .now {
    position: absolute;
    left: -4px;
    right: 0;
    height: 2px;
    background: #d13438;
    pointer-events: none;
    z-index: 3;
  }
  .now::before {
    content: "";
    position: absolute;
    left: -2px;
    top: -4px;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: #d13438;
  }
</style>
