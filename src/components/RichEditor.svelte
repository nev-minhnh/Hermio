<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import { onMount } from "svelte";
  import Icon from "../lib/Icon.svelte";

  let {
    html = "",
    placeholder = "",
    minHeight = 200,
    compact = false,
    el = $bindable(),
    onchange,
    onkeydown,
  }: {
    html?: string;
    placeholder?: string;
    minHeight?: number;
    compact?: boolean;
    el?: HTMLDivElement;
    onchange?: (html: string) => void;
    onkeydown?: (e: KeyboardEvent) => void;
  } = $props();

  let fmt = $state<Record<string, boolean>>({});
  let colorInput: HTMLInputElement;
  let hiliteInput: HTMLInputElement;
  let imageInput: HTMLInputElement;
  let empty = $state(true);

  // ---------- resizing pictures ----------
  // Click a picture: a frame with corner handles (drag to resize, keeping its proportions) and a
  // small bar with preset sizes. The size is saved as the picture's width (height follows).
  let wrap: HTMLDivElement;
  let img = $state<HTMLImageElement | null>(null);
  let box = $state<{ x: number; y: number; w: number; h: number } | null>(null);
  let drag: { sx: number; w0: number; dir: number } | null = null;
  function place() {
    if (!img || !wrap || !img.isConnected) {
      img = null;
      box = null;
      return;
    }
    const r = img.getBoundingClientRect();
    const o = wrap.getBoundingClientRect();
    box = { x: r.left - o.left, y: r.top - o.top, w: r.width, h: r.height };
  }
  function pick(e: MouseEvent) {
    const tgt = e.target as HTMLElement;
    if (tgt.tagName === "IMG" && el?.contains(tgt)) {
      img = tgt as HTMLImageElement;
      // Select the picture itself, so Delete / Ctrl+X / typing act on it.
      const r = document.createRange();
      r.selectNode(img);
      const s = window.getSelection();
      s?.removeAllRanges();
      s?.addRange(r);
      place();
    } else if (img) {
      img = null;
      box = null;
    }
  }
  function maxWidth() {
    return Math.max(40, (el?.clientWidth ?? 600) - 8);
  }
  function setWidth(px: number) {
    if (!img) return;
    const w = Math.round(Math.min(maxWidth(), Math.max(24, px)));
    img.removeAttribute("height");
    img.setAttribute("width", String(w));
    img.style.width = `${w}px`;
    img.style.height = "auto";
    img.style.maxWidth = "100%";
    place();
    changed();
  }
  function startDrag(e: PointerEvent, dir: number) {
    if (!img) return;
    e.preventDefault();
    e.stopPropagation();
    drag = { sx: e.clientX, w0: img.getBoundingClientRect().width, dir };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function onDrag(e: PointerEvent) {
    if (!drag) return;
    setWidth(drag.w0 + (e.clientX - drag.sx) * drag.dir);
  }
  function endDrag() {
    drag = null;
  }
  function presetPct(p: number) {
    setWidth((maxWidth() * p) / 100);
  }
  function original() {
    if (img) setWidth(img.naturalWidth || img.getBoundingClientRect().width);
  }
  function removeImg() {
    if (!img) return;
    img.remove();
    img = null;
    box = null;
    changed();
  }
  $effect(() => {
    if (!img) return;
    const ro = new ResizeObserver(place);
    ro.observe(img);
    if (el) ro.observe(el);
    const onScroll = () => place();
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", onScroll);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        img = null;
        box = null;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      ro.disconnect();
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", onScroll);
      window.removeEventListener("keydown", onKey);
    };
  });

  onMount(() => {
    el!.innerHTML = html;
    document.execCommand("defaultParagraphSeparator", false, "div");
    document.execCommand("styleWithCSS", false, "true");
    updateEmpty();
  });

  function updateEmpty() {
    empty = !el || (el.innerText.trim() === "" && !el.querySelector("img,table,hr,blockquote"));
  }

  function exec(cmd: string, value?: string) {
    el?.focus();
    document.execCommand(cmd, false, value);
    refresh();
    changed();
  }

  function refresh() {
    const q = (c: string) => {
      try {
        return document.queryCommandState(c);
      } catch {
        return false;
      }
    };
    fmt = {
      bold: q("bold"),
      italic: q("italic"),
      underline: q("underline"),
      strikeThrough: q("strikeThrough"),
      insertUnorderedList: q("insertUnorderedList"),
      insertOrderedList: q("insertOrderedList"),
    };
  }

  function changed() {
    updateEmpty();
    if (img && !img.isConnected) {
      img = null;
      box = null;
    } else if (img) place();
    onchange?.(el?.innerHTML ?? "");
  }

  function link() {
    const sel = window.getSelection();
    const url = prompt(t("Link address (https://…)"), "https://");
    if (!url || url === "https://") return;
    if (sel && sel.isCollapsed) exec("insertHTML", `<a href="${url.replace(/"/g, "&quot;")}">${url.replace(/</g, "&lt;")}</a>`);
    else exec("createLink", url);
  }

  function insertImageFile(file: File) {
    if (file.size > 4 * 1024 * 1024) {
      alert(t("Image is larger than 4 MB – attach it instead of inserting it."));
      return;
    }
    const r = new FileReader();
    r.onload = () => exec("insertHTML", `<img src="${r.result}" style="max-width:100%">`);
    r.readAsDataURL(file);
  }

  function onPaste(e: ClipboardEvent) {
    const files = [...(e.clipboardData?.files ?? [])].filter((f) => f.type.startsWith("image/"));
    if (files.length) {
      e.preventDefault();
      files.forEach(insertImageFile);
    }
  }

  const fonts = [
    ["Segoe UI", "'Segoe UI', sans-serif"],
    ["Arial", "Arial, sans-serif"],
    ["Calibri", "Calibri, sans-serif"],
    ["Aptos", "Aptos, sans-serif"],
    ["Times New Roman", "'Times New Roman', serif"],
    ["Georgia", "Georgia, serif"],
    ["Consolas", "Consolas, monospace"],
  ];
  const sizes = [
    [t("Small"), "2"],
    [t("Normal"), "3"],
    [t("Large"), "4"],
    [t("Huge"), "5"],
    [t("Heading"), "6"],
  ];
</script>

<div class="editor" class:compact>
  <div class="tools" role="toolbar">
    <select class="sel" title={t("Font")} onchange={(e) => exec("fontName", (e.target as HTMLSelectElement).value)}>
      {#each fonts as [label, value]}<option {value}>{label}</option>{/each}
    </select>
    <select class="sel small" title={t("Font size")} onchange={(e) => exec("fontSize", (e.target as HTMLSelectElement).value)}>
      {#each sizes as [label, value]}<option {value} selected={value === "3"}>{label}</option>{/each}
    </select>
    <span class="div"></span>
    <button class:on={fmt.bold} title={t("Bold (Ctrl+B)")} onmousedown={(e) => e.preventDefault()} onclick={() => exec("bold")}><Icon name="bold" size={16} /></button>
    <button class:on={fmt.italic} title={t("Italic (Ctrl+I)")} onmousedown={(e) => e.preventDefault()} onclick={() => exec("italic")}><Icon name="italic" size={16} /></button>
    <button class:on={fmt.underline} title={t("Underline (Ctrl+U)")} onmousedown={(e) => e.preventDefault()} onclick={() => exec("underline")}><Icon name="underline" size={16} /></button>
    <button class:on={fmt.strikeThrough} title={t("Strikethrough")} onmousedown={(e) => e.preventDefault()} onclick={() => exec("strikeThrough")}><Icon name="strike" size={16} /></button>
    <button title={t("Text color")} onmousedown={(e) => e.preventDefault()} onclick={() => colorInput.click()}><Icon name="textColor" size={16} /></button>
    <button title={t("Highlight")} onmousedown={(e) => e.preventDefault()} onclick={() => hiliteInput.click()}><Icon name="highlight" size={16} /></button>
    <span class="div"></span>
    <button class:on={fmt.insertUnorderedList} title={t("Bulleted list")} onmousedown={(e) => e.preventDefault()} onclick={() => exec("insertUnorderedList")}><Icon name="listBullet" size={16} /></button>
    <button class:on={fmt.insertOrderedList} title={t("Numbered list")} onmousedown={(e) => e.preventDefault()} onclick={() => exec("insertOrderedList")}><Icon name="listNumber" size={16} /></button>
    {#if !compact}
      <button title={t("Decrease indent")} onmousedown={(e) => e.preventDefault()} onclick={() => exec("outdent")}><Icon name="outdent" size={16} /></button>
      <button title={t("Increase indent")} onmousedown={(e) => e.preventDefault()} onclick={() => exec("indent")}><Icon name="indent" size={16} /></button>
      <button title={t("Align left")} onmousedown={(e) => e.preventDefault()} onclick={() => exec("justifyLeft")}><Icon name="alignLeft" size={16} /></button>
      <button title={t("Center")} onmousedown={(e) => e.preventDefault()} onclick={() => exec("justifyCenter")}><Icon name="alignCenter" size={16} /></button>
      <button title={t("Align right")} onmousedown={(e) => e.preventDefault()} onclick={() => exec("justifyRight")}><Icon name="alignRight" size={16} /></button>
      <button title={t("Quote")} onmousedown={(e) => e.preventDefault()} onclick={() => exec("formatBlock", "blockquote")}><Icon name="quote" size={16} /></button>
    {/if}
    <span class="div"></span>
    <button title={t("Insert link")} onmousedown={(e) => e.preventDefault()} onclick={link}><Icon name="link" size={16} /></button>
    <button title={t("Insert image")} onmousedown={(e) => e.preventDefault()} onclick={() => imageInput.click()}><Icon name="image" size={16} /></button>
    <button title={t("Clear formatting")} onmousedown={(e) => e.preventDefault()} onclick={() => exec("removeFormat")}><Icon name="clear" size={16} /></button>
    {#if !compact}
      <span class="div"></span>
      <button title={t("Undo (Ctrl+Z)")} onmousedown={(e) => e.preventDefault()} onclick={() => exec("undo")}><Icon name="undo" size={16} /></button>
      <button title={t("Redo (Ctrl+Y)")} onmousedown={(e) => e.preventDefault()} onclick={() => exec("redo")}><Icon name="redo" size={16} /></button>
    {/if}
    <input type="color" bind:this={colorInput} hidden onchange={(e) => exec("foreColor", (e.target as HTMLInputElement).value)} />
    <input type="color" bind:this={hiliteInput} value="#fff176" hidden onchange={(e) => exec("hiliteColor", (e.target as HTMLInputElement).value)} />
    <input type="file" accept="image/*" bind:this={imageInput} hidden onchange={(e) => { const f = (e.target as HTMLInputElement).files?.[0]; if (f) insertImageFile(f); (e.target as HTMLInputElement).value = ""; }} />
  </div>
  <div class="area-wrap" bind:this={wrap}>
    {#if empty && placeholder}<div class="ph">{placeholder}</div>{/if}
    <div
      class="area"
      bind:this={el}
      contenteditable="true"
      role="textbox"
      aria-multiline="true"
      tabindex="0"
      style:min-height="{minHeight}px"
      spellcheck="true"
      oninput={changed}
      onkeyup={refresh}
      onmouseup={refresh}
      onmousedown={pick}
      onpaste={onPaste}
      {onkeydown}
    ></div>
    {#if img && box}
      <div class="imgframe" style:left="{box.x}px" style:top="{box.y}px" style:width="{box.w}px" style:height="{box.h}px">
        {#each [["nw", -1], ["ne", 1], ["sw", -1], ["se", 1]] as [pos, dir]}
          <span class="h {pos}" role="presentation" onpointerdown={(e) => startDrag(e, dir as number)} onpointermove={onDrag} onpointerup={endDrag} onpointercancel={endDrag}></span>
        {/each}
        <span class="dim">{Math.round(box.w)} × {Math.round(box.h)}</span>
      </div>
      <div class="imgbar" style:left="{Math.max(0, box.x)}px" style:top="{Math.max(0, box.y - 40)}px">
        {#each [25, 50, 75, 100] as p}<button type="button" onmousedown={(e) => e.preventDefault()} onclick={() => presetPct(p)}>{p}%</button>{/each}
        <button type="button" onmousedown={(e) => e.preventDefault()} onclick={original} title={t("Original size")}>{t("Original")}</button>
        <span class="sep"></span>
        <button type="button" class="del" onmousedown={(e) => e.preventDefault()} onclick={removeImg} title={t("Remove picture")}><Icon name="trash" size={14} /></button>
      </div>
    {/if}
  </div>
</div>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
  }
  .tools {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 1px;
    padding: 4px 6px;
    border-radius: var(--radius);
    background: var(--surface-3);
    position: sticky;
    top: 0;
    z-index: 2;
  }
  .tools button {
    width: 30px;
    height: 28px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text-2);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .tools button:hover {
    background: var(--hover);
    color: var(--text);
  }
  .tools button.on {
    background: var(--selected-strong);
    color: var(--text-accent);
  }
  .sel {
    height: 28px;
    border: none;
    border-radius: 4px;
    background: transparent;
    padding: 0 4px;
    font-size: 0.86rem;
    max-width: 130px;
  }
  .sel:hover {
    background: var(--hover);
  }
  .sel.small {
    max-width: 104px;
  }
  .div {
    width: 1px;
    height: 18px;
    background: var(--border-strong);
    margin: 0 4px;
  }
  .area-wrap {
    position: relative;
    flex: 1;
  }
  .ph {
    position: absolute;
    top: 12px;
    left: 4px;
    color: var(--text-3);
    pointer-events: none;
  }
  .area {
    outline: none;
    padding: 12px 4px;
    font-family: "Segoe UI", sans-serif;
    font-size: 14.5px;
    line-height: 1.5;
    user-select: text;
    overflow-wrap: anywhere;
    cursor: text;
  }
  .area :global(img) {
    max-width: 100%;
    cursor: pointer;
  }
  .imgframe {
    position: absolute;
    outline: 2px solid var(--accent);
    outline-offset: 1px;
    pointer-events: none;
    z-index: 3;
  }
  .imgframe .h {
    position: absolute;
    width: 12px;
    height: 12px;
    border-radius: 3px;
    background: #fff;
    border: 2px solid var(--accent);
    pointer-events: auto;
    touch-action: none;
  }
  .h.nw {
    left: -7px;
    top: -7px;
    cursor: nwse-resize;
  }
  .h.ne {
    right: -7px;
    top: -7px;
    cursor: nesw-resize;
  }
  .h.sw {
    left: -7px;
    bottom: -7px;
    cursor: nesw-resize;
  }
  .h.se {
    right: -7px;
    bottom: -7px;
    cursor: nwse-resize;
  }
  .dim {
    position: absolute;
    right: 4px;
    bottom: 4px;
    padding: 1px 6px;
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.62);
    color: #fff;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  .imgbar {
    position: absolute;
    z-index: 4;
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 3px;
    border-radius: 8px;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    box-shadow: var(--shadow);
  }
  .imgbar button {
    height: 26px;
    min-width: 30px;
    padding: 0 7px;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: var(--text);
    font-size: 12px;
    font-weight: 600;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .imgbar button:hover {
    background: var(--hover);
  }
  .imgbar .del:hover {
    color: var(--danger);
  }
  .imgbar .sep {
    width: 1px;
    height: 16px;
    background: var(--border-strong);
    margin: 0 2px;
  }
  .area :global(blockquote) {
    margin: 0 0 0 0.4em;
    padding-left: 1em;
    border-left: 3px solid var(--border-strong);
    color: var(--text-2);
  }
  .area :global(.hermio-quote) {
    margin-top: 12px;
  }
  .area :global(table) {
    max-width: 100%;
  }
</style>
