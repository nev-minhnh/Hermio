<script lang="ts">
  import StreamText from "./StreamText.svelte";
  import { t, i18n } from "../lib/i18n.svelte";
  import { onMount, tick } from "svelte";
  import { slide, fade, fly, scale } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { cubicOut } from "svelte/easing";
  import Icon from "../lib/Icon.svelte";
  import { api, type AiProvider, type ChatMsg, type Cluster, type AiScope, type AiFile, type ChatSummary, type AiDelta, type Source, type AiResponse } from "../lib/api";
  import { store } from "../lib/store.svelte";
  import { escapeHtml, fileSize } from "../lib/util";
  import { listen, pickFiles } from "../lib/tauri";
  import { fileToBase64, pdfPages } from "../lib/pdf";

  /** Live state of a streamed answer. */
  type Live = { sid?: string; thinking?: string; thinkStart?: number; thinkMs?: number; status?: string; sources?: Source[]; streaming?: boolean; cancelled?: boolean };
  type Entry =
    | ({ kind: "msg"; role: "user" | "assistant"; text: string; used?: number[]; span?: string; files?: string[]; task?: string; hideText?: boolean } & Live)
    | ({ kind: "reply"; text: string; sourceId: number; instruction: string } & Live)
    | ({ kind: "clusters"; clusters: (Cluster & { done?: string })[] } & Live)
    | { kind: "error"; text: string };

  /** A file in the composer; scanned PDFs carry their rendered pages. */
  type ChatFile = AiFile & { pages?: { mime: string; data: string }[]; pageTotal?: number; busy?: boolean };

  let providers = $state<AiProvider[]>([]);
  let providerId = $state(store.settings.ai_default);
  let models = $state<string[]>([]);
  let model = $state("");
  let thinking = $state(store.settings.ai_thinking);
  let web = $state(store.settings.ai_web);
  let scopeKind = $state<string>("auto");
  let days = $state<number | "custom">(store.settings.ai_days || 30);
  let fromDate = $state("");
  let toDate = $state("");
  let entries = $state<Entry[]>([]);
  let input = $state("");
  let busy = $state(false);
  let progress = $state<{ done: number; total: number; stage?: string; bulk?: number } | null>(null);
  let scroller = $state<HTMLDivElement>();
  let inputEl = $state<HTMLTextAreaElement>();
  let files = $state<ChatFile[]>([]);
  let rewriting = $state<Extract<Entry, { kind: "reply" }> | null>(null);
  let chatId = $state(0);
  let chats = $state<ChatSummary[]>([]);
  let showHistory = $state(false);
  let dragging = $state(false);
  let currentSid = $state<string | null>(null);
  let now = $state(Date.now());
  let vlmTags = $state<Set<string> | null>(null);
  let lastIntent = 0;
  let stickBottom = true;

  onMount(() => {
    const uns: (() => void)[] = [];
    (async () => {
      providers = await api.aiProviders();
      if (!providers.some((p) => p.id === providerId)) providerId = providers[0]?.id ?? "";
      loadModels();
      uns.push(await listen<{ done: number; total: number; stage?: string; bulk?: number }>("ai-progress", (p) => (progress = p.done >= p.total ? null : p)));
      uns.push(await listen<AiDelta>("ai-delta", onDelta));
      try {
        const av = await api.npuAvailable();
        vlmTags = new Set(av.models.filter((m) => m.vision).map((m) => m.tag));
      } catch {}
    })();
    const tickH = setInterval(() => (now = Date.now()), 500);
    return () => {
      uns.forEach((u) => u());
      clearInterval(tickH);
    };
  });

  $effect(() => {
    const it = store.aiIntent;
    if (it && it.key !== lastIntent) {
      lastIntent = it.key;
      // Taken once: reopening the panel later must not run it again (and spend tokens).
      store.aiIntent = null;
      queueMicrotask(() => runTask(it.task, it.ids, it.prompt));
    }
  });

  const provider = $derived(providers.find((p) => p.id === providerId));
  const selected = $derived(store.selected);

  /** Can the chosen model look at images? */
  const canSee = $derived.by(() => {
    const p = provider;
    if (!p) return false;
    if (p.kind === "npu") {
      const m = model || p.model;
      if (vlmTags) return vlmTags.has(m);
      return /^(qwen3\.5|gemma4|gemma3:4b|minicpm-v)/.test(m) || m.includes("vl");
    }
    return true;
  });
  const searchesItself = $derived(provider ? ["gemini", "anthropic", "claude_cli"].includes(provider.kind) : false);

  async function loadModels() {
    const p = providers.find((x) => x.id === providerId);
    model = p?.model ?? "";
    models = p?.model ? [p.model] : [];
    if (!p) return;
    try {
      const list = await api.aiModels(p.id);
      if (providerId !== p.id) return;
      models = list.length ? list : models;
      if (model && !models.includes(model)) models = [model, ...models];
    } catch {
      // keep the provider's default model
    }
  }

  function changeProvider() {
    store.set("ai_default", providerId);
    loadModels();
  }

  function viewScope(): AiScope {
    const v = store.view;
    if (v.startsWith("folder:")) return { kind: "folder", ids: [Number(v.slice(7))] };
    if (v.startsWith("group:")) return { kind: "group", ids: [Number(v.slice(6))] };
    if (v.startsWith("account:")) return { kind: "accounts", ids: [Number(v.slice(8))] };
    if (v.startsWith("ids:")) return { kind: "selection", ids: v.slice(4).split(",").map(Number) };
    // "All inboxes", "All sent"…: just those folders, not every folder of every mailbox.
    if (v.startsWith("unified:")) {
      const role = v.slice(8);
      const ids = store.folders.filter((f) => f.role === role).map((f) => f.id);
      if (ids.length) return { kind: "folder", ids };
    }
    return store.settings.space ? { kind: "space", ids: [store.settings.space] } : { kind: "all", ids: [] };
  }

  function scope(ids?: number[]): AiScope {
    if (ids?.length) return { kind: "selection", ids };
    switch (scopeKind) {
      case "auto":
        return selected.length ? { kind: "selection", ids: [...selected] } : viewScope();
      case "view":
        return viewScope();
      case "space":
        return { kind: "space", ids: [store.settings.space] };
      case "all":
        return { kind: "all", ids: [] };
      default:
        return { kind: "accounts", ids: [Number(scopeKind.slice(4))] };
    }
  }

  const scopeLabel = $derived(scopeKind === "auto" ? (selected.length ? t("{n} selected messages", { n: selected.length }) : `“${store.viewTitle()}”`) : "");

  function range(sc: AiScope): { days: number; from: number | null; to: number | null } {
    if (sc.kind === "selection") return { days: 0, from: null, to: null };
    if (days === "custom") {
      const f = fromDate ? Math.floor(new Date(fromDate + "T00:00:00").getTime() / 1000) : null;
      const tt = toDate ? Math.floor(new Date(toDate + "T00:00:00").getTime() / 1000) + 86400 : null;
      return { days: 0, from: f, to: tt };
    }
    return { days, from: null, to: null };
  }

  function onScroll() {
    if (!scroller) return;
    stickBottom = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 60;
  }
  async function scrollDown(force = false) {
    await tick();
    if (scroller && (force || stickBottom)) scroller.scrollTop = scroller.scrollHeight;
  }

  /** The panel slides in from the right. Only transform and opacity change per frame (done by
   *  the GPU): animating its margin made the whole mail area re-layout on every frame, which
   *  stuttered with a long list or a big email open. */
  function panelSlide(_node: HTMLElement) {
    return {
      duration: 220,
      easing: cubicOut,
      css: (k: number) => `transform:translateX(${(1 - k) * 28}px);opacity:${Math.min(1, k * 1.25)}`,
    };
  }

  /** Keep a scrolling box pinned to its end while text streams in (unless the user scrolled up). */
  function followBottom(node: HTMLElement) {
    let pinned = true;
    const onScroll = () => (pinned = node.scrollHeight - node.scrollTop - node.clientHeight < 24);
    const mo = new MutationObserver(() => {
      if (pinned) node.scrollTop = node.scrollHeight;
    });
    mo.observe(node, { childList: true, subtree: true, characterData: true });
    node.addEventListener("scroll", onScroll, { passive: true });
    return {
      destroy() {
        mo.disconnect();
        node.removeEventListener("scroll", onScroll);
      },
    };
  }

  // ----- streaming -----
  function liveEntry(sid: string): (Entry & Live) | undefined {
    return entries.find((e) => e.kind !== "error" && (e as Live).sid === sid) as (Entry & Live) | undefined;
  }
  function onDelta(d: AiDelta) {
    const e = liveEntry(d.id);
    if (!e || e.kind === "error") return;
    if (d.kind === "thinking") {
      e.thinkStart ??= Date.now();
      e.thinking = (e.thinking ?? "") + (d.text ?? "");
      e.status = undefined;
    } else if (d.kind === "text") {
      if (e.thinkStart && !e.thinkMs) e.thinkMs = Date.now() - e.thinkStart;
      e.status = undefined;
      if (e.kind === "msg" || e.kind === "reply") e.text += d.text ?? "";
    } else if (d.kind === "status") {
      e.status = d.text;
      e.thinkStart ??= Date.now();
    } else if (d.kind === "refs" && d.ids) {
      // Which email each [E<n>] stands for, known before the answer streams in.
      if (e.kind === "msg") e.used = d.ids;
    } else if (d.kind === "source" && d.url) {
      e.sources = [...(e.sources ?? []), { title: d.title ?? d.url, url: d.url }];
    }
    scrollDown();
  }
  function finishLive(e: Entry & Live, r: AiResponse | null) {
    e.streaming = false;
    if (e.thinkStart && !e.thinkMs) e.thinkMs = Date.now() - e.thinkStart;
    if (r) {
      if (r.thinking && !e.thinking) e.thinking = r.thinking;
      if (r.sources?.length) e.sources = r.sources;
      e.cancelled = r.cancelled;
      if (r.span && e.kind === "msg") e.span = r.span;
    }
    e.status = undefined;
  }

  function newSid() {
    return `s${Date.now().toString(36)}${Math.random().toString(36).slice(2, 7)}`;
  }

  async function call(task: string, messages: ChatMsg[], sc: AiScope, withFiles: AiFile[] = [], sid: string | null = null, useWeb = false) {
    if (!providerId) throw new Error(t("No AI provider yet. Open Settings → AI assistant to add one."));
    const r = range(sc);
    return api.aiRun({
      provider_id: providerId,
      model: model || null,
      thinking,
      messages,
      scope: sc,
      days: r.days,
      from: r.from,
      to: r.to,
      task,
      files: withFiles,
      lang: i18n.lang,
      stream_id: sid,
      web: useWeb,
    });
  }

  function stop() {
    if (currentSid) api.aiCancel(currentSid);
  }

  async function runTask(task: string, ids?: number[], prompt?: string) {
    if (busy) return;
    const sc = scope(ids);
    const label: Record<string, string> = {
      summarize: sc.kind === "selection" ? (sc.ids.length === 1 ? t("Summarize this message") : t("Summarize {n} messages", { n: sc.ids.length })) : t("Summarize {scope}", { scope: scopeLabel || t("the selected scope") }),
      reply: t("Quick reply"),
      cluster: t("Cluster related mail"),
      report: t("Mailbox report"),
    };
    if (task === "reply") {
      const id = sc.kind === "selection" && sc.ids.length === 1 ? sc.ids[0] : store.current?.id;
      if (!id) {
        entries.push({ kind: "error", text: t("Select a message to reply to.") });
        return;
      }
      return draftReply(id, prompt ?? "");
    }
    entries.push({ kind: "msg", role: "user", text: prompt || label[task] || task, task });
    const sid = newSid();
    entries.push({ kind: "msg", role: "assistant", text: "", sid, streaming: true, hideText: task === "cluster" });
    const live = entries[entries.length - 1] as Entry & Live;
    busy = true;
    currentSid = sid;
    scrollDown(true);
    try {
      const r = await call(task, prompt ? [{ role: "user", content: prompt }] : [], sc, [], sid, web && task !== "cluster");
      finishLive(live, r);
      if (task === "cluster") {
        const i = entries.indexOf(live);
        entries.splice(i, 1, { kind: "clusters", clusters: r.clusters, thinking: live.thinking, thinkMs: live.thinkMs });
      } else if (live.kind === "msg") {
        if (!live.text.trim() || !r.cancelled) live.text = r.text || live.text;
        live.used = r.used;
      }
    } catch (e) {
      finishLive(live, null);
      const i = entries.indexOf(live);
      if (live.kind === "msg" && !live.text.trim() && !live.thinking) entries.splice(i, 1);
      entries.push({ kind: "error", text: String(e) });
    } finally {
      busy = false;
      currentSid = null;
      progress = null;
      scrollDown();
      persist();
    }
  }

  async function draftReply(id: number, instruction: string, previous?: string) {
    busy = true;
    entries.push({ kind: "msg", role: "user", text: instruction ? t("Draft a reply: {text}", { text: instruction }) : t("Quick reply to the selected message") });
    const sid = newSid();
    entries.push({ kind: "reply", text: "", sourceId: id, instruction, sid, streaming: true });
    const live = entries[entries.length - 1] as Extract<Entry, { kind: "reply" }>;
    currentSid = sid;
    scrollDown(true);
    try {
      const msgs: ChatMsg[] = previous
        ? [
            { role: "user", content: t("Write a suitable, polite and concise reply.") },
            { role: "assistant", content: previous },
            { role: "user", content: t("Rewrite the reply: {text}", { text: instruction }) },
          ]
        : [{ role: "user", content: instruction || t("Write a suitable, polite and concise reply.") }];
      const r = await call("reply", msgs, { kind: "selection", ids: [id] }, [], sid);
      finishLive(live, r);
      if (!r.cancelled) live.text = r.text.trim();
    } catch (e) {
      finishLive(live, null);
      entries.splice(entries.indexOf(live), 1);
      entries.push({ kind: "error", text: String(e) });
    } finally {
      busy = false;
      currentSid = null;
      scrollDown();
      persist();
    }
  }

  /** Rewrite asks for the instruction inline, in the chat box (no pop-up). */
  async function startRewrite(e: Extract<Entry, { kind: "reply" }>) {
    rewriting = e;
    input = "";
    await tick();
    inputEl?.focus();
  }

  /** Files as the backend expects them; scanned PDF pages become images. */
  function flatFiles(list: ChatFile[]): AiFile[] {
    const out: AiFile[] = [];
    for (const f of list) {
      if (f.pages?.length) {
        f.pages.forEach((p, i) =>
          out.push({ name: `${f.name} — ${t("page {n}", { n: i + 1 })}`, mime: p.mime, text: null, image: p.data, size: 0 }),
        );
      } else out.push({ name: f.name, mime: f.mime, text: f.text, image: f.image, size: f.size });
    }
    return out;
  }
  const hasImages = (list: ChatFile[]) => list.some((f) => !!f.image || !!f.pages?.length);

  async function send() {
    const q = input.trim();
    if (busy) return;
    // "#3" / "E3" opens email 3 of the latest answer instead of asking the model.
    const jump = q.match(/^(?:#|e)\s*(\d{1,3})$/i);
    if (jump && !files.length && !rewriting) {
      const e = lastCiting();
      if (e) {
        input = "";
        openCited(e, Number(jump[1]));
        return;
      }
    }
    if (rewriting) {
      if (!q) return;
      const r = rewriting;
      rewriting = null;
      input = "";
      return draftReply(r.sourceId, q, r.text);
    }
    if (!q && !files.length) return;
    if (files.some((f) => f.busy)) {
      store.info(t("Still preparing the attached files…"));
      return;
    }
    if (hasImages(files) && !canSee) {
      store.error(t("{model} can't read images. Choose a model that can (Qwen3.5, Gemma 4, Gemini, ChatGPT, Claude) or remove the images.", { model: model || provider?.model || "" }));
      return;
    }
    const sentFiles = files;
    files = [];
    input = "";
    const history: ChatMsg[] = entries
      .filter((e): e is Extract<Entry, { kind: "msg" }> => e.kind === "msg" && !!e.text)
      .slice(-8)
      .map((e) => ({ role: e.role, content: e.text }));
    const content = q || t("Look at the attached files.");
    entries.push({ kind: "msg", role: "user", text: content, files: sentFiles.map((f) => f.name) });
    const sid = newSid();
    entries.push({ kind: "msg", role: "assistant", text: "", sid, streaming: true });
    const live = entries[entries.length - 1] as Extract<Entry, { kind: "msg" }>;
    busy = true;
    currentSid = sid;
    scrollDown(true);
    try {
      const r = await call("chat", [...history, { role: "user", content }], scope(), flatFiles(sentFiles), sid, web);
      finishLive(live, r);
      if (!r.cancelled || !live.text.trim()) live.text = r.text || live.text;
      live.used = r.used;
    } catch (e) {
      finishLive(live, null);
      if (!live.text.trim() && !live.thinking) entries.splice(entries.indexOf(live), 1);
      entries.push({ kind: "error", text: String(e) });
    } finally {
      busy = false;
      currentSid = null;
      progress = null;
      scrollDown();
      persist();
    }
  }

  // ----- attachments: picker, drag & drop, paste -----
  async function addFile(f: AiFile) {
    if (!f.text && !f.image && !f.pdf) {
      store.error(t("{name} has no readable text", { name: f.name }));
      return;
    }
    if (f.image && !canSee) store.info(t("The current model can't read images — choose a model that can before sending."));
    if (f.pdf) {
      if (!canSee) {
        store.error(t("{name} is a scanned PDF. Reading it needs a model that can read images (Qwen3.5, Gemma 4, Gemini, ChatGPT, Claude).", { name: f.name }));
        return;
      }
      const entry: ChatFile = { ...f, pdf: null, busy: true };
      files = [...files, entry];
      try {
        const { pages, total } = await pdfPages(f.pdf, 8);
        const i = files.findIndex((x) => x === entry || (x.name === f.name && x.busy));
        // Update in place so the chip keeps its identity (no re-entrance animation).
        if (i >= 0) Object.assign(files[i], { busy: false, pages, pageTotal: total });
        if (total > pages.length) store.info(t("Only the first {n} pages of {name} are read.", { n: pages.length, name: f.name }));
      } catch (e) {
        files = files.filter((x) => x !== entry && !(x.name === f.name && x.busy));
        store.error(String(e));
      }
      return;
    }
    files = [...files, f];
  }

  async function attach() {
    const paths = await pickFiles();
    for (const p of paths) {
      try {
        await addFile(await api.aiReadFile(p));
      } catch (e) {
        store.error(String(e));
      }
    }
  }

  async function addBrowserFiles(list: File[]) {
    for (const f of list) {
      if (f.size > 40 * 1024 * 1024) {
        store.error(t("{name} is larger than 40 MB", { name: f.name }));
        continue;
      }
      try {
        const name = f.name || (f.type.startsWith("image/") ? `pasted-${Date.now()}.${f.type.split("/")[1] || "png"}` : "file");
        await addFile(await api.aiReadBytes(name, await fileToBase64(f)));
      } catch (e) {
        store.error(String(e));
      }
    }
  }

  function onDragOver(e: DragEvent) {
    if (!e.dataTransfer?.types.includes("Files")) return;
    e.preventDefault();
    dragging = true;
  }
  function onDrop(e: DragEvent) {
    dragging = false;
    const list = [...(e.dataTransfer?.files ?? [])];
    if (!list.length) return;
    e.preventDefault();
    addBrowserFiles(list);
  }
  function onPaste(e: ClipboardEvent) {
    const list = [...(e.clipboardData?.files ?? [])];
    if (!list.length) return;
    e.preventDefault();
    addBrowserFiles(list);
  }

  function toggleWeb() {
    web = !web;
    store.set("ai_web", web);
  }

  function statusText(s?: string): string {
    if (!s) return "";
    if (s === "loading") return t("Loading the model on the NPU…");
    if (s === "image") return t("Looking at the images…");
    if (s === "web") return t("Searching the web…");
    if (s.startsWith("web-error:")) return t("Web search failed: {e}", { e: s.slice(10) });
    if (s.startsWith("web:")) return t("Searching the web: {q}", { q: s.slice(4) });
    return s;
  }
  const secs = (ms: number) => Math.max(1, Math.round(ms / 1000));
  const host = (u: string) => {
    try {
      return new URL(u).hostname.replace(/^www\./, "");
    } catch {
      return u;
    }
  };

  // ----- saved conversations -----
  const titleOf = () => {
    const first = entries.find((e) => e.kind === "msg" && e.role === "user") as Extract<Entry, { kind: "msg" }> | undefined;
    return (first?.text ?? t("New chat")).slice(0, 80);
  };
  async function persist() {
    if (!entries.length) return;
    try {
      chatId = await api.aiSaveChat({ id: chatId, title: titleOf(), entries: $state.snapshot(entries) });
      if (showHistory) loadChats();
    } catch {}
  }
  async function loadChats() {
    chats = await api.aiChats();
  }
  function newChat() {
    entries = [];
    chatId = 0;
    files = [];
    rewriting = null;
    showHistory = false;
    inputEl?.focus();
  }
  async function openChat(id: number) {
    const c = await api.aiChat(id);
    entries = (Array.isArray(c.entries) ? c.entries : []) as Entry[];
    chatId = c.id;
    showHistory = false;
    scrollDown();
  }
  async function deleteChat(id: number) {
    await api.aiDeleteChat(id);
    if (id === chatId) newChat();
    loadChats();
  }
  function toggleHistory() {
    showHistory = !showHistory;
    if (showHistory) loadChats();
  }

  async function useReply(e: Extract<Entry, { kind: "reply" }>) {
    let msg = store.messages.find((m) => m.id === e.sourceId);
    if (!msg) {
      const rows = await api.messages({ view: `ids:${e.sourceId}`, search: "", search_mode: "content", filter: "", sort_asc: false, limit: 1, offset: 0 });
      msg = rows[0];
    }
    if (msg) store.startCompose("reply", msg, undefined, e.text);
  }

  function openIds(ids: number[], title: string) {
    store.idsTitle = title;
    store.open(`ids:${ids.join(",")}`);
  }

  // ----- citations: [E1], [E2]… → the emails the answer is based on -----
  type MsgEntry = Extract<Entry, { kind: "msg" }>;
  /** Citation numbers used in an answer, in order of first appearance. */
  function citedNums(e: MsgEntry): number[] {
    const out: number[] = [];
    for (const m of e.text.matchAll(/\[E(\d+)\]/g)) {
      const n = Number(m[1]);
      if (n >= 1 && n <= (e.used?.length ?? 0) && !out.includes(n)) out.push(n);
    }
    return out;
  }
  /** Open the cited emails as a list (each row shows its number) with email `n` selected. */
  function openCited(e: MsgEntry, n: number) {
    const used = e.used ?? [];
    const id = used[n - 1];
    if (!id) return store.info(t("This answer has no email {n}", { n }));
    const nums = citedNums(e);
    const list = (nums.length ? nums : [n]).map((k) => used[k - 1]).filter(Boolean);
    if (!list.includes(id)) list.push(id);
    const view = `ids:${list.join(",")}`;
    const cite = new Map<number, number>();
    used.forEach((x, i) => cite.set(x, i + 1));
    if (store.view !== view) {
      store.idsTitle = t("Emails cited by the assistant");
      store.open(view);
    }
    store.cite = cite;
    store.compose = null;
    store.selected = [id];
    // The list loads asynchronously: select again once it's there.
    setTimeout(() => (store.selected = [id]), 250);
  }
  /** The latest answer that cites emails (for Alt+1…9 and "#3" in the input box). */
  function lastCiting(): MsgEntry | null {
    for (let i = entries.length - 1; i >= 0; i--) {
      const e = entries[i];
      if (e.kind === "msg" && e.role === "assistant" && e.used?.length) return e;
    }
    return null;
  }
  function onGlobalKey(ev: KeyboardEvent) {
    if (!ev.altKey || ev.ctrlKey || ev.metaKey || !/^[1-9]$/.test(ev.key)) return;
    const e = lastCiting();
    if (!e) return;
    ev.preventDefault();
    openCited(e, Number(ev.key));
  }

  // Sender + subject of cited emails, for the list under each answer.
  let citeInfo = $state<Record<number, { from: string; subject: string }>>({});
  let citeLoading = new Set<number>();
  $effect(() => {
    const want: number[] = [];
    for (const e of entries) {
      if (e.kind !== "msg" || e.streaming || !e.used?.length) continue;
      for (const n of citedNums(e)) {
        const id = e.used[n - 1];
        if (id && !(id in citeInfo) && !citeLoading.has(id)) want.push(id);
      }
    }
    if (!want.length) return;
    want.forEach((id) => citeLoading.add(id));
    api
      .messages({ view: `ids:${want.join(",")}`, search: "", search_mode: "content", filter: "", sort_asc: false, limit: want.length, offset: 0 })
      .then((rows) => {
        const next = { ...citeInfo };
        for (const m of rows) next[m.id] = { from: m.from_name || m.from_addr, subject: m.subject };
        for (const id of want) next[id] ??= { from: "", subject: t("(deleted)") };
        citeInfo = next;
      })
      .catch(() => {});
  });

  /** Group for a cluster: local to a mailbox when all its mail comes from one account. */
  async function groupFor(c: Cluster) {
    const accs = new Set(c.ids.map((id) => store.messages.find((m) => m.id === id)?.account_id).filter((x) => x !== undefined));
    const accountId = accs.size === 1 ? [...accs][0]! : null;
    const existing = store.groups.find((g) => g.name.toLowerCase() === c.title.toLowerCase().slice(0, 60));
    if (existing) return existing.id;
    return api.saveGroup({ name: c.title.slice(0, 60), color: "#8764b8", hide_from_inbox: true, account_id: accountId });
  }

  async function clusterToGroup(c: Cluster & { done?: string }, withRule: boolean) {
    try {
      const gid = await groupFor(c);
      await api.addToGroup(c.ids, gid);
      if (withRule && c.rule?.conditions?.length) {
        const rid = await api.saveRule({
          name: c.title.slice(0, 60),
          enabled: true,
          match_all: c.rule.match_all,
          conditions: c.rule.conditions,
          account_ids: [],
          group_id: gid,
          mark_read: false,
          flag: false,
          stop: false,
        });
        const n = await api.runRules(rid);
        c.done = t("Group and rule created · {n} more messages sorted", { n });
      } else {
        c.done = t("Created group “{name}” with {n} messages", { name: c.title, n: c.ids.length });
      }
      store.success(c.done);
      await store.reloadAll();
      persist();
    } catch (e) {
      store.error(String(e));
    }
  }

  const condText = (x: { field: string; op: string; value: string }) => {
    const vals = x.value.split(/[,;]/).map((v) => v.trim()).filter(Boolean);
    const list = vals.length > 3 ? `${vals.slice(0, 3).join(", ")} +${vals.length - 3}` : vals.join(", ");
    if (x.field === "from" && x.op === "domain") return t("sender's domain is {v}", { v: list });
    if (x.field === "from") return t("sender is {v}", { v: list });
    if (x.field === "subject") return t("subject contains {v}", { v: list.split(", ").map((w) => `“${w}”`).join(", ") });
    return `${x.field} ${x.op} “${list}”`;
  };
  const ruleText = (c: Cluster) => c.rule?.conditions?.map(condText).join(c.rule.match_all ? t(" and ") : t(" or ")) ?? "";

  // Starter prompts disappear once used.
  const starters = $derived(
    [
      t("Who is waiting for my reply this week?"),
      t("Any deadlines coming up?"),
      t("Sum up this month's invoices"),
      t("What did I miss today?"),
    ].filter((s) => !store.settings.ai_hidden_hints.includes(s)),
  );
  function useStarter(s: string) {
    store.set("ai_hidden_hints", [...store.settings.ai_hidden_hints, s]);
    input = s;
    send();
  }

  function md(src: string): string {
    const lines = escapeHtml(src).split("\n");
    let html = "";
    let inList = false;
    for (const raw of lines) {
      const l = raw
        .replace(/\*\*(.+?)\*\*/g, "<b>$1</b>")
        .replace(/(^|[^*])\*([^*\s][^*]*)\*/g, "$1<i>$2</i>")
        .replace(/`([^`]+)`/g, "<code>$1</code>")
        .replace(/\[E(\d+)\]/g, '<a class="ref cite-n" data-n="$1" title="Alt+$1">$1</a>')
        .replace(/\[#(\d+)\]/g, '<a class="ref" data-id="$1">#$1</a>')
        .replace(/\[(\d{1,2})\]/g, '<sup class="cite" data-cite="$1">$1</sup>')
        .replace(/(https?:\/\/[^\s<)\]]+)/g, '<a class="ext" data-url="$1">$1</a>');
      const li = l.match(/^\s*[-*•]\s+(.*)$/) || l.match(/^\s*\d+[.)]\s+(.*)$/);
      if (li) {
        if (!inList) {
          html += "<ul>";
          inList = true;
        }
        html += `<li>${li[1]}</li>`;
        continue;
      }
      if (inList) {
        html += "</ul>";
        inList = false;
      }
      const h = l.match(/^(#{1,4})\s+(.*)$/);
      if (h) html += `<h4>${h[2]}</h4>`;
      else if (l.trim()) html += `<p>${l}</p>`;
    }
    if (inList) html += "</ul>";
    return html;
  }

  function onBodyClick(ev: MouseEvent) {
    const el = ev.target as HTMLElement;
    const cite = el.closest("sup.cite") as HTMLElement | null;
    if (cite) {
      const i = Number(el.closest("[data-entry]")?.getAttribute("data-entry"));
      const e = entries[i] as Entry & Live;
      const src = e?.sources?.[Number(cite.dataset.cite) - 1];
      if (src) api.openUrl(src.url);
      return;
    }
    const ext = el.closest("a.ext") as HTMLElement | null;
    if (ext?.dataset.url) {
      api.openUrl(ext.dataset.url);
      return;
    }
    const a = el.closest("a.ref") as HTMLElement | null;
    if (!a) return;
    if (a.dataset.n) {
      const i = Number(el.closest("[data-entry]")?.getAttribute("data-entry"));
      const e = entries[i];
      if (e?.kind === "msg") openCited(e, Number(a.dataset.n));
      return;
    }
    const id = Number(a.dataset.id);
    if (store.messages.some((m) => m.id === id)) {
      store.selected = [id];
      store.compose = null;
    } else {
      openIds([id], t("Message #{id}", { id }));
      setTimeout(() => (store.selected = [id]), 300);
    }
  }

  function copy(txt: string) {
    navigator.clipboard?.writeText(txt);
    store.info(t("Copied"));
  }
  const ago = (ts: number) => new Date(ts * 1000).toLocaleString(i18n.lang === "vi" ? "vi-VN" : "en-GB", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
</script>

{#snippet thinkBlock(e: Entry & Live)}
  {@const live = !!e.streaming && !e.thinkMs && !(e.kind === "msg" && e.text) && !(e.kind === "reply" && e.text)}
  <details class="think-block" class:live open={live || undefined}>
    <summary>
      {#if live}<span class="pulse"></span>{:else}<Icon name="brain" size={14} />{/if}
      <span class="lbl">
        {#if live}
          {e.status ? statusText(e.status) : e.thinking ? t("Thinking…") : t("Working…")} <span class="muted">{secs(now - (e.thinkStart ?? now))}s</span>
        {:else if e.thinkMs}
          {t("Thought for {n}s", { n: secs(e.thinkMs) })}
        {:else}
          {t("Thinking")}
        {/if}
      </span>
      {#if e.thinking}<Icon name="chevronDown" size={13} />{/if}
    </summary>
    {#if e.thinking}
      <div class="think-text selectable" use:followBottom><StreamText text={e.thinking} streaming={live} /></div>
    {/if}
  </details>
{/snippet}

<svelte:window onkeydown={onGlobalKey} />

<aside class="ai" ondragover={onDragOver} ondragleave={(ev) => { if (ev.currentTarget === ev.target) dragging = false; }} ondrop={onDrop} transition:panelSlide>
  <div class="head">
    <div class="title"><Icon name="sparkle" size={18} />{t("AI assistant")}</div>
    <span class="grow"></span>
    {#if store.npuBg}
      <span class="bgchip" transition:fade={{ duration: 150 }} title={t("The NPU sorts new mail that arrived while it slept. It stops by itself when you ask something.")}>
        <Icon name="sync" size={12} class="spin" />{t("NPU sorting {n} emails", { n: store.npuBg.count })}
        <button onclick={() => api.aiCancel("bg")} title={t("Stop")}>✕</button>
      </span>
    {/if}
    <button class="icon-btn" class:on={showHistory} title={t("Saved conversations")} onclick={toggleHistory}><Icon name="history" size={17} /></button>
    <button class="icon-btn" title={t("New chat")} onclick={newChat}><Icon name="newChat" size={17} /></button>
    <button class="icon-btn" title={t("Close")} onclick={() => (store.aiOpen = false)}><Icon name="close" size={16} /></button>
  </div>

  <div class="models">
    <select class="input" bind:value={providerId} onchange={changeProvider} title={t("Provider")}>
      {#each providers as p}<option value={p.id}>{p.name}</option>{/each}
      {#if !providers.length}<option value="">{t("No model")}</option>{/if}
    </select>
    <select class="input" bind:value={model} title={t("Model")} disabled={!models.length}>
      {#each models as m}<option value={m}>{m}</option>{/each}
    </select>

  </div>

  <div class="scope">
    <select class="input" bind:value={scopeKind} title={t("Mailboxes the AI may read")}>
      <option value="auto">{t("Automatic:")} {selected.length ? t("{n} selected messages", { n: selected.length }) : t("current folder")}</option>
      <option value="view">{t("Current folder / group")}</option>
      {#if store.settings.space}<option value="space">{t("Current space")}</option>{/if}
      <option value="all">{t("All allowed mailboxes")}</option>
      {#each store.accounts as a}<option value={`acc:${a.id}`}>{t("Account:")} {a.email}</option>{/each}
    </select>
    <select class="input days" bind:value={days} onchange={() => days !== "custom" && store.set("ai_days", days)} title={t("Time range")}>
      <option value={1}>{t("24 hours")}</option>
      <option value={7}>{t("7 days")}</option>
      <option value={30}>{t("30 days")}</option>
      <option value={90}>{t("90 days")}</option>
      <option value={365}>{t("1 year")}</option>
      <option value={0}>{t("All")}</option>
      <option value="custom">{t("Date range…")}</option>
    </select>
  </div>
  {#if days === "custom"}
    <div class="range" transition:slide={{ duration: 160 }}>
      <Icon name="calendar" size={15} />
      <input class="input" type="date" bind:value={fromDate} max={toDate || undefined} title={t("From")} />
      <span class="muted">→</span>
      <input class="input" type="date" bind:value={toDate} min={fromDate || undefined} title={t("To")} />
    </div>
  {/if}

  <div class="quick">
    <button class="qa" onclick={() => runTask("summarize")} disabled={busy}><Icon name="draft" size={15} />{t("Summarize")}</button>
    <button class="qa" onclick={() => runTask("reply")} disabled={busy || !(store.current || selected.length === 1)}><Icon name="reply" size={15} />{t("Quick reply")}</button>
    <button class="qa" onclick={() => runTask("cluster")} disabled={busy}><Icon name="layout" size={15} />{t("Cluster")}</button>
    <button class="qa" onclick={() => runTask("report")} disabled={busy}><Icon name="filter" size={15} />{t("Report")}</button>
  </div>

  <div class="stage">
    {#if showHistory}
      <div class="history" transition:fade={{ duration: 120 }}>
        <div class="hist-head"><b>{t("Saved conversations")}</b><button class="btn subtle" onclick={newChat}><Icon name="newChat" size={15} />{t("New chat")}</button></div>
        {#each chats as c (c.id)}
          <div class="hist-item" class:cur={c.id === chatId}>
            <button class="hist-open" onclick={() => openChat(c.id)}>
              <span class="ell">{c.title}</span>
              <span class="muted small">{ago(c.updated)}</span>
            </button>
            <button class="icon-btn" title={t("Delete")} onclick={() => deleteChat(c.id)}><Icon name="trash" size={15} /></button>
          </div>
        {:else}
          <p class="muted small" style="padding:0 6px">{t("No saved conversations yet.")}</p>
        {/each}
      </div>
    {/if}

    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="body scroll" bind:this={scroller} onclick={onBodyClick} onscroll={onScroll}>
      {#if !providers.length}
        <div class="empty">
          <div class="spark big"><Icon name="sparkle" size={36} /></div>
          <b>{t("No AI model connected")}</b>
          <p class="muted small">
            {#if store.npu.info && !store.npu.info.present}
              {t("Add ChatGPT, Gemini, Grok, Claude (API or Claude Code) or a local model.")}
            {:else}
              {t("Download an on-device model for the NPU, or add ChatGPT, Gemini, Grok, Claude (API or Claude Code) or a local model.")}
            {/if}
          </p>
          <button class="btn primary" onclick={() => { store.settingsTab = "ai"; store.route = "settings"; }}>{t("Open AI assistant settings")}</button>
        </div>
      {:else if !entries.length}
        <div class="empty">
          <div class="spark big"><Icon name="sparkle" size={36} /></div>
          <b>{t("Ask anything about your mail")}</b>
          <p class="muted small">{t("The assistant reads the full text and attachments of the mail in scope. It only reads mailboxes you allow, and never sends mail by itself.")}</p>
          {#if starters.length}
            <div class="starters">
              {#each starters as s (s)}
                <button class="starter" out:fade={{ duration: 150 }} onclick={() => useStarter(s)}>{s}</button>
              {/each}
            </div>
          {/if}
        </div>
      {/if}
      {#each entries as e, i (i)}
        {#if e.kind !== "error" && (e.thinking || e.status || (e.streaming && !(e.kind === "msg" && e.text) && !(e.kind === "reply" && e.text)))}
          {@render thinkBlock(e)}
        {/if}
        {#if e.kind === "msg"}
          {#if e.role === "user" || e.text || !e.streaming || e.hideText}
            {#if !(e.hideText && e.role === "assistant")}
              <div class="bubble {e.role}" data-entry={i} in:fly={{ y: 6, duration: 160 }}>
                {#if e.role === "assistant"}
                  <div class="md selectable"><StreamText text={e.text} streaming={!!e.streaming} render={md} onstep={() => scrollDown()} /></div>
                  {#if e.sources?.length}
                    <div class="sources">
                      {#each e.sources as src, si (src.url)}
                        <button class="src" title={src.url} onclick={() => api.openUrl(src.url)}><span class="n">{si + 1}</span><span class="ell">{src.title || host(src.url)}</span><span class="muted">{host(src.url)}</span></button>
                      {/each}
                    </div>
                  {/if}
                  {#if !e.streaming && e.used?.length}
                    {@const nums = citedNums(e)}
                    {#if nums.length}
                      <div class="cited" in:fade={{ duration: 180 }}>
                        {#each nums as n (n)}
                          {@const ci = citeInfo[e.used[n - 1]]}
                          <button class="cited-row" onclick={() => openCited(e, n)} title={t("Open email {n} (Alt+{n})", { n })}>
                            <span class="cite-badge">{n}</span>
                            <span class="ell"><b>{ci?.from ?? "…"}</b> <span class="muted">{ci?.subject ?? ""}</span></span>
                          </button>
                        {/each}
                        <span class="muted cite-hint">{t("Alt+1…9 or type #2 in the box below to open an email")}</span>
                      </div>
                    {/if}
                  {/if}
                  {#if !e.streaming}
                    <div class="meta">
                      {#if e.cancelled}<span class="muted small">{t("Stopped")}</span>{/if}
                      {#if e.used?.length}
                        <button class="link" onclick={() => openIds(e.used!, t("Messages the AI read"))} title={e.span ? t("Received {span}", { span: e.span }) : ""}>{t("Based on")} {e.used.length} {t("messages")}</button>
                        {#if e.span}<span class="muted small">· {e.span}</span>{/if}
                      {/if}
                      <button class="link" onclick={() => copy(e.text)}>{t("Copy")}</button>
                    </div>
                  {/if}
                {:else}
                  <div class="selectable">{e.text}</div>
                  {#if e.files?.length}<div class="sent-files">{#each e.files as f}<span><Icon name="attach" size={12} />{f}</span>{/each}</div>{/if}
                {/if}
              </div>
            {/if}
          {/if}
        {:else if e.kind === "reply"}
          <div class="draft card" class:active={rewriting === e}>
            <div class="draft-head"><Icon name="reply" size={15} /><b>{t("Reply draft")}</b><span class="muted small">{e.streaming ? t("writing…") : t("— review before sending")}</span></div>
            {#if e.streaming}
              <div class="input draft-live" use:followBottom><StreamText text={e.text} streaming onstep={() => scrollDown()} /></div>
            {:else}
              <textarea class="input" rows="8" bind:value={e.text} in:fade={{ duration: 120 }}></textarea>
            {/if}
            <div class="row">
              <button class="btn primary" disabled={e.streaming || !e.text.trim()} onclick={() => useReply(e)}><Icon name="compose" size={15} />{t("Open to edit and send")}</button>
              <button class="btn" disabled={busy} onclick={() => startRewrite(e)}><Icon name="sync" size={15} />{t("Rewrite")}</button>
            </div>
          </div>
        {:else if e.kind === "clusters"}
          <div class="clusters">
            {#each e.clusters as c, ci (ci)}
              {#if c.done}
                <div class="cluster done" transition:slide={{ duration: 180 }}><Icon name="check" size={15} />{c.done}</div>
              {:else}
                <div class="cluster card" transition:slide={{ duration: 180 }}>
                  <div class="row"><b class="grow">{c.title}</b><span class="pill">{c.ids.length} {t("messages")}</span></div>
                  {#if c.summary}<div class="muted small">{c.summary}</div>{/if}
                  {#if c.rule?.conditions?.length}
                    <div class="rule small">
                      <Icon name="rule" size={14} />
                      <span>
                        {t("Suggested rule:")} {ruleText(c)}
                        {#if c.rule_hits !== undefined && c.rule_hits > 0}
                          <span class="muted">· {t("catches {h} of these {n}", { h: c.rule_hits, n: c.ids.length })}{#if c.rule_extra}{", "}{t("+{x} other emails", { x: c.rule_extra })}{/if}</span>
                        {/if}
                      </span>
                    </div>
                  {:else}
                    <div class="muted small">{t("No reliable rule: these emails have little in common besides their topic. You can still group them.")}</div>
                  {/if}
                  <div class="row wrap">
                    <button class="btn subtle" onclick={() => openIds(c.ids, c.title)}><Icon name="mail" size={15} />{t("View")}</button>
                    {#if c.rule?.conditions?.length}
                      <button class="btn primary" onclick={() => clusterToGroup(c, true)}><Icon name="rule" size={15} />{t("Group + rule")}</button>
                    {/if}
                    <button class="btn subtle" onclick={() => clusterToGroup(c, false)}><Icon name="tag" size={15} />{t("Create group")}</button>
                    <button class="btn subtle" onclick={() => runTask("summarize", c.ids)}><Icon name="draft" size={15} />{t("Summarize")}</button>
                  </div>
                </div>
              {/if}
            {/each}
          </div>
        {:else}
          <div class="error selectable" in:fly={{ y: 6, duration: 180 }}><Icon name="warning" size={15} />{e.text}</div>
        {/if}
      {/each}
      {#if busy && progress}
        <div class="bubble assistant typing" in:fade={{ duration: 120 }}>
          <i></i><i></i><i></i>
          <span class="muted small prog">
            {progress.stage === "condense" ? t("Combining notes {done}/{total}…", progress) : t("Reading {done}/{total} emails…", progress)}{#if progress.bulk}{" "}{t("(+{n} bulk emails from their subjects)", { n: progress.bulk })}{/if}
          </span>
          <span class="pbar"><span style:width="{Math.round((progress.done / Math.max(1, progress.total)) * 100)}%"></span></span>
        </div>
      {/if}
    </div>
  </div>

  <div class="composer-wrap">
    {#if rewriting}
      <div class="rewrite" transition:slide={{ duration: 150 }}>
        <Icon name="sync" size={14} />
        <span class="grow">{t("How should the draft change? (e.g. shorter, accept Thursday's meeting, write in Vietnamese)")}</span>
        <button class="icon-btn sm" title={t("Cancel")} onclick={() => (rewriting = null)}><Icon name="close" size={14} /></button>
      </div>
    {/if}
    {#if files.length}
      <div class="files" transition:slide={{ duration: 150 }}>
        {#each files as f (f)}
          <span class="file" title={f.name} animate:flip={{ duration: 180 }} in:scale={{ start: 0.85, duration: 160 }} out:scale={{ start: 0.85, duration: 120 }}>
            {#if f.busy}<Icon name="sync" size={13} class="spin" />{:else}<Icon name={f.image || f.pages ? "image" : "attach"} size={13} />{/if}
            <span class="ell">{f.name}</span>
            <span class="muted">{f.pages ? t("{n} pages · OCR", { n: f.pages.length }) : f.busy ? t("reading…") : fileSize(f.size)}</span>
            <button onclick={() => (files = files.filter((x) => x !== f))} aria-label={t("Remove")}><Icon name="close" size={11} /></button>
          </span>
        {/each}
      </div>
    {/if}
    <div class="composer">
      <button class="icon-btn sm attach" title={t("Attach files for reference")} onclick={attach} disabled={!providers.length || !!rewriting}><Icon name="attach" size={16} /></button>
      <textarea
        class="ask"
        rows="1"
        bind:this={inputEl}
        placeholder={rewriting ? t("Type how to rewrite, then Enter") : providers.length ? t("Ask about your mail… (Enter to send)") : t("Add an AI model in Settings")}
        bind:value={input}
        disabled={!providers.length}
        onpaste={onPaste}
        onkeydown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            send();
          } else if (e.key === "Escape" && rewriting) rewriting = null;
        }}
      ></textarea>
      {#if busy && currentSid}
        <button class="send stop" onclick={stop} title={t("Stop")}><span class="sq"></span></button>
      {:else}
        <button class="send" onclick={send} disabled={busy || (!input.trim() && !files.length)} title={t("Send")}><Icon name="send" size={16} /></button>
      {/if}
    </div>
    <div class="tools">
      <button class="tool" class:on={web} onclick={toggleWeb} title={searchesItself ? t("The model searches the web itself and cites its sources") : t("Hermio searches the web (only your question is sent) and gives the results to the model")}>
        <Icon name="globe" size={14} />{t("Web")}
      </button>
      <button class="tool" class:on={thinking} onclick={() => (thinking = !thinking)} title={t("Thinking: slower, more careful answers")}>
        <Icon name="brain" size={14} />{t("Think")}
      </button>
      {#if !canSee}<span class="muted small novis" title={t("This model can't read images")}><Icon name="eyeOff" size={13} />{t("No images")}</span>{/if}
    </div>
  </div>
  {#if dragging}
    <div class="dropzone" transition:fade={{ duration: 120 }}>
      <Icon name="upload" size={28} />
      <b>{t("Drop files to attach")}</b>
      <span class="muted small">{t("PDF, Word, Excel, PowerPoint, text, images")}</span>
    </div>
  {/if}
</aside>

<style>
  .ai {
    width: 380px;
    flex: none;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border-left: 1px solid var(--border);
    min-height: 0;
  }
  @keyframes slide-in {
    from { transform: translateX(16px); opacity: 0; }
    to { transform: none; opacity: 1; }
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 10px 6px 16px;
  }
  .title {
    font-family: var(--font-display);
    font-weight: 600;
    font-size: 1.05rem;
    white-space: nowrap;
  }
  .spark {
    color: var(--ai);
  }
  .spark.big {
    font-size: 2rem;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .title :global(svg) {
    color: var(--ai);
  }
  .head .icon-btn.on {
    background: var(--selected);
    color: var(--text-accent);
  }
  .models {
    display: flex;
    gap: 6px;
    padding: 2px 12px 6px 16px;
  }
  .models select {
    height: 30px;
    font-size: 0.84rem;
    min-width: 0;
    flex: 1;
  }
  .think {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 30px;
    padding: 0 10px;
    border-radius: 8px;
    border: 1px solid var(--border-strong);
    background: transparent;
    font-size: 0.8rem;
    font-weight: 600;
    color: var(--text-2);
  }
  .think.on {
    border-color: color-mix(in srgb, var(--ai) 50%, transparent);
    background: color-mix(in srgb, var(--ai) 14%, transparent);
    color: var(--ai);
  }
  .range {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 12px 8px 16px;
    color: var(--text-2);
  }
  .range input {
    height: 30px;
    flex: 1;
    min-width: 0;
    font-size: 0.84rem;
  }
  .stage {
    flex: 1;
    min-height: 0;
    position: relative;
    display: flex;
  }
  .history {
    position: absolute;
    inset: 0;
    z-index: 3;
    background: var(--surface);
    padding: 10px 12px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .hist-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: 0 0 6px 6px;
  }
  .hist-item {
    display: flex;
    align-items: center;
    border-radius: 8px;
  }
  .hist-item:hover,
  .hist-item.cur {
    background: var(--hover);
  }
  .hist-item.cur {
    background: var(--selected);
  }
  .hist-open {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    border: none;
    background: transparent;
    padding: 7px 8px;
    text-align: left;
  }
  .ell {
    max-width: 100%;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .starters {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 10px;
    width: 100%;
  }
  .starter {
    border: 1px solid var(--border-strong);
    background: var(--surface);
    border-radius: 10px;
    padding: 8px 12px;
    text-align: left;
    font-size: 0.86rem;
  }
  .starter:hover {
    border-color: color-mix(in srgb, var(--ai) 45%, transparent);
    background: color-mix(in srgb, var(--ai) 6%, transparent);
  }
  .sent-files {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 6px;
    font-size: 0.78rem;
    opacity: 0.9;
  }
  .sent-files span {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .draft.active {
    border-color: color-mix(in srgb, var(--accent) 60%, transparent);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 14%, transparent);
  }
  .cluster.done {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-radius: 10px;
    background: color-mix(in srgb, var(--success) 12%, transparent);
    color: var(--success);
    font-size: 0.86rem;
    font-weight: 600;
  }
  .rule {
    display: flex;
    gap: 6px;
    align-items: flex-start;
    color: var(--text-2);
    padding: 6px 8px;
    border-radius: 6px;
    background: var(--surface-3);
  }
  .row.wrap {
    flex-wrap: wrap;
    gap: 6px;
  }
  .prog {
    margin-left: 8px;
    white-space: nowrap;
  }
  .composer-wrap {
    margin: 6px 12px 12px 14px;
  }
  .rewrite {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px 6px 10px;
    margin-bottom: 6px;
    border-radius: 10px;
    background: var(--selected);
    color: var(--text-accent);
    font-size: 0.82rem;
    font-weight: 600;
  }
  .files {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 6px;
  }
  .file {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    max-width: 100%;
    padding: 3px 4px 3px 8px;
    border-radius: 8px;
    background: var(--surface-3);
    font-size: 0.8rem;
  }
  .file .ell {
    max-width: 180px;
  }
  .file button {
    border: none;
    background: transparent;
    width: 20px;
    height: 20px;
    border-radius: 4px;
    display: grid;
    place-items: center;
  }
  .file button:hover {
    background: var(--hover);
  }
  .icon-btn.sm {
    width: 28px;
    height: 28px;
  }
  .attach {
    margin-bottom: 2px;
  }
  .scope {
    display: flex;
    gap: 6px;
    padding: 4px 12px 8px 16px;
  }
  .scope select {
    height: 30px;
    font-size: 0.84rem;
    flex: 1;
    min-width: 0;
  }
  .scope .days {
    flex: none;
    width: 96px;
  }
  .quick {
    display: flex;
    gap: 6px;
    padding: 0 12px 10px 16px;
    border-bottom: 1px solid var(--border);
    flex-wrap: wrap;
  }
  .qa {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 28px;
    padding: 0 10px;
    border-radius: 14px;
    border: 1px solid color-mix(in srgb, var(--ai) 35%, var(--border));
    background: color-mix(in srgb, var(--ai) 7%, transparent);
    font-size: 0.82rem;
    font-weight: 600;
    transition: background var(--dur);
  }
  .qa:hover {
    background: color-mix(in srgb, var(--ai) 16%, transparent);
  }
  .qa:disabled {
    opacity: 0.45;
  }
  .body {
    flex: 1;
    min-width: 0;
    min-height: 0;
    padding: 14px 14px 8px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .empty {
    margin: auto 0;
    text-align: center;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 0 10px;
  }
  .empty p {
    margin: 0;
    line-height: 1.5;
  }
  .bubble {
    max-width: 100%;
    padding: 9px 12px;
    border-radius: 12px;
    font-size: 0.92rem;
    line-height: 1.5;
    animation: slide-up 180ms var(--ease);
  }
  .bubble.user {
    align-self: flex-end;
    background: var(--accent);
    color: white;
    border-bottom-right-radius: 4px;
    max-width: 85%;
  }
  .bubble.assistant {
    background: var(--surface-3);
    border-bottom-left-radius: 4px;
  }
  .md :global(p) {
    margin: 0 0 6px;
  }
  .md :global(ul) {
    margin: 2px 0 8px;
    padding-left: 18px;
  }
  .md :global(h4) {
    margin: 8px 0 4px;
    font-size: 0.95rem;
  }
  .md :global(code) {
    font-family: Consolas, monospace;
    font-size: 0.85em;
    background: var(--hover);
    padding: 0 3px;
    border-radius: 3px;
  }
  .md :global(a.ref.cite-n) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 17px;
    height: 17px;
    padding: 0 4px;
    margin: 0 1px;
    border-radius: 9px;
    font-size: 0.72rem;
    vertical-align: 0.12em;
    color: white;
    background: var(--accent);
    transition: transform 120ms var(--ease), filter 120ms;
  }
  .md :global(a.ref.cite-n:hover) {
    transform: scale(1.12);
    filter: brightness(1.1);
  }
  .cited {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 8px 0 2px;
    padding-top: 8px;
    border-top: 1px solid var(--border);
  }
  .cited-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    padding: 4px 6px;
    border: none;
    border-radius: 6px;
    background: transparent;
    text-align: left;
    font-size: 0.82rem;
  }
  .cited-row:hover {
    background: var(--hover);
  }
  .typing .pbar {
    flex: 1;
    min-width: 60px;
    height: 4px;
    border-radius: 2px;
    background: var(--surface-3);
    overflow: hidden;
  }
  .typing .pbar span {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width 300ms var(--ease);
  }
  .bgchip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 4px 2px 8px;
    border-radius: 12px;
    font-size: 0.74rem;
    color: var(--text-2);
    background: var(--surface-3, color-mix(in srgb, var(--text) 8%, transparent));
    white-space: nowrap;
  }
  .bgchip button {
    border: none;
    background: none;
    padding: 0 4px;
    color: inherit;
    cursor: pointer;
    border-radius: 8px;
  }
  .bgchip button:hover {
    background: var(--hover);
  }
  .cite-hint {
    font-size: 0.74rem;
    padding: 2px 6px 0;
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
  }
  .cited-row .ell {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .md :global(a.ref) {
    color: var(--text-accent);
    font-weight: 600;
    cursor: pointer;
    text-decoration: none;
    background: var(--selected);
    padding: 0 4px;
    border-radius: 4px;
  }
  .meta {
    display: flex;
    gap: 12px;
    margin-top: 4px;
  }
  .link {
    border: none;
    background: none;
    padding: 0;
    color: var(--text-accent);
    font-size: 0.78rem;
    font-weight: 600;
  }
  .typing {
    display: flex;
    align-items: center;
    gap: 4px;
    width: fit-content;
  }
  .typing i {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--text-3);
    animation: blink 1s infinite;
  }
  .typing i:nth-child(2) { animation-delay: 0.15s; }
  .typing i:nth-child(3) { animation-delay: 0.3s; }
  @keyframes blink {
    0%, 100% { opacity: 0.25; }
    50% { opacity: 1; }
  }
  .draft,
  .cluster {
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .draft-head {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .draft textarea,
  .draft-live {
    font-size: 0.9rem;
    line-height: 1.5;
  }
  .draft-live {
    min-height: calc(8 * 1.5em + 16px);
    max-height: 320px;
    overflow-y: auto;
    padding: 8px 10px;
    color: var(--text);
  }
  .clusters {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .pill {
    font-size: 0.74rem;
    padding: 1px 8px;
    border-radius: 10px;
    background: var(--selected);
    color: var(--text-accent);
    font-weight: 700;
  }
  .error {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    padding: 9px 12px;
    border-radius: 10px;
    background: color-mix(in srgb, var(--danger) 10%, transparent);
    color: var(--danger);
    font-size: 0.88rem;
    white-space: pre-wrap;
  }
  .composer {
    display: flex;
    gap: 6px;
    align-items: flex-end;
    padding: 6px 6px 6px 6px;
    border-radius: 12px;
    border: 1px solid var(--border-strong);
    background: var(--input-bg);
  }
  .composer:focus-within {
    border-color: color-mix(in srgb, var(--accent) 60%, transparent);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 14%, transparent);
  }
  .ask {
    flex: 1;
    border: none;
    outline: none;
    resize: none;
    background: transparent;
    max-height: 140px;
    field-sizing: content;
    padding: 5px 0;
    user-select: text;
  }
  .send {
    width: 32px;
    height: 32px;
    border-radius: 8px;
    border: none;
    background: var(--accent);
    color: white;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .send:disabled {
    opacity: 0.4;
  }
  .ai {
    position: relative;
  }
  .think-block {
    border-radius: 10px;
    background: color-mix(in srgb, var(--ai) 6%, var(--surface-2));
    border: 1px solid color-mix(in srgb, var(--ai) 18%, var(--border));
    font-size: 0.84rem;
    animation: slide-up 160ms var(--ease);
  }
  .think-block summary {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    cursor: pointer;
    list-style: none;
    color: var(--text-2);
    user-select: none;
  }
  .think-block summary::-webkit-details-marker {
    display: none;
  }
  .think-block summary .lbl {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .think-block[open] summary :global(svg:last-child) {
    transform: rotate(180deg);
  }
  .think-block summary :global(svg) {
    color: var(--ai);
    transition: transform var(--dur) var(--ease);
  }
  .think-text {
    padding: 2px 12px 10px;
    max-height: 220px;
    overflow-y: auto;
    white-space: pre-wrap;
    color: var(--text-3);
    font-size: 0.8rem;
    line-height: 1.5;
    border-top: 1px dashed color-mix(in srgb, var(--ai) 20%, transparent);
    padding-top: 8px;
  }
  .think-block.live .think-text {
    mask-image: linear-gradient(to bottom, transparent 0, #000 24px);
  }
  .pulse {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--ai);
    animation: pulse 1.1s ease-in-out infinite;
    flex: none;
  }
  @keyframes pulse {
    0%,
    100% {
      transform: scale(0.75);
      opacity: 0.5;
    }
    50% {
      transform: scale(1.1);
      opacity: 1;
    }
  }
  .caret {
    display: inline-block;
    width: 7px;
    height: 1em;
    margin-left: 2px;
    vertical-align: text-bottom;
    background: var(--text-2);
    border-radius: 1px;
    animation: blink 1s steps(2) infinite;
  }
  .md :global(sup.cite) {
    display: inline-block;
    min-width: 15px;
    padding: 0 4px;
    margin: 0 1px;
    border-radius: 7px;
    background: var(--selected);
    color: var(--text-accent);
    font-size: 0.7rem;
    font-weight: 700;
    line-height: 15px;
    text-align: center;
    cursor: pointer;
    vertical-align: super;
  }
  .md :global(a.ext) {
    color: var(--text-accent);
    cursor: pointer;
    word-break: break-all;
  }
  .sources {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    margin-top: 8px;
  }
  .src {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    max-width: 100%;
    padding: 3px 8px 3px 4px;
    border-radius: 8px;
    border: 1px solid var(--border-strong);
    background: var(--surface);
    font-size: 0.76rem;
    text-align: left;
  }
  .src:hover {
    border-color: color-mix(in srgb, var(--accent) 50%, transparent);
  }
  .src .n {
    flex: none;
    width: 16px;
    height: 16px;
    border-radius: 5px;
    background: var(--selected);
    color: var(--text-accent);
    font-weight: 700;
    display: grid;
    place-items: center;
    font-size: 0.68rem;
  }
  .src .ell {
    max-width: 150px;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 6px;
  }
  .tool {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    padding: 0 10px;
    border-radius: 13px;
    border: 1px solid var(--border-strong);
    background: transparent;
    font-size: 0.78rem;
    font-weight: 600;
    color: var(--text-2);
  }
  .tool.on {
    border-color: color-mix(in srgb, var(--accent) 55%, transparent);
    background: var(--selected);
    color: var(--text-accent);
  }
  .novis {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-left: auto;
  }
  .send.stop {
    background: var(--text);
  }
  .send .sq {
    width: 11px;
    height: 11px;
    border-radius: 2px;
    background: var(--surface);
  }
  .dropzone {
    position: absolute;
    inset: 8px;
    z-index: 5;
    border: 2px dashed color-mix(in srgb, var(--accent) 60%, transparent);
    border-radius: 14px;
    background: color-mix(in srgb, var(--surface) 88%, transparent);
    backdrop-filter: blur(2px);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    color: var(--text-accent);
    pointer-events: none;
  }
</style>
