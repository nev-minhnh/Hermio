<script lang="ts">
  import { slide } from "svelte/transition";
  import { pill } from "../lib/motion";
  import { t } from "../lib/i18n.svelte";
  import { onMount } from "svelte";
  import Icon from "../lib/Icon.svelte";
  import { api, type Account } from "../lib/api";
  import { store } from "../lib/store.svelte";
  import { pickFolder } from "../lib/tauri";

  let { accountId = 0, prefill = null, initialPassword = "" }: { accountId?: number; prefill?: Partial<Account> | null; initialPassword?: string } = $props();

  const blank: Account = {
    id: 0, name: "", email: "", display_name: "", protocol: "imap", auth: "password", provider: "other",
    in_host: "", in_port: 993, in_security: "ssl", out_host: "", out_port: 465, out_security: "ssl",
    username: "", pop3_dir: "", pop3_leave_on_server: true, pop3_recent_mode: false, color: "#1d4fe0",
    sort_order: 0, sig_new: null, sig_reply: null, calendar_sync: true, enabled: true, last_sync: 0, last_error: "",
    smtp_username: "", space_id: null, sync_limit: 2000, body_limit: 200, role_map: "{}",
  };

  let a = $state<Account>({ ...blank });
  let password = $state("");
  let smtpSeparate = $state(false);
  let smtpPassword = $state("");
  let roleMap = $state<Record<string, string>>({});
  let busy = $state(false);
  let testResult = $state<{ ok: boolean; text: string } | null>(null);
  let defaultDir = $state("");
  let advanced = $state(false);
  const colors = ["#1d4fe0", "#c239b3", "#038387", "#ca5010", "#8764b8", "#498205", "#d13438", "#986f0b", "#5b5fc7", "#e3008c"];
  const roles: [string, string][] = [["sent", t("Sent")], ["drafts", t("Drafts")], ["trash", t("Deleted items")], ["junk", t("Junk")], ["archive", t("Archive")]];

  onMount(async () => {
    if (accountId) {
      const found = store.accountById(accountId);
      if (found) a = { ...blank, ...found };
      defaultDir = await api.defaultPop3Dir(accountId);
    } else {
      a = { ...blank, ...(prefill ?? {}) };
      password = initialPassword;
      advanced = true;
      defaultDir = (await api.dataDir()) + "\\pop3\\<id>";
    }
    smtpSeparate = !!a.smtp_username;
    try {
      roleMap = JSON.parse(a.role_map || "{}");
    } catch {
      roleMap = {};
    }
  });

  const accountFolders = $derived(store.folders.filter((f) => f.account_id === a.id && f.selectable));

  async function guess() {
    if (!a.email.includes("@")) return;
    busy = true;
    try {
      const d = await api.discover(a.email);
      a.provider = d.provider;
      const inc = (a.protocol === "pop3" ? d.pop3 : d.imap) ?? d.imap ?? d.pop3;
      if (inc) {
        a.in_host = inc.host;
        a.in_port = inc.port;
        a.in_security = inc.security;
        a.username = inc.username;
      }
      if (d.smtp) {
        a.out_host = d.smtp.host;
        a.out_port = d.smtp.port;
        a.out_security = d.smtp.security;
      }
      store.info(t("Settings filled in ({source})", { source: d.source }));
    } catch (e) {
      store.error(String(e));
    } finally {
      busy = false;
    }
  }

  async function choosePop3Dir() {
    const d = await pickFolder(t("Choose a folder for POP3 mail"));
    if (!d) return;
    if (a.id && a.protocol === "pop3") {
      const move = confirm(t("Move downloaded mail to\n{dir}\n\nOK: move existing data.\nCancel: only use the new folder for new mail.", { dir: d }));
      busy = true;
      try {
        const n = await api.relocatePop3(a.id, d, move);
        a.pop3_dir = d;
        store.success(move ? t("Moved {n} messages to the new folder", { n }) : t("Storage folder changed"));
        await store.loadAccounts();
      } catch (e) {
        store.error(String(e));
      } finally {
        busy = false;
      }
    } else {
      a.pop3_dir = d;
    }
  }

  async function save() {
    if (!a.email.includes("@")) return store.error(t("Invalid email address"));
    if (!a.in_host || !a.out_host) return store.error(t("Enter the incoming and outgoing servers"));
    if (!a.id && a.auth === "password" && !password) return store.error(t("Enter the password"));
    if (!smtpSeparate) a.smtp_username = "";
    a.role_map = JSON.stringify(Object.fromEntries(Object.entries(roleMap).filter(([, v]) => v)));
    busy = true;
    try {
      const id = await api.saveAccount($state.snapshot(a), password || null, smtpSeparate ? smtpPassword || null : null);
      password = "";
      smtpPassword = "";
      await Promise.all([store.loadAccounts(), store.loadFolders()]);
      store.success(accountId ? t("Account saved") : t("Account added, syncing…"));
      if (!accountId) store.settingsTab = `account:${id}`;
      else if (a.role_map !== (store.accountById(id)?.role_map ?? "")) api.syncNow(id);
    } catch (e) {
      store.error(String(e));
    } finally {
      busy = false;
    }
  }

  async function reSignIn() {
    busy = true;
    try {
      await api.googleSignIn({ accountId: a.id, protocol: a.protocol, loginHint: a.email, popup: true });
      await store.loadAccounts();
      store.success(t("Signed in again"));
    } catch (e) {
      store.error(String(e));
    } finally {
      busy = false;
    }
  }

  async function test() {
    testResult = null;
    busy = true;
    try {
      testResult = { ok: true, text: await api.testAccount(a.id) };
    } catch (e) {
      testResult = { ok: false, text: String(e) };
    } finally {
      busy = false;
    }
  }

  async function remove() {
    if (!confirm(t("Remove {email} from Hermio?\n\nMail on the server is not affected. For POP3, downloaded .eml files are kept.", { email: a.email }))) return;
    await api.deleteAccount(a.id);
    await Promise.all([store.loadAccounts(), store.loadFolders()]);
    store.success(t("{email} removed", { email: a.email }));
    store.open("unified:inbox");
    store.route = "settings";
    store.settingsTab = "accounts";
  }
</script>

<h2>{accountId ? a.email : t("Advanced setup")}</h2>
{#if a.last_error && accountId}
  <div class="warn err" transition:slide={{ duration: 180 }}><Icon name="warning" size={16} /><span class="grow selectable">{a.last_error}</span></div>
{/if}

<div class="form">
  <div class="grid2">
    <label class="field">
      <span>Email</span>
      <input class="input" bind:value={a.email} placeholder="ban@vidu.com" disabled={!!accountId && a.auth === "oauth_google"} />
    </label>
    <label class="field">
      <span>{t("Display name when sending")}</span>
      <input class="input" bind:value={a.display_name} placeholder={t("Jane Doe")} />
    </label>
    <label class="field">
      <span>{t("Account name (in the list)")}</span>
      <input class="input" bind:value={a.name} placeholder={a.email || t("Work")} />
    </label>
    <label class="field">
      <span>{t("Space")}</span>
      <select class="input" bind:value={a.space_id}>
        <option value={null}>{t("No space")}</option>
        {#each store.spaces as sp}<option value={sp.id}>{sp.name}</option>{/each}
      </select>
    </label>
  </div>

  <div class="field">
    <span>{t("Account color")}</span>
    <div class="colors">
      {#each colors as c}
        <button class="sw" class:on={a.color === c} style:background={c} onclick={() => (a.color = c)} aria-label={c}></button>
      {/each}
    </div>
  </div>

  {#if a.auth === "oauth_google"}
    <div class="oauth card">
      <Icon name="key" size={18} />
      <div class="grow"><b>{t("Signed in with Google (OAuth)")}</b><div class="muted small">{t("No password is stored; the token lives in Windows Credential Manager.")}</div></div>
      <button class="btn" onclick={reSignIn} disabled={busy}>{t("Sign in again")}</button>
    </div>
  {:else}
    <label class="field">
      <span>{t("Password")}{accountId ? t(" (leave empty to keep)") : ""}</span>
      <input class="input" type="password" bind:value={password} autocomplete="new-password" />
    </label>
  {/if}

  <button class="adv-toggle" onclick={() => (advanced = !advanced)}>
    <span class="chev" class:open={advanced}><Icon name="chevronRight" size={14} /></span> {t("Server & advanced settings")}
  </button>

  {#if advanced}
    <div class="adv" transition:slide={{ duration: 220 }}>
    <div class="field">
      <span>{t("Incoming protocol")}</span>
      <div class="segmented" use:pill>
        <button class:on={a.protocol === "imap"} onclick={() => !accountId && (a.protocol = "imap")} disabled={!!accountId}>IMAP</button>
        <button class:on={a.protocol === "pop3"} onclick={() => !accountId && (a.protocol = "pop3")} disabled={!!accountId}>POP3</button>
      </div>
      {#if accountId}<span class="muted small">{t("To change protocol, add a new account with the other protocol.")}</span>{/if}
    </div>

    <fieldset>
      <legend>{t("Incoming server (")}{a.protocol.toUpperCase()})</legend>
      <div class="srv">
        <label class="field grow"><span>{t("Server")}</span><input class="input" bind:value={a.in_host} placeholder="imap.vidu.com" /></label>
        <label class="field port"><span>{t("Port")}</span><input class="input" type="number" bind:value={a.in_port} /></label>
        <label class="field"><span>{t("Security")}</span>
          <select class="input" bind:value={a.in_security}>
            <option value="ssl">SSL/TLS</option>
            <option value="starttls">STARTTLS</option>
            <option value="none">{t("None")}</option>
          </select>
        </label>
      </div>
      <label class="field"><span>{t("Username")}</span><input class="input" bind:value={a.username} placeholder={a.email} /></label>
    </fieldset>

    <fieldset>
      <legend>{t("Outgoing server (SMTP)")}</legend>
      <div class="srv">
        <label class="field grow"><span>{t("Server")}</span><input class="input" bind:value={a.out_host} placeholder="smtp.vidu.com" /></label>
        <label class="field port"><span>{t("Port")}</span><input class="input" type="number" bind:value={a.out_port} /></label>
        <label class="field"><span>{t("Security")}</span>
          <select class="input" bind:value={a.out_security}>
            <option value="ssl">SSL/TLS</option>
            <option value="starttls">STARTTLS</option>
            <option value="none">{t("None")}</option>
          </select>
        </label>
      </div>
      {#if a.auth === "password"}
        <button class="toggle" onclick={() => (smtpSeparate = !smtpSeparate)}>
          <span class="switch" class:on={smtpSeparate}></span>{t("Outgoing server uses a separate username/password")}
        </button>
        {#if smtpSeparate}
          <div class="grid2">
            <label class="field"><span>{t("SMTP username")}</span><input class="input" bind:value={a.smtp_username} /></label>
            <label class="field"><span>{t("SMTP password")}</span><input class="input" type="password" bind:value={smtpPassword} placeholder={accountId ? t("(leave empty to keep)") : ""} /></label>
          </div>
        {/if}
      {/if}
    </fieldset>
    {#if !accountId}
      <div class="row"><button class="btn subtle" onclick={guess} disabled={busy}><Icon name="globe" size={16} />{t("Detect settings from the email again")}</button></div>
    {/if}

    {#if a.protocol === "pop3"}
      <fieldset>
        <legend>{t("POP3 storage")}</legend>
        <div class="field">
          <span>{t("Storage folder for this account")}</span>
          <div class="row">
            <input class="input grow mono" readonly value={a.pop3_dir || defaultDir} />
            <button class="btn" onclick={choosePop3Dir} disabled={busy}><Icon name="folder" size={16} />{t("Change…")}</button>
          </div>
          <span class="muted small">{t("Each message is saved as an .eml file, split by month. Each account can live on its own drive/folder.")}</span>
        </div>
        <button class="toggle" onclick={() => (a.pop3_leave_on_server = !a.pop3_leave_on_server)}>
          <span class="switch" class:on={a.pop3_leave_on_server}></span>{t("Leave a copy on the server")}
        </button>
        {#if a.provider === "google"}
          <button class="toggle" onclick={() => (a.pop3_recent_mode = !a.pop3_recent_mode)}>
            <span class="switch" class:on={a.pop3_recent_mode}></span>{t("Gmail “recent” mode (last 30 days)")}
          </button>
        {/if}
      </fieldset>
    {:else}
      <fieldset>
        <legend>{t("IMAP sync")}</legend>
        <div class="grid2">
          <label class="field">
            <span>{t("Newest messages downloaded per folder (first sync)")}</span>
            <input class="input" type="number" min="100" step="100" bind:value={a.sync_limit} />
          </label>
          <label class="field">
            <span>{t("Message bodies downloaded per batch (for content search)")}</span>
            <input class="input" type="number" min="0" step="50" bind:value={a.body_limit} />
          </label>
        </div>
        {#if accountId && accountFolders.length}
          <div class="field">
            <span>{t("Special folders (empty = detect automatically)")}</span>
            <div class="grid2">
              {#each roles as [role, label]}
                <label class="field small-field">
                  <span>{label}</span>
                  <select class="input" bind:value={roleMap[role]}>
                    <option value="">{t("Automatic")}</option>
                    {#each accountFolders as f}<option value={f.path}>{f.path}</option>{/each}
                  </select>
                </label>
              {/each}
            </div>
          </div>
        {/if}
      </fieldset>
    {/if}
    </div>
  {/if}

  {#if accountId}
    <fieldset>
      <legend>{t("Custom")}</legend>
      <div class="grid2">
        <label class="field">
          <span>{t("Signature for new messages")}</span>
          <select class="input" bind:value={a.sig_new}>
            <option value={null}>{t("None")}</option>
            {#each store.signatures as s}<option value={s.id}>{s.name}</option>{/each}
          </select>
        </label>
        <label class="field">
          <span>{t("Signature for replies/forwards")}</span>
          <select class="input" bind:value={a.sig_reply}>
            <option value={null}>{t("None")}</option>
            {#each store.signatures as s}<option value={s.id}>{s.name}</option>{/each}
          </select>
        </label>
      </div>
      {#if a.auth === "oauth_google"}
        <button class="toggle" onclick={() => (a.calendar_sync = !a.calendar_sync)}>
          <span class="switch" class:on={a.calendar_sync}></span>{t("Sync Google Calendar")}
        </button>
      {/if}
      <button class="toggle" onclick={() => (a.enabled = !a.enabled)}>
        <span class="switch" class:on={a.enabled}></span>{t("Sync this account")}
      </button>
    </fieldset>
  {/if}

  {#if testResult}
    <div class="note" class:bad={!testResult.ok} class:good={testResult.ok} transition:slide={{ duration: 180 }}>{testResult.text}</div>
  {/if}

  <div class="row actions">
    <button class="btn primary" onclick={save} disabled={busy}>{accountId ? t("Save changes") : t("Add account")}</button>
    {#if accountId}
      <button class="btn" onclick={test} disabled={busy}>{#if busy}<Icon name="sync" size={16} class="spin" />{/if}{t("Test connection")}</button>
      <button class="btn" onclick={() => store.syncNow(a.id)}><Icon name="sync" size={16} />{t("Sync now")}</button>
      <span class="grow"></span>
      <button class="btn danger" onclick={remove}><Icon name="trash" size={16} />{t("Remove account")}</button>
    {/if}
  </div>
</div>

<style>
  h2 {
    margin: 0 0 18px;
    font-family: var(--font-display);
    font-weight: 600;
    font-size: 1.35rem;
  }
  .form {
    display: flex;
    flex-direction: column;
    gap: 16px;
    max-width: 760px;
  }
  .chev {
    display: inline-flex;
    transition: transform 200ms var(--ease);
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .adv {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .grid2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px 16px;
  }
  fieldset {
    border: 1px solid var(--border);
    border-radius: var(--radius-l);
    padding: 12px 16px 16px;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  legend {
    padding: 0 6px;
    font-weight: 700;
    font-size: 0.9rem;
  }
  .srv {
    display: flex;
    gap: 10px;
    align-items: flex-end;
  }
  .port {
    width: 96px;
  }
  .mono {
    font-family: Consolas, monospace;
    font-size: 0.86rem;
  }
  .toggle {
    display: flex;
    align-items: center;
    gap: 10px;
    border: none;
    background: none;
    padding: 2px 0;
    text-align: left;
  }
  .adv-toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    border: none;
    background: none;
    padding: 4px 0;
    font-weight: 700;
    color: var(--text-accent);
    align-self: flex-start;
  }
  .colors {
    display: flex;
    gap: 8px;
  }
  .sw {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    border: 2px solid transparent;
    box-shadow: 0 0 0 1px var(--border);
  }
  .sw.on {
    border-color: var(--surface);
    box-shadow: 0 0 0 2px var(--text);
  }
  .oauth {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 14px;
  }
  .oauth :global(.icon) {
    color: var(--text-accent);
  }
  .warn,
  .note {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 10px 12px;
    border-radius: var(--radius);
    font-size: 0.9rem;
    user-select: text;
    background: var(--surface-3);
    color: var(--text-2);
  }
  .note.good {
    background: color-mix(in srgb, var(--success) 14%, transparent);
    color: var(--text);
  }
  .note.bad,
  .warn.err {
    background: color-mix(in srgb, var(--danger) 12%, transparent);
    color: var(--danger);
  }
  .warn.err {
    margin-bottom: 14px;
  }
  .small-field span {
    font-weight: 500;
  }
  .actions {
    padding-top: 4px;
  }
</style>
