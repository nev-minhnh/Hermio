<script lang="ts">
  /**
   * Hermio AI popup — Ask AI, Translate, Meaning — opened by the pointer (double-click a word,
   * Ctrl+Alt+K / T / D, or the composer's ✦ menu).
   *
   * Unlike the version it comes from, it opens *at the pointer* (not at the text caret, which
   * can be far away) and doesn't close just because the pointer isn't over it yet: it closes
   * with Esc, a click outside, or once the pointer has been inside and then leaves for a while.
   */
  import { onDestroy, onMount, tick } from "svelte";
  import { fade, scale } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { t } from "../lib/i18n.svelte";
  import Icon from "../lib/Icon.svelte";
  import { api } from "../lib/api";
  import { store, type AssistOpen } from "../lib/store.svelte";
  import { LANGUAGES, askSystem, defineSystem, preview, run, translateSystem } from "../lib/writing";
  import ModelPicker from "./ModelPicker.svelte";

  let { open }: { open: AssistOpen } = $props();

  type Mode = AssistOpen["mode"];
  let mode = $state<Mode>(open.mode);
  let request = $state("");
  let answer = $state<string | null>(null);
  let note = $state("");
  let error = $state("");
  let busy = $state(false);
  let started = $state(0);
  let now = $state(Date.now());
  let stop: (() => void) | null = null;
  let token = 0;

  let box = $state<HTMLDivElement>();
  let x = $state(0);
  let y = $state(0);
  /** When the pointer came in (a real visit arms closing on leave; passing over it doesn't). */
  let enteredAt = 0;
  let awayTimer: ReturnType<typeof setTimeout> | undefined;
  let clock: ReturnType<typeof setInterval>;

  const target = $derived(open.target);

  onMount(async () => {
    await tick();
    place(open.x + 10, open.y + 12);
    clock = setInterval(() => (now = Date.now()), 250);
    if (mode !== "ask") start();
  });
  onDestroy(() => {
    clearInterval(clock);
    clearTimeout(awayTimer);
    stop?.();
  });

  /** Put the popup by the pointer, kept inside the window. */
  function place(px: number, py: number) {
    const w = box?.offsetWidth ?? 440;
    const h = box?.offsetHeight ?? 300;
    x = Math.max(8, Math.min(px, window.innerWidth - w - 8));
    // Not enough room below the pointer: open above it.
    y = py + h > window.innerHeight - 8 ? Math.max(8, py - h - 20) : py;
  }

  function close() {
    store.assist = null;
  }

  async function start() {
    error = "";
    answer = null;
    note = "";
    stop?.();
    stop = null;
    const my = ++token;
    const text = target.text.trim();
    busy = true;
    started = Date.now();
    try {
      let out = "";
      if (mode === "ask") {
        if (!request.trim()) return;
        const prompt = text ? `Request: ${request.trim()}\n\nText:\n<<<\n${target.text}\n>>>` : `Request: ${request.trim()}\n\n(There is no text yet: write the text to insert.)`;
        const [p, s] = run(askSystem(), prompt);
        stop = s;
        out = await p;
      } else if (mode === "translate") {
        if (!text) throw new Error(t("Select the text to translate."));
        const to = store.settings.translate_to;
        if (store.settings.translate_engine === "google" || !store.settings.translate_engine) {
          const r = await api.translateText(target.text, to);
          out = r.text;
          if (r.from) note = t("Detected language: {lang}", { lang: r.from });
        } else {
          const [p, s] = run(translateSystem(to), target.text, { provider: store.settings.translate_engine, model: "", thinking: store.settings.assist_thinking });
          stop = s;
          out = await p;
        }
      } else {
        if (!text) throw new Error(t("Select a word or a phrase."));
        const english = /^[A-Za-z'-]+$/.test(text);
        let done = false;
        if (english) {
          try {
            out = await api.defineWord(text);
            note = t("Source: Free Dictionary (dictionaryapi.dev)");
            done = true;
          } catch {}
        }
        if (!done) {
          const [p, s] = run(defineSystem(), `Word or phrase: ${text}\nContext: ${target.context || text}`);
          stop = s;
          out = await p;
        }
      }
      if (my === token) answer = out.trim();
    } catch (e) {
      if (my === token) error = String(e).replace(/^Error: /, "");
    } finally {
      if (my === token) {
        busy = false;
        stop = null;
      }
    }
  }

  function cancel() {
    token++;
    stop?.();
    stop = null;
    busy = false;
  }

  function setMode(m: Mode) {
    if (m === mode) return;
    mode = m;
    cancel();
    answer = null;
    error = "";
    note = "";
    if (m !== "ask") start();
  }

  async function copy() {
    if (!answer) return;
    await navigator.clipboard.writeText(answer).catch(() => {});
    store.success(t("Copied to the clipboard"));
  }
  function replace() {
    if (!answer || !target.replace) return;
    target.replace(answer);
    store.success(t("Text replaced"));
    close();
  }
  function insert() {
    if (!answer || !target.insert) return;
    target.insert(answer);
    store.success(t("Text inserted"));
    close();
  }

  // ---- closing: Esc, a click outside, or leaving after having been inside ----
  function onDown(e: PointerEvent) {
    if (box && !box.contains(e.target as Node)) close();
  }
  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      close();
    }
  }
  function onEnter() {
    enteredAt = performance.now();
    clearTimeout(awayTimer);
  }
  function onLeave() {
    if (!enteredAt || performance.now() - enteredAt < 400) return;
    const typing = mode === "ask" && (request.trim() !== "" || box?.contains(document.activeElement));
    const picking = box?.querySelector("select:focus");
    if (typing || picking || busy) return;
    clearTimeout(awayTimer);
    awayTimer = setTimeout(close, 1200);
  }

  // ---- drag by the header ----
  function dragStart(e: PointerEvent) {
    if ((e.target as HTMLElement).closest("button,select,input,textarea")) return;
    const sx = e.clientX - x;
    const sy = e.clientY - y;
    const move = (ev: PointerEvent) => {
      x = Math.max(4, Math.min(ev.clientX - sx, window.innerWidth - 60));
      y = Math.max(4, Math.min(ev.clientY - sy, window.innerHeight - 40));
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }

  const quick = ["Fix grammar", "More formal", "Shorter", "Clearer", "Friendlier"];
  const secs = $derived(Math.max(0, Math.floor((now - started) / 1000)));
</script>

<svelte:window onpointerdown={onDown} onkeydown={onKey} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="assist card"
  bind:this={box}
  style:left="{x}px"
  style:top="{y}px"
  onpointerenter={onEnter}
  onpointerleave={onLeave}
  role="dialog"
  aria-label="Hermio AI"
  transition:scale={{ start: 0.96, duration: 170, easing: cubicOut }}
>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="head" onpointerdown={dragStart}>
    <span class="spark">✦</span>
    <b>Hermio AI</b>
    <div class="tabs" role="tablist">
      <button role="tab" class:on={mode === "ask"} onclick={() => setMode("ask")}>{t("Ask AI")}</button>
      <button role="tab" class:on={mode === "translate"} onclick={() => setMode("translate")}>{t("Translate")}</button>
      <button role="tab" class:on={mode === "define"} onclick={() => setMode("define")}>{t("Meaning")}</button>
    </div>
    <button class="icon-btn x" title={t("Close")} onclick={close}><Icon name="close" size={15} /></button>
  </div>

  <div class="opts">
    {#if mode === "translate"}
      <ModelPicker google />
      <select class="input to" value={store.settings.translate_to} onchange={(e) => { store.set("translate_to", (e.target as HTMLSelectElement).value); start(); }} title={t("Translate into")}>
        {#each LANGUAGES as [code, name]}<option value={code}>{t(name)}</option>{/each}
      </select>
    {:else}
      <ModelPicker />
    {/if}
  </div>

  <div class="body">
    {#if target.text.trim()}
      <div class="src small">{mode === "ask" ? (target.selection ? t("Selection") : t("Paragraph")) + ": " : ""}{preview(target.text, mode === "ask" ? 90 : 140)}</div>
    {/if}

    {#if mode === "ask"}
      <textarea
        class="input req"
        rows="2"
        bind:value={request}
        placeholder={t("What should AI do? e.g. make it more formal, shorter, fix the tone, continue this paragraph…")}
        onkeydown={(e) => {
          if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
            e.preventDefault();
            start();
          }
        }}
      ></textarea>
      <div class="chips">
        {#each quick as q}<button class="chip" onclick={() => { request = t(q); start(); }} disabled={busy}>{t(q)}</button>{/each}
        <span class="grow"></span>
        <button class="btn primary sm" onclick={start} disabled={busy || !request.trim()} title="Ctrl+Enter"><Icon name="send" size={14} />{t("Send")}</button>
      </div>
    {/if}

    {#if busy}
      <div class="status small" in:fade={{ duration: 120 }}>
        <span class="spinner"></span>
        {mode === "ask" ? t("Writing…") : mode === "translate" ? t("Translating…") : t("Looking up…")}
        {secs} s
        <button class="btn sm" onclick={cancel}>{t("Cancel")}</button>
      </div>
    {:else if error}
      <div class="err small" in:fade={{ duration: 120 }}>
        <Icon name="warning" size={15} /><span class="grow">{error}</span>
        <button class="btn sm" onclick={start}>{t("Try again")}</button>
      </div>
    {:else if answer !== null}
      <div class="answer selectable" in:fade={{ duration: 160 }}>{answer}</div>
      {#if note}<div class="note small">{note}</div>{/if}
      <div class="acts">
        {#if target.replace && mode !== "define"}<button class="btn primary sm" onclick={replace}><Icon name="check" size={14} />{t("Replace")}</button>{/if}
        {#if target.insert && mode === "ask"}<button class="btn sm" onclick={insert}><Icon name="plus" size={14} />{t("Insert")}</button>{/if}
        <button class="btn sm" onclick={copy}><Icon name="copy" size={14} />{t("Copy")}</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .assist {
    position: fixed;
    z-index: 1200;
    width: min(440px, calc(100vw - 16px));
    padding: 12px 14px 14px;
    border-radius: 14px;
    box-shadow: var(--shadow-l);
    background: var(--surface);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: grab;
    user-select: none;
  }
  .spark {
    color: #e8b931;
    font-size: 1.05rem;
    line-height: 1;
  }
  .tabs {
    display: flex;
    gap: 2px;
    padding: 2px;
    margin-left: 4px;
    border-radius: 9px;
    background: var(--surface-3);
  }
  /* Readable in both themes: the active tab is the accent with white text, others normal text. */
  .tabs button {
    border: none;
    background: transparent;
    color: var(--text-2);
    font-size: 0.82rem;
    font-weight: 600;
    padding: 4px 10px;
    border-radius: 7px;
    transition: background var(--dur) var(--ease), color var(--dur) var(--ease);
  }
  .tabs button:hover {
    color: var(--text);
    background: var(--hover);
  }
  .tabs button.on {
    background: var(--accent);
    color: #fff;
    box-shadow: 0 1px 3px color-mix(in srgb, var(--accent) 40%, transparent);
  }
  .x {
    margin-left: auto;
    width: 26px;
    height: 26px;
  }
  .opts {
    display: flex;
    gap: 6px;
    margin: 10px 0 8px;
  }
  .opts :global(.mp) {
    flex: 1;
  }
  .to {
    width: 150px;
    flex: none;
    padding: 4px 8px;
    font-size: 0.82rem;
  }
  .body {
    border-top: 1px solid var(--border);
    padding-top: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .src {
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
  }
  .req {
    resize: vertical;
    min-height: 52px;
    font: inherit;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
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
    background: var(--accent-soft);
  }
  .grow {
    flex: 1;
  }
  .btn.sm {
    height: 28px;
    padding: 0 10px;
    font-size: 0.82rem;
    gap: 5px;
  }
  .status,
  .err {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-2);
  }
  .err {
    color: var(--danger);
    padding: 8px 10px;
    border-radius: var(--radius);
    background: color-mix(in srgb, var(--danger) 9%, transparent);
  }
  .answer {
    max-height: 300px;
    overflow: auto;
    white-space: pre-wrap;
    line-height: 1.45;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    color: var(--text);
  }
  .note {
    color: var(--text-3);
  }
  .acts {
    display: flex;
    gap: 6px;
  }
</style>
