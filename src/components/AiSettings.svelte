<script lang="ts">
  import { slide, fade } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { t } from "../lib/i18n.svelte";
  import { onMount } from "svelte";
  import Icon from "../lib/Icon.svelte";
  import { api, type AiProvider } from "../lib/api";
  import { store } from "../lib/store.svelte";
  import Dialog from "./Dialog.svelte";
  import NpuSettings from "./NpuSettings.svelte";
  import type { Skill } from "../lib/api";

  const kinds: { kind: string; label: string; hint: string; needsKey: boolean; base: string }[] = [
    { kind: "openai", label: "ChatGPT (OpenAI API)", hint: t("Get an API key at platform.openai.com → API keys."), needsKey: true, base: "https://api.openai.com/v1" },
    { kind: "gemini", label: "Google Gemini (AI Studio)", hint: t("Get a free API key at aistudio.google.com → Get API key."), needsKey: true, base: "" },
    { kind: "grok", label: "Grok (xAI API)", hint: t("Get an API key at console.x.ai."), needsKey: true, base: "https://api.x.ai/v1" },
    { kind: "anthropic", label: "Claude (Anthropic API)", hint: t("Get an API key at console.anthropic.com."), needsKey: true, base: "" },
    { kind: "claude_cli", label: t("Claude Code (signed in)"), hint: t("Uses the signed-in `claude` command on this PC (Pro/Max plan), no API key needed. Install Claude Code and run `claude` once to sign in."), needsKey: false, base: "" },
    { kind: "local", label: t("Local model (OpenAI-compatible)"), hint: "llama.cpp server, Ollama (http://127.0.0.1:11434/v1), LM Studio (http://127.0.0.1:1234/v1)…", needsKey: false, base: "http://127.0.0.1:8080/v1" },
  ];

  let providers = $state<AiProvider[]>([]);
  let edit = $state<AiProvider | null>(null);
  let apiKey = $state("");
  let models = $state<string[]>([]);
  let busy = $state("");
  let result = $state<{ ok: boolean; text: string } | null>(null);

  onMount(() => {
    load();
    loadSkills();
    loadWeb();
  });

  // ----- web search -----
  let webEngine = $state("duckduckgo");
  let searx = $state("");
  let braveKey = $state("");
  let hasBrave = $state(false);
  let webBusy = $state(false);
  let webResult = $state<{ ok: boolean; text: string } | null>(null);
  async function loadWeb() {
    try {
      const c = await api.webConfig();
      webEngine = c.engine;
      searx = c.searxng;
      hasBrave = c.has_brave_key;
    } catch {}
  }
  async function saveWeb() {
    await api.setWebConfig(webEngine, searx.trim(), braveKey.trim() || null);
    if (braveKey.trim()) hasBrave = true;
    braveKey = "";
    store.success(t("Web search settings saved"));
  }
  async function testWeb() {
    webBusy = true;
    webResult = null;
    try {
      await saveWeb();
      const hits = await api.webTest("Ryzen AI NPU");
      webResult = { ok: true, text: t("{n} results · {title}", { n: hits.length, title: hits[0]?.title ?? "" }) };
    } catch (e) {
      webResult = { ok: false, text: String(e) };
    } finally {
      webBusy = false;
    }
  }
  async function load() {
    providers = await api.aiProviders();
  }

  // ----- skills -----
  let skills = $state<Skill[]>([]);
  let skillEdit = $state<Skill | null>(null);
  async function loadSkills() {
    skills = await api.aiSkills();
  }
  async function saveSkill() {
    if (!skillEdit || !skillEdit.content.trim()) return;
    await api.aiSaveSkill($state.snapshot(skillEdit));
    skillEdit = null;
    await loadSkills();
    store.success(t("Skill saved"));
  }
  async function toggleSkill(sk: Skill) {
    await api.aiSaveSkill({ ...sk, enabled: !sk.enabled });
    await loadSkills();
  }
  async function removeSkill(sk: Skill) {
    if (!confirm(t("Delete {name}?", { name: sk.name }))) return;
    await api.aiDeleteSkill(sk.id);
    await loadSkills();
  }
  async function importSkillDir() {
    const { pickFolder } = await import("../lib/tauri");
    const dir = await pickFolder(t("Choose a skill folder (with SKILL.md)"));
    if (!dir) return;
    try {
      const sk = await api.aiInstallSkillDir(dir);
      store.success(t("Installed skill “{name}”", { name: sk.name }));
      await loadSkills();
    } catch (e) {
      store.error(String(e));
    }
  }
  async function importSkill() {
    const { pickFiles } = await import("../lib/tauri");
    const files = await pickFiles();
    for (const path of files) {
      try {
        const f = await api.aiReadFile(path);
        if (!f.text) throw new Error(t("{name} has no readable text", { name: f.name }));
        await api.aiSaveSkill({ id: "", name: "", description: "", content: f.text, enabled: true });
      } catch (e) {
        store.error(String(e));
      }
    }
    await loadSkills();
  }
  const skillTemplates = [
    {
      name: t("Polite Vietnamese replies"),
      description: t("Formal Vietnamese tone for work mail"),
      content: "When drafting replies in Vietnamese, use a polite, formal register (Kính gửi / Trân trọng), address people with anh/chị/thầy/cô as in the original email, and keep it under 120 words.",
    },
    {
      name: t("Action items first"),
      description: t("Lead every summary with tasks and deadlines"),
      content: "In every summary, start with a section 'Action items' listing what the user must do, with the deadline in bold, before any other information.",
    },
    {
      name: t("Student at university"),
      description: t("Prioritise course, assignment and admin mail"),
      content: "The user is a university student. Treat mail about assignments, exams, grades, enrolment and scholarships as the highest priority and call out due dates clearly.",
    },
  ];

  async function suggestNow() {
    try {
      const n = await api.aiSuggestNow();
      await store.loadSuggestions();
      store.info(n ? t("{n} new suggestions", { n }) : t("No new suggestions"));
    } catch (e) {
      store.error(String(e));
    }
  }

  const info = (k: string) => kinds.find((x) => x.kind === k) ?? kinds[0];

  function add(kind: string) {
    const k = info(kind);
    edit = { id: "", kind, name: k.label, base_url: k.base, model: kind === "claude_cli" ? "sonnet" : "", has_key: false };
    apiKey = "";
    models = kind === "claude_cli" ? ["sonnet", "opus", "haiku"] : [];
    result = null;
  }

  async function save(close = true) {
    if (!edit) return null;
    const saved = await api.aiSaveProvider($state.snapshot(edit), apiKey || null);
    edit = { ...saved };
    apiKey = "";
    await load();
    if (!store.settings.ai_default) store.set("ai_default", saved.id);
    if (close) edit = null;
    return saved;
  }

  async function loadModels() {
    if (!edit) return;
    busy = "models";
    result = null;
    try {
      const saved = await save(false);
      if (!saved) return;
      models = await api.aiModels(saved.id);
      if (!edit.model && models.length) edit.model = models[0];
      if (!models.length) result = { ok: false, text: t("No models were returned.") };
    } catch (e) {
      result = { ok: false, text: String(e) };
    } finally {
      busy = "";
    }
  }

  async function test() {
    if (!edit) return;
    busy = "test";
    result = null;
    try {
      const saved = await save(false);
      if (!saved) return;
      const r = await api.aiTest(saved.id);
      result = { ok: true, text: t("Connected. The model replied: “{text}”", { text: r.trim().slice(0, 80) }) };
    } catch (e) {
      result = { ok: false, text: String(e) };
    } finally {
      busy = "";
    }
  }

  async function remove(p: AiProvider) {
    if (!confirm(t("Delete {name}?", { name: p.name }))) return;
    await api.aiDeleteProvider(p.id);
    await load();
  }

  // Access control: null = every account.
  const allowed = $derived(store.settings.ai_allowed_accounts);
  function isAllowed(id: number) {
    return allowed === null || allowed.includes(id);
  }
  function toggleAccount(id: number) {
    const cur = allowed ?? store.accounts.map((a) => a.id);
    const next = cur.includes(id) ? cur.filter((x) => x !== id) : [...cur, id];
    store.set("ai_allowed_accounts", next.length === store.accounts.length ? null : next);
  }
</script>

<h2>{t("AI assistant")}</h2>
<p class="muted lead">
  {t("Connect one or more models to ask questions, summarize, cluster related mail, draft quick replies (you always review before sending) and write reports across mailboxes. Open the assistant with the")} <b>{t("✦ Assistant")}</b> {t("button in the title bar or")} <kbd>Ctrl</kbd>+<kbd>J</kbd>.
</p>

{#if !(store.npu.info && !store.npu.info.present)}
  <NpuSettings onchange={load} />
{:else}
  <p class="muted small no-npu">{t("On-device AI is hidden: this PC has no AMD Ryzen AI or Intel Core Ultra NPU.")}</p>
{/if}

<section>
  <h3>{t("Connected models")}</h3>
  <div class="list">
    {#each providers as p (p.id)}
      <div class="prov card" animate:flip={{ duration: 200 }} transition:slide={{ duration: 180 }}>
        <span class="badge-kind">{p.kind === "npu" ? "NPU" : info(p.kind).label.split(" (")[0]}</span>
        <div class="grow">
          <b>{p.name}</b>
          <div class="muted small">
            {p.model || (p.id === "gemini-builtin" ? t("Newest Gemini Flash (automatic)") : t("no model selected"))}{p.base_url && p.kind !== "gemini" && p.kind !== "anthropic" ? ` · ${p.base_url}` : ""}{info(p.kind).needsKey && !p.has_key ? t(" · missing API key") : ""}{p.id === "gemini-builtin" ? t(" · included with Hermio") : ""}
          </div>
        </div>
        {#if store.settings.ai_default === p.id}
          <span class="pill">{t("Default")}</span>
        {:else}
          <button class="btn subtle" onclick={() => store.set("ai_default", p.id)}>{t("Make default")}</button>
        {/if}
        {#if p.kind !== "npu"}
          <button class="icon-btn" title={t("Edit")} onclick={() => { edit = { ...p }; apiKey = ""; models = p.kind === "claude_cli" ? ["sonnet", "opus", "haiku"] : []; result = null; }}><Icon name="settings" size={16} /></button>
          <button class="icon-btn" title={t("Delete")} onclick={() => remove(p)}><Icon name="trash" size={16} /></button>
        {/if}
      </div>
    {:else}
      <p class="muted small">{t("No models yet.")}</p>
    {/each}
  </div>
  <div class="add">
    {#each kinds as k}
      <button class="kind card" onclick={() => add(k.kind)}>
        <Icon name="plus" size={16} />
        <span>{k.label}</span>
      </button>
    {/each}
  </div>
</section>

<section>
  <h3>{t("Behaviour")}</h3>
  <div class="accs">
    <button class="acc" onclick={() => store.set("ai_thinking", !store.settings.ai_thinking)}>
      <span class="switch" class:on={store.settings.ai_thinking}></span>
      <span class="grow">{t("Thinking on by default")}<br /><span class="muted small">{t("Slower but more careful answers on models that support it. You can switch it per message in the assistant.")}</span></span>
    </button>
    <button class="acc tall" onclick={() => store.set("ai_autosuggest", !store.settings.ai_autosuggest)}>
      <span class="switch" class:on={store.settings.ai_autosuggest}></span>
      <span class="grow">{t("Suggest groups for new mail")}<br /><span class="muted small">{t("After each sync the default model reads new inbox mail that no rule matched and suggests a group. You accept or dismiss each suggestion.")}</span></span>
    </button>
    {#if store.settings.ai_autosuggest}
      <div class="row" style="padding:2px 10px" transition:slide={{ duration: 180 }}>
        <button class="btn subtle" onclick={suggestNow}><Icon name="sparkle" size={16} />{t("Suggest now")}</button>
        {#if store.suggestQueue > 0}<span class="muted small" transition:fade={{ duration: 150 }}>{t("{n} new emails are waiting for the NPU to wake up", { n: store.suggestQueue })}</span>{/if}
      </div>
    {/if}
  </div>
</section>

<section>
  <div class="sec-row">
    <h3>{t("Skills")}</h3>
    <div class="row">
      <button class="btn subtle" onclick={importSkillDir}><Icon name="folder" size={16} />{t("Install folder…")}</button>
      <button class="btn subtle" onclick={importSkill}><Icon name="upload" size={16} />{t("Install from file…")}</button>
      <button class="btn" onclick={() => (skillEdit = { id: "", name: "", description: "", content: "", enabled: true })}><Icon name="plus" size={16} />{t("New skill")}</button>
    </div>
  </div>
  <p class="muted small">{t("Skills are standing instructions the assistant follows (a SKILL.md or any text/Markdown file). Enabled skills are added to every request.")}</p>
  <div class="list">
    {#each skills as sk (sk.id)}
      <div class="prov card" animate:flip={{ duration: 200 }} transition:slide={{ duration: 180 }}>
        <button class="switch" class:on={sk.enabled} onclick={() => toggleSkill(sk)} aria-label={t("Toggle")}></button>
        <Icon name="book" size={16} />
        <div class="grow"><b>{sk.name}</b><div class="muted small">{sk.description || sk.content.slice(0, 120)}</div></div>
        <button class="icon-btn" title={t("Edit")} onclick={() => (skillEdit = { ...sk })}><Icon name="settings" size={16} /></button>
        <button class="icon-btn" title={t("Delete")} onclick={() => removeSkill(sk)}><Icon name="trash" size={16} /></button>
      </div>
    {:else}
      <div class="templates">
        {#each skillTemplates as tp}
          <button class="kind card" onclick={() => (skillEdit = { id: "", enabled: true, ...tp })}>
            <Icon name="plus" size={16} /><span><b>{tp.name}</b><br /><span class="muted small">{tp.description}</span></span>
          </button>
        {/each}
      </div>
    {/each}
  </div>
</section>

<section>
  <h3>{t("Web search")}</h3>
  <p class="muted small">
    {t("Turn on “Web” in the chat box to let the assistant use live information from the internet. Gemini, Claude API and Claude Code search by themselves; for other models Hermio searches and passes the results along. Only your question is sent to the search engine — never your mail.")}
  </p>
  <div class="webcfg card">
    <label class="field">
      <span>{t("Search engine for other models")}</span>
      <select class="input" bind:value={webEngine} onchange={saveWeb}>
        <option value="duckduckgo">{t("DuckDuckGo (free, no key)")}</option>
        <option value="brave">{t("Brave Search API (key, more reliable)")}</option>
        <option value="searxng">{t("SearXNG (your own instance)")}</option>
      </select>
    </label>
    {#if webEngine === "brave"}
      <label class="field" transition:slide={{ duration: 180 }}>
        <span>Brave Search API key {hasBrave ? t("— saved, type to replace") : ""}</span>
        <input class="input mono" type="password" bind:value={braveKey} placeholder={hasBrave ? "••••••••" : "BSA…"} onchange={saveWeb} />
      </label>
    {:else if webEngine === "searxng"}
      <label class="field">
        <span>{t("SearXNG address (JSON format must be enabled)")}</span>
        <input class="input mono" bind:value={searx} placeholder="http://127.0.0.1:8888" onchange={saveWeb} />
      </label>
    {/if}
    <div class="row">
      <button class="btn" onclick={testWeb} disabled={webBusy}>{#if webBusy}<Icon name="sync" size={15} class="spin" />{/if}{t("Test search")}</button>
      {#if webResult}<span class="small" class:errtxt={!webResult.ok} transition:fade={{ duration: 150 }}>{webResult.text}</span>{/if}
    </div>
  </div>
</section>

<section>
  <h3>{t("Mailbox access")}</h3>
  <p class="muted small">{t("The assistant can only read the accounts enabled below. Message content is sent to the AI provider you choose (except local models).")}</p>
  <div class="accs">
    {#each store.accounts as a (a.id)}
      <button class="acc" onclick={() => toggleAccount(a.id)}>
        <span class="switch" class:on={isAllowed(a.id)}></span>
        <span class="dot" style:background={a.color}></span>
        <span class="grow">{a.name || a.email}</span>
        <span class="muted small">{a.email}</span>
      </button>
    {/each}
  </div>
</section>

{#if skillEdit}
  <Dialog title={skillEdit.id ? t("Edit skill") : t("New skill")} width={640} onclose={() => (skillEdit = null)}>
    <label class="field"><span>{t("Name")}</span><input class="input" bind:value={skillEdit.name} placeholder={t("Taken from the SKILL.md front matter if empty")} /></label>
    <label class="field"><span>{t("Description")}</span><input class="input" bind:value={skillEdit.description} /></label>
    <label class="field"><span>{t("Instructions")}</span><textarea class="input mono" rows="12" bind:value={skillEdit.content} placeholder={t("e.g. Always answer in bullet points and end with the next step.")}></textarea></label>
    {#snippet footer()}
      <button class="btn" onclick={() => (skillEdit = null)}>{t("Cancel")}</button>
      <button class="btn primary" onclick={saveSkill}>{t("Save")}</button>
    {/snippet}
  </Dialog>
{/if}

{#if edit}
  <Dialog title={edit.id ? t("Edit {name}", { name: info(edit.kind).label }) : t("Add {name}", { name: info(edit.kind).label })} width={600} onclose={() => (edit = null)}>
    <p class="muted small hint">{info(edit.kind).hint}</p>
    <label class="field"><span>{t("Display name")}</span><input class="input" bind:value={edit.name} /></label>
    {#if edit.kind === "openai" || edit.kind === "grok" || edit.kind === "local"}
      <label class="field"><span>{t("API address (base URL)")}</span><input class="input mono" bind:value={edit.base_url} /></label>
    {:else if edit.kind === "claude_cli"}
      <label class="field">
        <span>{t("Path to the claude command (empty = find in PATH)")}</span>
        <input class="input mono" bind:value={edit.base_url} placeholder="claude" />
      </label>
    {/if}
    {#if edit.kind !== "claude_cli"}
      <label class="field">
        <span>API key {edit.kind === "local" ? t("(if the server requires one)") : ""} {edit.has_key ? t("— saved, type to replace") : ""}</span>
        <input class="input mono" type="password" bind:value={apiKey} placeholder={edit.has_key ? "••••••••" : ""} autocomplete="off" />
      </label>
    {/if}
    <div class="field">
      <span>Model</span>
      <div class="row">
        {#if models.length}
          <select class="input grow" bind:value={edit.model}>
            {#if edit.model && !models.includes(edit.model)}<option value={edit.model}>{edit.model}</option>{/if}
            {#each models as m}<option value={m}>{m}</option>{/each}
          </select>
        {:else}
          <input class="input grow mono" bind:value={edit.model} placeholder={t("Click “Load list” or type a model name")} />
        {/if}
        {#if edit.kind !== "claude_cli"}
          <button class="btn" onclick={loadModels} disabled={!!busy}>{#if busy === "models"}<Icon name="sync" size={15} class="spin" />{/if}{t("Load list")}</button>
        {/if}
      </div>
    </div>
    {#if result}<div class="result" class:bad={!result.ok} transition:slide={{ duration: 160 }}>{result.text}</div>{/if}
    <p class="muted small">{t("API keys are stored in Windows Credential Manager.")}</p>
    {#snippet footer()}
      <button class="btn" onclick={test} disabled={!!busy}>{#if busy === "test"}<Icon name="sync" size={15} class="spin" />{/if}{t("Test")}</button>
      <span class="grow"></span>
      <button class="btn" onclick={() => (edit = null)}>{t("Cancel")}</button>
      <button class="btn primary" onclick={() => save(true)}>{t("Save")}</button>
    {/snippet}
  </Dialog>
{/if}

<style>
  h2 {
    margin: 0 0 6px;
    font-family: var(--font-display);
    font-weight: 600;
    font-size: 1.35rem;
  }
  h3 {
    margin: 0 0 10px;
    font-size: 1.02rem;
  }
  .lead {
    line-height: 1.55;
    margin: 0 0 20px;
    max-width: 760px;
  }
  section {
    margin-bottom: 26px;
  }
  .webcfg {
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-width: 640px;
  }
  .errtxt {
    color: var(--danger);
  }
  .sec-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin-bottom: 4px;
  }
  .sec-row h3 {
    margin: 0;
  }
  .templates {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
  }
  .acc.tall,
  .accs .acc:has(br) {
    height: auto;
    padding: 8px 10px;
  }
  textarea.input {
    height: auto;
    padding: 8px 10px;
    resize: vertical;
    line-height: 1.45;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 12px;
  }
  .prov {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 8px 8px 12px;
  }
  .badge-kind {
    font-size: 0.72rem;
    font-weight: 700;
    padding: 3px 8px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--ai) 14%, transparent);
    color: var(--ai);
    white-space: nowrap;
  }
  .pill {
    font-size: 0.74rem;
    padding: 2px 8px;
    border-radius: 10px;
    background: var(--selected);
    color: var(--text-accent);
    font-weight: 700;
  }
  .add {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
  }
  .kind {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    text-align: left;
    font-size: 0.88rem;
    font-weight: 600;
    transition: box-shadow var(--dur), border-color var(--dur);
  }
  .kind:hover {
    box-shadow: var(--shadow);
    border-color: color-mix(in srgb, var(--accent) 40%, transparent);
  }
  .accs {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-width: 640px;
  }
  .acc {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 38px;
    padding: 0 10px;
    border: none;
    border-radius: var(--radius);
    background: transparent;
    text-align: left;
  }
  .acc:hover {
    background: var(--hover);
  }
  .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
  }
  .hint {
    margin: 0;
    line-height: 1.5;
  }
  .mono {
    font-family: Consolas, monospace;
    font-size: 0.86rem;
  }
  .result {
    padding: 9px 12px;
    border-radius: var(--radius);
    background: color-mix(in srgb, var(--success) 14%, transparent);
    font-size: 0.88rem;
    user-select: text;
  }
  .result.bad {
    background: color-mix(in srgb, var(--danger) 12%, transparent);
    color: var(--danger);
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
</style>
