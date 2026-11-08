<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import { onMount, untrack } from "svelte";
  import { fade, scale } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import Icon from "../lib/Icon.svelte";
  import { store } from "../lib/store.svelte";
  import { api, type Attachment, type AttachmentData } from "../lib/api";
  import { fileSize } from "../lib/util";
  import { pickSavePath } from "../lib/tauri";

  let {
    messageId,
    attachments,
    index,
    onclose,
  }: { messageId: number; attachments: Attachment[]; index: number; onclose: () => void } = $props();

  let cur = $state(0);
  let data = $state<AttachmentData | null>(null);
  let error = $state("");
  let url = $state("");
  let text = $state<string | null>(null);
  let zoom = $state(1);

  const att = $derived(attachments[cur]);
  const kind = $derived.by(() => {
    if (!data) return "";
    const n = data.name.toLowerCase();
    const m = data.mime.toLowerCase();
    if (m.startsWith("image/") || /\.(png|jpe?g|gif|webp|bmp|svg|ico|avif)$/.test(n)) return "image";
    if (m === "application/pdf" || n.endsWith(".pdf")) return "pdf";
    if (m.startsWith("video/") || /\.(mp4|webm|mov)$/.test(n)) return "video";
    if (m.startsWith("audio/") || /\.(mp3|wav|ogg|m4a)$/.test(n)) return "audio";
    if (m === "text/html" || /\.html?$/.test(n)) return "html";
    if (data.text) return "doc";
    if (m.startsWith("text/") || /\.(txt|csv|tsv|log|md|json|xml|ics|vcf|ya?ml|ini|cfg|js|ts|py|rs|c|cpp|h|java|cs|sql|sh|bat|ps1)$/.test(n)) return "text";
    return "none";
  });

  onMount(() => {
    cur = index;
  });

  $effect(() => {
    const a = attachments[cur];
    if (!a) return;
    untrack(() => load(a.index));
  });

  function revoke() {
    if (url) URL.revokeObjectURL(url);
    url = "";
  }

  async function load(i: number) {
    revoke();
    data = null;
    text = null;
    error = "";
    try {
      const d = await api.attachmentData(messageId, i);
      if (attachments[cur]?.index !== i) return;
      data = d;
      const bin = atob(d.data);
      const bytes = new Uint8Array(bin.length);
      for (let k = 0; k < bin.length; k++) bytes[k] = bin.charCodeAt(k);
      const type = d.mime === "application/octet-stream" && d.name.toLowerCase().endsWith(".pdf") ? "application/pdf" : d.mime;
      url = URL.createObjectURL(new Blob([bytes], { type }));
      if (d.text) text = d.text;
      else if (kind === "text") text = new TextDecoder("utf-8").decode(bytes);
    } catch (e) {
      error = String(e);
    }
  }

  $effect(() => () => untrack(revoke));

  async function save() {
    if (!att) return;
    const path = await pickSavePath(att.name);
    if (!path) return;
    try {
      await api.saveAttachment(messageId, att.index, path);
      store.success(t("File saved"));
    } catch (e) {
      store.error(String(e));
    }
  }
  function openExternal() {
    if (att) api.openAttachment(messageId, att.index).catch((e) => store.error(String(e)));
  }
  const setZoom = (z: number) => (zoom = Math.min(4, Math.max(0.25, Math.round(z * 100) / 100)));

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
    else if (e.key === "ArrowRight" && attachments.length > 1) cur = (cur + 1) % attachments.length;
    else if (e.key === "ArrowLeft" && attachments.length > 1) cur = (cur - 1 + attachments.length) % attachments.length;
    else if ((e.ctrlKey || e.metaKey) && (e.key === "+" || e.key === "=")) {
      e.preventDefault();
      setZoom(zoom + 0.1);
    } else if ((e.ctrlKey || e.metaKey) && e.key === "-") {
      e.preventDefault();
      setZoom(zoom - 0.1);
    } else if ((e.ctrlKey || e.metaKey) && e.key === "0") {
      e.preventDefault();
      setZoom(1);
    }
  }
  function onWheel(e: WheelEvent) {
    if (!e.ctrlKey) return;
    e.preventDefault();
    setZoom(zoom * (e.deltaY < 0 ? 1.1 : 1 / 1.1));
  }
</script>

<svelte:window onkeydown={onKey} />

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" transition:fade={{ duration: 160 }} onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="viewer" transition:scale={{ start: 0.96, duration: 200, easing: cubicOut }}>
    <header>
      <Icon name="attach" size={16} />
      <div class="name">
        <b title={att?.name}>{att?.name}</b>
        <span class="muted small">{att ? fileSize(att.size) : ""}{attachments.length > 1 ? ` · ${cur + 1}/${attachments.length}` : ""}</span>
      </div>
      {#if attachments.length > 1}
        <button class="icon-btn" title={t("Previous")} onclick={() => (cur = (cur - 1 + attachments.length) % attachments.length)}><Icon name="chevronLeft" /></button>
        <button class="icon-btn" title={t("Next")} onclick={() => (cur = (cur + 1) % attachments.length)}><Icon name="chevronRight" /></button>
        <span class="sep"></span>
      {/if}
      {#if kind !== "none" && kind !== "audio"}
        <button class="icon-btn" title={t("Zoom out (Ctrl -)")} onclick={() => setZoom(zoom - 0.1)}><Icon name="zoomOut" /></button>
        <button class="zoomval" title={t("Reset zoom (Ctrl 0)")} onclick={() => setZoom(1)}>{Math.round(zoom * 100)}%</button>
        <button class="icon-btn" title={t("Zoom in (Ctrl +)")} onclick={() => setZoom(zoom + 0.1)}><Icon name="zoomIn" /></button>
        <span class="sep"></span>
      {/if}
      <button class="btn subtle" onclick={save}><Icon name="download" size={16} />{t("Download")}</button>
      <button class="btn subtle" onclick={openExternal}><Icon name="open" size={16} />{t("Open with app")}</button>
      <button class="icon-btn" title={t("Close")} onclick={onclose}><Icon name="close" /></button>
    </header>
    <div class="stage" onwheel={onWheel}>
      {#if error}
        <div class="msg"><Icon name="warning" size={28} /><span>{error}</span></div>
      {:else if !data}
        <div class="msg"><Icon name="sync" size={24} class="spin" /><span class="muted">{t("Loading preview…")}</span></div>
      {:else if kind === "image"}
        <div class="imgwrap"><img src={url} alt={data.name} style:width="{zoom * 100}%" style:max-width={zoom <= 1 ? "100%" : "none"} /></div>
      {:else if kind === "pdf"}
        {#key zoom}<iframe class="fill" title={data.name} src="{url}#zoom={Math.round(zoom * 100)}&toolbar=1"></iframe>{/key}
      {:else if kind === "video"}
        <!-- svelte-ignore a11y_media_has_caption -->
        <video src={url} controls style:zoom={zoom}></video>
      {:else if kind === "audio"}
        <audio src={url} controls></audio>
      {:else if kind === "html"}
        <iframe class="fill white" title={data.name} sandbox="" src={url} style:zoom={zoom}></iframe>
      {:else if text !== null}
        <pre class="text selectable" style:font-size="{13 * zoom}px">{text}</pre>
      {:else}
        <div class="msg">
          <Icon name="attach" size={32} />
          <b>{t("No preview for this file type")}</b>
          <span class="muted small">{data.mime}</span>
          <div class="row" style="gap:8px;margin-top:6px">
            <button class="btn primary" onclick={openExternal}><Icon name="open" size={16} />{t("Open with app")}</button>
            <button class="btn" onclick={save}><Icon name="download" size={16} />{t("Download")}</button>
          </div>
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 80;
    background: rgba(0, 0, 0, 0.45);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 28px;
  }
  .viewer {
    width: min(1180px, 100%);
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border-radius: 14px;
    box-shadow: var(--shadow-l);
    overflow: hidden;
  }
  header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px 8px 16px;
    border-bottom: 1px solid var(--border);
  }
  .name {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .name b {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sep {
    width: 1px;
    height: 20px;
    background: var(--border-strong);
    margin: 0 4px;
  }
  .zoomval {
    min-width: 52px;
    height: 30px;
    border: none;
    border-radius: 6px;
    background: transparent;
    font-variant-numeric: tabular-nums;
    color: var(--text-2);
  }
  .zoomval:hover {
    background: var(--hover);
  }
  .stage {
    flex: 1;
    min-height: 0;
    overflow: auto;
    background: var(--bg-canvas);
    display: flex;
    overscroll-behavior: contain;
  }
  .imgwrap {
    margin: auto;
    padding: 16px;
    display: flex;
    justify-content: center;
    min-width: 0;
  }
  .imgwrap img {
    display: block;
    height: auto;
    box-shadow: var(--shadow);
    background: repeating-conic-gradient(#ddd 0 25%, #fff 0 50%) 0 0 / 16px 16px;
  }
  .fill {
    flex: 1;
    width: 100%;
    height: 100%;
    border: none;
  }
  .fill.white {
    background: #fff;
  }
  video {
    margin: auto;
    max-width: 100%;
    max-height: 100%;
  }
  audio {
    margin: auto;
  }
  .text {
    margin: 0;
    padding: 18px 22px;
    width: 100%;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    font-family: Consolas, "Cascadia Mono", monospace;
    line-height: 1.5;
    background: var(--surface);
    align-self: flex-start;
    min-height: 100%;
  }
  .msg {
    margin: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    color: var(--text-2);
    text-align: center;
    padding: 24px;
  }
</style>
