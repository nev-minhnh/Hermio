<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import { onMount, tick, untrack } from "svelte";
  import { reduced } from "../lib/motion";
  import Icon from "../lib/Icon.svelte";
  import { store } from "../lib/store.svelte";
  import { api, type Message } from "../lib/api";
  import { avatarColor, groupLabel, initials, listTime, highlight, searchTerms, hitHtml } from "../lib/util";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";
  import SuggestionsBar from "./SuggestionsBar.svelte";
  import { slide } from "svelte/transition";

  const terms = $derived(searchTerms(store.search));
  const hl = (s: string) => highlight(s, terms);

  let { onopen }: { onopen?: () => void } = $props();

  type Item = { kind: "h"; label: string; top: number; h: number } | { kind: "m"; msg: Message; top: number; h: number; index: number };

  const HEADER_H = 34;
  const searching = $derived(!!store.search.trim());
  const rowH = $derived.by(() => {
    const compact = store.settings.density === "compact";
    const preview = store.settings.preview_lines > 0 || searching;
    const h = compact ? (preview ? 62 : 46) : preview ? 80 : 62;
    // Search results show two lines of matching text.
    return searching ? h + 18 : h;
  });
  const hitLabel = (f?: string) => (f === "from" ? t("From") : f === "to" ? t("To") : f === "cc" ? "Cc" : "");

  // Flatten messages + date group headers, with precomputed offsets.
  const layout = $derived.by(() => {
    const items: Item[] = [];
    let top = 0;
    let last = "";
    store.messages.forEach((m, index) => {
      const g = groupLabel(m.date);
      if (g !== last) {
        items.push({ kind: "h", label: g, top, h: HEADER_H });
        top += HEADER_H;
        last = g;
      }
      items.push({ kind: "m", msg: m, top, h: rowH, index });
      top += rowH;
    });
    return { items, total: top };
  });

  let viewport: HTMLDivElement;
  let ghostLayer: HTMLDivElement;

  // ----- list motion -----
  // Rows that appear because the data changed (new mail, a search, another folder) fade in;
  // rows that disappear fade out where they were while the rest slide into place (the rows'
  // `transform` transitions). Rows that merely scroll into view never animate.
  let prevIds = new Set<number>();
  let ctx = "";
  let enter = $state<Map<number, number>>(new Map());
  let enterAll = $state(false);
  let enterTimer: ReturnType<typeof setTimeout> | undefined;
  $effect.pre(() => {
    const msgs = store.messages;
    // Typing more of a search refines the same list (rows come and go smoothly); switching view,
    // search mode or filter is a new list (staggered entrance).
    const c = `${store.view}|${store.search.trim() ? "search" : ""}|${store.settings.search_mode}|${store.filter ?? ""}`;
    untrack(() => {
      const ids = new Set(msgs.map((m) => m.id));
      if (viewport && !reduced()) {
        if (c === ctx) {
          // Same list, updated: ghosts for removed rows, fade-in for new ones.
          for (const id of prevIds) {
            if (ids.has(id)) continue;
            const el = viewport.querySelector<HTMLElement>(`.msg[data-id="${id}"]`);
            if (!el || !ghostLayer) continue;
            const g = el.cloneNode(true) as HTMLElement;
            g.removeAttribute("data-id");
            g.classList.add("ghost");
            ghostLayer.appendChild(g);
            g.animate([{ opacity: 1, scale: "1" }, { opacity: 0, scale: "0.97" }], { duration: 200, easing: "cubic-bezier(.2,.8,.2,1)", fill: "forwards" }).onfinish = () => g.remove();
          }
          if (prevIds.size) {
            const fresh = new Map<number, number>();
            for (const m of msgs) if (!prevIds.has(m.id)) fresh.set(m.id, 0);
            if (fresh.size && fresh.size < 60) setEnter(fresh);
          }
        } else if (ids.size) {
          // Another list: the first screenful comes in with a short stagger.
          const fresh = new Map<number, number>();
          msgs.slice(0, 16).forEach((m, i) => fresh.set(m.id, i));
          setEnter(fresh);
          enterAll = true;
        }
      }
      prevIds = ids;
      ctx = c;
    });
  });
  function setEnter(m: Map<number, number>) {
    enter = m;
    clearTimeout(enterTimer);
    enterTimer = setTimeout(() => {
      enter = new Map();
      enterAll = false;
    }, 700);
  }
  let scrollTop = $state(0);
  let height = $state(600);
  const overscan = 600;

  const visible = $derived.by(() => {
    const items = layout.items;
    if (!items.length) return [];
    const from = scrollTop - overscan;
    const to = scrollTop + height + overscan;
    // binary search for the first visible item
    let lo = 0;
    let hi = items.length - 1;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (items[mid].top + items[mid].h < from) lo = mid + 1;
      else hi = mid;
    }
    const out: Item[] = [];
    for (let i = lo; i < items.length && items[i].top < to; i++) out.push(items[i]);
    return out;
  });

  let raf = 0;
  function onScroll() {
    cancelAnimationFrame(raf);
    raf = requestAnimationFrame(() => {
      scrollTop = viewport.scrollTop;
      if (store.hasMore && !store.loading && scrollTop + height > layout.total - 1200) store.loadMessages(false);
    });
  }

  onMount(() => {
    const ro = new ResizeObserver(() => (height = viewport.clientHeight));
    ro.observe(viewport);
    return () => ro.disconnect();
  });

  // Reset scroll when the view changes.
  let lastView = "";
  $effect(() => {
    const key = `${store.view}|${store.search}|${store.filter}`;
    if (key !== lastView) {
      lastView = key;
      if (viewport) viewport.scrollTop = 0;
      scrollTop = 0;
    }
  });

  // Asked from outside (a toast click…): scroll the selected email into view.
  $effect(() => {
    if (store.reveal && store.selected.length === 1) {
      const id = store.selected[0];
      setTimeout(() => ensureVisible(id), 60);
    }
  });

  function select(e: MouseEvent, m: Message, index: number) {
    store.compose = null;
    if (store.checkMode && !e.shiftKey) {
      store.selected = store.selected.includes(m.id) ? store.selected.filter((x) => x !== m.id) : [...store.selected, m.id];
      store.anchor = m.id;
      return;
    }
    if (e.shiftKey && store.anchor !== null) {
      const a = store.messages.findIndex((x) => x.id === store.anchor);
      const [s, t] = a < index ? [a, index] : [index, a];
      store.selected = store.messages.slice(s, t + 1).map((x) => x.id);
    } else if (e.ctrlKey || e.metaKey) {
      store.selected = store.selected.includes(m.id) ? store.selected.filter((x) => x !== m.id) : [...store.selected, m.id];
      store.anchor = m.id;
    } else {
      store.selected = [m.id];
      store.anchor = m.id;
      openDraft(m);
    }
  }

  /** A draft opens straight in the editor, showing what was being written. */
  function openDraft(m: Message) {
    if (store.folderById(m.folder_id)?.role === "drafts") store.startCompose("draft", m);
  }

  const allSelected = $derived(store.messages.length > 0 && store.selected.length >= store.messages.length && store.messages.every((m) => store.selected.includes(m.id)));
  const someSelected = $derived((store.selected.length > 1 || (store.checkMode && store.selected.length > 0)) && !allSelected);
  let selectingAll = $state(false);

  /** Select every message in the view (loads the remaining pages first). */
  async function selectAll() {
    if (allSelected) {
      store.selected = [];
      return;
    }
    selectingAll = true;
    try {
      let guard = 0;
      while (store.hasMore && guard++ < 30) await store.loadMessages(false);
      store.selected = store.messages.map((m) => m.id);
      store.checkMode = true;
    } finally {
      selectingAll = false;
    }
  }
  function toggleCheckMode() {
    store.checkMode = !store.checkMode;
    if (!store.checkMode && store.selected.length > 1) store.selected = store.selected.slice(-1);
  }

  function groupMenu(e: MouseEvent) {
    const ids = [...store.selected];
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menu = { x: r.left, y: r.bottom + 4, items: groupItems(ids) };
  }
  function groupItems(ids: number[]): MenuItem[] {
    const accs = new Set(store.messages.filter((x) => ids.includes(x.id)).map((x) => x.account_id));
    const allowed = store.groups.filter((g) => g.account_id === null || (accs.size === 1 && accs.has(g.account_id)));
    if (!allowed.length) return [{ label: t("No groups yet."), disabled: true, run: () => {} }];
    return allowed.map((g) => ({
      label: g.name + (g.account_id ? " · " + (store.accountById(g.account_id)?.email ?? "") : ""),
      color: g.color,
      run: async () => {
        await api.addToGroup(ids, g.id);
        store.success(t("Moved {n} messages to “{name}”", { n: ids.length, name: g.name }));
        if (!store.view.startsWith("group:")) store.messages = store.messages.filter((x) => !ids.includes(x.id));
        store.selected = [];
        await store.reloadAll();
      },
    }));
  }

  function toggleCheck(e: MouseEvent, m: Message) {
    e.stopPropagation();
    store.compose = null;
    store.selected = store.selected.includes(m.id) ? store.selected.filter((x) => x !== m.id) : [...store.selected, m.id];
    store.anchor = m.id;
  }

  async function ensureVisible(id: number) {
    await tick();
    const it = layout.items.find((x) => x.kind === "m" && x.msg.id === id);
    if (!it || !viewport) return;
    if (it.top < viewport.scrollTop) viewport.scrollTo({ top: it.top - HEADER_H, behavior: "smooth" });
    else if (it.top + it.h > viewport.scrollTop + height) viewport.scrollTo({ top: it.top + it.h - height, behavior: "smooth" });
  }

  function onKey(e: KeyboardEvent) {
    const msgs = store.messages;
    if (!msgs.length) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const curId = store.selected[store.selected.length - 1] ?? store.anchor;
      let i = msgs.findIndex((m) => m.id === curId);
      i = e.key === "ArrowDown" ? Math.min(msgs.length - 1, i + 1) : Math.max(0, i - 1);
      const id = msgs[i].id;
      if (e.shiftKey && store.anchor !== null) {
        const a = msgs.findIndex((m) => m.id === store.anchor);
        const [s, t] = a < i ? [a, i] : [i, a];
        store.selected = msgs.slice(s, t + 1).map((m) => m.id);
      } else {
        store.selected = [id];
        store.anchor = id;
      }
      store.compose = null;
      if (!e.shiftKey) openDraft(msgs[i]);
      ensureVisible(id);
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
      e.preventDefault();
      store.selected = msgs.map((m) => m.id);
    } else if (e.key === "Enter") {
      onopen?.();
    } else if (e.key === "Home") {
      e.preventDefault();
      store.selected = [msgs[0].id];
      viewport.scrollTo({ top: 0, behavior: "smooth" });
    }
  }

  function dragStart(e: DragEvent, m: Message) {
    const ids = store.selected.includes(m.id) ? store.selected : [m.id];
    if (!store.selected.includes(m.id)) store.selected = [m.id];
    e.dataTransfer!.setData("application/x-hermio-ids", JSON.stringify(ids));
    e.dataTransfer!.effectAllowed = "move";
    const ghost = document.createElement("div");
    ghost.className = "drag-ghost";
    ghost.textContent = ids.length > 1 ? t("{n} messages", { n: ids.length }) : m.subject || t("(No subject)");
    document.body.appendChild(ghost);
    e.dataTransfer!.setDragImage(ghost, 12, 12);
    setTimeout(() => ghost.remove());
  }

  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  function rowMenu(e: MouseEvent, m: Message) {
    e.preventDefault();
    if (!store.selected.includes(m.id)) {
      store.selected = [m.id];
      store.anchor = m.id;
    }
    const ids = store.selected;
    const one = ids.length === 1;
    const msgs = store.messages.filter((x) => ids.includes(x.id));
    const read = msgs.every((x) => x.seen);
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        ...(one
          ? ([
              { icon: "reply", label: t("Reply"), run: () => store.startCompose("reply", m) },
              { icon: "replyAll", label: t("Reply all"), run: () => store.startCompose("replyAll", m) },
              { icon: "forward", label: t("Forward"), run: () => store.startCompose("forward", m) },
              { sep: true },
            ] as MenuItem[])
          : []),
        { icon: "sparkle", label: one ? t("AI: summarize this message") : t("AI: summarize {n} messages", { n: ids.length }), run: () => store.ai("summarize", ids) },
        ...(one ? ([{ icon: "sparkle", label: t("AI: quick reply"), run: () => store.ai("reply", ids) }] as MenuItem[]) : []),
        { sep: true },
        { icon: read ? "unread" : "read", label: read ? t("Mark as unread") : t("Mark as read"), run: () => store.markSeen(ids, !read) },
        { icon: "flag", label: msgs.every((x) => x.flagged) ? t("Unflag") : t("Flag"), run: () => store.toggleFlag(ids) },
        { sep: true },
        ...(store.groups.length ? ([{ header: true, label: t("Move to group") }, ...groupItems(ids)] as MenuItem[]) : []),
        ...(store.view.startsWith("group:")
          ? ([{
              icon: "close",
              label: t("Remove from this group"),
              run: async () => {
                await api.removeFromGroup(ids, Number(store.view.slice(6)));
                store.messages = store.messages.filter((x) => !ids.includes(x.id));
                await store.reloadAll();
                store.success(t("{n} email(s) removed from the group", { n: ids.length }));
              },
            }] as MenuItem[])
          : []),
        ...(one
          ? ([{ icon: "filter", label: t("Create rule from this sender…"), run: () => { store.ruleDraft = { from: m.from_addr }; store.settingsTab = "rules"; store.route = "settings"; } }] as MenuItem[])
          : []),
        { sep: true },
        { icon: "archive", label: t("Archive"), run: () => store.archive(ids) },
        { icon: "junk", label: t("Report spam"), run: () => store.spam(ids) },
        { icon: "trash", label: t("Delete"), run: () => store.remove(ids) },
        { icon: "trash", label: t("Delete permanently"), danger: true, run: () => store.remove(ids, true) },
      ],
    };
  }

  const accountColor = (id: number) => store.accountById(id)?.color ?? "var(--accent)";
  const multiAccount = $derived(store.accounts.length > 1 && (store.view.startsWith("unified") || !!store.search));
</script>

<div class="wrap">
  <div class="head">
    <button
      class="cb"
      class:on={allSelected}
      class:some={someSelected}
      title={allSelected ? t("Deselect all") : t("Select all")}
      aria-label={t("Select all")}
      onclick={selectAll}
      disabled={!store.messages.length || selectingAll}
    >
      {#if selectingAll}<Icon name="sync" size={12} class="spin" />{:else if allSelected}<Icon name="check" size={13} />{:else if someSelected}<i></i>{/if}
    </button>
    <div class="title">
      <h2>{store.viewTitle()}</h2>
      {#if store.filter}<span class="chip">{{ unread: t("Unread"), flagged: t("Flagged"), attachments: t("Attachments") }[store.filter]}</span>{/if}
    </div>
    <button class="icon-btn" class:on={store.checkMode} title={t("Select messages")} onclick={toggleCheckMode}><Icon name="selectAll" size={17} /></button>
  </div>
  {#if store.selected.length > 1 || (store.checkMode && store.selected.length)}
    <div class="bulk" transition:slide={{ duration: 160 }}>
      <span class="count">{t("{n} selected", { n: store.selected.length })}</span>
      <span class="grow"></span>
      <button class="icon-btn" title={t("Mark as read")} onclick={() => store.markSeen(store.selected, true)}><Icon name="read" size={16} /></button>
      <button class="icon-btn" title={t("Mark as unread")} onclick={() => store.markSeen(store.selected, false)}><Icon name="unread" size={16} /></button>
      <button class="icon-btn" title={t("Flag")} onclick={() => store.toggleFlag(store.selected)}><Icon name="flag" size={16} /></button>
      <button class="icon-btn" title={t("Move to group")} onclick={groupMenu}><Icon name="tag" size={16} /></button>
      <button class="icon-btn" title={t("Archive")} onclick={() => store.archive([...store.selected])}><Icon name="archive" size={16} /></button>
      <button class="icon-btn" title={t("Delete")} onclick={() => store.remove([...store.selected])}><Icon name="trash" size={16} /></button>
      <button class="icon-btn" title={t("Deselect")} onclick={() => { store.selected = []; store.checkMode = false; }}><Icon name="close" size={16} /></button>
    </div>
  {/if}
  {#if !store.search.trim() && (store.view === "unified:inbox" || store.view.startsWith("account:") || store.folderById(Number(store.view.slice(7)))?.role === "inbox")}
    <SuggestionsBar />
  {/if}

  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="viewport scroll" bind:this={viewport} onscroll={onScroll} tabindex="0" onkeydown={onKey} role="listbox" aria-multiselectable="true">
    <div class="spacer" style:height="{layout.total}px">
      <div class="ghosts" bind:this={ghostLayer} aria-hidden="true"></div>
      {#each visible as it (it.kind === "m" ? it.msg.id : "h" + it.top)}
        {#if it.kind === "h"}
          <div class="group" style:transform="translateY({it.top}px)" class:enter={enterAll}>{it.label}</div>
        {:else}
          {@const m = it.msg}
          {@const sel = store.selected.includes(m.id)}
          {@const ei = enter.get(m.id)}
          <div
            class="msg"
            data-id={m.id}
            class:enter={ei !== undefined}
            style:--d="{Math.min(ei ?? 0, 12) * 18}ms"
            class:unread={!m.seen}
            class:sel
            class:multi={store.selected.length > 1 || store.checkMode}
            style:transform="translateY({it.top}px)"
            style:height="{it.h}px"
            role="option"
            aria-selected={sel}
            tabindex="-1"
            draggable="true"
            ondragstart={(e) => dragStart(e, m)}
            onclick={(e) => select(e, m, it.index)}
            ondblclick={() => (store.folderById(m.folder_id)?.role === "drafts" ? store.startCompose("draft", m) : onopen?.())}
            oncontextmenu={(e) => rowMenu(e, m)}
            onkeydown={() => {}}
          >
            {#if store.checkMode && !store.settings.show_avatars}
              <span class="rowcb" class:on={sel}>{#if sel}<Icon name="check" size={12} />{/if}</span>
            {/if}
            {#if store.settings.show_avatars}
              <button class="avatar" style:background={avatarColor(m.from_addr || m.from_name)} onclick={(e) => toggleCheck(e, m)} title={t("Select")}>
                <span class="ini">{initials(m.from_name, m.from_addr)}</span>
                <span class="check"><Icon name="check" size={16} /></span>
              </button>
            {/if}
            <div class="lines">
              <div class="l1">
                {#if multiAccount}<i class="acc" style:background={accountColor(m.account_id)}></i>{/if}
                <span class="from">{@html hl(m.from_name || m.from_addr || t("(Unknown)"))}</span>
                <span class="tools">
                  <button class="mini" title={m.seen ? t("Mark as unread") : t("Mark as read")} onclick={(e) => { e.stopPropagation(); store.markSeen([m.id], !m.seen); }}>
                    <Icon name={m.seen ? "unread" : "read"} size={15} />
                  </button>
                  <button class="mini" title={t("Delete")} onclick={(e) => { e.stopPropagation(); store.remove([m.id]); }}>
                    <Icon name="trash" size={15} />
                  </button>
                </span>
                <button class="mini flag" class:on={m.flagged} title={m.flagged ? t("Unflag") : t("Flag")} onclick={(e) => { e.stopPropagation(); store.toggleFlag([m.id]); }}>
                  <Icon name="flag" size={15} fill={m.flagged} />
                </button>
              </div>
              <div class="l2">
                {#if store.cite.has(m.id)}<span class="cite-badge" title={t("Email {n} in the assistant's answer", { n: store.cite.get(m.id) ?? 0 })}>{store.cite.get(m.id)}</span>{/if}
                <span class="subject">{@html hl(m.subject || t("(No subject)"))}</span>
                {#if m.has_attachments}<Icon name="attach" size={14} />{/if}
                {#if m.answered}<Icon name="reply" size={14} />{/if}
                <span class="time">{listTime(m.date)}</span>
              </div>
              {#if searching}
                <div class="l3 hit">{#if m.hit && m.hit_field && m.hit_field !== "body"}<span class="hit-label">{hitLabel(m.hit_field)}</span>{/if}{@html m.hit ? hitHtml(m.hit, terms) : hl(m.snippet)}</div>
              {:else if store.settings.preview_lines > 0}
                <div class="l3">{@html hl(m.snippet)}</div>
              {/if}
            </div>
          </div>
        {/if}
      {/each}
    </div>

    {#if !store.loading && store.messages.length === 0}
      <div class="empty">
        <svg width="120" height="96" viewBox="0 0 120 96" aria-hidden="true">
          <rect x="14" y="22" width="92" height="62" rx="10" fill="var(--selected)" />
          <path d="M16 30 60 58 104 30" fill="none" stroke="var(--accent)" stroke-width="4" stroke-linecap="round" stroke-linejoin="round" />
          <circle cx="94" cy="20" r="10" fill="var(--accent)" opacity=".35" />
        </svg>
        <b>{store.search ? t("No matching messages") : t("No messages")}</b>
        <span class="muted small">
          {store.search
            ? store.settings.search_mode === "subject"
              ? t("Try switching search mode to “Content”.")
              : t("Newly downloaded mail is indexed gradually after each sync.")
            : t("All caught up ✨")}
        </span>
      </div>
    {/if}
    {#if store.loading && store.messages.length === 0}
      <div class="skeletons">
        {#each Array(8) as _}
          <div class="sk"><i></i><div><b></b><b></b><b></b></div></div>
        {/each}
      </div>
    {/if}
  </div>
</div>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

<style>
  .wrap {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .head {
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 46px;
    padding: 0 10px 0 18px;
  }
  .title {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  h2 {
    margin: 0;
    font-family: var(--font-display);
    font-size: 1.12rem;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .chip {
    font-size: 0.76rem;
    font-weight: 600;
    padding: 2px 8px;
    border-radius: 10px;
    background: var(--selected);
    color: var(--text-accent);
  }
  .count {
    font-size: 0.85rem;
    color: var(--text-2);
    font-weight: 600;
  }
  .head .icon-btn.on {
    background: var(--selected);
    color: var(--text-accent);
  }
  .cb,
  .rowcb {
    flex: none;
    width: 18px;
    height: 18px;
    border-radius: 5px;
    border: 1.5px solid var(--border-strong);
    background: var(--surface);
    display: grid;
    place-items: center;
    padding: 0;
    color: white;
  }
  .cb.on,
  .rowcb.on {
    background: var(--accent);
    border-color: var(--accent);
  }
  .cb.some i {
    width: 8px;
    height: 2px;
    border-radius: 1px;
    background: var(--accent);
  }
  .rowcb {
    margin-top: 9px;
  }
  .bulk {
    flex: none;
    display: flex;
    align-items: center;
    gap: 2px;
    margin: 0 8px 6px;
    padding: 4px 4px 4px 12px;
    border-radius: 10px;
    background: var(--selected);
  }
  .bulk .grow {
    flex: 1;
  }
  .viewport {
    flex: 1;
    min-height: 0;
    position: relative;
    outline: none;
    scroll-behavior: auto;
  }
  .spacer {
    position: relative;
    contain: strict;
    width: 100%;
  }
  .ghosts {
    position: absolute;
    inset: 0;
    pointer-events: none;
    z-index: 0;
  }
  .ghosts :global(.ghost) {
    pointer-events: none;
    transition: none;
  }
  .enter {
    animation: row-in 260ms var(--ease) both;
    animation-delay: var(--d, 0ms);
  }
  @keyframes row-in {
    from {
      opacity: 0;
      translate: 0 8px;
    }
    to {
      opacity: 1;
      translate: 0 0;
    }
  }
  .group {
    position: absolute;
    left: 0;
    right: 0;
    height: 34px;
    transition: transform 220ms var(--ease);
    padding: 12px 18px 0;
    font-size: 0.8rem;
    font-weight: 700;
    color: var(--text-2);
  }
  .msg {
    position: absolute;
    left: 6px;
    right: 6px;
    display: flex;
    gap: 12px;
    padding: 9px 10px 9px 12px;
    border-radius: var(--radius);
    contain: layout paint;
    transition:
      background 120ms var(--ease),
      transform 220ms var(--ease);
    will-change: transform;
  }
  .msg::after {
    content: "";
    position: absolute;
    left: 12px;
    right: 12px;
    bottom: 0;
    height: 1px;
    background: var(--border);
  }
  .msg:hover {
    background: var(--hover);
  }
  .msg.sel {
    background: var(--selected);
  }
  .msg.sel::after {
    opacity: 0;
  }
  .msg.unread::before {
    content: "";
    position: absolute;
    left: 2px;
    top: 10px;
    bottom: 10px;
    width: 3px;
    border-radius: 2px;
    background: var(--accent);
  }
  .avatar {
    position: relative;
    flex: none;
    width: 34px;
    height: 34px;
    margin-top: 2px;
    border-radius: 50%;
    border: none;
    color: white;
    font-size: 0.78rem;
    font-weight: 700;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
  }
  .avatar .check {
    position: absolute;
    inset: 0;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--surface);
    color: var(--text-3);
    border: 1.5px solid var(--border-strong);
    opacity: 0;
    transition: opacity 120ms;
  }
  .msg:hover .avatar .check,
  .msg.multi .avatar .check {
    opacity: 1;
  }
  .msg.multi.sel .avatar .check,
  .msg.sel:hover .avatar .check {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
  }
  .lines {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .l1,
  .l2 {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    height: 20px;
  }
  .acc {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex: none;
  }
  .from {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.97rem;
  }
  .unread .from {
    font-weight: 700;
  }
  .subject {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text);
    font-size: 0.9rem;
  }
  .unread .subject {
    color: var(--text-accent);
    font-weight: 600;
  }
  .l2 :global(.icon) {
    color: var(--text-3);
  }
  .time {
    flex: none;
    font-size: 0.8rem;
    color: var(--text-3);
  }
  .unread .time {
    color: var(--text-accent);
    font-weight: 600;
  }
  .l3 {
    font-size: 0.85rem;
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .l3.hit {
    white-space: normal;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    line-height: 1.35;
    color: var(--text-2);
    animation: fade 200ms var(--ease);
  }
  .hit-label {
    display: inline-block;
    margin-right: 6px;
    padding: 0 5px;
    border-radius: 4px;
    font-size: 0.74rem;
    font-weight: 700;
    line-height: 1.5;
    color: var(--text-accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .tools {
    display: none;
    gap: 2px;
  }
  .msg:hover .tools {
    display: flex;
  }
  .mini {
    width: 24px;
    height: 22px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text-2);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
  }
  .mini:hover {
    background: var(--press);
    color: var(--text);
  }
  .flag {
    display: none;
  }
  .msg:hover .flag,
  .flag.on {
    display: flex;
  }
  .flag.on {
    color: #c4314b;
  }
  .cite-badge {
    flex: none;
    min-width: 18px;
    height: 18px;
    padding: 0 5px;
    border-radius: 9px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 0.72rem;
    font-weight: 700;
    color: white;
    background: var(--accent);
    animation: pop 200ms var(--ease);
  }
  .flag.on :global(svg) {
    animation: pop 220ms var(--ease);
  }
  .from,
  .subject {
    transition: color 160ms var(--ease);
  }
  .msg.unread::before {
    animation: fade 200ms var(--ease);
  }
  .empty {
    animation: fade 220ms var(--ease);
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    text-align: center;
    padding: 24px;
    animation: fade 200ms;
  }
  .skeletons {
    position: absolute;
    inset: 0;
    padding: 10px 16px;
  }
  .sk {
    display: flex;
    gap: 12px;
    height: 76px;
    align-items: flex-start;
    padding-top: 10px;
  }
  .sk i {
    width: 34px;
    height: 34px;
    border-radius: 50%;
    background: var(--surface-3);
  }
  .sk div {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .sk b {
    height: 10px;
    border-radius: 5px;
    background: linear-gradient(90deg, var(--surface-3), var(--hover), var(--surface-3));
    background-size: 200% 100%;
    animation: shimmer 1.2s infinite linear;
  }
  .sk b:nth-child(1) { width: 45%; }
  .sk b:nth-child(2) { width: 80%; }
  .sk b:nth-child(3) { width: 65%; }
  @keyframes shimmer {
    to {
      background-position: -200% 0;
    }
  }
  :global(.drag-ghost) {
    position: fixed;
    top: -100px;
    padding: 6px 12px;
    border-radius: 8px;
    background: var(--accent);
    color: white;
    font-weight: 600;
    font-size: 13px;
    max-width: 260px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
