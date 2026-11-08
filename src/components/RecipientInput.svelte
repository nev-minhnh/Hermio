<script lang="ts">
  import { scale, fly } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { t } from "../lib/i18n.svelte";
  import Icon from "../lib/Icon.svelte";
  import { api } from "../lib/api";
  import { debounce, displayOf, isEmail, splitAddresses, avatarColor, initials, addrOnly } from "../lib/util";

  let { label, values = $bindable([]), autofocus = false }: { label: string; values: string[]; autofocus?: boolean } = $props();

  let text = $state("");
  let suggestions = $state<{ addr: string; name: string }[]>([]);
  let active = $state(0);
  let input: HTMLInputElement;

  const lookup = debounce(async (q: string) => {
    if (q.trim().length < 1) {
      suggestions = [];
      return;
    }
    const r = await api.contacts(q.trim()).catch(() => []);
    suggestions = r.filter((c) => !values.some((v) => addrOnly(v) === c.addr));
    active = 0;
  }, 120);

  // Keyed chips (for the add/remove animation) need unique values.
  const unique = $derived([...new Set(values)]);

  function commit(raw = text) {
    const parts = splitAddresses(raw);
    if (!parts.length) return;
    const add = parts.filter((p) => !values.some((v) => addrOnly(v) === addrOnly(p)));
    values = [...values, ...add];
    text = "";
    suggestions = [];
  }

  function pick(c: { addr: string; name: string }) {
    commit(c.name ? `${c.name} <${c.addr}>` : c.addr);
    input.focus();
  }

  function onKey(e: KeyboardEvent) {
    if (suggestions.length && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
      e.preventDefault();
      active = (active + (e.key === "ArrowDown" ? 1 : suggestions.length - 1)) % suggestions.length;
    } else if (e.key === "Enter" || e.key === "Tab") {
      if (suggestions.length && text.trim()) {
        e.preventDefault();
        pick(suggestions[active]);
      } else if (text.trim()) {
        if (e.key === "Enter") e.preventDefault();
        commit();
      }
    } else if (e.key === "," || e.key === ";") {
      e.preventDefault();
      commit();
    } else if (e.key === "Backspace" && !text && values.length) {
      values = values.slice(0, -1);
    } else if (e.key === "Escape") {
      suggestions = [];
    }
  }

  function onPaste(e: ClipboardEvent) {
    const t = e.clipboardData?.getData("text") ?? "";
    if (/[,;\n]/.test(t)) {
      e.preventDefault();
      commit(t.replace(/\n/g, ","));
    }
  }
</script>

<div class="rcpt">
  <span class="lbl">{label}</span>
  <div class="box" onclick={() => input.focus()} role="presentation">
    {#each unique as v (v)}
      <span class="chip" class:bad={!isEmail(v)} title={v} animate:flip={{ duration: 180 }} in:scale={{ start: 0.8, duration: 160 }} out:scale={{ start: 0.8, duration: 120 }}>
        <span class="av" style:background={avatarColor(addrOnly(v))}>{initials(displayOf(v), addrOnly(v)).slice(0, 1)}</span>
        {displayOf(v)}
        <button onclick={(e) => { e.stopPropagation(); values = values.filter((x) => x !== v); }} title={t("Delete")}><Icon name="close" size={12} /></button>
      </span>
    {/each}
    <!-- svelte-ignore a11y_autofocus -->
    <input
      bind:this={input}
      bind:value={text}
      oninput={() => lookup(text)}
      onkeydown={onKey}
      onblur={() => setTimeout(() => { if (text.trim()) commit(); suggestions = []; }, 150)}
      onpaste={onPaste}
      {autofocus}
      spellcheck="false"
    />
  </div>
  {#if suggestions.length}
    <div class="menu sugg" transition:fly={{ y: -4, duration: 140 }}>
      {#each suggestions as c, i}
        <button class:act={i === active} onmousedown={(e) => { e.preventDefault(); pick(c); }}>
          <span class="av big" style:background={avatarColor(c.addr)}>{initials(c.name, c.addr)}</span>
          <span class="txt"><b>{c.name || c.addr}</b>{#if c.name}<span class="muted small">{c.addr}</span>{/if}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .rcpt {
    position: relative;
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 4px 0;
    border-bottom: 1px solid var(--border);
  }
  .lbl {
    flex: none;
    width: 42px;
    padding-top: 6px;
    color: var(--text-2);
    font-size: 0.9rem;
  }
  .box {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
    min-height: 32px;
    cursor: text;
  }
  .box input {
    flex: 1;
    min-width: 140px;
    height: 28px;
    border: none;
    outline: none;
    background: transparent;
    user-select: text;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 4px 0 3px;
    border-radius: 13px;
    background: var(--surface-3);
    border: 1px solid var(--border);
    font-size: 0.88rem;
    max-width: 260px;
    white-space: nowrap;
    overflow: hidden;
  }
  .chip.bad {
    border-color: var(--danger);
    color: var(--danger);
  }
  .chip button {
    border: none;
    background: transparent;
    display: flex;
    padding: 3px;
    border-radius: 50%;
    color: var(--text-3);
  }
  .chip button:hover {
    background: var(--hover);
  }
  .av {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    color: white;
    font-size: 0.66rem;
    font-weight: 700;
    display: flex;
    align-items: center;
    justify-content: center;
    flex: none;
  }
  .av.big {
    width: 28px;
    height: 28px;
    font-size: 0.72rem;
  }
  .sugg {
    position: absolute;
    top: 100%;
    left: 50px;
    width: 340px;
  }
  .sugg button {
    height: 44px;
  }
  .sugg button.act {
    background: var(--selected);
  }
  .txt {
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: 1.2;
  }
</style>
