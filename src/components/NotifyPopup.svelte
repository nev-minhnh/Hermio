<script lang="ts">
  // Hermio's new-mail popup (its own small window above the taskbar): bold subject, From / To,
  // time. Stays 7 s (paused while the mouse is over it); a click opens the email.
  import { onMount } from "svelte";
  import { fly } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { invoke, listen } from "../lib/tauri";
  import { api } from "../lib/api";
  import Logo from "../lib/Logo.svelte";

  type Item = { key: string; subject: string; from: string; to: string; time: string; launch: string; from_label: string; to_label: string };
  const LIFE = 7000;
  let items = $state<Item[]>([]);
  let box = $state<HTMLDivElement>();
  const timers = new Map<string, ReturnType<typeof setTimeout>>();
  let hovering = false;

  function arm(key: string) {
    clearTimeout(timers.get(key));
    timers.set(key, setTimeout(() => close(key), LIFE));
  }
  function close(key: string) {
    clearTimeout(timers.get(key));
    timers.delete(key);
    items = items.filter((i) => i.key !== key);
  }
  async function pull() {
    const got = await invoke<Item[]>("popup_take").catch(() => [] as Item[]);
    if (!got.length) return;
    // Newest at the bottom (closest to the taskbar); at most three on screen.
    const all = [...items, ...got];
    for (const old of all.slice(0, Math.max(0, all.length - 3))) {
      clearTimeout(timers.get(old.key));
      timers.delete(old.key);
    }
    items = all.slice(-3);
    if (!hovering) for (const it of got) arm(it.key);
  }
  function open(it: Item) {
    invoke("popup_open", { launch: it.launch }).catch(() => {});
    close(it.key);
  }
  function enter() {
    hovering = true;
    for (const t of timers.values()) clearTimeout(t);
  }
  function leave() {
    hovering = false;
    for (const it of items) arm(it.key);
  }

  onMount(() => {
    document.documentElement.classList.add("popup-page");
    api
      .settings()
      .then((s) => {
        const th = String(s.theme ?? "system").replace(/"/g, "");
        const dark = th === "dark" || (th !== "light" && matchMedia("(prefers-color-scheme: dark)").matches);
        document.documentElement.dataset.theme = dark ? "dark" : "light";
        if (typeof s.accent === "string") document.documentElement.style.setProperty("--accent", s.accent.replace(/"/g, ""));
      })
      .catch(() => {});
    // The window takes the size of what is shown (and hides when nothing is left).
    // (A card still sliding out keeps the window until it's gone.)
    const ro = new ResizeObserver(() => invoke("popup_layout", { height: box!.querySelector(".card") ? box!.offsetHeight : 0 }).catch(() => {}));
    ro.observe(box!);
    let off: (() => void) | undefined;
    listen("popup-add", () => pull()).then((f) => (off = f));
    pull();
    return () => {
      ro.disconnect();
      off?.();
    };
  });
</script>

<div class="stack" bind:this={box} onmouseenter={enter} onmouseleave={leave} role="presentation">
  {#each items as it (it.key)}
    <div class="card" in:fly={{ x: 40, duration: 260, easing: cubicOut }} out:fly={{ x: 40, duration: 200 }}>
      <div class="head">
        <span class="app"><Logo size={18} />Hermio</span>
        <button class="x" aria-label="Close" onclick={() => close(it.key)}>
          <svg width="12" height="12" viewBox="0 0 12 12"><path d="M2 2l8 8M10 2l-8 8" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" /></svg>
        </button>
      </div>
      <button class="body" onclick={() => open(it)}>
        <span class="subject">{it.subject}</span>
        <span class="grid">
          <b>{it.from_label}</b><span class="v">{it.from}</span><span class="time">{it.time}</span>
          <b>{it.to_label}</b><span class="v">{it.to}</span><span></span>
        </span>
      </button>
    </div>
  {/each}
</div>

<style>
  :global(html.popup-page),
  :global(html.popup-page body),
  :global(html.popup-page #app) {
    background: transparent !important;
    overflow: hidden;
  }
  .stack {
    position: fixed;
    left: 0;
    right: 0;
    bottom: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 6px;
  }
  .card {
    border-radius: 10px;
    background: color-mix(in srgb, var(--surface) 96%, transparent);
    border: 1px solid var(--border-strong);
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.28);
    color: var(--text);
    overflow: hidden;
  }
  .head {
    display: flex;
    align-items: center;
    padding: 10px 10px 0 14px;
  }
  .app {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    color: var(--text-2);
  }
  .x {
    width: 24px;
    height: 24px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text-3);
    display: grid;
    place-items: center;
    cursor: pointer;
    opacity: 0;
    transition: opacity 120ms ease, background 120ms ease;
  }
  .card:hover .x {
    opacity: 1;
  }
  .x:hover {
    background: var(--hover);
    color: var(--text);
  }
  .body {
    display: block;
    width: 100%;
    text-align: left;
    border: none;
    background: transparent;
    color: inherit;
    padding: 6px 14px 12px;
    cursor: pointer;
  }
  .subject {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    font-weight: 600;
    font-size: 14.5px;
    line-height: 1.3;
  }
  .grid {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    column-gap: 12px;
    row-gap: 0;
    margin-top: 5px;
    font-size: 13px;
    line-height: 1.4;
  }
  .grid b {
    font-weight: 600;
  }
  .v {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-2);
  }
  .time {
    font-size: 12px;
    color: var(--text-3);
  }
</style>
