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
      altEdit = null;
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
  /** The widest a picture can be: from where it starts to the right edge of the editor. */
  function maxWidth() {
    if (!el) return 600;
    const e = el.getBoundingClientRect();
    const left = img ? img.getBoundingClientRect().left : e.left;
    return Math.max(40, Math.floor(e.right - left - 6));
  }
  /**
   * Cells and boxes that only hold this picture take its width, so making the picture smaller
   * also closes the gap beside it (signatures are often tables with a fixed-width logo cell).
   */
  function hug(w: number) {
    let n = img?.parentElement ?? null;
    while (n && n !== el) {
      const pic = img!;
      const others = Array.from(n.childNodes).some((c) => {
        if (c === pic || (c.nodeType === 1 && (c as Element).contains(pic))) return false;
        if (c.nodeType === 3) return !!c.textContent?.trim();
        return c.nodeType === 1 && (c as Element).tagName !== "BR";
      });
      if (others) break;
      const cs = getComputedStyle(n);
      const pad = parseFloat(cs.paddingLeft) + parseFloat(cs.paddingRight) + parseFloat(cs.borderLeftWidth) + parseFloat(cs.borderRightWidth);
      if (n.tagName === "TD" || n.tagName === "TH") {
        n.removeAttribute("width");
        n.style.width = `${Math.ceil(w + pad)}px`;
        n.style.minWidth = "";
        break;
      }
      // A box with a fixed width around the picture: let it follow the picture.
      if (n.style.width || n.getAttribute("width")) {
        n.removeAttribute("width");
        n.style.width = "";
        n.style.minWidth = "";
      }
      n = n.parentElement;
    }
  }
  function setWidth(px: number) {
    if (!img) return;
    const w = Math.round(Math.max(16, px));
    img.removeAttribute("height");
    img.setAttribute("width", String(w));
    img.style.width = `${w}px`;
    img.style.height = "auto";
    img.style.maxWidth = "100%";
    hug(w);
    place();
    changed();
  }
  const natural = () => (img ? img.naturalWidth || img.getBoundingClientRect().width : 0);
  /** Gmail's sizes: small and medium (never larger than the picture), best fit, original. */
  function size(kind: "small" | "medium" | "fit" | "original") {
    if (!img) return;
    const nat = natural();
    const fit = maxWidth();
    const w = kind === "small" ? Math.min(nat, Math.max(48, fit * 0.2)) : kind === "medium" ? Math.min(nat, Math.max(96, fit * 0.45)) : kind === "fit" ? fit : Math.min(nat, fit);
    setWidth(w);
  }
  let altEdit = $state<string | null>(null);
  function saveAlt() {
    if (img && altEdit !== null) {
      img.alt = altEdit.trim();
      changed();
    }
    altEdit = null;
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
      justifyLeft: q("justifyLeft"),
      justifyCenter: q("justifyCenter"),
      justifyRight: q("justifyRight"),
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

  let pop = $state<"align" | "emoji" | null>(null);
  const alignIcon = $derived(fmt.justifyCenter ? "alignCenter" : fmt.justifyRight ? "alignRight" : "alignLeft");
  const EMOJI = "😀 😁 😂 🙂 😉 😊 😍 🤔 😅 😢 😮 😎 👍 👎 👏 🙏 💪 👋 🤝 🎉 🎂 ❤️ 🔥 ⭐ ✅ ❌ ⚠️ 📌 📎 📅 ⏰ 📧 📞 💡 🚀 ☕ 🌟 💯".split(" ");
  // Close the align / emoji pop-ups on a click elsewhere.
  $effect(() => {
    if (!pop) return;
    const off = (e: MouseEvent) => {
      if (!(e.target as HTMLElement).closest?.(".popwrap")) pop = null;
    };
    window.addEventListener("mousedown", off, true);
    return () => window.removeEventListener("mousedown", off, true);
  });

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

{#snippet tb(icon: string, title: string, run: () => void, on = false)}
  <button class:on {title} onmousedown={(e) => e.preventDefault()} onclick={run}><Icon name={icon} size={16} /></button>
{/snippet}

<div class="editor" class:compact>
  <!-- Gmail-like formatting bar (the same in the composer and the signature editor) -->
  <div class="tools" role="toolbar">
    {@render tb("undo", t("Undo (Ctrl+Z)"), () => exec("undo"))}
    {@render tb("redo", t("Redo (Ctrl+Y)"), () => exec("redo"))}
    <span class="div"></span>
    <select class="sel" title={t("Font")} onchange={(e) => exec("fontName", (e.target as HTMLSelectElement).value)}>
      {#each fonts as [label, value]}<option {value}>{label}</option>{/each}
    </select>
    <select class="sel small" title={t("Font size")} onchange={(e) => exec("fontSize", (e.target as HTMLSelectElement).value)}>
      {#each sizes as [label, value]}<option {value} selected={value === "3"}>{label}</option>{/each}
    </select>
    <span class="div"></span>
    {@render tb("bold", t("Bold (Ctrl+B)"), () => exec("bold"), fmt.bold)}
    {@render tb("italic", t("Italic (Ctrl+I)"), () => exec("italic"), fmt.italic)}
    {@render tb("underline", t("Underline (Ctrl+U)"), () => exec("underline"), fmt.underline)}
    {@render tb("textColor", t("Text color"), () => colorInput.click())}
    {@render tb("highlight", t("Highlight"), () => hiliteInput.click())}
    <span class="div"></span>
    <span class="popwrap">
      <button class:on={pop === "align"} title={t("Align")} onmousedown={(e) => e.preventDefault()} onclick={() => (pop = pop === "align" ? null : "align")}><Icon name={alignIcon} size={16} /><Icon name="chevronDown" size={10} /></button>
      {#if pop === "align"}
        <div class="pop row">
          {@render tb("alignLeft", t("Align left"), () => ((pop = null), exec("justifyLeft")), fmt.justifyLeft)}
          {@render tb("alignCenter", t("Center"), () => ((pop = null), exec("justifyCenter")), fmt.justifyCenter)}
          {@render tb("alignRight", t("Align right"), () => ((pop = null), exec("justifyRight")), fmt.justifyRight)}
        </div>
      {/if}
    </span>
    {@render tb("listNumber", t("Numbered list"), () => exec("insertOrderedList"), fmt.insertOrderedList)}
    {@render tb("listBullet", t("Bulleted list"), () => exec("insertUnorderedList"), fmt.insertUnorderedList)}
    {@render tb("outdent", t("Decrease indent"), () => exec("outdent"))}
    {@render tb("indent", t("Increase indent"), () => exec("indent"))}
    {@render tb("quote", t("Quote"), () => exec("formatBlock", "blockquote"))}
    {@render tb("strike", t("Strikethrough"), () => exec("strikeThrough"), fmt.strikeThrough)}
    {@render tb("clear", t("Remove formatting"), () => exec("removeFormat"))}
    <span class="div"></span>
    {@render tb("link", t("Insert link"), link)}
    {@render tb("image", t("Insert image"), () => imageInput.click())}
    <span class="popwrap">
      <button class="emo" class:on={pop === "emoji"} title={t("Insert emoji")} onmousedown={(e) => e.preventDefault()} onclick={() => (pop = pop === "emoji" ? null : "emoji")}>☺</button>
      {#if pop === "emoji"}
        <div class="pop grid">
          {#each EMOJI as em}<button onmousedown={(e) => e.preventDefault()} onclick={() => { pop = null; exec("insertText", em); }}>{em}</button>{/each}
        </div>
      {/if}
    </span>
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
      <!-- Gmail-style picture bar, under the picture -->
      <div class="imgbar" style:left="{Math.max(0, box.x)}px" style:top="{box.y + box.h + 8}px">
        {#if altEdit !== null}
          <!-- svelte-ignore a11y_autofocus -->
          <input class="alt" autofocus placeholder={t("Describe the picture")} bind:value={altEdit} onkeydown={(e) => { e.stopPropagation(); if (e.key === "Enter") { e.preventDefault(); saveAlt(); } if (e.key === "Escape") altEdit = null; }} />
          <button type="button" class="lnk" onclick={saveAlt}>{t("OK")}</button>
        {:else}
          <button type="button" class="lnk" onmousedown={(e) => e.preventDefault()} onclick={() => size("small")}>{t("Small")}</button><i>|</i>
          <button type="button" class="lnk" onmousedown={(e) => e.preventDefault()} onclick={() => size("medium")}>{t("Medium")}</button><i>|</i>
          <button type="button" class="lnk" onmousedown={(e) => e.preventDefault()} onclick={() => size("fit")}>{t("Best fit")}</button><i>|</i>
          <button type="button" class="lnk" onmousedown={(e) => e.preventDefault()} onclick={() => size("original")}>{t("Original size")}</button><i>|</i>
          <button type="button" class="lnk" onmousedown={(e) => e.preventDefault()} onclick={() => (altEdit = img?.alt ?? "")}>{t("Edit alt text")}</button><i>|</i>
          <button type="button" class="lnk del" onmousedown={(e) => e.preventDefault()} onclick={removeImg}>{t("Remove")}</button>
        {/if}
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
  .imgbar {
    white-space: nowrap;
    padding: 3px 8px;
  }
  .imgbar i {
    color: var(--text-3);
    font-style: normal;
    margin: 0 1px;
  }
  .imgbar .lnk {
    height: 26px;
    padding: 0 5px;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: var(--text-accent);
    font-size: 12.5px;
    font-weight: 600;
  }
  .imgbar .lnk:hover {
    background: var(--hover);
    text-decoration: underline;
  }
  .imgbar .del {
    color: var(--danger);
  }
  .imgbar .alt {
    width: 220px;
    height: 26px;
    border: 1px solid var(--border-strong);
    border-radius: 5px;
    background: var(--input-bg);
    color: var(--text);
    padding: 0 8px;
    font-size: 12.5px;
    outline: none;
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
  .popwrap {
    position: relative;
    display: inline-flex;
  }
  .popwrap > button {
    width: auto !important;
    padding: 0 4px;
    gap: 1px;
  }
  .emo {
    font-size: 16px;
  }
  .pop {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 10;
    padding: 4px;
    border-radius: 8px;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    box-shadow: var(--shadow-l);
  }
  .pop.row {
    display: flex;
    gap: 2px;
  }
  .pop.grid {
    display: grid;
    grid-template-columns: repeat(8, 30px);
    gap: 2px;
  }
  .pop.grid button {
    font-size: 17px;
  }
</style>
