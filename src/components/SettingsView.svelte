<script lang="ts">
  import { slide, fly } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { pill } from "../lib/motion";
  import { t, locale, i18n, languages, setLang, type Lang } from "../lib/i18n.svelte";
  import { onMount } from "svelte";
  import Icon from "../lib/Icon.svelte";
  import Logo from "../lib/Logo.svelte";
  import { store } from "../lib/store.svelte";
  import { api, type Signature } from "../lib/api";
  import AccountEditor from "./AccountEditor.svelte";
  import AddAccount from "./AddAccount.svelte";
  import RulesSettings from "./RulesSettings.svelte";
  import AiSettings from "./AiSettings.svelte";
  import RichEditor from "./RichEditor.svelte";
  import { pickIcsFile, pickConfigSave, pickConfigOpen, pickFolder, pickBackupSave, pickBackupOpen } from "../lib/tauri";
  import type { Calendar } from "../lib/api";

  const tabs = [
    { id: "accounts", icon: "person", label: t("Accounts") },
    { id: "rules", icon: "filter", label: t("Groups & rules") },
    { id: "ai", icon: "sparkle", label: t("AI assistant") },
    { id: "signatures", icon: "signature", label: t("Signatures") },
    { id: "general", icon: "layout", label: t("Startup & tray") },
    { id: "google", icon: "key", label: t("Google sign-in") },
    { id: "appearance", icon: "palette", label: t("Appearance") },
    { id: "mail", icon: "mail", label: t("Mail") },
    { id: "calendar", icon: "calendar", label: t("Calendar") },
    { id: "language", icon: "language", label: t("Language") },
    { id: "backup", icon: "download", label: t("Backup & transfer") },
    { id: "about", icon: "mail", label: t("About") },
  ];
  const tab = $derived(store.settingsTab);
  const accountTab = $derived(tab.startsWith("account:") ? Number(tab.slice(8)) : 0);

  // ---- google ----
  let clientId = $state("");
  let clientSecret = $state("");
  let hasSecret = $state(false);
  let autostart = $state(false);
  let calendars = $state<Calendar[]>([]);
  let icsUrl = $state("");
  let icsName = $state("");
  let icsBusy = $state(false);
  onMount(async () => {
    const g = await api.googleConfig();
    clientId = g.client_id;
    hasSecret = g.has_secret;
    autostart = await api.getAutostart().catch(() => false);
    calendars = await api.calendars();
  });

  async function toggleAutostart() {
    try {
      await api.setAutostart(!autostart);
      autostart = !autostart;
      store.success(autostart ? t("Hermio will start with Windows, hidden in the tray") : t("Start with Windows turned off"));
    } catch (e) {
      store.error(String(e));
    }
  }

  async function addIcs() {
    if (!icsUrl.trim()) return;
    icsBusy = true;
    try {
      const n = await api.addIcs(icsName || t("Outlook calendar"), icsUrl.trim(), "#0078d4");
      store.success(t("Calendar added with {n} events", { n }));
      icsUrl = "";
      icsName = "";
      calendars = await api.calendars();
    } catch (e) {
      store.error(String(e));
      calendars = await api.calendars();
    } finally {
      icsBusy = false;
    }
  }

  async function importIcs() {
    const path = await pickIcsFile();
    if (!path) return;
    try {
      const n = await api.importIcs(path, 1);
      store.success(t("Imported {n} events into “My calendar”", { n }));
    } catch (e) {
      store.error(String(e));
    }
  }

  async function removeCal(c: Calendar) {
    if (!confirm(t("Remove calendar “{name}”?", { name: c.name }))) return;
    await api.deleteCalendar(c.id);
    calendars = await api.calendars();
  }

  // ---- spaces ----
  async function addSpace() {
    const name = prompt(t("Space name (e.g. Work, Study, Personal)"));
    if (!name?.trim()) return;
    await api.saveSpace({ name: name.trim(), color: accents[store.spaces.length % accents.length] });
    await store.loadSpaces();
    store.success(t("Space “{name}” created", { name: name.trim() }));
  }
  async function renameSpace(id: number, old: string, color: string) {
    const name = prompt(t("Rename space"), old);
    if (!name?.trim()) return;
    await api.saveSpace({ id, name: name.trim(), color });
    await store.loadSpaces();
    store.success(t("Space renamed"));
  }
  async function removeSpace(id: number) {
    if (!confirm(t("Delete this space? Its accounts are not removed."))) return;
    await api.deleteSpace(id);
    await store.loadSpaces();
    await store.loadAccounts();
    store.success(t("Space deleted"));
    if (store.settings.space === id) store.setSpace(0);
  }
  async function accountSpace(accountId: number, v: string) {
    await api.setAccountSpace(accountId, v ? Number(v) : null);
    await store.loadAccounts();
    await store.loadFolders();
  }
  async function saveGoogle() {
    await api.setGoogleConfig(clientId, clientSecret || null);
    if (clientSecret) hasSecret = true;
    clientSecret = "";
    store.success(t("Google settings saved"));
  }

  // ---- signatures ----
  let sigEdit = $state<Signature | null>(null);
  let sigPrompt = $state("");
  let sigBusy = $state(false);
  async function aiSignature() {
    if (!sigEdit || sigBusy) return;
    const provider = store.settings.ai_default;
    if (!provider) {
      store.error(t("No AI provider yet. Open Settings → AI assistant to add one."));
      return;
    }
    const me = store.accounts[0];
    const ask = sigPrompt.trim() || t("A clean professional signature for {name} <{email}>", { name: me?.display_name || me?.name || "", email: me?.email ?? "" });
    sigBusy = true;
    try {
      const r = await api.aiRun({ provider_id: provider, thinking: false, messages: [{ role: "user", content: ask }], scope: { kind: "all", ids: [] }, days: 0, task: "signature", lang: i18n.lang });
      const html = r.text.replace(/<script[\s\S]*?<\/script>/gi, "").replace(/\son\w+="[^"]*"/gi, "");
      sigEdit = { ...sigEdit, html };
      // re-mount the editor with the new content
      const keep = sigEdit;
      sigEdit = null;
      await Promise.resolve();
      sigEdit = keep;
    } catch (e) {
      store.error(String(e));
    } finally {
      sigBusy = false;
    }
  }
  let sigEl = $state<HTMLDivElement>();
  async function saveSig() {
    if (!sigEdit) return;
    const id = await api.saveSignature({ ...sigEdit, html: sigEl?.innerHTML ?? sigEdit.html });
    await store.loadSignatures();
    sigEdit = store.signatures.find((s) => s.id === id) ?? null;
    store.success(t("Signature saved"));
  }
  async function deleteSig(s: Signature) {
    if (!confirm(t("Delete signature “{name}”?", { name: s.name }))) return;
    await api.deleteSignature(s.id);
    await store.loadSignatures();
    await store.loadAccounts();
    if (sigEdit?.id === s.id) sigEdit = null;
  }
  async function moveAccount(i: number, d: number) {
    const ids = store.accounts.map((a) => a.id);
    const j = i + d;
    if (j < 0 || j >= ids.length) return;
    [ids[i], ids[j]] = [ids[j], ids[i]];
    await api.reorderAccounts(ids);
    await store.loadAccounts();
  }

  async function testNotification() {
    try {
      await api.testNotification();
      store.info(t("Sent. If nothing appeared, check Windows Settings → System → Notifications (Hermio on, Do not disturb off)."));
    } catch (e) {
      store.error(String(e));
    }
  }
  const accents = ["#1d4fe0", "#3d7bff", "#0a2a8f", "#5b5fc7", "#8764b8", "#c239b3", "#d13438", "#ca5010", "#107c10", "#038387"];
  const customAccent = $derived(!accents.includes(store.settings.accent));

  // ----- backup -----
  let exSecrets = $state(true);
  let exChats = $state(false);
  let backupBusy = $state(false);
  let importReport = $state<Awaited<ReturnType<typeof api.configImport>> | null>(null);
  async function doExport() {
    const path = await pickConfigSave(`Hermio-${__APP_VERSION__}-settings-${new Date().toISOString().slice(0, 10)}.json`);
    if (!path) return;
    backupBusy = true;
    try {
      await api.configExport(path, exSecrets, exChats);
      store.success(t("Settings exported"));
    } catch (e) {
      store.error(String(e));
    } finally {
      backupBusy = false;
    }
  }
  // ----- resource use (About) -----
  type Usage = Awaited<ReturnType<typeof api.resourceUsage>>;
  let usage = $state<Usage | null>(null);
  let cpuPct = $state(0);
  let lastCpu: [number, number] | null = null;
  const mb = (b: number) => `${Math.round(b / 1048576)} MB`;
  const kindLabel = (k: string) => ({ browser: t("manager"), renderer: t("page"), gpu: t("graphics (GPU)"), utility: t("service") } as Record<string, string>)[k] ?? k;
  $effect(() => {
    if (tab !== "about") return;
    let alive = true;
    const tick = async () => {
      const u = await api.resourceUsage().catch(() => null);
      if (!alive || !u) return;
      const now = performance.now();
      if (lastCpu) cpuPct = Math.max(0, Math.round(((u.total_cpu_ms - lastCpu[0]) / Math.max(1, now - lastCpu[1]) / (navigator.hardwareConcurrency || 1)) * 100));
      lastCpu = [u.total_cpu_ms, now];
      usage = u;
    };
    tick();
    const h = setInterval(tick, 2000);
    return () => {
      alive = false;
      clearInterval(h);
      lastCpu = null;
    };
  });
  function copyUsage() {
    if (!usage) return;
    const lines = [`Hermio ${__APP_VERSION__} — private ${mb(usage.total_private)}, working set ${mb(usage.total_working_set)}, CPU now ${cpuPct}%`, ...usage.processes.map((p) => `${p.name} #${p.pid}${p.kind ? ` (${p.kind})` : ""}: private ${mb(p.private)}, working set ${mb(p.working_set)}, CPU ${(p.cpu_ms / 1000).toFixed(1)} s`)];
    navigator.clipboard.writeText(lines.join("\n")).then(() => store.success(t("Copied to the clipboard")));
  }

  // ----- moving all data -----
  // The job lives in the store: it keeps running (and its progress stays) while you go elsewhere.
  const dataBusy = $derived(!!store.dataJob);
  const dataProg = $derived(store.dataJob?.prog ?? null);
  async function copyFromOld() {
    const dir = await pickFolder(t("Choose the folder of the older copy (with Hermio.exe or Nelo.exe) or its data folder"));
    if (!dir) return;
    const r = await store.runDataJob("restore", () => api.dataStageRestore(dir));
    if (r) store.dataStaged = r;
  }
  async function restoreBackup() {
    const f = await pickBackupOpen();
    if (!f) return;
    const r = await store.runDataJob("restore", () => api.dataStageRestore(f));
    if (r) store.dataStaged = r;
  }
  async function makeBackup() {
    const f = await pickBackupSave(`Hermio-${__APP_VERSION__}-backup-${new Date().toISOString().slice(0, 10)}.zip`);
    if (!f) return;
    const n = await store.runDataJob("backup", () => api.dataBackup(f, true));
    if (n !== undefined) store.success(t("Backup saved"));
  }
  async function doImport() {
    const path = await pickConfigOpen();
    if (!path) return;
    if (!confirm(t("Import settings from this file into Hermio?"))) return;
    backupBusy = true;
    try {
      importReport = await api.configImport(path);
      api.syncNow(null).catch(() => {});
    } catch (e) {
      store.error(String(e));
    } finally {
      backupBusy = false;
    }
  }
  const ago = (ts: number) => (ts ? new Date(ts * 1000).toLocaleString(locale()) : t("not synced yet"));
</script>

<div class="settings">
  <nav class="tabs">
    <h1>{t("Settings")}</h1>
    {#each tabs as tb}
      <button class:on={tab === tb.id || (tb.id === "accounts" && (tab === "add" || tab.startsWith("account:")))} onclick={() => (store.settingsTab = tb.id)}>
        <Icon name={tb.icon} size={18} />{tb.label}
      </button>
    {/each}
  </nav>

  <section class="panel scroll">
    {#key tab}
    <div class="inner tabanim">
      {#if tab === "accounts"}
        <div class="head">
          <h2>{t("Accounts")}</h2>
          <button class="btn primary" onclick={() => (store.settingsTab = "add")}><Icon name="plus" size={16} />{t("Add account")}</button>
        </div>
        {#if !store.accounts.length}
          <p class="muted">{t("No accounts yet.")}</p>
        {/if}
        <div class="spaces-box">
          <div class="row">
            <b class="grow">{t("Space")}</b>
            <button class="btn subtle" onclick={addSpace}><Icon name="plus" size={16} />{t("New space")}</button>
          </div>
          <p class="muted small">{t("Group mailboxes by purpose (Work, Study, Personal…). Each space has its own unified inbox; switch spaces right in the sidebar.")}</p>
          <div class="chips">
            {#each store.spaces as sp (sp.id)}
              <span class="chip" style:--c={sp.color}>
                <i style:background={sp.color}></i>{sp.name}
                <span class="muted small">{store.accounts.filter((a) => a.space_id === sp.id).length}</span>
                <button onclick={() => renameSpace(sp.id, sp.name, sp.color)} title={t("Rename")}><Icon name="compose" size={12} /></button>
                <button onclick={() => removeSpace(sp.id)} title={t("Delete")}><Icon name="close" size={12} /></button>
              </span>
            {:else}
              <span class="muted small">{t("No spaces yet.")}</span>
            {/each}
          </div>
        </div>
        <div class="list">
          {#each store.accounts as a, i (a.id)}
            <div class="acc card">
              <span class="dot" style:background={a.color}></span>
              <button class="acc-main" onclick={() => (store.settingsTab = `account:${a.id}`)}>
                <b>{a.name || a.email}</b>
                <span class="muted small">
                  {a.email} · {a.protocol.toUpperCase()} · {a.auth === "oauth_google" ? "Google OAuth" : t("Password")}
                  {#if a.protocol === "pop3"} · {a.pop3_dir || t("default folder")}{/if}
                </span>
                <span class="small" class:errtxt={!!a.last_error} class:muted={!a.last_error}>
                  {a.last_error ? t("Error: ") + a.last_error : t("Last sync: ") + ago(a.last_sync)}
                </span>
              </button>
              {#if store.spaces.length}
                <select class="input spsel" value={a.space_id ?? ""} onchange={(e) => accountSpace(a.id, (e.target as HTMLSelectElement).value)} title={t("Space")}>
                  <option value="">—</option>
                  {#each store.spaces as sp}<option value={sp.id}>{sp.name}</option>{/each}
                </select>
              {/if}
              <button class="icon-btn" title={t("Up")} onclick={() => moveAccount(i, -1)} disabled={i === 0}><Icon name="chevronUp" size={16} /></button>
              <button class="icon-btn" title={t("Down")} onclick={() => moveAccount(i, 1)} disabled={i === store.accounts.length - 1}><Icon name="chevronDown" size={16} /></button>
              <button class="icon-btn" title={t("Edit")} onclick={() => (store.settingsTab = `account:${a.id}`)}><Icon name="settings" size={16} /></button>
            </div>
          {/each}
        </div>
      {:else if tab === "add"}
        <button class="btn subtle back" onclick={() => (store.settingsTab = "accounts")}><Icon name="chevronLeft" size={16} />{t("All accounts")}</button>
        <AddAccount />
      {:else if tab === "rules"}
        <RulesSettings />
      {:else if tab === "ai"}
        <AiSettings />
      {:else if tab === "general"}
        <h2>{t("Startup & system tray")}</h2>
        <div class="form">
          <button class="toggle" onclick={toggleAutostart}>
            <span class="switch" class:on={autostart}></span>
            <span><b>{t("Start with Windows")}</b><br /><span class="muted small">{t("Hermio runs hidden in the system tray (taskbar corner) without opening a window; mail keeps syncing and notifying.")}</span></span>
          </button>
          <button class="toggle" onclick={() => store.set("close_to_tray", !store.settings.close_to_tray)}>
            <span class="switch" class:on={store.settings.close_to_tray}></span>
            <span><b>{t("Closing the window minimizes to the tray")}</b><br /><span class="muted small">{t("Clicking ✕ only hides the window. Quit by right-clicking the Hermio tray icon → Quit.")}</span></span>
          </button>
          <div class="row"><button class="btn danger" onclick={() => api.quit()}><Icon name="close" size={16} />{t("Quit Hermio now")}</button></div>
        </div>
      {:else if accountTab}
        <button class="btn subtle back" onclick={() => (store.settingsTab = "accounts")}><Icon name="chevronLeft" size={16} />{t("All accounts")}</button>
        {#key accountTab}<AccountEditor accountId={accountTab} />{/key}
      {:else if tab === "google"}
        <h2>{t("Google sign-in (OAuth 2.0)")}</h2>
        <p class="muted lead">
          {t("To sign in to Gmail / Google Workspace securely and sync Google Calendar, Hermio needs your own OAuth Client (free). It is stored only on this PC; the Client Secret lives in Windows Credential Manager.")}
        </p>
        <ol class="steps">
          <li>{t("Open")} <button class="lnk" onclick={() => api.openUrl("https://console.cloud.google.com/projectcreate")}>Google Cloud Console</button> {t("and create a project.")}</li>
          <li>{t("On")} <button class="lnk" onclick={() => api.openUrl("https://console.cloud.google.com/apis/library/gmail.googleapis.com")}>Gmail API</button> {t("and")} <button class="lnk" onclick={() => api.openUrl("https://console.cloud.google.com/apis/library/calendar-json.googleapis.com")}>Google Calendar API</button>.</li>
          <li>{t("Go to")} <b>Google Auth Platform → Branding/Audience</b>{t(": choose")} <i>External</i>{t(", add your email to")} <i>Test users</i>.</li>
          <li>{t("Go to")} <b>Clients → Create client</b>{t(", choose type")} <b>Desktop app</b>{t(", then paste the Client ID and Client Secret here.")}</li>
          <li>{t("After that, adding an account is just a click on")} <b>{t("Continue with Google")}</b> {t("— the Google sign-in window opens right inside Hermio.")}</li>
        </ol>
        <div class="form">
          <label class="field"><span>Client ID</span><input class="input mono" bind:value={clientId} placeholder="xxxxxxxx.apps.googleusercontent.com" /></label>
          <label class="field">
            <span>Client Secret {hasSecret ? t("(saved – type to replace)") : ""}</span>
            <input class="input mono" type="password" bind:value={clientSecret} placeholder={hasSecret ? "••••••••" : "GOCSPX-…"} />
          </label>
          <div class="row"><button class="btn primary" onclick={saveGoogle}>{t("Save")}</button></div>
          <p class="muted small">
            {t("Don't want to create an OAuth Client? You can still add Gmail with an")} <b>App Password</b> {t("(requires 2-step verification), but Google Calendar won't sync then.")}
          </p>
        </div>
      {:else if tab === "signatures"}
        <div class="head">
          <h2>{t("Signatures")}</h2>
          <button class="btn primary" onclick={() => (sigEdit = { id: 0, name: t("New signature"), html: "" })}><Icon name="plus" size={16} />{t("New signature")}</button>
        </div>
        <div class="sigs">
          <div class="siglist">
            {#each store.signatures as s (s.id)}
              <div class="sigitem" class:on={sigEdit?.id === s.id} animate:flip={{ duration: 200 }} transition:slide={{ duration: 160 }}>
                <button class="grow" onclick={() => (sigEdit = { ...s })}>{s.name}</button>
                <button class="icon-btn" title={t("Delete")} onclick={() => deleteSig(s)}><Icon name="trash" size={16} /></button>
              </div>
            {:else}
              <p class="muted small">{t("No signatures yet. Create one, then assign it to an account under Accounts.")}</p>
            {/each}
          </div>
          {#if sigEdit}
            <div class="sigedit card" in:fly={{ x: 12, duration: 220 }}>
              <label class="field"><span>{t("Signature name")}</span><input class="input" bind:value={sigEdit.name} /></label>
              {#key sigEdit.id}
                <RichEditor html={sigEdit.html} bind:el={sigEl} compact minHeight={160} placeholder={t("E.g. Best regards, name, title, phone number…")} />
              {/key}
              <div class="ai-sig">
                <Icon name="sparkle" size={16} />
                <input class="input grow" bind:value={sigPrompt} placeholder={t("Describe it: name, title, company, phone, style…")} onkeydown={(e) => e.key === "Enter" && aiSignature()} />
                <button class="btn" onclick={aiSignature} disabled={sigBusy}>{#if sigBusy}<Icon name="sync" size={15} class="spin" />{/if}{t("Generate with AI")}</button>
              </div>
              <div class="row"><button class="btn primary" onclick={saveSig}>{t("Save signature")}</button><button class="btn" onclick={() => (sigEdit = null)}>{t("Close")}</button></div>
              <p class="muted small">{t("Assign a default signature to each account in Settings → Accounts.")}</p>
            </div>
          {/if}
        </div>
      {:else if tab === "language"}
        <h2>{t("Language")}</h2>
        <div class="form">
          <div class="langs">
            {#each languages as l}
              <button class="lang card" class:on={i18n.lang === l.id} onclick={async () => { await store.set("lang", l.id as Lang); setLang(l.id); }}>
                <span class="lname">{l.name}</span>
                <span class="muted small">{l.id === "en" ? "Default" : "Tiếng Việt"}</span>
                {#if i18n.lang === l.id}<Icon name="check" size={16} />{/if}
              </button>
            {/each}
          </div>
          <p class="muted small">{t("The assistant answers in this language unless you write to it in another one. The window reloads to apply the change.")}</p>
        </div>
      {:else if tab === "appearance"}
        <h2>{t("Appearance")}</h2>
        <div class="form">
          <div class="field">
            <span>{t("Theme")}</span>
            <div class="themes">
              {#each [["system", t("System")], ["light", t("Light")], ["dark", t("Dark")]] as [v, label]}
                <button class="theme card" class:on={store.settings.theme === v} onclick={() => store.set("theme", v as any)}>
                  <span class="prev">
                    {#if v === "system"}
                      <span class="half">{@render mini("light")}</span>
                      <span class="half dark-half">{@render mini("dark")}</span>
                      <span class="divider"></span>
                    {:else}
                      {@render mini(v)}
                    {/if}
                  </span>
                  {label}
                </button>
              {/each}
            </div>
          </div>
          <div class="field badge-opt">
            <span>{t("Email background")}<br /><span class="muted small">{t("White shows mail as senders designed it. Dark also darkens formatted mail (pictures keep their colours).")}</span></span>
            <div class="segmented" use:pill>
              <button class:on={store.settings.mail_bg === "light"} onclick={() => store.set("mail_bg", "light")}>{t("White")}</button>
              <button class:on={store.settings.mail_bg === "dark"} onclick={() => store.set("mail_bg", "dark")}>{t("Dark")}</button>
              <button class:on={store.settings.mail_bg === "system"} onclick={() => store.set("mail_bg", "system")}>{t("System")}</button>
            </div>
          </div>
          <div class="field">
            <span>{t("Accent color")}</span>
            <div class="colors">
              {#each accents as c}
                <button class="sw" class:on={store.settings.accent === c} style:background={c} onclick={() => store.set("accent", c)} aria-label={c}></button>
              {/each}
              <label class="sw custom" class:on={customAccent} title={t("Custom color")} style:--cur={store.settings.accent}>
                <input type="color" value={store.settings.accent} oninput={(e) => store.set("accent", (e.target as HTMLInputElement).value)} />
                <span class="core" class:filled={customAccent}>{#if !customAccent}<Icon name="plus" size={13} />{/if}</span>
              </label>
            </div>
          </div>
          <div class="grid2">
            <div class="field">
              <span>{t("List density")}</span>
              <div class="segmented" use:pill>
                <button class:on={store.settings.density === "comfortable"} onclick={() => store.set("density", "comfortable")}>{t("Comfortable")}</button>
                <button class:on={store.settings.density === "compact"} onclick={() => store.set("density", "compact")}>{t("Compact")}</button>
              </div>
            </div>
            <div class="field">
              <span>{t("Reading pane")}</span>
              <div class="segmented" use:pill>
                <button class:on={store.settings.reading_pane === "right"} onclick={() => store.set("reading_pane", "right")}>{t("Right")}</button>
                <button class:on={store.settings.reading_pane === "bottom"} onclick={() => store.set("reading_pane", "bottom")}>{t("Bottom")}</button>
                <button class:on={store.settings.reading_pane === "off"} onclick={() => store.set("reading_pane", "off")}>{t("Hide")}</button>
              </div>
            </div>
            <label class="field">
              <span>{t("Text size (")}{Math.round(store.settings.font_scale * 100)}%)</span>
              <input type="range" min="0.85" max="1.25" step="0.05" value={store.settings.font_scale} oninput={(e) => store.set("font_scale", Number((e.target as HTMLInputElement).value))} />
            </label>
          </div>
          <button class="toggle" onclick={() => store.set("preview_lines", store.settings.preview_lines ? 0 : 1)}>
            <span class="switch" class:on={store.settings.preview_lines > 0}></span>{t("Show a content preview in the list")}
          </button>
          <button class="toggle" onclick={() => store.set("show_avatars", !store.settings.show_avatars)}>
            <span class="switch" class:on={store.settings.show_avatars}></span>{t("Show sender avatars")}
          </button>
        </div>
      {:else if tab === "mail"}
        <h2>{t("Mail")}</h2>
        <div class="form">
          <div class="grid2">
            <label class="field">
              <span>{t("Automatic sync")}</span>
              <select class="input" value={String(store.settings.sync_interval)} onchange={(e) => store.set("sync_interval", Number((e.target as HTMLSelectElement).value))}>
                <option value="1">{t("Every minute")}</option>
                <option value="2">{t("Every 2 minutes")}</option>
                <option value="5">{t("Every 5 minutes")}</option>
                <option value="15">{t("Every 15 minutes")}</option>
                <option value="30">{t("Every 30 minutes")}</option>
                <option value="0">{t("Only when I click sync")}</option>
              </select>
            </label>
            <label class="field">
              <span>{t("Mark as read")}</span>
              <select class="input" value={String(store.settings.mark_read_delay)} onchange={(e) => store.set("mark_read_delay", Number((e.target as HTMLSelectElement).value))}>
                <option value="0">{t("Immediately")}</option>
                <option value="1">{t("After 1 second")}</option>
                <option value="3">{t("After 3 seconds")}</option>
                <option value="-1">{t("Never automatically")}</option>
              </select>
            </label>
            <label class="field">
              <span>{t("Remote images")}</span>
              <select class="input" value={store.settings.remote_images} onchange={(e) => store.set("remote_images", (e.target as HTMLSelectElement).value as any)}>
                <option value="ask">{t("Block, allow per message")}</option>
                <option value="always">{t("Always load")}</option>
              </select>
            </label>
            <div class="field">
              <span>{t("Default search mode")}</span>
              <div class="segmented" use:pill>
                <button class:on={store.settings.search_mode === "subject"} onclick={() => store.set("search_mode", "subject")}>{t("Subject")}</button>
                <button class:on={store.settings.search_mode === "content"} onclick={() => store.set("search_mode", "content")}>{t("Content")}</button>
              </div>
            </div>
          </div>
          <div class="notif">
            <div class="row">
              <button class="toggle grow" onclick={() => store.set("notifications", !store.settings.notifications)}>
                <span class="switch" class:on={store.settings.notifications}></span>{t("Notify me of new mail")}
              </button>
              <button class="btn subtle" onclick={testNotification} disabled={!store.settings.notifications}><Icon name="bell" size={15} />{t("Send a test notification")}</button>
            </div>
            {#if store.settings.notifications}
              <button class="toggle sub" onclick={() => store.set("notify_groups", !store.settings.notify_groups)} transition:slide={{ duration: 180 }}>
                <span class="switch" class:on={store.settings.notify_groups}></span>
                <span>{t("Also notify for mail sorted into groups")}<br /><span class="muted small">{t("Off: mail that rules or AI put into a group hidden from the inbox arrives quietly.")}</span></span>
              </button>
              <div class="field badge-opt" transition:slide={{ duration: 180 }}>
                <span>{t("Notification style")}<br /><span class="muted small">{t("Hermio's popup shows the subject in bold with From and To right under it; a copy still goes to the Windows notification centre. Windows notifications can't use bold.")}</span></span>
                <div class="segmented" use:pill>
                  <button class:on={store.settings.notify_style !== "windows"} onclick={() => store.set("notify_style", "hermio")}>{t("Hermio popup")}</button>
                  <button class:on={store.settings.notify_style === "windows"} onclick={() => store.set("notify_style", "windows")}>{t("Windows")}</button>
                </div>
              </div>
              {#if store.settings.notify_style !== "windows"}
                <button class="toggle sub" onclick={() => store.set("notify_sound", !store.settings.notify_sound)} transition:slide={{ duration: 180 }}>
                  <span class="switch" class:on={store.settings.notify_sound}></span>
                  <span>{t("Play the new-mail sound")}</span>
                </button>
              {/if}
            {/if}
            <div class="field badge-opt">
              <span>{t("Unread count on the taskbar icon")}<br /><span class="muted small">{t("All unread mail in the inboxes of every signed-in mailbox. Also shown on the tray icon.")}</span></span>
              <div class="segmented" use:pill>
                <button class:on={store.settings.taskbar_badge === "count"} onclick={() => store.set("taskbar_badge", "count")}>{t("Number")}</button>
                <button class:on={store.settings.taskbar_badge === "dot"} onclick={() => store.set("taskbar_badge", "dot")}>{t("Dot")}</button>
                <button class:on={store.settings.taskbar_badge === "off"} onclick={() => store.set("taskbar_badge", "off")}>{t("Off")}</button>
              </div>
            </div>
            <button class="toggle rem" onclick={() => store.set("reminders", !store.settings.reminders)}>
              <span class="switch" class:on={store.settings.reminders}></span>
              <span>{t("Remind me before calendar events")}<br /><span class="muted small">{t("12 h, 6 h, 4 h, 2 h, 1 h, 30, 15 and 10 minutes before. Each reminder can skip the rest.")}</span></span>
            </button>
          </div>
          <label class="field">
            <span>{t("Undo send window: {n} seconds", { n: store.settings.send_delay })}</span>
            <div class="row">
              <input type="range" min="15" max="300" step="5" value={store.settings.send_delay} oninput={(e) => (store.settings.send_delay = Number((e.target as HTMLInputElement).value))} onchange={(e) => store.set("send_delay", Number((e.target as HTMLInputElement).value))} class="grow" />
              <span class="muted small" style="width:56px;text-align:right">{store.settings.send_delay} s</span>
            </div>
            <span class="muted small">{t("After you press Send, the message waits in the corner so you can still cancel or send it right away.")}</span>
          </label>
          <button class="toggle" onclick={() => store.set("confirm_delete", !store.settings.confirm_delete)}>
            <span class="switch" class:on={store.settings.confirm_delete}></span>{t("Ask before deleting")}
          </button>
          {#if store.settings.trusted_senders.length}
            <div class="field">
              <span>{t("Trusted senders (always load images)")}</span>
              <div class="chips">
                {#each store.settings.trusted_senders as s}
                  <span class="chip">{s}<button onclick={() => store.set("trusted_senders", store.settings.trusted_senders.filter((x) => x !== s))}><Icon name="close" size={12} /></button></span>
                {/each}
              </div>
            </div>
          {/if}
          <div class="tipbox">
            <b>{t("Keyboard shortcuts")}</b>
            <div class="keys">
              <span><kbd>Ctrl</kbd>+<kbd>N</kbd> {t("New mail")}</span>
              <span><kbd>Ctrl</kbd>+<kbd>E</kbd> {t("Search")}</span>
              <span><kbd>Ctrl</kbd>+<kbd>R</kbd> {t("Reply")}</span>
              <span><kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>R</kbd> {t("Reply all")}</span>
              <span><kbd>Ctrl</kbd>+<kbd>F</kbd> {t("Forward")}</span>
              <span><kbd>Ctrl</kbd>+<kbd>Enter</kbd> {t("Send")}</span>
              <span><kbd>Delete</kbd> {t("Delete")}</span>
              <span><kbd>Backspace</kbd> {t("Archive")}</span>
              <span><kbd>Ctrl</kbd>+<kbd>Q</kbd>/<kbd>U</kbd> {t("Read/Unread")}</span>
              <span><kbd>Insert</kbd> {t("Flag")}</span>
              <span><kbd>F5</kbd> {t("Sync")}</span>
              <span><kbd>Ctrl</kbd>+<kbd>J</kbd> {t("AI assistant")}</span>
              <span><kbd>↑</kbd><kbd>↓</kbd> {t("Switch message")}</span>
            </div>
          </div>
        </div>
      {:else if tab === "calendar"}
        <h2>{t("Calendar")}</h2>
        <div class="form">
          <div class="field">
            <span>{t("First day of the week")}</span>
            <div class="segmented" use:pill>
              <button class:on={store.settings.week_start === 1} onclick={() => store.set("week_start", 1)}>{t("Monday")}</button>
              <button class:on={store.settings.week_start === 0} onclick={() => store.set("week_start", 0)}>{t("Sunday")}</button>
            </div>
          </div>
          <p class="muted">{t("Google Calendar syncs for Google accounts signed in with OAuth (toggle per account). Events created in “My calendar” are stored only on this PC.")}</p>
          <fieldset class="ics">
            <legend>{t("Microsoft Outlook / ICS calendars")}</legend>
            <p class="muted small">{t("In Outlook on the web: Settings → Calendar → Shared calendars → Publish a calendar → copy the")} <b>ICS</b>{t("link. Paste it here (webcal:// works too). Calendars refresh on every sync.")}</p>
            <div class="row">
              <input class="input" style="width:180px" bind:value={icsName} placeholder={t("Calendar name")} />
              <input class="input grow mono" bind:value={icsUrl} placeholder="https://outlook.office365.com/owa/calendar/…/calendar.ics" />
              <button class="btn primary" onclick={addIcs} disabled={icsBusy || !icsUrl.trim()}>{#if icsBusy}<Icon name="sync" size={15} class="spin" />{/if}{t("More")}</button>
            </div>
            <div class="row"><button class="btn" onclick={importIcs}><Icon name="download" size={16} />{t("Import an .ics file into “My calendar”")}</button></div>
            {#each calendars.filter((c) => c.kind === "ics") as c (c.id)}
              <div class="calrow">
                <i style:background={c.color}></i>
                <div class="grow">
                  <b>{c.name}</b>
                  <div class="muted small mono ellipsis">{c.url}</div>
                  {#if c.last_error}<div class="small errtxt">{c.last_error}</div>{:else if c.last_sync}<div class="muted small">{t("Update")} {new Date(c.last_sync * 1000).toLocaleString(locale())}</div>{/if}
                </div>
                <button class="icon-btn" title={t("Remove calendar")} onclick={() => removeCal(c)}><Icon name="trash" size={16} /></button>
              </div>
            {/each}
          </fieldset>
          <div class="row"><button class="btn" onclick={() => api.syncCalendars().then(() => store.success(t("Calendars synced"))).catch((e) => store.error(String(e)))}><Icon name="sync" size={16} />{t("Sync calendars now")}</button></div>
        </div>
      {:else if tab === "about"}
        <div class="about-head"><div class="about-logo"><Logo size={64} /></div><div><h2 style="margin:0">Hermio</h2><div class="tagline">{t("Automate your inbox flow.")}</div></div></div>
        <p class="lead">{t("A light, fast email app for Windows 11 — IMAP, POP3, Google Workspace, Google Calendar.")}</p>
        <p class="ver"><b>{t("Version {v}", { v: __APP_VERSION__ })}</b> <span class="muted">· {t("built {d}", { d: __BUILD_DATE__ })}</span></p>
        <p class="muted">{t("Data (mail index, settings, downloaded mail) is stored in Documents\\Hermio_Emails; passwords and tokens are stored in Windows Credential Manager.")}</p>
        <p class="muted small">{t("Open source under the MIT or Apache-2.0 licence.")} <button class="lnk" onclick={() => api.openUrl("https://github.com/nev-minhnh/Hermio")}>github.com/nev-minhnh/Hermio</button></p>
        <fieldset class="usage">
          <legend>{t("Resource use")}</legend>
          <p class="small muted" style="margin:0 0 8px">{t("Memory and CPU time of Hermio and the WebView2 processes it runs (refreshed every 2 s while this page is open).")}</p>
          {#if usage}
            <div class="ustats">
              <div><b>{mb(usage.total_private)}</b><span>{t("Private memory")}</span></div>
              <div><b>{mb(usage.total_working_set)}</b><span>{t("In use (working set)")}</span></div>
              <div><b>{cpuPct}%</b><span>{t("CPU now")}</span></div>
              <div><b>{usage.processes.length}</b><span>{t("Processes")}</span></div>
            </div>
            <table class="utable small">
              <thead><tr><th>{t("Process")}</th><th>{t("Private")}</th><th>{t("In use")}</th><th>{t("CPU time")}</th></tr></thead>
              <tbody>
                {#each usage.processes as p (p.pid)}<tr><td>{p.name} <span class="muted">#{p.pid}</span>{#if p.kind}<span class="kind">{kindLabel(p.kind)}</span>{/if}</td><td>{mb(p.private)}</td><td>{mb(p.working_set)}</td><td>{(p.cpu_ms / 1000).toFixed(1)} s</td></tr>{/each}
              </tbody>
            </table>
            <div class="row">
              <button class="btn subtle" onclick={copyUsage}><Icon name="copy" size={15} />{t("Copy")}</button>
              <button class="btn subtle" onclick={() => api.trimMemory().then(() => store.success(t("Memory freed")))} title={t("Ask WebView2 to drop its caches now")}><Icon name="sync" size={15} />{t("Free memory")}</button>
            </div>
          {:else}
            <span class="spinner"></span>
          {/if}
        </fieldset>
      {:else if tab === "backup"}
        <h2>{t("Backup & transfer")}</h2>
        <p class="muted">{t("Save all settings to a file and load them into another copy of Hermio — a new version, a new portable folder or another PC. Mail itself isn't included: it downloads again from the server.")}</p>
        <div class="form">
          <fieldset>
            <legend>{t("Move all data (mail, settings, everything)")}</legend>
            <p class="small muted" style="margin:0 0 8px">{t("For a new version or a new portable folder: copies the mail index, downloaded and POP3 mail, groups, rules and every setting. The NPU runtime and models aren't copied (point the new copy at the same models folder instead). Hermio restarts to finish; your current data is kept in a before-restore folder.")}</p>
            <div class="row wrap">
              <button class="btn primary" onclick={copyFromOld} disabled={dataBusy}><Icon name="folder" size={16} />{t("Copy data from an older copy (Hermio or Nelo)…")}</button>
              <button class="btn" onclick={restoreBackup} disabled={dataBusy}><Icon name="upload" size={16} />{t("Restore a full backup…")}</button>
              <button class="btn" onclick={makeBackup} disabled={dataBusy}><Icon name="download" size={16} />{t("Create a full backup…")}</button>
            </div>
            {#if dataBusy}
              <div class="prog" transition:slide={{ duration: 160 }}>
                <div class="track"><div class="fill" class:indet={!dataProg?.total || dataProg.stage === "database"} style:width="{dataProg?.total ? Math.round((dataProg.done / dataProg.total) * 100) : 30}%"></div></div>
                <span class="small muted">{!dataProg ? t("Preparing…") : dataProg.stage === "database" ? t("Copying the database…") : t("Copying files {done}/{total}…", dataProg)}</span>
              </div>
            {/if}
            {#if store.dataStaged}
              <div class="note small" transition:slide={{ duration: 180 }}>
                <div>{t("Ready to restore: {what}. Hermio restarts and swaps the data in.", { what: store.dataStaged })}</div>
                <div class="row" style="margin-top:6px">
                  <button class="btn primary" onclick={() => api.appRestart()}>{t("Restart now")}</button>
                  <button class="btn subtle" onclick={() => { api.dataCancelRestore(); store.dataStaged = null; }}>{t("Cancel")}</button>
                </div>
              </div>
            {/if}
          </fieldset>
          <fieldset>
            <legend>{t("Export")}</legend>
            <p class="small muted" style="margin:0 0 8px">{t("Mailboxes (server settings), spaces, groups, rules, signatures, calendar subscriptions, contacts, AI providers and skills, appearance and all other preferences.")}</p>
            <button class="toggle" onclick={() => (exSecrets = !exSecrets)}>
              <span class="switch" class:on={exSecrets}></span>{t("Include passwords, sign-ins and API keys")}
            </button>
            {#if exSecrets}<p class="small muted note-indent" transition:slide={{ duration: 160 }}>{t("They are encrypted for your Windows account: the file only imports them for the same Windows user on this PC. On another PC you sign in again.")}</p>{/if}
            <button class="toggle" onclick={() => (exChats = !exChats)}>
              <span class="switch" class:on={exChats}></span>{t("Include saved AI conversations")}
            </button>
            <div class="row" style="margin-top:8px"><button class="btn primary" onclick={doExport} disabled={backupBusy}><Icon name="download" size={16} />{t("Export settings…")}</button></div>
          </fieldset>
          <fieldset>
            <legend>{t("Import")}</legend>
            <p class="small muted" style="margin:0 0 8px">{t("Merges into this copy: mailboxes with the same address are updated, groups and rules with the same name are replaced, everything else is added.")}</p>
            <div class="row"><button class="btn" onclick={doImport} disabled={backupBusy}><Icon name="upload" size={16} />{t("Import settings…")}</button></div>
            {#if importReport}
              <div class="note small" transition:slide={{ duration: 180 }}>
                <div>{t("Imported: {a} new and {u} updated mailboxes, {g} groups, {r} rules, {s} signatures, {k} settings, {p} passwords/keys.", { a: importReport.accounts_added, u: importReport.accounts_updated, g: importReport.groups, r: importReport.rules, s: importReport.signatures, k: importReport.settings, p: importReport.secrets })}</div>
                {#each importReport.notes as n}<div class="muted">{t(n)}</div>{/each}
                <div class="row" style="margin-top:6px"><button class="btn primary" onclick={() => location.reload()}>{t("Reload Hermio to apply")}</button></div>
              </div>
            {/if}
          </fieldset>
        </div>
      {/if}
    </div>
    {/key}
  </section>
</div>

{#snippet mini(kind: string)}
  <span class="win {kind}">
    <span class="bar"><i></i><i></i><i></i></span>
    <span class="body">
      <span class="side"><b class="acc"></b><b></b><b></b><b></b></span>
      <span class="list"><b></b><b></b><b></b></span>
      <span class="read"><b class="h"></b><b></b><b></b><b class="s"></b></span>
    </span>
  </span>
{/snippet}

<style>
  .prog {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-top: 10px;
  }
  .prog .track {
    height: 5px;
    border-radius: 3px;
    background: var(--surface-3, color-mix(in srgb, var(--text) 10%, transparent));
    overflow: hidden;
  }
  .prog .fill {
    height: 100%;
    background: var(--accent);
    transition: width 250ms var(--ease);
  }
  .prog .fill.indet {
    animation: indet 1.2s ease-in-out infinite;
  }
  @keyframes indet {
    0% {
      transform: translateX(-100%);
    }
    100% {
      transform: translateX(330%);
    }
  }
  .row.wrap {
    flex-wrap: wrap;
  }
  .ver {
    margin: 4px 0;
  }
  .note-indent {
    margin: -4px 0 6px 46px;
  }
  .tabanim {
    animation: slide-left 260ms var(--ease-out);
  }
  .langs {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 220px));
    gap: 10px;
  }
  .lang {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    padding: 12px 14px;
    position: relative;
    text-align: left;
  }
  .lang :global(svg) {
    position: absolute;
    top: 12px;
    right: 12px;
    color: var(--accent);
  }
  .lang.on {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .lname {
    font-weight: 600;
    font-size: 1rem;
  }
  .ai-sig {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-radius: 10px;
    background: color-mix(in srgb, var(--ai) 7%, transparent);
    border: 1px solid color-mix(in srgb, var(--ai) 25%, transparent);
  }
  .ai-sig > :global(svg) {
    color: var(--ai);
  }
  .settings {
    flex: 1;
    min-width: 0;
    display: flex;
    gap: 8px;
  }
  .tabs {
    width: 220px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 0 4px;
  }
  h1 {
    font-family: var(--font-display);
    font-weight: 650;
    font-size: 1.35rem;
    margin: 6px 10px 12px;
    background: var(--brand-grad);
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
    width: fit-content;
  }
  :global([data-theme="dark"]) h1 {
    background: linear-gradient(135deg, #9dbcff, #5b8cff);
    -webkit-background-clip: text;
    background-clip: text;
  }
  .tabs button {
    display: flex;
    align-items: center;
    gap: 11px;
    height: 34px;
    padding: 0 12px;
    border: none;
    border-radius: var(--radius);
    background: transparent;
    font-size: 0.92rem;
    text-align: left;
    position: relative;
    transition: background var(--dur) var(--ease), color var(--dur) var(--ease);
  }
  .tabs button :global(.icon) {
    color: var(--text-2);
  }
  .tabs button:hover {
    background: var(--hover);
  }
  .tabs button.on {
    background: var(--selected);
    color: var(--text-accent);
    font-weight: 600;
  }
  .tabs button.on :global(.icon) {
    color: var(--text-accent);
  }
  .tabs button.on::before {
    content: "";
    position: absolute;
    left: 2px;
    top: 9px;
    bottom: 9px;
    width: 3px;
    border-radius: 2px;
    background: var(--accent);
    animation: bar-in 260ms var(--ease-out);
  }
  @keyframes bar-in {
    from {
      transform: scaleY(0.2);
      opacity: 0;
    }
  }
  .panel {
    flex: 1;
    min-width: 0;
    background:
      linear-gradient(90deg, var(--brand-1), var(--brand-2) 55%, var(--brand-3)) top / 100% 3px no-repeat,
      var(--surface);
    border-radius: var(--radius-l);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-s);
  }
  .inner {
    padding: 26px 34px 40px;
    max-width: 900px;
    animation: slide-up 260ms var(--ease-out);
  }
  h2 {
    margin: 0 0 18px;
    font-family: var(--font-display);
    font-weight: 600;
    font-size: 1.35rem;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 18px;
  }
  .head h2 {
    margin: 0;
  }
  .back {
    margin: -10px 0 8px -10px;
  }
  .lead {
    line-height: 1.55;
    max-width: 720px;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .acc {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px 8px 16px;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    flex: none;
  }
  .acc-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    border: none;
    background: none;
    text-align: left;
    padding: 4px 6px;
  }
  .acc-main span {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .errtxt {
    color: var(--danger);
  }
  .form {
    display: flex;
    flex-direction: column;
    gap: 16px;
    max-width: 720px;
  }
  .grid2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px 18px;
  }
  .mono {
    font-family: Consolas, monospace;
  }
  .steps {
    line-height: 1.8;
    padding-left: 20px;
    margin: 0 0 18px;
  }
  .lnk {
    border: none;
    background: none;
    padding: 0;
    color: var(--text-accent);
    text-decoration: underline;
    font: inherit;
  }
  .notif .badge-opt {
    margin-top: 10px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }
  .notif .rem {
    margin-top: 8px;
    align-items: flex-start;
  }
  .notif .sub {
    margin: 6px 0 0 52px;
    align-items: flex-start;
  }
  .toggle {
    display: flex;
    align-items: center;
    gap: 12px;
    border: none;
    background: none;
    padding: 2px 0;
    text-align: left;
    line-height: 1.45;
  }
  .themes {
    display: flex;
    gap: 12px;
  }
  .theme {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 10px 10px 8px;
    font-weight: 600;
    font-size: 0.88rem;
  }
  .theme.on {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  /* Theme previews: a tiny Hermio window in each theme; "System" shows both, split down the middle. */
  .prev {
    position: relative;
    width: 132px;
    height: 80px;
    border-radius: 8px;
    overflow: hidden;
    border: 1px solid var(--border);
    display: block;
  }
  .prev .half {
    position: absolute;
    inset: 0;
    display: block;
  }
  .prev .dark-half {
    clip-path: inset(0 0 0 50%);
  }
  .prev .divider {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 50%;
    width: 1px;
    background: color-mix(in srgb, #888 60%, transparent);
  }
  .win {
    --bg: #eef2fa;
    --pane: #ffffff;
    --side: #e2e8f6;
    --line: #d3dbee;
    --dot: #b9c4e0;
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    background: var(--bg);
  }
  .win.dark {
    --bg: #0b1022;
    --pane: #161e38;
    --side: #10172d;
    --line: #2a355a;
    --dot: #3a4670;
  }
  .win .bar {
    display: flex;
    gap: 3px;
    justify-content: flex-end;
    padding: 4px 6px 0;
    height: 10px;
  }
  .win .bar i {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: var(--dot);
  }
  .win .body {
    flex: 1;
    display: grid;
    grid-template-columns: 26% 30% 1fr;
    gap: 3px;
    padding: 3px 5px 5px;
    min-height: 0;
  }
  .win .side,
  .win .list,
  .win .read {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 4px;
    border-radius: 4px;
  }
  .win .side {
    background: var(--side);
  }
  .win .list,
  .win .read {
    background: var(--pane);
  }
  .win b {
    display: block;
    height: 4px;
    border-radius: 2px;
    background: var(--line);
  }
  .win .side b.acc {
    background: var(--accent);
    width: 80%;
  }
  .win .list b {
    height: 9px;
  }
  .win .read b.h {
    width: 70%;
    height: 5px;
    background: var(--dot);
  }
  .win .read b.s {
    width: 55%;
  }
  .colors {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
  }
  .sw {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    border: 2px solid transparent;
    box-shadow: 0 0 0 1px var(--border);
  }
  .sw.on {
    border-color: var(--surface);
    box-shadow: 0 0 0 2px var(--text);
  }
  /* Custom colour: a colour-wheel ring around the chosen colour (or a "+" before one is chosen). */
  .sw.custom {
    position: relative;
    display: block;
    padding: 3px;
    border: none;
    background: conic-gradient(from 90deg, #ff4d4d, #ffb020, #f5e542, #3ccf5a, #2bc4e8, #4c6ef5, #b44cf5, #ff4d9a, #ff4d4d);
    cursor: pointer;
    transition: transform 120ms var(--ease);
  }
  .sw.custom:hover {
    transform: scale(1.08);
  }
  .sw.custom.on {
    box-shadow: 0 0 0 2px var(--surface), 0 0 0 4px var(--text);
  }
  .sw.custom .core {
    display: grid;
    place-items: center;
    width: 100%;
    height: 100%;
    border-radius: 50%;
    background: var(--surface);
    color: var(--text-2);
    pointer-events: none;
  }
  .sw.custom .core.filled {
    background: var(--cur);
  }
  .sw.custom input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    opacity: 0;
    cursor: pointer;
    border: none;
    padding: 0;
  }
  .sigs {
    display: grid;
    grid-template-columns: 220px 1fr;
    gap: 18px;
    align-items: start;
  }
  .siglist {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .sigitem {
    display: flex;
    align-items: center;
    border-radius: var(--radius);
  }
  .sigitem.on {
    background: var(--selected);
  }
  .sigitem > button.grow {
    border: none;
    background: none;
    text-align: left;
    height: 34px;
    padding: 0 10px;
    font-weight: 600;
  }
  .sigitem:hover {
    background: var(--hover);
  }
  .sigedit {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 4px 2px 10px;
    border-radius: 12px;
    background: var(--surface-3);
    font-size: 0.86rem;
  }
  .chip button {
    border: none;
    background: none;
    display: flex;
    padding: 3px;
    border-radius: 50%;
  }
  .about-head {
    display: flex;
    align-items: center;
    gap: 18px;
    margin: -6px 0 18px;
    padding: 20px 22px;
    border-radius: 16px;
    color: white;
    background:
      radial-gradient(120% 140% at 0% 0%, rgba(255, 255, 255, 0.18), transparent 55%),
      var(--brand-grad);
    box-shadow: 0 10px 30px color-mix(in srgb, var(--brand-2) 30%, transparent);
  }
  .about-head h2 {
    color: white;
  }
  .about-head .tagline {
    color: rgba(255, 255, 255, 0.86);
  }
  .about-logo {
    border-radius: 16px;
    box-shadow: 0 6px 18px rgba(0, 8, 60, 0.45);
    animation: float-in 520ms var(--ease-out);
  }
  @keyframes float-in {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.92);
    }
  }
  .tagline {
    color: var(--text-accent);
    font-weight: 600;
    font-family: var(--font-display);
  }
  .spaces-box {
    padding: 12px 14px;
    border-radius: var(--radius-l);
    background: var(--surface-3);
    margin-bottom: 14px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .spaces-box p {
    margin: 0;
  }
  .chip i {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    display: inline-block;
    margin-right: 6px;
  }
  .spsel {
    height: 28px;
    width: 130px;
    font-size: 0.84rem;
  }
  fieldset.ics {
    border: 1px solid var(--border);
    border-radius: var(--radius-l);
    padding: 12px 16px 16px;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  fieldset.ics legend {
    padding: 0 6px;
    font-weight: 700;
  }
  fieldset.ics p {
    margin: 0;
    line-height: 1.5;
  }
  .calrow {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 0;
    border-top: 1px solid var(--border);
  }
  .calrow i {
    width: 10px;
    height: 10px;
    border-radius: 3px;
  }
  .ellipsis {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 520px;
  }
  .tipbox {
    padding: 14px 16px;
    border-radius: var(--radius-l);
    background: var(--surface-3);
  }
  .keys {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px 16px;
    margin-top: 10px;
    font-size: 0.88rem;
  }
  kbd {
    display: inline-block;
    padding: 0 5px;
    border-radius: 4px;
    border: 1px solid var(--border-strong);
    border-bottom-width: 2px;
    background: var(--surface);
    font-family: inherit;
    font-size: 0.78rem;
  }
  .usage {
    margin-top: 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-l);
    padding: 10px 14px 12px;
  }
  .usage legend {
    padding: 0 6px;
    font-weight: 600;
  }
  .ustats {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 8px;
    margin-bottom: 10px;
  }
  .ustats div {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--surface-2);
    border: 1px solid var(--border);
  }
  .ustats b {
    font-size: 1.15rem;
    font-variant-numeric: tabular-nums;
  }
  .ustats span {
    font-size: 0.78rem;
    color: var(--text-3);
  }
  .utable {
    width: 100%;
    border-collapse: collapse;
    margin-bottom: 8px;
    font-variant-numeric: tabular-nums;
  }
  .utable th,
  .utable td {
    text-align: left;
    padding: 4px 6px;
    border-bottom: 1px solid var(--border);
  }
  .utable th {
    color: var(--text-3);
    font-weight: 600;
  }
  .utable .kind {
    margin-left: 6px;
    padding: 0 6px;
    border-radius: 6px;
    background: var(--surface-3);
    color: var(--text-2);
    font-size: 0.74rem;
  }
</style>
