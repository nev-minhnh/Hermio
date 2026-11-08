<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import { onMount } from "svelte";
  import { slide, fade } from "svelte/transition";
  import Icon from "../lib/Icon.svelte";
  import { api, type NpuModel, type NpuProgress, type RuntimeModel } from "../lib/api";
  import { store } from "../lib/store.svelte";
  import { fileSize } from "../lib/util";
  import { pickFolder } from "../lib/tauri";

  let { onchange }: { onchange?: () => void } = $props();

  let catalog = $state<NpuModel[]>([]);
  let busy = $state("");
  let defaultModel = $state("");
  let idle = $state(120);
  let pmode = $state("auto");
  let query = $state("");
  let onlyFits = $state(true);
  let onlySmall = $state(true);
  let showBrowse = $state(false);

  const npu = $derived(store.npu);
  const status = $derived(npu.status);
  const info = $derived(npu.info);
  const avail = $derived(npu.avail?.models ?? []);
  const ramGb = $derived(npu.avail?.ram_gb || info?.ram_gb || 0);
  const pulls = $derived(npu.pulls);
  const runtime = $derived(npu.runtime);
  const installing = $derived(npu.installing);
  const blocked = $derived(!!info && info.present && !info.supported && info.arch === "xdna1");
  const noNpu = $derived(!!info && !info.present);
  const intel = $derived(info?.vendor === "intel");
  const engine = $derived(intel ? "OpenVINO Model Server" : "FastFlowLM");
  const driverBad = $derived(!!info && info.present && (info.driver_missing || !info.driver_ok));
  const driver = $derived(npu.driver);
  const dirs = $derived(npu.dirs);
  const moving = $derived(npu.moving);
  const collectionUrl = $derived(
    npu.avail?.collection || (intel ? "https://huggingface.co/collections/OpenVINO/llms-optimized-for-npu-686e7f0bf7bc184bd71f8ba0" : "https://huggingface.co/collections/FastFlowLM/flm-models-xdna2"),
  );
  // Folder change dialog
  let dirAsk = $state<{ which: "runtime" | "models"; path: string } | null>(null);

  onMount(() => {
    (async () => {
      catalog = await api.npuCatalog();
      const raw = await api.settings();
      idle = Number(raw.npu_idle ?? 120) || 120;
      pmode = String(raw.npu_pmode ?? "auto");
      await refresh();
      store.loadDirs();
      // The slower check (`flm list`) runs after the page is already shown.
      if (store.npu.status?.installed) store.syncNpuModels();
    })();
  });

  async function refresh() {
    await store.loadNpu();
    store.loadDirs();
    try {
      const provs = await api.aiProviders();
      defaultModel = provs.find((p) => p.kind === "npu")?.model ?? "";
      onchange?.();
    } catch {}
  }

  /** Recommended models, only those offered for this NPU (AMD: curated list ∩ collection; Intel: marked by the backend). */
  const recommendedRows = $derived(
    intel
      ? avail.filter((m) => m.recommended)
      : catalog
          .map((c) => {
            const a = avail.find((x) => x.tag === c.tag);
            return a ? { ...a, name: c.name, note: c.note, recommended: c.recommended } : null;
          })
          .filter((x): x is RuntimeModel => !!x),
  );
  const curatedTags = $derived(new Set(recommendedRows.map((c) => c.tag)));
  const paramB = (p: string) => {
    const m = p.toLowerCase().match(/([\d.]+)\s*([bm])/);
    if (!m) return 99;
    return m[2] === "m" ? Number(m[1]) / 1000 : Number(m[1]);
  };
  const fits = (m: RuntimeModel) => !ramGb || !m.footprint || m.footprint <= ramGb * 0.6;
  const others = $derived(avail.filter((m) => !curatedTags.has(m.tag)));
  const browseRows = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return others
      .filter((m) => !onlyFits || fits(m))
      .filter((m) => !onlySmall || paramB(m.params) <= 5.5)
      .filter((m) => !q || `${m.tag} ${m.name} ${m.family} ${m.repo} ${m.labels.join(" ")}`.toLowerCase().includes(q));
  });

  const nameOf = (tag: string) => catalog.find((m) => m.tag === tag)?.name ?? avail.find((m) => m.tag === tag)?.name ?? tag;
  const gb = (n: number) => (n >= 1e9 ? `${(n / 1e9).toFixed(1)} GB` : `${Math.round(n / 1e6)} MB`);
  function eta(p: NpuProgress) {
    if (!p.rate || !p.total || p.rate < 1000) return "";
    const s = Math.max(0, (p.total - p.done) / p.rate);
    return s > 90 ? t("{n} min left", { n: Math.ceil(s / 60) }) : t("{n} s left", { n: Math.ceil(s) });
  }
  function phase(p: NpuProgress | null): string {
    if (!p) return "";
    switch (p.tag) {
      case "check":
        return t("Finding the latest {engine} release…", { engine });
      case "download":
        return p.total ? `${t("Downloading")} · ${fileSize(p.done)} / ${fileSize(p.total)}` : t("Downloading…");
      case "extract":
        return p.total ? t("Installing files {done}/{total}…", { done: p.done, total: p.total }) : t("Installing files…");
      case "done":
        return t("Finishing…");
      default:
        return t("Working…");
    }
  }

  async function remove(tag: string) {
    if (!confirm(t("Delete {name} from this PC?", { name: nameOf(tag) }))) return;
    busy = tag;
    try {
      await api.npuRemove(tag);
      await refresh();
    } catch (e) {
      store.error(String(e));
    } finally {
      busy = "";
    }
  }

  async function redownload(tag: string) {
    if (!confirm(t("Delete the files of {name} and download it again?", { name: nameOf(tag) }))) return;
    await store.pullNpu(tag, true);
    await refresh();
  }

  async function makeDefault(tag: string) {
    await api.aiSaveProvider({ id: "npu", kind: "npu", name: "On-device (NPU)", base_url: "", model: tag }, null);
    await store.set("ai_default", "npu");
    await refresh();
  }

  async function toggleServer() {
    busy = "server";
    try {
      if (status?.running) await api.npuStop();
      else await api.npuStart();
      await refresh();
    } catch (e) {
      store.error(String(e));
    } finally {
      busy = "";
    }
  }
  function drvPhase(p: NpuProgress | null): string {
    if (!p) return "";
    if (p.tag === "search") return t("Looking for the right driver…");
    if (p.tag === "download") return p.total ? `${t("Downloading the driver")} · ${fileSize(p.done)} / ${fileSize(p.total)}` : t("Downloading the driver…");
    if (p.tag === "install") return t("Installing — approve the Windows prompt…");
    return t("Working…");
  }
  async function changeDir(which: "runtime" | "models") {
    const path = await pickFolder(which === "runtime" ? t("Folder for the NPU runtime") : t("Folder for NPU models"));
    if (!path) return;
    dirAsk = { which, path };
  }
  async function applyDir(move: boolean) {
    if (!dirAsk) return;
    const { which, path } = dirAsk;
    dirAsk = null;
    await store.setNpuDirs(which === "runtime" ? path : null, which === "models" ? path : null, move);
  }
  async function resetDir(which: "runtime" | "models") {
    dirAsk = { which, path: "" };
  }
  async function recheck() {
    busy = "info";
    try {
      store.npu.info = await api.npuInfo(true);
      await refresh();
    } finally {
      busy = "";
    }
  }
</script>

<section class="npu">
  <div class="sec-head">
    <div>
      <h3><Icon name="sparkle" size={18} />{t("On-device AI (NPU)")}</h3>
      <p class="muted small">
        {t("Run a small model on this PC's NPU — private, offline, free. Mail never leaves this PC.")}
        {#if !info || !info.present}
          {t("Needs an AMD Ryzen AI 300-series (or newer) or an Intel Core Ultra processor.")}
        {:else if intel}
          {t("Intel Core Ultra NPU · runs with OpenVINO Model Server.")}
        {:else}
          {t("AMD Ryzen AI NPU · runs with FastFlowLM.")}
        {/if}
      </p>
    </div>
    {#if status?.installed && !noNpu}
      <button class="btn subtle" onclick={toggleServer} disabled={busy === "server"}>
        <span class="led" class:on={status.running}></span>
        {status.running ? t("Running · stop") : t("Stopped · start")}
      </button>
    {/if}
  </div>

  <div class="hw card" class:bad={blocked || noNpu || driverBad}>
    <div class="rt-ico hwico"><Icon name="flex" size={20} /></div>
    <div class="grow">
      {#if !info}
        <b>{t("Checking this PC's NPU…")}</b>
        <div class="muted small"><Icon name="sync" size={12} class="spin" /> {t("Reading the processor, NPU device and driver")}</div>
      {:else if noNpu}
        <b>{t("No NPU found")}</b>
        <div class="muted small">{[info.cpu, info.ram_gb ? `${Math.round(info.ram_gb)} GB RAM` : ""].filter(Boolean).join(" · ")}</div>
        <div class="hwnote small">{t("This PC has no AMD or Intel NPU, so on-device AI is turned off. Use a cloud model below instead.")}</div>
      {:else}
        <b>{info.device || t("NPU")}</b>
        <div class="muted small">
          {[info.cpu, info.arch === "xdna2" ? "XDNA 2" : info.arch === "xdna1" ? "XDNA 1" : info.arch === "intel" ? "Intel AI Boost" : "", info.driver ? t("driver {v}", { v: info.driver }) : "", info.ram_gb ? `${Math.round(info.ram_gb)} GB RAM` : ""].filter(Boolean).join(" · ")}
        </div>
        {#if info.driver_missing}
          <div class="hwnote small">{t("Windows has no working driver for the NPU.")}</div>
        {:else if !info.driver_ok}
          <div class="hwnote small">{t("NPU driver {v} is too old: version {min} or newer is needed.", { v: info.driver, min: info.driver_min })}</div>
        {:else if info.note === "driver-old"}
          <div class="hwnote small">{t("The NPU driver should be updated.")}</div>
        {:else if info.note}<div class="hwnote small">{t(info.note)}</div>{/if}
        {#if driver}
          <div class="prog" transition:slide={{ duration: 160 }}>
            <div class="track"><div class="fill" class:indet={driver.percent == null} style:width="{driver.percent ?? 30}%"></div></div>
            <span class="muted small">{drvPhase(driver)}{#if driver.percent != null}{` · ${Math.floor(driver.percent)}%`}{/if}</span>
          </div>
        {/if}
      {/if}
    </div>
    {#if info && driverBad && !blocked}
      <button class="btn primary" onclick={() => store.installDriver()} disabled={!!driver} title={intel ? t("Downloads the driver from Intel and installs it (needs administrator approval)") : t("Installs the driver from Windows Update (needs administrator approval)")}>
        {#if driver}<Icon name="sync" size={16} class="spin" />{t("Installing…")}{:else}<Icon name="download" size={16} />{info.driver_missing ? t("Install driver") : t("Update driver")}{/if}
      </button>
    {/if}
    {#if info}
      <span class="pill {info.supported ? 'ok' : 'warn'}">{info.supported ? t("Supported") : t("Not supported")}</span>
    {/if}
    <button class="icon-btn" title={t("Check again")} onclick={recheck} disabled={busy === "info"}><Icon name="sync" size={15} class={busy === "info" ? "spin" : ""} /></button>
  </div>

  {#if info && !noNpu}
  <div class="runtime card" class:busy={installing}>
    <div class="rt-ico"><Icon name="brain" size={20} /></div>
    <div class="grow">
      <b>{engine}</b>
      <div class="muted small">
        {#if installing}
          <span class="installing">{t("Installing…")}</span>
        {:else if !status}
          {t("Checking…")}
        {:else if status.installed}
          {t("Installed")}{status.version ? ` · ${status.version}` : ""} · {t("port {n}", { n: status.port })}
        {:else if intel}
          {t("NPU runtime · about 90 MB download, no admin rights needed")}
        {:else}
          {t("NPU runtime · about 35 MB download, no admin rights needed")}
        {/if}
      </div>
      {#if installing}
        <div class="prog" transition:slide={{ duration: 160 }}>
          <div class="track"><div class="fill" class:indet={runtime?.percent == null} style:width="{runtime?.percent ?? 30}%"></div></div>
          <span class="muted small">{phase(runtime)}{#if runtime?.percent != null}{` · ${Math.floor(runtime.percent)}%`}{/if}</span>
        </div>
      {/if}
    </div>
    {#if !status}
      <Icon name="sync" size={16} class="spin" />
    {:else if !status.installed}
      <button class="btn primary" onclick={() => store.installNpu()} disabled={installing || blocked || noNpu}>
        {#if installing}<Icon name="sync" size={16} class="spin" />{t("Installing…")}{:else}<Icon name="download" size={16} />{t("Install runtime")}{/if}
      </button>
    {:else}
      <button class="btn subtle" onclick={() => store.installNpu()} disabled={installing} title={t("Download the latest version")}>
        <Icon name="sync" size={16} class={installing ? "spin" : ""} />{installing ? t("Installing…") : t("Update")}
      </button>
    {/if}
  </div>

  <div class="dirs card">
    <div class="dir-row">
      <Icon name="folder" size={16} />
      <div class="grow ell">
        <div class="small"><b>{t("Runtime folder")}</b>{#if dirs?.runtime_size}<span class="muted"> · {gb(dirs.runtime_size)}</span>{/if}</div>
        <div class="muted small mono ell" title={dirs?.runtime}>{dirs?.runtime ?? "…"}</div>
      </div>
      {#if dirs && dirs.runtime !== dirs.runtime_default}<button class="link small" onclick={() => resetDir("runtime")} disabled={!!moving}>{t("Default")}</button>{/if}
      <button class="icon-btn" title={t("Open folder")} onclick={() => api.npuOpenDir("runtime").catch((e) => store.error(String(e)))}><Icon name="open" size={15} /></button>
      <button class="btn subtle" onclick={() => changeDir("runtime")} disabled={!!moving || installing}>{t("Change…")}</button>
    </div>
    <div class="dir-row">
      <Icon name="folder" size={16} />
      <div class="grow ell">
        <div class="small"><b>{t("Models folder")}</b>{#if dirs?.models_size}<span class="muted"> · {gb(dirs.models_size)}</span>{/if}</div>
        <div class="muted small mono ell" title={dirs?.models}>{dirs?.models ?? "…"}</div>
      </div>
      {#if dirs && dirs.models !== dirs.models_default}<button class="link small" onclick={() => resetDir("models")} disabled={!!moving}>{t("Default")}</button>{/if}
      <button class="icon-btn" title={t("Open folder")} onclick={() => api.npuOpenDir("models").catch((e) => store.error(String(e)))}><Icon name="open" size={15} /></button>
      <button class="btn subtle" onclick={() => changeDir("models")} disabled={!!moving || Object.keys(pulls).length > 0}>{t("Change…")}</button>
    </div>
    {#if dirs && (dirs.found.length || dirs.foreign.length)}
      <div class="scan small" transition:slide={{ duration: 160 }}>
        {#if dirs.found.length}
          <div><Icon name="check" size={13} /> {t("{n} model(s) found in this folder and ready to use: {list}", { n: dirs.found.length, list: dirs.found.join(", ") })}</div>
        {/if}
        {#each dirs.foreign as f}
          <div class="foreign"><Icon name="warning" size={13} /> {t("Can't be used by this PC's NPU runtime — {what}", { what: f })}</div>
        {/each}
      </div>
    {/if}
    {#if dirAsk}
      <div class="dir-ask" transition:slide={{ duration: 160 }}>
        <span class="grow small">
          {dirAsk.path ? t("Use {path}?", { path: dirAsk.path }) : t("Go back to the default folder?")}
          {t("Move the files that are already downloaded, or start empty there?")}
        </span>
        <button class="btn primary" onclick={() => applyDir(true)}>{t("Move files")}</button>
        <button class="btn subtle" onclick={() => applyDir(false)}>{t("Start empty")}</button>
        <button class="btn subtle" onclick={() => (dirAsk = null)}>{t("Cancel")}</button>
      </div>
    {/if}
    {#if moving}
      <div class="prog" transition:slide={{ duration: 160 }}>
        <div class="track"><div class="fill" class:indet={moving.percent == null} style:width="{moving.percent ?? 30}%"></div></div>
        <span class="muted small">{t("Moving files {done}/{total}…", { done: moving.done, total: moving.total || "?" })}</span>
      </div>
    {/if}
  </div>

  {#if intel && status?.installed}
    <p class="muted small" style="margin:2px 0 8px">{t("The first time a model loads, OpenVINO compiles it for the NPU — that can take a few minutes. Later starts are fast.")}</p>
  {/if}
  {#if status?.installed && !avail.length && npu.loading}
    <div class="muted small loading-models" transition:fade={{ duration: 120 }}><Icon name="sync" size={14} class="spin" />{t("Loading the model list from Hugging Face…")}</div>
  {/if}

  {#if recommendedRows.length}
    <div class="models">
      {#each recommendedRows as m (m.tag)}
        {@render modelRow(m)}
      {/each}
    </div>
  {/if}

  {#if status?.installed && others.length}
    <div class="browse-head">
      <button class="btn subtle" onclick={() => (showBrowse = !showBrowse)}>
        <Icon name={showBrowse ? "chevronUp" : "chevronDown"} size={15} />{t("Browse more models for {engine} ({n})", { engine, n: others.length })}
      </button>
      <button class="link small" onclick={() => api.openUrl(collectionUrl)}>{intel ? t("Source: OpenVINO NPU collection") : t("Source: FastFlowLM XDNA 2 collection")}</button>
    </div>
    {#if npu.avail?.newer.length}
      <div class="newer small" transition:slide={{ duration: 180 }}>
        <Icon name="sparkle" size={14} />
        <span class="grow">{t("{n} newer models need a newer runtime: {names}", { n: npu.avail.newer.length, names: npu.avail.newer.slice(0, 4).map((r) => r.replace(/-NPU2$/, "")).join(", ") })}</span>
        <button class="btn subtle" onclick={() => store.installNpu()} disabled={installing}>{t("Update")}</button>
      </div>
    {/if}
    {#if showBrowse}
      <div class="browse" transition:slide={{ duration: 180 }}>
        <div class="filters">
          <div class="search">
            <Icon name="search" size={15} />
            <input class="input" placeholder={t("Search models (name, family, vision, reasoning…)")} bind:value={query} />
          </div>
          <button class="chip" class:on={onlySmall} onclick={() => (onlySmall = !onlySmall)}>{t("Small (≤ 5B)")}</button>
          <button class="chip" class:on={onlyFits} onclick={() => (onlyFits = !onlyFits)}>{t("Fits this PC")}</button>

        </div>
        <div class="models">
          {#each browseRows as m (m.tag)}
            {@render modelRow(m)}
          {:else}
            <p class="muted small">{t("No models match. Try turning a filter off.")}</p>
          {/each}
        </div>
      </div>
    {/if}
  {/if}

  <div class="power card">
    <div class="pw-row">
      <Icon name="clock" size={16} />
      <span class="grow">{t("Release the model from the NPU after")}</span>
      <select class="input" bind:value={idle} onchange={() => api.setSetting("npu_idle", idle)}>
        <option value={30}>{t("30 seconds idle")}</option>
        <option value={60}>{t("1 minute idle")}</option>
        <option value={120}>{t("2 minutes idle")}</option>
        <option value={300}>{t("5 minutes idle")}</option>
        <option value={900}>{t("15 minutes idle")}</option>
      </select>
    </div>
    {#if !intel}
    <div class="pw-row">
      <Icon name="flex" size={16} />
      <span class="grow">{t("NPU power mode")}</span>
      <select class="input" bind:value={pmode} onchange={() => api.setSetting("npu_pmode", pmode)}>
        <option value="auto">{t("Auto — power saver on battery, balanced when plugged in")}</option>
        <option value="powersaver">{t("Power saver")}</option>
        <option value="balanced">{t("Balanced")}</option>
        <option value="performance">{t("Performance")}</option>
        <option value="turbo">{t("Turbo")}</option>
      </select>
    </div>
    {/if}
    <p class="muted small" style="margin:4px 0 0">
      {t("The runtime starts only when you ask the AI something, stops when Hermio goes to the tray or quits, and is killed by Windows even if Hermio crashes. On battery the model is released after at most 1 minute and background suggestions never wake the NPU.")}
    </p>
  </div>

  <p class="muted small credit">
    {t("Small models get a tuned prompt with worked examples, a tighter context and step-by-step summaries so they stay close to cloud quality for everyday mail tasks.")}
    {#if intel}
      Powered by <a href="https://github.com/openvinotoolkit/model_server" onclick={(e) => { e.preventDefault(); api.openUrl("https://github.com/openvinotoolkit/model_server"); }}>OpenVINO Model Server</a>.
    {:else}
      Powered by <a href="https://github.com/FastFlowLM/FastFlowLM" onclick={(e) => { e.preventDefault(); api.openUrl("https://github.com/FastFlowLM/FastFlowLM"); }}>FastFlowLM</a>.
    {/if}
  </p>
  {/if}
</section>

{#snippet modelRow(m: RuntimeModel)}
  {@const have = status?.models.includes(m.tag) || m.installed}
  {@const p = pulls[m.tag]}
  {@const pending = !!p}
  <div class="model card" class:have class:off={!m.compatible} transition:slide={{ duration: 160 }}>
    <div class="grow">
      <div class="mname">
        <b>{m.name}</b>
        <span class="tag mono">{m.tag}</span>
        {#if m.recommended}<span class="pill rec">{t("Recommended")}</span>{/if}
        {#if m.vision}<span class="pill">{t("Images")}</span>{/if}
        {#if m.audio}<span class="pill">{t("Audio")}</span>{/if}
        {#if m.thinking}<span class="pill">{t("Thinking")}</span>{/if}
        {#if m.tools}<span class="pill">{t("Tools")}</span>{/if}
        {#if !m.compatible}<span class="pill warn">{m.min_version ? t("Needs runtime {v}", { v: m.min_version }) : t("Other NPU")}</span>{/if}
        {#if !fits(m)}<span class="pill warn">{t("Too big for this PC")}</span>{/if}
        {#if have && defaultModel === m.tag}<span class="pill def">{t("Default")}</span>{/if}
      </div>
      <div class="muted small">
        {[m.params, m.size ? t("download {s}", { s: gb(m.size) }) : "", m.footprint ? t("needs ~{n} GB RAM", { n: m.footprint }) : "", m.note ? t(m.note) : m.labels.join(", ")].filter(Boolean).join(" · ")}
      </div>
      {#if p}
        <div class="prog" transition:slide={{ duration: 160 }}>
          <div class="track"><div class="fill" class:indet={p.percent === null} style:width="{p.percent ?? 30}%"></div></div>
          <span class="muted small ell" title={p.line}>
            {#if p.total}
              {[`${Math.floor(p.percent ?? 0)}%`, `${fileSize(p.done)} / ${fileSize(p.total)}`, p.rate ? `${fileSize(p.rate)}/s` : "", eta(p)].filter(Boolean).join(" · ")}
            {:else}
              {p.percent !== null ? `${Math.floor(p.percent)}% · ` : ""}{p.line}
            {/if}
          </span>
        </div>
      {/if}
    </div>
    {#if m.page}<button class="icon-btn" title={t("Model page on Hugging Face")} onclick={() => api.openUrl(m.page)}><Icon name="open" size={15} /></button>{/if}
    {#if p}
      <button class="btn subtle" onclick={() => api.npuCancelPull(m.tag)} disabled={!pending}>{t("Cancel")}</button>
    {:else if have}
      {#if defaultModel !== m.tag && m.chat}<button class="btn subtle" onclick={() => makeDefault(m.tag)}>{t("Use")}</button>{/if}
      {#if m.repo || m.url}<button class="icon-btn" title={t("Download again (replaces damaged or incomplete files)")} onclick={() => redownload(m.tag)} disabled={busy === m.tag || !status?.installed || installing}><Icon name="download" size={16} /></button>{/if}
      <button class="icon-btn" title={t("Delete")} onclick={() => remove(m.tag)} disabled={busy === m.tag}><Icon name="trash" size={16} /></button>
    {:else}
      <button class="btn" onclick={() => store.pullNpu(m.tag)} disabled={!status?.installed || !m.compatible || installing}><Icon name="download" size={16} />{t("Download")}</button>
    {/if}
  </div>
{/snippet}

<style>
  .dirs {
    padding: 8px 12px;
    margin-bottom: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .dir-row {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  .dir-row > :global(svg) {
    color: var(--text-3);
    flex: none;
  }
  .dir-row .grow {
    min-width: 0;
  }
  .mono {
    font-family: var(--mono, ui-monospace, monospace);
  }
  .ell {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .scan {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 6px 10px;
    border-radius: 8px;
    background: var(--surface-2, color-mix(in srgb, var(--text) 4%, transparent));
    color: var(--text-2);
  }
  .scan > div {
    display: flex;
    gap: 6px;
    align-items: flex-start;
    line-height: 1.4;
  }
  .scan :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  .scan .foreign {
    color: #b7791f;
  }
  .dir-ask {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding: 8px 10px;
    border-radius: 8px;
    background: color-mix(in srgb, var(--accent) 8%, transparent);
  }
  .npu {
    margin-bottom: 26px;
  }
  .sec-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 10px;
  }
  h3 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 4px;
    font-size: 1.02rem;
  }
  h3 :global(svg) {
    color: var(--ai);
  }
  .sec-head p {
    margin: 0;
    max-width: 720px;
    line-height: 1.5;
  }
  .led {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--text-3);
  }
  .led.on {
    background: var(--success);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--success) 25%, transparent);
  }
  .runtime {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    margin-bottom: 8px;
  }
  .rt-ico {
    width: 38px;
    height: 38px;
    border-radius: 10px;
    display: grid;
    place-items: center;
    color: white;
    background: var(--brand-grad);
    flex: none;
  }
  .models {
    display: flex;
    flex-direction: column;
    gap: 6px;
    transition: opacity var(--dur);
  }
  .models.disabled {
    opacity: 0.6;
  }
  .model {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 10px 9px 14px;
    transition:
      border-color var(--dur),
      box-shadow var(--dur);
  }
  .model.have {
    border-color: color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .mname {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
  }
  .tag {
    font-size: 0.76rem;
    color: var(--text-3);
  }
  .mono {
    font-family: Consolas, monospace;
  }
  .pill {
    font-size: 0.7rem;
    padding: 1px 7px;
    border-radius: 10px;
    background: var(--surface-3);
    color: var(--text-2);
    font-weight: 600;
  }
  .pill.rec {
    background: color-mix(in srgb, var(--ai) 16%, transparent);
    color: var(--ai);
  }
  .pill.def {
    background: var(--selected);
    color: var(--text-accent);
  }
  .prog {
    display: flex;
    flex-direction: column;
    gap: 3px;
    margin-top: 6px;
    max-width: 520px;
  }
  .track {
    height: 6px;
    border-radius: 3px;
    background: var(--surface-3);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    border-radius: 3px;
    background: linear-gradient(90deg, var(--accent), var(--ai));
    transition: width 200ms linear;
  }
  .fill.indet {
    animation: indet 1.2s ease-in-out infinite;
  }
  @keyframes indet {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(340%);
    }
  }
  .ell {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .power {
    margin-top: 10px;
    padding: 10px 14px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .pw-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .pw-row select {
    width: auto;
    max-width: 420px;
  }
  .hw {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    margin-bottom: 8px;
  }
  .hw.bad {
    border-color: color-mix(in srgb, var(--warning-fg) 40%, transparent);
  }
  .hwico {
    background: linear-gradient(135deg, #e04b2b, #f08a24);
  }
  .hwnote {
    margin-top: 4px;
    color: var(--warning-fg);
  }
  .pill.ok {
    background: color-mix(in srgb, var(--success) 16%, transparent);
    color: var(--success);
  }
  .runtime.busy {
    border-color: color-mix(in srgb, var(--accent) 45%, transparent);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 10%, transparent);
  }
  .installing {
    color: var(--text-accent);
    font-weight: 600;
  }
  .loading-models {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 6px 2px;
  }
  .newer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 6px 6px 12px;
    margin-bottom: 8px;
    border-radius: 10px;
    background: var(--selected);
  }
  .link {
    border: none;
    background: none;
    color: var(--text-accent);
    padding: 0;
  }
  .model.off {
    opacity: 0.6;
  }
  .pill.warn {
    background: var(--warning-bg);
    color: var(--warning-fg);
  }
  .browse-head {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 10px 0 6px;
  }
  .browse {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .filters {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
  }
  .search {
    flex: 1;
    min-width: 220px;
    display: flex;
    align-items: center;
    gap: 6px;
    padding-left: 10px;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    background: var(--input-bg);
  }
  .search .input {
    border: none;
    background: transparent;
    box-shadow: none;
  }
  .chip {
    height: 28px;
    padding: 0 12px;
    border-radius: 14px;
    border: 1px solid var(--border-strong);
    background: transparent;
    font-size: 0.8rem;
    font-weight: 600;
    color: var(--text-2);
  }
  .chip.on {
    background: var(--selected);
    border-color: color-mix(in srgb, var(--accent) 50%, transparent);
    color: var(--text-accent);
  }
  .credit {
    margin-top: 10px;
    line-height: 1.5;
    max-width: 760px;
  }
</style>
