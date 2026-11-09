<script lang="ts">
  import { slide, fade } from "svelte/transition";
  import { reduced } from "../lib/motion";
  import { t, locale } from "../lib/i18n.svelte";
  import { untrack } from "svelte";
  import Icon from "../lib/Icon.svelte";
  import { store } from "../lib/store.svelte";
  import { api, type MessageBody, type Message } from "../lib/api";
  import { avatarColor, fileSize, fullTime, initials, parseIcs, splitAddresses, displayOf, searchTerms, highlightDom, highlight } from "../lib/util";
  import { pickSavePath } from "../lib/tauri";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";
  import EventDialog from "./EventDialog.svelte";
  import InviteDialog from "./InviteDialog.svelte";
  import { inviteDone } from "../lib/invite";
  import type { InviteInfo } from "../lib/api";
  import AttachmentViewer from "./AttachmentViewer.svelte";

  let { onback }: { onback?: () => void } = $props();

  let body = $state<MessageBody | null>(null);
  let error = $state("");
  let allowImages = $state(false);
  let frame = $state<HTMLIFrameElement>();
  let showDetails = $state(false);
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  let eventDraft = $state<any>(null);
  let preview = $state<number | null>(null);
  let seenTimer: ReturnType<typeof setTimeout> | undefined;

  // Keyed on the id only: background refreshes replace row data in place and must not
  // reload (or flash) the open message.
  const curId = $derived(store.selected.length === 1 ? store.selected[0] : null);
  /**
   * The email on screen. It lags behind the selection on purpose: the previous email stays (dimmed)
   * until the next one's body is loaded, then the two cross-fade, instead of the pane blanking,
   * showing a skeleton and popping the new email in.
   */
  let shownId = $state<number | null>(null);
  let leaving = $state(false);
  /** Copy of the email on screen, for the moment it leaves the list (deleted, archived…). */
  let snap = null as Message | null;
  const msg = $derived(shownId === null ? null : (store.messages.find((m) => m.id === shownId) ?? (snap?.id === shownId ? snap : null)));
  let skeletonTimer: ReturnType<typeof setTimeout> | undefined;
  /** When the current email started fading out (so it gets its full fade before the swap). */
  let leftAt = 0;
  const FADE_OUT = 110;
  /** Fade (and rise slightly) into view. */
  function enter(_node: Element) {
    return reduced()
      ? { duration: 0 }
      : { duration: 220, easing: (t: number) => 1 - Math.pow(1 - t, 3), css: (t: number) => `opacity:${t};transform:translateY(${(1 - t) * 6}px)` };
  }
  const multi = $derived(store.selected.length > 1);
  const invite = $derived(body?.ics ? parseIcs(body.ics) : null);
  /** The calendar event in this email, as the back end reads it (with time zones), and what was done with it. */
  let inviteInfo = $state<InviteInfo | null>(null);
  /** Ask about it (popup) — once per email. */
  let askInvite = $state(false);
  let inviteBusy = $state<string | null>(null);
  function loadInvite(id: number) {
    inviteInfo = null;
    askInvite = false;
    api
      .inviteInfo(id)
      .then((info) => {
        if (shownId !== id) return;
        inviteInfo = info;
        const now = Date.now() / 1000;
        askInvite = !!info && info.state === "" && !info.in_calendar && !["CANCEL", "REPLY"].includes(info.method) && info.end_ts > now;
      })
      .catch(() => {});
  }
  async function answer(a: "add" | "accepted" | "tentative" | "declined") {
    if (!msg || !inviteInfo || inviteBusy) return;
    const id = msg.id;
    inviteBusy = a;
    try {
      const r = await api.inviteRespond(id, a);
      inviteDone(a, r, inviteInfo);
      if (shownId === id && inviteInfo) inviteInfo = { ...inviteInfo, state: a === "add" ? "added" : a, in_calendar: a !== "declined" || inviteInfo.in_calendar };
    } catch (e) {
      store.error(t("Couldn't update the calendar: {err}", { err: String(e) }));
    } finally {
      inviteBusy = null;
    }
  }
  const dark = $derived(document.documentElement.dataset.theme === "dark");
  const zoom = $derived(store.settings.reading_zoom || 1);
  const terms = $derived(searchTerms(store.search));
  const files = $derived(body ? body.attachments.filter((a) => !a.inline) : []);

  /** Put email `id` on screen (with its body, or none yet → skeleton). */
  function show(id: number, b: MessageBody | null, err = "") {
    clearTimeout(skeletonTimer);
    if (shownId !== id) {
      showDetails = false;
      preview = null;
    }
    body = b;
    error = err;
    snap = store.messages.find((m) => m.id === id) ?? null;
    if (shownId !== id) {
      if (b?.ics) loadInvite(id);
      else {
        inviteInfo = null;
        askInvite = false;
      }
    } else if (b?.ics && !inviteInfo) loadInvite(id);
    // Set together with the body, so the email still on screen doesn't re-render meanwhile.
    if (shownId !== id && snap)
      allowImages = store.settings.remote_images === "always" || store.settings.trusted_senders.includes(snap.from_addr.toLowerCase());
    shownId = id;
    leaving = false;
  }

  $effect(() => {
    const id = curId;
    clearTimeout(seenTimer);
    clearTimeout(skeletonTimer);
    if (id === null) {
      untrack(() => {
        shownId = null;
        body = null;
        error = "";
        leaving = false;
      });
      return;
    }
    const m = untrack(() => store.messages.find((x) => x.id === id));
    if (!m) return;
    untrack(() => {
      if (shownId !== id) {
        if (shownId === null) {
          // Nothing on screen yet: show the header at once, the body fades in when ready.
          show(id, null);
        } else {
          // Quick successive picks (arrow keys) keep the first fade-out's clock.
          if (!leaving) leftAt = performance.now();
          leaving = true;
          // A slow body: don't keep the old email up for long, show the new one's skeleton.
          skeletonTimer = setTimeout(() => curId === id && shownId !== id && show(id, null), 280);
        }
        load(m);
      }
      const delay = store.settings.mark_read_delay;
      if (!m.seen && delay >= 0) {
        seenTimer = setTimeout(() => {
          const cur = store.messages.find((x) => x.id === id);
          if (curId === id && cur && !cur.seen) store.markSeen([id], true, false);
        }, delay * 1000);
      }
    });
    return () => clearTimeout(seenTimer);
  });

  async function load(m: Message) {
    try {
      const b = await api.message(m.id);
      // Let the previous email finish fading out first.
      const wait = leaving ? FADE_OUT - (performance.now() - leftAt) : 0;
      if (wait > 0) await new Promise((r) => setTimeout(r, wait));
      if (curId === m.id) show(m.id, b);
    } catch (e) {
      if (curId === m.id) show(m.id, null, String(e));
    }
  }

  const srcdoc = $derived.by(() => {
    if (!body) return "";
    const imgs = allowImages ? "data: cid: https: http:" : "data: cid:";
    const fonts = allowImages ? "data: https:" : "data:";
    const isPlain = body.html.startsWith('<div style="white-space:pre-wrap');
    const fg = isPlain && dark ? "#f2f2f2" : "#1b1b1b";
    const bg = isPlain && dark ? "#262626" : "#ffffff";
    const sb = isPlain && dark ? "rgba(255,255,255,.24)" : "rgba(0,0,0,.22)";
    return `<!doctype html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src ${imgs}; style-src 'unsafe-inline' https:; font-src ${fonts}; media-src data:">
<base target="_blank">
<style>
html{background:${bg};color:${fg};scrollbar-color:${sb} transparent;}
html,body{margin:0;padding:0;}
body{font-family:'Segoe UI',system-ui,sans-serif;font-size:15px;line-height:1.55;padding:20px 26px 40px;overflow-wrap:anywhere;}
img{max-width:100%;height:auto;}
pre{white-space:pre-wrap;}
a{color:${isPlain && dark ? "#7fb5ff" : "#1d4fe0"};}
blockquote{margin:0 0 0 .5em;padding-left:1em;border-left:3px solid #c8c8c8;color:${isPlain && dark ? "#bbb" : "#555"};}
table{max-width:100%;}
mark.hermio-hl{background:#ffe066;color:#1b1b1b;border-radius:2px;padding:0 1px;}
::-webkit-scrollbar{width:12px;height:12px}
::-webkit-scrollbar-thumb{background:${sb};border-radius:8px;border:3px solid transparent;background-clip:content-box}
</style></head><body>${body.html}</body></html>`;
  });

  /** Apply the zoom to the body without reloading the frame (no flicker). */
  function applyZoom() {
    const b = frame?.contentDocument?.body;
    if (b) b.style.zoom = String(zoom);
  }
  $effect(() => {
    zoom;
    applyZoom();
  });

  let frameReady = $state(false);
  // A new message starts hidden and fades in once its body has rendered.
  const msgId = $derived(msg?.id ?? null);
  $effect.pre(() => {
    void msgId;
    frameReady = false;
  });
  function onFrameLoad() {
    const doc = frame?.contentDocument;
    frameReady = true;
    if (!doc) return;
    applyZoom();
    const first = highlightDom(doc.body, untrack(() => terms));
    if (first) first.scrollIntoView({ block: "center" });
    doc.addEventListener("click", (e) => {
      const a = (e.target as HTMLElement).closest("a");
      if (!a) return;
      e.preventDefault();
      const href = a.getAttribute("href") ?? "";
      if (href.startsWith("mailto:")) {
        store.startCompose("new", undefined, [decodeURIComponent(href.slice(7).split("?")[0])]);
      } else if (/^https?:/i.test(href)) {
        api.openUrl(href);
      }
    });
    doc.addEventListener("contextmenu", (e) => e.stopPropagation());
    doc.addEventListener(
      "wheel",
      (e) => {
        if (!e.ctrlKey) return;
        e.preventDefault();
        setZoom(zoom * (e.deltaY < 0 ? 1.1 : 1 / 1.1));
      },
      { passive: false },
    );
    doc.addEventListener("keydown", (e) => {
      if (!(e.ctrlKey || e.metaKey)) return;
      if (e.key === "+" || e.key === "=") {
        e.preventDefault();
        setZoom(zoom + 0.1);
      } else if (e.key === "-") {
        e.preventDefault();
        setZoom(zoom - 0.1);
      } else if (e.key === "0") {
        e.preventDefault();
        setZoom(1);
      }
    });
  }

  let zoomSave: ReturnType<typeof setTimeout> | undefined;
  function setZoom(z: number) {
    const v = Math.min(3, Math.max(0.5, Math.round(z * 100) / 100));
    store.settings.reading_zoom = v;
    clearTimeout(zoomSave);
    zoomSave = setTimeout(() => store.set("reading_zoom", v), 400);
  }

  function trustSender() {
    if (!msg) return;
    const list = [...store.settings.trusted_senders, msg.from_addr.toLowerCase()];
    store.set("trusted_senders", list);
    allowImages = true;
  }

  async function saveAtt(index: number, name: string) {
    if (!msg) return;
    const path = await pickSavePath(name);
    if (!path) return;
    try {
      await api.saveAttachment(msg.id, index, path);
      store.success(t("File saved"));
    } catch (e) {
      store.error(String(e));
    }
  }

  function attMenu(e: MouseEvent, i: number) {
    if (!msg) return;
    e.preventDefault();
    const a = files[i];
    const m = msg;
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { icon: "eye", label: t("Preview"), run: () => (preview = i) },
        { icon: "open", label: t("Open with app"), run: () => api.openAttachment(m.id, a.index).catch((er) => store.error(String(er))) },
        { icon: "download", label: t("Download"), run: () => saveAtt(a.index, a.name) },
      ],
    };
  }

  function moreMenu(e: MouseEvent) {
    if (!msg) return;
    const m = msg;
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menu = {
      x: r.right - 220,
      y: r.bottom + 4,
      items: [
        { icon: m.seen ? "unread" : "read", label: m.seen ? t("Mark as unread") : t("Mark as read"), run: () => store.markSeen([m.id], !m.seen) },
        { icon: "flag", label: m.flagged ? t("Unflag") : t("Flag"), run: () => store.toggleFlag([m.id]) },
        { icon: "archive", label: t("Archive"), run: () => store.archive([m.id]) },
        { sep: true },
        { icon: "print", label: t("Print"), run: () => frame?.contentWindow?.print() },
        {
          icon: "code",
          label: t("View message source"),
          run: async () => {
            const src = await api.source(m.id);
            const w = window.open("", "_blank");
            if (w) {
              w.document.title = t("Message source");
              const pre = w.document.createElement("pre");
              pre.textContent = src;
              pre.style.cssText = "white-space:pre-wrap;font:12px Consolas,monospace;padding:12px";
              w.document.body.appendChild(pre);
            }
          },
        },
        { sep: true },
        { icon: "junk", label: t("Report spam"), run: () => store.spam([m.id]) },
        { icon: "trash", label: t("Delete permanently"), danger: true, run: () => store.remove([m.id], true) },
      ],
    };
  }

  function addInvite() {
    if (!invite) return;
    eventDraft = {
      id: 0,
      title: invite.title,
      location: invite.location,
      description: invite.description,
      start_ts: invite.start,
      end_ts: invite.end,
      all_day: invite.allDay,
    };
  }

  function multiGroup(e: MouseEvent) {
    const ids = [...store.selected];
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const allowed = store.groups.filter((g) => g.account_id === null || ids.every((id) => store.messages.find((m) => m.id === id)?.account_id === g.account_id));
    menu = {
      x: r.left,
      y: r.bottom + 4,
      items: allowed.length
        ? allowed.map((g) => ({
            label: g.name + (g.account_id ? ` · ${store.accountById(g.account_id)?.email ?? ""}` : ""),
            color: g.color,
            run: async () => {
              await api.addToGroup(ids, g.id);
              store.success(t("Moved {n} messages to “{name}”", { n: ids.length, name: g.name }));
              store.reloadAll();
            },
          }))
        : [{ label: t("No groups yet."), disabled: true, run: () => {} }],
    };
  }

  const recipients = (s: string) => splitAddresses(s).map(displayOf).join(", ");
  const extColor = (name: string) => {
    const e = (name.split(".").pop() ?? "").toLowerCase();
    if (e === "pdf") return "#d13438";
    if (["doc", "docx", "odt", "rtf"].includes(e)) return "#185abd";
    if (["xls", "xlsx", "csv", "ods"].includes(e)) return "#107c41";
    if (["ppt", "pptx", "odp"].includes(e)) return "#c43e1c";
    if (["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "heic"].includes(e)) return "var(--ai)";
    if (["zip", "rar", "7z", "gz"].includes(e)) return "#986f0b";
    return "var(--accent)";
  };
</script>

<div class="pane">
  {#if multi}
    <div class="center fade-in">
      <div class="stack">{store.selected.length}</div>
      <b>{store.selected.length} {t("messages selected")}</b>
      <div class="row wrap">
        <button class="btn" onclick={() => store.markSeen(store.selected, true)}><Icon name="read" size={16} />{t("Mark as read")}</button>
        <button class="btn" onclick={() => store.markSeen(store.selected, false)}><Icon name="unread" size={16} />{t("Mark as unread")}</button>
        <button class="btn" onclick={() => store.toggleFlag(store.selected)}><Icon name="flag" size={16} />{t("Flag")}</button>
        <button class="btn" onclick={multiGroup}><Icon name="tag" size={16} />{t("Move to group")}</button>
      </div>
      <div class="row wrap">
        <button class="btn" onclick={() => store.archive(store.selected)}><Icon name="archive" size={16} />{t("Archive")}</button>
        <button class="btn" onclick={() => store.ai("summarize", [...store.selected])}><Icon name="sparkle" size={16} />{t("AI: summarize")}</button>
        <button class="btn danger" onclick={() => store.remove(store.selected)}><Icon name="trash" size={16} />{t("Delete")}</button>
      </div>
      <button class="btn subtle" onclick={() => (store.selected = [])}>{t("Clear selection")}</button>
    </div>
  {:else if !msg}
    <div class="center fade-in">
      <svg width="148" height="110" viewBox="0 0 148 110" aria-hidden="true">
        <rect x="18" y="18" width="112" height="78" rx="12" fill="var(--surface-3)" />
        <rect x="32" y="34" width="56" height="8" rx="4" fill="var(--border-strong)" />
        <rect x="32" y="50" width="84" height="6" rx="3" fill="var(--border)" />
        <rect x="32" y="62" width="76" height="6" rx="3" fill="var(--border)" />
        <rect x="32" y="74" width="48" height="6" rx="3" fill="var(--border)" />
        <circle cx="122" cy="20" r="12" fill="var(--accent)" opacity=".3" />
      </svg>
      <b>{t("Select a message to read")}</b>
      <span class="muted small">{t("Nothing selected")}</span>
    </div>
  {:else}
    <div class="xfade" class:leaving>
    {#key msg.id}
      <div class="content" in:enter>
        <div class="head">
          <div class="titlerow">
            {#if onback}
              <button class="icon-btn" onclick={onback} title={t("Back")}><Icon name="chevronLeft" /></button>
            {/if}
            <h1 class="selectable">{@html highlight(msg.subject || t("(No subject)"), terms)}</h1>
            <div class="top-acts">
              <button class="icon-btn" title={msg.flagged ? t("Unflag") : t("Flag")} onclick={() => store.toggleFlag([msg.id])}><Icon name={msg.flagged ? "flagFill" : "flag"} /></button>
              <button class="icon-btn" title={t("Archive")} onclick={() => store.archive([msg.id])}><Icon name="archive" /></button>
              <button class="icon-btn" title={t("Delete")} onclick={() => store.remove([msg.id])}><Icon name="trash" /></button>
              <button class="icon-btn" title={t("More")} onclick={moreMenu}><Icon name="more" /></button>
            </div>
          </div>
          <div class="sender">
            <div class="avatar" style:background={avatarColor(msg.from_addr || msg.from_name)}>{initials(msg.from_name, msg.from_addr)}</div>
            <div class="who">
              <div class="name selectable">
                <b>{@html highlight(msg.from_name || msg.from_addr, terms)}</b>
                {#if msg.from_name}<span class="muted">&lt;{msg.from_addr}&gt;</span>{/if}
              </div>
              <span class="date-sm">{fullTime(msg.date)}</span>
              <button class="to" onclick={() => (showDetails = !showDetails)}>
                <span class="ell">{t("To:")} {@html highlight(recipients(body?.to || msg.to_list) || "—", terms)}{#if body?.cc || msg.cc_list}; Cc: {@html highlight(recipients(body?.cc || msg.cc_list), terms)}{/if}</span>
                <span class="chev" class:open={showDetails}><Icon name="chevronDown" size={12} /></span>
              </button>
            </div>
            <span class="date">{fullTime(msg.date)}</span>
          </div>
          {#if showDetails}
            <div class="details selectable" transition:slide={{ duration: 200 }}>
              <div><span>{t("From")}</span>{@html highlight(`${msg.from_name} <${msg.from_addr}>`, terms)}</div>
              <div><span>{t("To")}</span>{@html highlight(body?.to || msg.to_list, terms)}</div>
              {#if body?.cc || msg.cc_list}<div><span>Cc</span>{@html highlight(body?.cc || msg.cc_list, terms)}</div>{/if}
              {#if body?.reply_to}<div><span>{t("Reply to")}</span>{body.reply_to}</div>{/if}
              <div><span>{t("Date")}</span>{fullTime(msg.date)}</div>
              <div><span>{t("Mailbox")}</span>{store.accountById(msg.account_id)?.email} · {store.folderById(msg.folder_id)?.name}</div>
            </div>
          {/if}

          {#if store.folderById(msg.folder_id)?.role === "drafts"}
            <div class="note info">
              <Icon name="draft" size={18} />
              <span class="grow">{t("This message hasn't been sent.")}</span>
              <button class="btn primary" onclick={() => store.startCompose("draft", msg)}><Icon name="compose" size={16} />{t("Continue editing")}</button>
            </div>
          {/if}

          {#if body && body.has_remote_images && !allowImages}
            <div class="note warn" transition:slide={{ duration: 200 }}>
              <Icon name="image" size={16} />
              <span class="grow">{t("Remote images were blocked to protect your privacy.")}</span>
              <button class="btn subtle" onclick={() => (allowImages = true)}>{t("Load images")}</button>
              <button class="btn subtle" onclick={trustSender}>{t("Always trust this sender")}</button>
            </div>
          {/if}

          {#if inviteInfo || invite}
            {@const ii = inviteInfo}
            <div class="note info inv" transition:slide={{ duration: 200 }}>
              <div class="cal-ico"><Icon name="calendar" size={18} /></div>
              <div class="grow">
                <b>{ii?.title ?? invite?.title}</b>
                <div class="muted small">
                  {#if ii}
                    {new Date(ii.start_ts * 1000).toLocaleString(locale(), { dateStyle: "full", timeStyle: ii.all_day ? undefined : "short" })}
                    {#if ii.location} · {ii.location}{/if}
                  {:else if invite}
                    {new Date(invite.start * 1000).toLocaleString(locale(), { dateStyle: "full", timeStyle: invite.allDay ? undefined : "short" })}
                    {#if invite.location} · {invite.location}{/if}
                  {/if}
                </div>
                {#if ii && (ii.state === "accepted" || ii.state === "tentative" || ii.state === "declined")}
                  <div class="small chip-state {ii.state}" in:fade={{ duration: 160 }}>
                    <Icon name={ii.state === "declined" ? "close" : ii.state === "tentative" ? "help" : "check"} size={13} />
                    {ii.state === "accepted" ? t("You said Yes") : ii.state === "tentative" ? t("You said Maybe") : t("You said No")}
                  </div>
                {:else if ii?.in_calendar}
                  <div class="small chip-state accepted" in:fade={{ duration: 160 }}><Icon name="check" size={13} />{t("In your calendar")}</div>
                {/if}
              </div>
              {#if ii?.method === "CANCEL"}
                <span class="muted small">{t("Cancelled by the organizer")}</span>
              {:else if ii && ii.method === "REQUEST" && ii.can_reply}
                <div class="rsvp">
                  <button class="btn" class:on={ii.state === "accepted"} onclick={() => answer("accepted")} disabled={!!inviteBusy}>{#if inviteBusy === "accepted"}<span class="spinner"></span>{/if}{t("Yes")}</button>
                  <button class="btn" class:on={ii.state === "tentative"} onclick={() => answer("tentative")} disabled={!!inviteBusy}>{#if inviteBusy === "tentative"}<span class="spinner"></span>{/if}{t("Maybe")}</button>
                  <button class="btn" class:on={ii.state === "declined"} onclick={() => answer("declined")} disabled={!!inviteBusy}>{#if inviteBusy === "declined"}<span class="spinner"></span>{/if}{t("No")}</button>
                </div>
              {:else if ii && !ii.in_calendar}
                <button class="btn" onclick={() => answer("add")} disabled={!!inviteBusy}>{#if inviteBusy === "add"}<span class="spinner"></span>{:else}<Icon name="plus" size={16} />{/if}{t("Add to calendar")}</button>
              {:else if ii}
                <button class="btn subtle" onclick={() => ii && store.showEvent(0, ii.start_ts)}><Icon name="calendar" size={15} />{t("Open calendar")}</button>
              {:else}
                <button class="btn" onclick={addInvite}><Icon name="plus" size={16} />{t("Add to calendar")}</button>
              {/if}
            </div>
          {/if}

          {#if files.length}
            <div class="atts">
              {#each files as a, i}
                <div class="att" role="group" oncontextmenu={(e) => attMenu(e, i)}>
                  <button class="att-main" onclick={() => (preview = i)} title={t("Preview")}>
                    <span class="ext" style:background={extColor(a.name)}>{(a.name.split(".").pop() ?? "").slice(0, 4).toUpperCase()}</span>
                    <span class="att-text">
                      <span class="att-name">{@html highlight(a.name, terms)}</span>
                      <span class="muted small">{fileSize(a.size)}</span>
                    </span>
                  </button>
                  <button class="icon-btn sm" title={t("Download")} onclick={() => saveAtt(a.index, a.name)}><Icon name="download" size={16} /></button>
                </div>
              {/each}
              {#if files.length > 1}
                <span class="muted small count">{t("{n} attachments", { n: files.length })}</span>
              {/if}
            </div>
          {/if}
        </div>

        <div class="frame" class:plain={body?.html.startsWith('<div style="white-space:pre-wrap')}>
          {#if error}
            <div class="err" in:fade={{ duration: 160 }}><Icon name="warning" /> {error}</div>
          {:else if !body}
            <div class="loading" out:fade={{ duration: 120 }}>
              <div class="bar"></div><div class="bar" style="width:80%"></div><div class="bar" style="width:60%"></div><div class="bar" style="width:70%"></div>
            </div>
          {:else}
            <iframe
              bind:this={frame}
              title={t("Message body")}
              sandbox="allow-same-origin allow-popups allow-modals"
              {srcdoc}
              class:ready={frameReady}
              onload={onFrameLoad}
            ></iframe>
          {/if}
        </div>

        <div class="foot">
          <button class="btn" title={t("Reply")} onclick={() => store.startCompose("reply", msg)}><Icon name="reply" size={16} /><span class="lbl">{t("Reply")}</span></button>
          <button class="btn" title={t("Reply all")} onclick={() => store.startCompose("replyAll", msg)}><Icon name="replyAll" size={16} /><span class="lbl">{t("Reply all")}</span></button>
          <button class="btn" title={t("Forward")} onclick={() => store.startCompose("forward", msg)}><Icon name="forward" size={16} /><span class="lbl">{t("Forward")}</span></button>
          <span class="divider"></span>
          <button class="btn aibtn" title={t("Reply with AI")} onclick={() => store.ai("reply", [msg.id])}><Icon name="sparkle" size={16} /><span class="lbl">{t("Reply with AI")}</span></button>
          <button class="btn aibtn subtle" title={t("Summarize")} onclick={() => store.ai("summarize", [msg.id])}><Icon name="sparkle" size={16} /><span class="lbl">{t("Summarize")}</span></button>
          <span class="grow"></span>
          <div class="zoom" role="group" aria-label={t("Zoom")}>
            <button class="icon-btn sm" title={t("Zoom out (Ctrl -)")} onclick={() => setZoom(zoom - 0.1)}><Icon name="zoomOut" size={16} /></button>
            <button class="zoomval" title={t("Reset zoom (Ctrl 0)")} onclick={() => setZoom(1)}>{Math.round(zoom * 100)}%</button>
            <button class="icon-btn sm" title={t("Zoom in (Ctrl +)")} onclick={() => setZoom(zoom + 0.1)}><Icon name="zoomIn" size={16} /></button>
          </div>
        </div>
      </div>
    {/key}
    </div>
  {/if}
</div>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}
{#if askInvite && inviteInfo && msg}
  <InviteDialog
    messageId={msg.id}
    info={inviteInfo}
    onclose={() => (askInvite = false)}
    ondone={(st) => inviteInfo && (inviteInfo = { ...inviteInfo, state: st, in_calendar: st === "added" || st === "accepted" || st === "tentative" || inviteInfo.in_calendar })}
  />
{/if}
{#if eventDraft}
  <EventDialog event={eventDraft} onclose={() => (eventDraft = null)} />
{/if}
{#if preview !== null && msg && files.length}
  <AttachmentViewer messageId={msg.id} attachments={files} index={preview} onclose={() => (preview = null)} />
{/if}

<style>
  .pane {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .center {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 20px;
  }
  .row.wrap {
    flex-wrap: wrap;
    justify-content: center;
    gap: 6px;
  }
  .stack {
    width: 72px;
    height: 72px;
    border-radius: 18px;
    background: var(--selected);
    color: var(--text-accent);
    font-size: 1.8rem;
    font-weight: 700;
    display: flex;
    align-items: center;
    justify-content: center;
    margin-bottom: 6px;
    box-shadow: 6px 6px 0 var(--selected-strong);
    animation: scale-in 220ms var(--ease);
  }
  .xfade {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    transition: opacity 110ms ease-out;
  }
  .xfade.leaving {
    opacity: 0;
  }
  .content {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    container-type: inline-size;
  }
  .date-sm {
    display: none;
    font-size: 0.78rem;
    color: var(--text-3);
  }
  @container (max-width: 620px) {
    h1 {
      font-size: 1.12rem;
      margin-bottom: 8px;
    }
    .head {
      padding: 10px 12px 8px 14px;
    }
    .date {
      display: none;
    }
    .date-sm {
      display: block;
    }
    .name .muted {
      display: none;
    }
    .foot .lbl {
      display: none;
    }
    .foot {
      padding: 8px 10px 10px;
      flex-wrap: nowrap;
    }
    .foot .btn {
      padding: 0 9px;
    }
    .frame {
      margin: 4px 8px 0;
    }
    .note {
      font-size: 0.82rem;
    }
    .att {
      max-width: 200px;
    }
  }
  .head {
    flex: none;
    padding: 14px 18px 10px 22px;
    max-height: 48%;
    overflow-y: auto;
    overscroll-behavior: contain;
  }
  .titlerow {
    display: flex;
    align-items: flex-start;
    gap: 8px;
  }
  h1 {
    flex: 1;
    min-width: 0;
    margin: 2px 0 12px;
    font-family: var(--font-display);
    font-size: 1.32rem;
    font-weight: 600;
    line-height: 1.3;
    overflow-wrap: anywhere;
  }
  .top-acts {
    display: flex;
    gap: 2px;
    flex: none;
  }
  .sender {
    display: flex;
    gap: 12px;
    align-items: center;
  }
  .avatar {
    width: 40px;
    height: 40px;
    flex: none;
    border-radius: 50%;
    color: white;
    font-weight: 700;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .who {
    flex: 1;
    min-width: 0;
  }
  .name {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    align-items: baseline;
  }
  .name .muted {
    font-size: 0.88rem;
  }
  .to {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-top: 2px;
    max-width: 100%;
    border: none;
    padding: 0;
    background: none;
    color: var(--text-2);
    font-size: 0.86rem;
    text-align: left;
  }
  .ell {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .date {
    font-size: 0.8rem;
    color: var(--text-3);
    white-space: nowrap;
    align-self: flex-start;
    margin-top: 2px;
  }
  .details {
    margin-top: 10px;
    padding: 10px 12px;
    border-radius: var(--radius);
    background: var(--surface-3);
    font-size: 0.85rem;
    display: grid;
    gap: 3px;
  }
  .details span {
    display: inline-block;
    width: 84px;
    color: var(--text-3);
  }
  .note {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 10px;
    margin-top: 12px;
    padding: 6px 6px 6px 12px;
    border-radius: var(--radius);
    font-size: 0.88rem;
    animation: slide-up 200ms var(--ease);
  }
  .note .grow {
    flex: 1 1 200px;
  }
  .note.warn {
    background: var(--warning-bg);
    color: var(--warning-fg);
  }
  .note.warn .btn {
    color: inherit;
    height: 28px;
  }
  .note.info {
    background: var(--selected);
    color: var(--text);
    padding: 8px 10px;
  }
  .inv {
    align-items: center;
  }
  .rsvp {
    display: flex;
    gap: 6px;
  }
  .rsvp .btn.on {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-fg);
  }
  .chip-state {
    display: inline-flex;
    gap: 4px;
    align-items: center;
    margin-top: 4px;
    padding: 1px 8px;
    border-radius: 999px;
    font-weight: 600;
    background: color-mix(in srgb, var(--success) 14%, transparent);
    color: var(--success);
  }
  .chip-state.tentative {
    background: color-mix(in srgb, #b7791f 16%, transparent);
    color: #b7791f;
  }
  .chip-state.declined {
    background: color-mix(in srgb, var(--danger) 12%, transparent);
    color: var(--danger);
  }
  .cal-ico {
    width: 34px;
    height: 34px;
    border-radius: var(--radius);
    background: var(--accent);
    color: white;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .atts {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: 12px;
  }
  .att {
    display: flex;
    align-items: center;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    background: var(--surface);
    padding-right: 2px;
    max-width: 260px;
    transition:
      box-shadow var(--dur),
      transform var(--dur) var(--ease);
  }
  .att:hover {
    box-shadow: var(--shadow);
    transform: translateY(-1px);
  }
  .att-main {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    border: none;
    background: transparent;
    padding: 5px 4px 5px 5px;
    text-align: left;
  }
  .ext {
    flex: none;
    width: 32px;
    height: 32px;
    border-radius: 6px;
    color: white;
    font-size: 0.62rem;
    font-weight: 800;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .att-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .att-name {
    font-size: 0.85rem;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .icon-btn.sm {
    width: 28px;
    height: 28px;
  }
  .count {
    margin-left: 4px;
  }
  /* The message body lives in a fixed frame that scrolls on its own: no nested page scroll. */
  .frame {
    flex: 1;
    min-height: 120px;
    margin: 4px 14px 0;
    border-radius: var(--radius-l);
    background: #ffffff;
    border: 1px solid var(--border);
    overflow: hidden;
    display: flex;
    position: relative;
  }
  .frame.plain {
    background: var(--surface);
  }
  .chev {
    display: inline-flex;
    transition: transform 200ms var(--ease);
  }
  .chev.open {
    transform: rotate(180deg);
  }
  iframe {
    flex: 1;
    display: block;
    width: 100%;
    height: 100%;
    border: none;
    background: transparent;
    opacity: 0;
    transition: opacity 200ms var(--ease);
  }
  iframe.ready {
    opacity: 1;
  }
  .loading {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 24px 26px;
  }
  .bar {
    height: 10px;
    border-radius: 5px;
    background: linear-gradient(90deg, var(--surface-3) 0%, var(--surface-2) 40%, var(--surface-3) 80%);
    background-size: 200% 100%;
    animation: shimmer 1.2s linear infinite;
  }
  @keyframes shimmer {
    from {
      background-position: 200% 0;
    }
    to {
      background-position: -200% 0;
    }
  }
  .err {
    display: flex;
    gap: 8px;
    align-items: center;
    color: var(--danger);
    user-select: text;
    padding: 20px;
  }
  .foot {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 14px 12px;
    flex-wrap: wrap;
  }
  .divider {
    width: 1px;
    height: 20px;
    background: var(--border-strong);
    margin: 0 2px;
  }
  .aibtn :global(svg) {
    color: var(--ai);
  }
  .zoom {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 2px;
    border-radius: 8px;
    background: var(--surface-3);
  }
  .zoomval {
    min-width: 48px;
    height: 28px;
    border: none;
    border-radius: 6px;
    background: transparent;
    font-variant-numeric: tabular-nums;
    color: var(--text-2);
    font-size: 0.84rem;
  }
  .zoomval:hover {
    background: var(--hover);
  }
  .btn.danger {
    color: var(--danger);
  }
</style>
