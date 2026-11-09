<script lang="ts">
  /**
   * The writing assistant in the composer, working the way Grammarly does (ported from
   * Themisio):
   *  1. Collect — every paragraph of the email you write (not the quoted message or the
   *     signature); only paragraphs that changed are sent again.
   *  2. Analyse — Correctness, Clarity, Engagement and Delivery, against your writing goals.
   *  3. Suggest — issues are underlined (red, blue, green, purple). Hover an underline for a card:
   *     accept the fix, dismiss it, or add the word to your dictionary. The panel lists them all.
   *  4. Write with AI — rephrase, shorten, expand, ideas, or anything you ask.
   *  5. Report — an overall score, counts, tone and your progress.
   * Suggestions are applied as ordinary edits, so Ctrl+Z undoes them.
   */
  import { onDestroy, onMount } from "svelte";
  import { fade, fly, scale } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { t } from "../lib/i18n.svelte";
  import Icon from "../lib/Icon.svelte";
  import { store } from "../lib/store.svelte";
  import {
    AUDIENCE,
    CATS,
    CAT_COLOR,
    CAT_LABEL,
    DEFAULT_GOALS,
    DOMAINS,
    EMPTY_STATS,
    FORMALITY,
    INTENT,
    VARIANTS,
    checkSystem,
    hash,
    parseBatch,
    preview,
    run,
    score as scoreOf,
    weekKey,
    writeSystem,
    type Cat,
    type Issue,
  } from "../lib/writing";
  import ModelPicker from "./ModelPicker.svelte";
  import { portal } from "../lib/motion";

  let { el }: { el: HTMLDivElement | undefined } = $props();

  // ---------- 1. collect ----------
  interface Unit {
    nodes: Node[];
    text: string;
    hash: string;
  }
  let units = $state.raw<Unit[]>([]);
  let checked = $state.raw<Record<string, Issue[]>>({});
  let pending = new Set<string>();
  let dismissed = $state.raw<Set<string>>(new Set());
  let tone = $state<[string, boolean] | null>(null);
  let busy = $state(false);
  let error = $state("");
  let stopCheck: (() => void) | null = null;

  let panel = $state(false);
  let tab = $state<"suggestions" | "goals" | "write" | "report">("suggestions");
  let filter = $state<Cat | null>(null);
  let expanded = $state<string | null>(null);

  const goals = $derived({ ...DEFAULT_GOALS, ...(store.settings.writing_goals ?? {}) });
  const dict = $derived(store.settings.writing_dictionary ?? []);
  const stats = $derived(store.settings.writing_stats ?? EMPTY_STATS);

  const BLOCK = new Set(["DIV", "P", "LI", "UL", "OL", "H1", "H2", "H3", "H4", "H5", "H6", "PRE", "TABLE", "TBODY", "THEAD", "TR", "TD", "TH", "SECTION", "ARTICLE", "HEADER", "FOOTER"]);
  const SKIP = ".hermio-quote, .hermio-signature, blockquote";
  const isBlock = (n: Node) => n.nodeType === 1 && BLOCK.has((n as Element).tagName);
  const skip = (n: Node) => n.nodeType === 1 && ((n as Element).matches(SKIP) || (n as Element).tagName === "IMG" || (n as Element).tagName === "HR");

  /** Paragraphs: leaf blocks, and runs of inline content between blocks. */
  function collectUnits(): Unit[] {
    if (!el) return [];
    const out: Unit[] = [];
    const g = goals;
    const push = (nodes: Node[]) => {
      if (!nodes.length) return;
      const text = nodes.map((n) => n.textContent ?? "").join("").replace(/ /g, " ");
      if (!/\p{L}.*\p{L}/su.test(text)) return;
      out.push({ nodes, text, hash: hash(text, g) });
    };
    const walk = (container: Node) => {
      let run: Node[] = [];
      for (const c of Array.from(container.childNodes)) {
        if (skip(c)) {
          push(run);
          run = [];
          continue;
        }
        if (isBlock(c)) {
          push(run);
          run = [];
          if (Array.from(c.childNodes).some((x) => isBlock(x) || skip(x))) walk(c);
          else push([c]);
        } else if (c.nodeType === 1 && (c as Element).tagName === "BR") {
          // A line break ends a paragraph only when it's a blank line.
          run.push(c);
        } else run.push(c);
      }
      push(run);
    };
    walk(el);
    return out;
  }

  /** A DOM range over characters [start, end) of a unit's text. */
  function rangeFor(u: Unit, start: number, end: number): Range | null {
    const r = document.createRange();
    let pos = 0;
    let started = false;
    for (const n of u.nodes) {
      const w = document.createTreeWalker(n, NodeFilter.SHOW_TEXT);
      let tn: Node | null = n.nodeType === 3 ? n : w.nextNode();
      while (tn) {
        const len = tn.textContent?.length ?? 0;
        if (!started && start <= pos + len) {
          r.setStart(tn, start - pos);
          started = true;
        }
        if (started && end <= pos + len) {
          r.setEnd(tn, end - pos);
          return r;
        }
        pos += len;
        tn = n.nodeType === 3 ? null : w.nextNode();
      }
    }
    return null;
  }

  // ---------- 2. analyse ----------
  let again = false;
  async function check(all = true) {
    if (busy) {
      // Typing during a check: check the new text right after.
      again = true;
      return;
    }
    units = collectUnits();
    error = "";
    do {
      let size = 0;
      const todo: Unit[] = [];
      for (const u of units) {
        if (checked[u.hash] || pending.has(u.hash) || todo.some((x) => x.hash === u.hash)) continue;
        if (todo.length >= 24 || size + u.text.length > 7000) break;
        todo.push(u);
        size += u.text.length;
      }
      if (!todo.length) break;
      todo.forEach((u) => pending.add(u.hash));
      busy = true;
      try {
        const [p, stop] = run(checkSystem(goals, dict), todo.map((u, i) => `[${i + 1}] ${u.text}`).join("\n\n"));
        stopCheck = stop;
        const { issues, tone: tn } = parseBatch(await p, todo.map((u) => u.text));
        const next = { ...checked };
        todo.forEach((u, i) => (next[u.hash] = issues[i]));
        checked = next;
        if (tn) tone = tn;
      } catch (e) {
        error = String(e).replace(/^Error: /, "");
        all = false;
      } finally {
        todo.forEach((u) => pending.delete(u.hash));
        busy = false;
        stopCheck = null;
      }
      units = collectUnits();
    } while (all);
    measure();
    if (again) {
      again = false;
      if (store.settings.writing_auto) setTimeout(() => check(true), 300);
    }
  }
  function stop() {
    stopCheck?.();
  }

  // As you type (off by default: every check is a request to the provider).
  let typeTimer: ReturnType<typeof setTimeout> | undefined;
  let collectTimer: ReturnType<typeof setTimeout> | undefined;
  function onInput() {
    clearTimeout(collectTimer);
    collectTimer = setTimeout(() => {
      units = collectUnits();
      measure();
    }, 120);
    if (store.settings.writing_auto) {
      clearTimeout(typeTimer);
      typeTimer = setTimeout(() => check(true), Math.max(600, store.settings.writing_delay_ms || 1500));
    }
  }

  // ---------- 3. suggest ----------
  interface Shown {
    unit: Unit;
    issue: Issue;
    id: string;
  }
  const shown = $derived.by<Shown[]>(() => {
    const d = new Set(dict.map((w) => w.toLowerCase()));
    const out: Shown[] = [];
    for (const u of units) {
      for (const issue of checked[u.hash] ?? []) {
        const id = `${u.hash}|${issue.original}`;
        if (dismissed.has(id) || d.has(issue.original.trim().toLowerCase()) || !u.text.includes(issue.original)) continue;
        out.push({ unit: u, issue, id });
      }
    }
    return out;
  });
  const listed = $derived(shown.filter((s) => !filter || s.issue.cat === filter));
  const words = (s: string) => s.split(/\s+/).filter(Boolean).length;
  const checkedWords = $derived(units.filter((u) => checked[u.hash]).reduce((a, u) => a + words(u.text), 0));
  const score = $derived(scoreOf(shown.map((s) => s.issue), checkedWords));
  const worst = $derived(CATS.find((c) => shown.some((s) => s.issue.cat === c)) ?? null);
  const allChecked = $derived(units.length > 0 && units.every((u) => checked[u.hash]));

  // Underlines: drawn over the editor from the text's own rectangles (the editor isn't touched).
  interface Mark {
    s: Shown;
    rects: DOMRect[];
  }
  let marks = $state.raw<Mark[]>([]);
  let clip = $state({ top: 0, bottom: 0, left: 0, right: 0 });
  let raf = 0;
  function measure() {
    cancelAnimationFrame(raf);
    raf = requestAnimationFrame(() => {
      if (!el) return;
      const box = (el.closest(".scroll") ?? el).getBoundingClientRect();
      clip = { top: box.top, bottom: box.bottom, left: box.left, right: box.right };
      const out: Mark[] = [];
      for (const s of shown.slice(0, 200)) {
        const i = s.unit.text.indexOf(s.issue.original);
        const r = i >= 0 ? rangeFor(s.unit, i, i + s.issue.original.length) : null;
        if (!r) continue;
        const rects = Array.from(r.getClientRects()).filter((x) => x.width > 1 && x.bottom > box.top && x.top < box.bottom);
        if (rects.length) out.push({ s, rects });
      }
      marks = out;
    });
  }
  $effect(() => {
    void shown;
    measure();
  });

  // The card for the underline under the pointer.
  let hover = $state<{ s: Shown; rect: DOMRect } | null>(null);
  let cardEl = $state<HTMLDivElement>();
  let hideTimer: ReturnType<typeof setTimeout> | undefined;
  function onMove(e: PointerEvent) {
    if (cardEl?.contains(e.target as Node)) {
      clearTimeout(hideTimer);
      return;
    }
    for (const m of marks) {
      const r = m.rects.find((r) => e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top - 1 && e.clientY <= r.bottom + 3);
      if (r) {
        clearTimeout(hideTimer);
        if (hover?.s.id !== m.s.id) hover = { s: m.s, rect: r };
        return;
      }
    }
    if (hover && !hideTimer) hideTimer = setTimeout(() => ((hover = null), (hideTimer = undefined)), 350);
  }

  function accept(s: Shown) {
    if (!el) return;
    const i = s.unit.text.indexOf(s.issue.original);
    const r = i >= 0 ? rangeFor(s.unit, i, i + s.issue.original.length) : null;
    if (!r) return store.error(t("Couldn't apply the suggestion: the text has changed."));
    el.focus();
    const sel = window.getSelection()!;
    sel.removeAllRanges();
    sel.addRange(r);
    if (s.issue.replacement) document.execCommand("insertText", false, s.issue.replacement);
    else document.execCommand("delete");
    // The paragraph's other suggestions carry over to its new text (no new check needed).
    const after = s.unit.text.slice(0, i) + s.issue.replacement + s.unit.text.slice(i + s.issue.original.length);
    const carried = (checked[s.unit.hash] ?? []).filter((x) => x.original !== s.issue.original && after.includes(x.original));
    checked = { ...checked, [hash(after, goals)]: carried };
    const st = structuredClone($state.snapshot(stats)) as typeof stats;
    st.accepted[CATS.indexOf(s.issue.cat)]++;
    st.weeks[weekKey()] = (st.weeks[weekKey()] ?? 0) + 1;
    store.set("writing_stats", st);
    hover = null;
    expanded = null;
    units = collectUnits();
  }
  function dismiss(s: Shown) {
    dismissed = new Set([...dismissed, s.id]);
    const st = structuredClone($state.snapshot(stats)) as typeof stats;
    st.dismissed++;
    store.set("writing_stats", st);
    hover = null;
  }
  function learn(s: Shown) {
    const w = s.issue.original.trim();
    if (!w || dict.some((d) => d.toLowerCase() === w.toLowerCase())) return;
    store.set("writing_dictionary", [...dict, w]);
    store.success(t("“{word}” added to your dictionary", { word: w }));
    hover = null;
  }
  function focusIssue(s: Shown) {
    expanded = s.id;
    const i = s.unit.text.indexOf(s.issue.original);
    const r = i >= 0 ? rangeFor(s.unit, i, i + s.issue.original.length) : null;
    const node = r?.startContainer.parentElement;
    node?.scrollIntoView({ block: "center", behavior: "smooth" });
  }

  // ---------- selection, Ask AI / Translate / Meaning ----------
  let lastRange: Range | null = null;
  function onSelection() {
    const sel = window.getSelection();
    if (el && sel && sel.rangeCount && el.contains(sel.anchorNode)) lastRange = sel.getRangeAt(0).cloneRange();
  }
  /** The selection, or the paragraph at the cursor. */
  function currentTarget(): { text: string; context: string; selection: boolean; range: Range | null } {
    const r = lastRange;
    const para = (n: Node | null) => units.find((u) => u.nodes.some((x) => x === n || x.contains(n)));
    if (r && !r.collapsed && r.toString().trim()) {
      const u = para(r.startContainer);
      return { text: r.toString(), context: u?.text ?? r.toString(), selection: true, range: r.cloneRange() };
    }
    const u = r ? para(r.startContainer) : null;
    if (u) {
      const range = rangeFor(u, 0, u.text.length);
      return { text: u.text, context: u.text, selection: false, range };
    }
    return { text: "", context: "", selection: false, range: r };
  }
  function replaceRange(range: Range | null, text: string) {
    if (!el) return;
    el.focus();
    const sel = window.getSelection()!;
    sel.removeAllRanges();
    if (range) sel.addRange(range);
    document.execCommand("insertText", false, text);
  }
  function insertAfter(range: Range | null, text: string) {
    if (!el) return;
    el.focus();
    const sel = window.getSelection()!;
    sel.removeAllRanges();
    if (range) {
      const r = range.cloneRange();
      r.collapse(false);
      sel.addRange(r);
    }
    document.execCommand("insertText", false, (range && !range.collapsed ? " " : "") + text);
  }

  /** Open the Hermio AI popup at (x, y) on the selection / paragraph. */
  export function assist(mode: "ask" | "translate" | "define", x?: number, y?: number) {
    units = collectUnits();
    const tg = currentTarget();
    if (x === undefined || y === undefined) {
      const rr = tg.range?.getBoundingClientRect();
      x = rr && rr.width + rr.height > 0 ? rr.left : (el?.getBoundingClientRect().left ?? 200) + 40;
      y = rr && rr.width + rr.height > 0 ? rr.bottom : (el?.getBoundingClientRect().top ?? 200) + 40;
    }
    store.openAssist(mode, x, y, {
      text: tg.text,
      context: tg.context,
      selection: tg.selection,
      replace: (s) => replaceRange(tg.range, s),
      insert: (s) => insertAfter(tg.range, s),
    });
  }
  /** Check writing: open the panel and check everything. */
  export function checkWriting() {
    panel = true;
    tab = "suggestions";
    check(true);
  }

  function onKey(e: KeyboardEvent) {
    if (!(e.ctrlKey && e.altKey)) return;
    const k = e.key.toLowerCase();
    const map: Record<string, () => void> = { g: checkWriting, k: () => assist("ask"), t: () => assist("translate"), d: () => assist("define") };
    if (map[k]) {
      e.preventDefault();
      map[k]();
    }
  }
  function onDbl(e: MouseEvent) {
    if (!store.settings.define_on_dblclick) return;
    // The browser has selected the word by now.
    setTimeout(() => {
      onSelection();
      const s = window.getSelection()?.toString().trim() ?? "";
      if (!s || s.split(/\s+/).length > 3 || s.length > 48) return;
      assist("define", e.clientX, e.clientY);
    }, 0);
  }

  // ---------- 4. write with AI ----------
  let prompt = $state("");
  let goBusy = $state(false);
  let goResult = $state<string | null>(null);
  let goError = $state("");
  let goStop: (() => void) | null = null;
  let goTarget: ReturnType<typeof currentTarget> | null = null;
  const actions: [string, string][] = [
    ["More formal", "Rewrite it in a more formal, professional tone."],
    ["Friendlier", "Rewrite it in a warmer, friendlier tone."],
    ["Concise", "Make it more concise without losing meaning."],
    ["Expand", "Expand it with more detail and supporting points."],
    ["Simplify", "Rewrite it in simpler words and shorter sentences."],
    ["Improve it", "Improve its clarity, flow and word choice."],
    ["Ideas", "Brainstorm 8 ideas, angles or subject lines related to it, as a list."],
    ["Summarise", "Summarise it in a few sentences."],
  ];
  async function generate(instruction: string) {
    units = collectUnits();
    goTarget = currentTarget();
    const text = goTarget.text;
    goBusy = true;
    goError = "";
    goResult = null;
    try {
      const [p, s] = run(writeSystem(goals), text.trim() ? `${instruction}\n\nText:\n<<<\n${text}\n>>>` : instruction);
      goStop = s;
      goResult = (await p).trim();
    } catch (e) {
      goError = String(e).replace(/^Error: /, "");
    } finally {
      goBusy = false;
      goStop = null;
    }
  }

  // ---------- wiring ----------
  // While writing (new mail, reply, draft) the assistant keeps checking on its own: once the
  // composer opens (a reply or a draft may already have text) and after each pause in typing.
  let firstCheck: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const e = el;
    if (!e) return;
    if (store.settings.writing_auto) {
      clearTimeout(firstCheck);
      firstCheck = setTimeout(() => units.length && check(true), 1200);
    }
    e.addEventListener("input", onInput);
    e.addEventListener("keydown", onKey);
    e.addEventListener("dblclick", onDbl);
    units = collectUnits();
    // Always on while writing: what's already there (an AI draft, a reply you started) is checked
    // as soon as the composer opens; after that, each paragraph you change after a pause in typing.
    if (store.settings.writing_auto) {
      clearTimeout(typeTimer);
      typeTimer = setTimeout(() => check(true), 1200);
    }
    return () => {
      e.removeEventListener("input", onInput);
      e.removeEventListener("keydown", onKey);
      e.removeEventListener("dblclick", onDbl);
    };
  });
  onMount(() => {
    document.addEventListener("selectionchange", onSelection);
    window.addEventListener("scroll", measure, true);
    window.addEventListener("resize", measure);
  });
  onDestroy(() => {
    document.removeEventListener("selectionchange", onSelection);
    window.removeEventListener("scroll", measure, true);
    window.removeEventListener("resize", measure);
    cancelAnimationFrame(raf);
    clearTimeout(typeTimer);
    clearTimeout(collectTimer);
    clearTimeout(firstCheck);
    stopCheck?.();
    goStop?.();
  });

  function setGoal(k: keyof typeof goals, v: string) {
    store.set("writing_goals", { ...goals, [k]: v });
    // New goals: check again against them.
    units = collectUnits();
  }
  const cardPos = $derived(hover ? { x: Math.min(hover.rect.left, window.innerWidth - 360), y: hover.rect.bottom + 6 } : null);
  const topWords = $derived.by(() => {
    const stop = new Set("the a an and or of to in on for is are was were be it this that with as at by i we you he she they not but from have has will can và là của các những một cho với trong được có không này đã để khi thì về người như từ đến cũng".split(" "));
    const f = new Map<string, number>();
    for (const w of units.map((u) => u.text).join(" ").toLowerCase().split(/[^\p{L}\p{N}']+/u)) if (w.length > 2 && !stop.has(w) && !/^\d+$/.test(w)) f.set(w, (f.get(w) ?? 0) + 1);
    return [...f].filter(([, n]) => n > 1).sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).slice(0, 10);
  });
  const allText = $derived(units.map((u) => u.text).join("\n"));
  const nWords = $derived(words(allText));
  const nSentences = $derived(Math.max(1, allText.split(/[.!?\n]/).filter((x) => x.trim()).length));
</script>

<svelte:window onpointermove={onMove} />

<!-- Underlines -->
<div class="marks" use:portal style:clip-path="inset({clip.top}px calc(100vw - {clip.right}px) calc(100vh - {clip.bottom}px) {clip.left}px)">
  {#each marks as m (m.s.id)}
    {#each m.rects as r}
      <span class="mark" class:on={hover?.s.id === m.s.id} style:left="{r.left}px" style:top="{r.top}px" style:width="{r.width}px" style:height="{r.height}px" style:--c={CAT_COLOR[m.s.issue.cat]}></span>
    {/each}
  {/each}
</div>

<!-- The card for the hovered underline -->
{#if hover && cardPos}
  <div class="card wa-card" use:portal bind:this={cardEl} style:left="{cardPos.x}px" style:top="{cardPos.y}px" transition:fly={{ y: 4, duration: 150 }}>
    {@render issueBody(hover.s, true)}
    {#if !panel}<button class="btn sm ghost wide" onclick={() => ((panel = true), (hover = null))}><Icon name="sparkle" size={14} />{t("Open the writing panel")}</button>{/if}
  </div>
{/if}

{#snippet issueBody(s: Shown, card: boolean)}
  <div class="ib">
    <div class="cat small" style:--c={CAT_COLOR[s.issue.cat]}><i></i>{t(CAT_LABEL[s.issue.cat])}{#if s.issue.kind}<span class="muted"> · {s.issue.kind}</span>{/if}</div>
    {#if s.issue.title}<b class="ttl">{s.issue.title}</b>{/if}
    <div class="fix">
      <s class="orig">{s.issue.original}</s>
      <button class="repl" onclick={() => accept(s)} title={t("Accept")}>{s.issue.replacement || t("(remove)")}</button>
    </div>
    {#if s.issue.explanation}<div class="why small">{s.issue.explanation}</div>{/if}
    <div class="ib-acts">
      <button class="btn sm primary" onclick={() => accept(s)}><Icon name="check" size={14} />{t("Accept")}</button>
      <button class="btn sm" onclick={() => dismiss(s)}><Icon name="close" size={13} />{t("Dismiss")}</button>
      <button class="btn sm" onclick={() => learn(s)}><Icon name="plus" size={13} />{t("Add to dictionary")}</button>
    </div>
  </div>
{/snippet}

<!-- Grammarly's round button: the number of suggestions, ringed by the most serious kind -->
{#if !panel}
  <button class="fab" style:--ring={worst ? CAT_COLOR[worst] : "#e8b931"} class:busy onclick={() => (panel = true)} title={t("Writing assistant")} transition:scale={{ start: 0.6, duration: 200 }}>
    {#if shown.length}<span>{Math.min(99, shown.length)}</span>{:else}<span class="spark">✦</span>{/if}
  </button>
{/if}

{#if panel}
  <aside class="wa-panel" transition:fly={{ x: 40, duration: 240, easing: cubicOut }}>
    <header>
      <span class="spark">✦</span><b>{t("Writing assistant")}</b>
      <span class="grow"></span>
      <button class="icon-btn" title={t("Close")} onclick={() => (panel = false)}><Icon name="close" size={15} /></button>
    </header>
    <div class="pbody">
      <div class="top">
        <div class="ring" style:--p={score ?? 0} style:--rc={score === null ? "var(--border)" : score >= 85 ? CAT_COLOR.engagement : score >= 60 ? "#e8b931" : CAT_COLOR.correctness}>
          <span>{score ?? "—"}</span>
        </div>
        <div class="grow">
          <b>{t("Overall score")}</b>
          <div class="muted small">{t("{n} suggestions · {c} of {t} paragraphs checked", { n: shown.length, c: units.filter((u) => checked[u.hash]).length, t: units.length })}</div>
        </div>
        {#if busy}
          <button class="btn sm" onclick={stop}><span class="spinner"></span>{t("Stop")}</button>
        {:else}
          <button class="btn sm primary" onclick={() => check(true)}>{t("Check now")}</button>
        {/if}
      </div>
      <ModelPicker compact />
      <div class="tiles">
        {#each CATS as c}
          {@const n = shown.filter((s) => s.issue.cat === c).length}
          <button class="tile" class:on={filter === c} style:--c={CAT_COLOR[c]} onclick={() => ((filter = filter === c ? null : c), (tab = "suggestions"))} title={t(CAT_LABEL[c])}>
            <b class:zero={!n}>{n}</b><span>{t(CAT_LABEL[c])}</span>
          </button>
        {/each}
      </div>
      {#if tone}<div class="tone small" style:color={tone[1] ? CAT_COLOR.engagement : CAT_COLOR.delivery}>{tone[1] ? t("Tone: {tone} — fits your goals", { tone: tone[0] }) : t("Tone: {tone} — doesn't quite fit your goals", { tone: tone[0] })}</div>{/if}
      <nav class="ptabs">
        {#each [["suggestions", "Suggestions"], ["goals", "Goals"], ["write", "Write"], ["report", "Report"]] as [id, label]}
          <button class:on={tab === id} onclick={() => (tab = id as typeof tab)}>{t(label)}</button>
        {/each}
      </nav>

      <div class="pane">
        {#if tab === "suggestions"}
          {#if error}<div class="err small"><Icon name="warning" size={14} />{error}</div>{/if}
          {#if !listed.length}
            <div class="empty small">
              <Icon name="check" size={16} />
              {#if allChecked}{t("No suggestions — your writing looks great.")}
              {:else if !units.length}{t("Start writing, and suggestions show up here.")}
              {:else}{t("Press Check now, or turn on Check as you type in Settings › AI assistant.")}{/if}
            </div>
          {/if}
          {#each listed.slice(0, 300) as s (s.id)}
            <div class="sug" class:open={expanded === s.id} style:--c={CAT_COLOR[s.issue.cat]} in:fade={{ duration: 140 }}>
              {#if expanded === s.id}
                {@render issueBody(s, false)}
              {:else}
                <button class="fold" onclick={() => focusIssue(s)}>
                  <i></i><s>{preview(s.issue.original, 26)}</s><span class="arrow">→</span><b>{s.issue.replacement ? preview(s.issue.replacement, 30) : "∅"}</b>
                  <span class="muted small sub">{s.issue.title || s.issue.kind}</span>
                </button>
              {/if}
            </div>
          {/each}
        {:else if tab === "goals"}
          <p class="muted small">{t("Set goals to get suggestions that fit what you're writing.")}</p>
          {#each [["domain", "Domain", DOMAINS], ["formality", "Formality", FORMALITY], ["audience", "Audience", AUDIENCE], ["intent", "Intent", INTENT]] as [k, label, opts]}
            <div class="glabel">{t(label as string)}</div>
            <div class="gchips">
              {#each opts as o}<button class="chip" class:on={goals[k as keyof typeof goals] === o} onclick={() => setGoal(k as keyof typeof goals, o)}>{t(o)}</button>{/each}
            </div>
          {/each}
          <div class="glabel">{t("Language variety")}</div>
          <select class="input" value={goals.variant} onchange={(e) => setGoal("variant", (e.target as HTMLSelectElement).value)}>
            {#each VARIANTS as [c, n]}<option value={c}>{t(n)}</option>{/each}
          </select>
          <div class="glabel">{t("Personal dictionary")}</div>
          <p class="muted small">{t("Words here are never flagged. Add them from a suggestion card.")}</p>
          <div class="gchips">
            {#each dict as w, i}<button class="chip" title={t("Remove")} onclick={() => store.set("writing_dictionary", dict.filter((_, j) => j !== i))}>{w} ×</button>{/each}
          </div>
        {:else if tab === "write"}
          <p class="muted small">{t("Compose, rewrite or brainstorm with AI. Works on the selection (or the paragraph at the cursor).")}</p>
          <div class="gchips">
            {#each actions as [label, instr]}<button class="chip" disabled={goBusy} onclick={() => generate(instr)}>{t(label)}</button>{/each}
          </div>
          <textarea class="input" rows="3" bind:value={prompt} placeholder={t("Or tell AI what to write, e.g. Write an email asking for a deadline extension…")}></textarea>
          <div class="row">
            {#if goBusy}
              <span class="spinner"></span><span class="small">{t("Writing…")}</span>
              <button class="btn sm" onclick={() => goStop?.()}>{t("Stop")}</button>
            {:else}
              <button class="btn sm primary" disabled={!prompt.trim()} onclick={() => generate(prompt.trim())}>{t("Generate")}</button>
            {/if}
          </div>
          {#if goError}<div class="err small"><Icon name="warning" size={14} />{goError}</div>{/if}
          {#if goResult !== null}
            <div class="answer selectable" in:fade={{ duration: 160 }}>{goResult}</div>
            <div class="row">
              <button class="btn sm primary" onclick={() => { insertAfter(goTarget?.range ?? null, goResult ?? ""); store.success(t("Text inserted")); }}>{t("Insert")}</button>
              {#if goTarget?.text}<button class="btn sm" onclick={() => { replaceRange(goTarget?.range ?? null, goResult ?? ""); store.success(t("Text replaced")); }}>{t("Replace selection")}</button>{/if}
              <button class="btn sm" onclick={() => navigator.clipboard.writeText(goResult ?? "").then(() => store.success(t("Copied to the clipboard")))}>{t("Copy")}</button>
            </div>
          {/if}
        {:else}
          <table class="rep">
            <tbody>
              <tr><td>{t("Overall score")}</td><td><b>{score ?? "—"}</b></td></tr>
              <tr><td>{t("Words")}</td><td><b>{nWords}</b></td></tr>
              <tr><td>{t("Sentences")}</td><td><b>{nSentences}</b></td></tr>
              <tr><td>{t("Average sentence length")}</td><td><b>{(nWords / nSentences).toFixed(1)}</b></td></tr>
              <tr><td>{t("Reading time")}</td><td><b>{Math.max(1, Math.ceil(nWords / 230))} min</b></td></tr>
              {#each CATS as c}<tr><td>{t(CAT_LABEL[c])}</td><td><b style:color={CAT_COLOR[c]}>{shown.filter((s) => s.issue.cat === c).length}</b></td></tr>{/each}
            </tbody>
          </table>
          {#if topWords.length}
            <div class="glabel">{t("Most used words")}</div>
            <div class="small">{topWords.map(([w, n]) => `${w} (${n})`).join(", ")}</div>
          {/if}
          <div class="glabel">{t("Your progress")}</div>
          <div class="small">{t("{n} suggestions accepted this week, {t} in all", { n: stats.weeks[weekKey()] ?? 0, t: stats.accepted.reduce((a, b) => a + b, 0) })}</div>
          {#each CATS as c, i}<div class="small muted">{t(CAT_LABEL[c])}: {stats.accepted[i]}</div>{/each}
          <div class="small muted">{t("{n} dismissed", { n: stats.dismissed })}</div>
        {/if}
      </div>
    </div>
  </aside>
{/if}

<style>
  .marks {
    position: fixed;
    inset: 0;
    pointer-events: none;
    z-index: 30;
  }
  .mark {
    position: fixed;
    border-bottom: 2px solid var(--c);
    border-radius: 2px;
    transition: background 120ms;
  }
  .mark.on {
    background: color-mix(in srgb, var(--c) 16%, transparent);
  }
  .wa-card {
    position: fixed;
    z-index: 1100;
    width: 340px;
    padding: 12px;
    border-radius: 12px;
    box-shadow: var(--shadow-l);
    background: var(--surface);
  }
  .ib {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .cat {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--c);
    font-weight: 600;
  }
  .cat i,
  .fold i {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--c);
    flex: none;
  }
  .ttl {
    color: var(--text);
  }
  .fix {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  .orig {
    color: color-mix(in srgb, #e5484d 85%, var(--text));
  }
  /* The fix itself: a solid green button with white text (readable in both themes). */
  .repl {
    border: none;
    border-radius: 7px;
    padding: 4px 10px;
    font-weight: 600;
    color: #fff;
    background: #1f9d55;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.18);
    transition: background 120ms, transform 90ms;
  }
  .repl:hover {
    background: #188a49;
  }
  .repl:active {
    transform: scale(0.97);
  }
  .why {
    color: var(--text-2);
  }
  .ib-acts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 2px;
  }
  .btn.sm {
    height: 28px;
    padding: 0 10px;
    font-size: 0.82rem;
    gap: 5px;
  }
  .btn.ghost {
    margin-top: 8px;
  }
  .btn.wide {
    width: 100%;
    justify-content: center;
  }
  .fab {
    position: absolute;
    right: 22px;
    bottom: 22px;
    z-index: 35;
    width: 42px;
    height: 42px;
    border-radius: 50%;
    border: 2.5px solid var(--ring);
    background: var(--brand-grad);
    color: #fff;
    font-weight: 700;
    display: grid;
    place-items: center;
    box-shadow: 0 4px 14px rgba(10, 30, 110, 0.35);
    transition: transform 160ms var(--ease), box-shadow 160ms;
  }
  .fab:hover {
    transform: scale(1.06);
  }
  .fab.busy {
    border-color: transparent;
    background:
      var(--brand-grad) padding-box,
      conic-gradient(#e8b931 0 25%, transparent 0 100%) border-box;
    animation: spin 0.9s linear infinite;
  }
  .fab.busy > span {
    animation: spin 0.9s linear infinite reverse;
  }
  .spark {
    color: #e8b931;
  }
  .wa-panel {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    z-index: 36;
    width: min(360px, 92%);
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border-left: 1px solid var(--border);
    box-shadow: -10px 0 30px rgba(10, 20, 60, 0.12);
  }
  .wa-panel header {
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
  .pbody {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 14px;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .ring {
    width: 50px;
    height: 50px;
    border-radius: 50%;
    flex: none;
    display: grid;
    place-items: center;
    background: conic-gradient(var(--rc) calc(var(--p) * 1%), var(--surface-3) 0);
    font-weight: 700;
  }
  .ring span {
    width: 40px;
    height: 40px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: var(--surface);
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 6px;
  }
  .tile {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    padding: 8px 8px 6px;
    border-radius: 9px;
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--text-2);
    font-size: 0.72rem;
    overflow: hidden;
  }
  .tile::before {
    content: "";
    position: absolute;
    left: 8px;
    right: 8px;
    top: 0;
    height: 3px;
    border-radius: 0 0 2px 2px;
    background: var(--c);
  }
  .tile b {
    font-size: 1rem;
    color: var(--c);
  }
  .tile b.zero {
    color: var(--text-3);
  }
  .tile.on {
    border-color: var(--c);
    background: color-mix(in srgb, var(--c) 12%, var(--surface));
  }
  .ptabs {
    display: flex;
    gap: 14px;
    border-bottom: 1px solid var(--border);
  }
  .ptabs button {
    border: none;
    background: transparent;
    padding: 6px 0;
    color: var(--text-3);
    font-weight: 600;
    font-size: 0.84rem;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    transition: color var(--dur), border-color var(--dur);
  }
  .ptabs button.on {
    color: var(--text);
    border-bottom-color: var(--accent);
  }
  .pane {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-bottom: 8px;
  }
  .sug {
    border-radius: 9px;
    background: var(--surface-2);
    border: 1px solid var(--border);
  }
  .sug.open {
    border-color: var(--c);
    padding: 10px;
  }
  .fold {
    width: 100%;
    display: grid;
    grid-template-columns: auto auto auto 1fr;
    align-items: center;
    gap: 6px;
    padding: 8px 10px;
    border: none;
    background: transparent;
    text-align: left;
    color: var(--text);
  }
  .fold s {
    color: var(--text-3);
  }
  .fold .arrow {
    color: var(--text-3);
  }
  .fold b {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .fold .sub {
    grid-column: 2 / -1;
  }
  .empty {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 10px;
    color: var(--text-2);
  }
  .empty :global(.icon) {
    color: #1f9d55;
  }
  .err {
    display: flex;
    gap: 7px;
    align-items: flex-start;
    padding: 8px 10px;
    border-radius: var(--radius);
    color: var(--danger);
    background: color-mix(in srgb, var(--danger) 9%, transparent);
  }
  .glabel {
    font-weight: 600;
    margin-top: 6px;
  }
  .gchips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    border: 1px solid var(--border-strong);
    background: var(--surface-2);
    color: var(--text);
    border-radius: 999px;
    padding: 3px 10px;
    font-size: 0.8rem;
    transition: background var(--dur), border-color var(--dur), color var(--dur);
  }
  .chip:hover:not(:disabled) {
    border-color: var(--accent);
    color: var(--text-accent);
  }
  .chip.on {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .row {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .answer {
    max-height: 260px;
    overflow: auto;
    white-space: pre-wrap;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--surface-2);
    border: 1px solid var(--border);
  }
  .rep {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.86rem;
  }
  .rep td {
    padding: 3px 0;
  }
  .rep td:last-child {
    text-align: right;
  }
  .tone {
    font-weight: 600;
  }
  textarea.input {
    resize: vertical;
    font: inherit;
  }
</style>
