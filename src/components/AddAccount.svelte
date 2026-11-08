<script lang="ts">
  import { slide, fade, fly } from "svelte/transition";
  import { t } from "../lib/i18n.svelte";
  import { onMount } from "svelte";
  import Dialog from "./Dialog.svelte";
  import Icon from "../lib/Icon.svelte";
  import { api, type Discovery, type Account } from "../lib/api";
  import { store } from "../lib/store.svelte";
  import { pickFolder } from "../lib/tauri";
  import AccountEditor from "./AccountEditor.svelte";

  let name = $state("");
  let email = $state("");
  let password = $state("");
  let showPw = $state(false);
  let spaceId = $state<number | null>(store.settings.space || null);
  let found = $state<Discovery | null>(null);
  let protocol = $state<"imap" | "pop3">("imap");
  let pop3Dir = $state("");
  let busy = $state<"" | "discover" | "login" | "google">("");
  let error = $state("");
  let manual = $state<Partial<Account> | null>(null);
  let hasClient = $state(false);
  let emailEl: HTMLInputElement;

  onMount(async () => {
    const g = await api.googleConfig();
    hasClient = (!!g.client_id && g.has_secret) || !!g.builtin;
    emailEl?.focus();
  });

  const valid = $derived(/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email.trim()));

  async function discover() {
    if (!valid) return;
    error = "";
    found = null;
    busy = "discover";
    try {
      found = await api.discover(email.trim());
      protocol = found.imap ? "imap" : "pop3";
    } catch (e) {
      error = String(e);
    } finally {
      busy = "";
    }
  }

  async function done() {
    if (!found) return discover();
    if (!password) {
      error = t("Enter the password") + (found.provider === "google" ? t(" (Google App Password)") : "");
      return;
    }
    error = "";
    busy = "login";
    try {
      const id = await api.quickAdd({ email: email.trim(), password, displayName: name.trim(), protocol, pop3Dir: protocol === "pop3" ? pop3Dir : null, spaceId });
      await Promise.all([store.loadAccounts(), store.loadFolders()]);
      store.success(t("Account added, downloading mail…"));
      store.settingsTab = `account:${id}`;
    } catch (e) {
      error = String(e);
    } finally {
      busy = "";
    }
  }

  // Google: ask IMAP or POP3 (and the "recent:" mode for POP3) before opening the sign-in window.
  let gAsk = $state(false);
  let gProto = $state<"imap" | "pop3">("imap");
  let gRecent = $state<boolean | null>(null);
  let gPopup = true;
  function google(popup = true) {
    if (!hasClient) {
      store.settingsTab = "google";
      store.info(t("Set up Google OAuth once, then come back to add the account"));
      return;
    }
    gPopup = popup;
    gProto = protocol;
    gRecent = null;
    gAsk = true;
  }
  async function googleGo() {
    if (gProto === "pop3" && gRecent === null) return;
    gAsk = false;
    protocol = gProto;
    error = "";
    busy = "google";
    try {
      const id = await api.googleSignIn({
        protocol: gProto,
        loginHint: email.trim(),
        pop3Dir: gProto === "pop3" ? pop3Dir : undefined,
        popup: gPopup,
        recent: gProto === "pop3" ? !!gRecent : undefined,
      });
      if (spaceId) await api.setAccountSpace(id, spaceId);
      await Promise.all([store.loadAccounts(), store.loadFolders()]);
      store.success(t("Google account added, syncing mail and calendar…"));
      store.settingsTab = `account:${id}`;
    } catch (e) {
      error = String(e);
    } finally {
      busy = "";
    }
  }

  function toManual() {
    const inc = protocol === "pop3" ? found?.pop3 : found?.imap;
    manual = {
      email: email.trim(),
      display_name: name.trim(),
      provider: found?.provider ?? "other",
      protocol,
      in_host: inc?.host ?? "",
      in_port: inc?.port ?? (protocol === "pop3" ? 995 : 993),
      in_security: inc?.security ?? "ssl",
      username: inc?.username || email.trim(),
      out_host: found?.smtp?.host ?? "",
      out_port: found?.smtp?.port ?? 465,
      out_security: found?.smtp?.security ?? "ssl",
      pop3_dir: pop3Dir,
      space_id: spaceId,
    };
  }

  const srvText = (s: { host: string; port: number; security: string } | null | undefined) =>
    s ? `${s.host}:${s.port} · ${s.security === "ssl" ? "SSL/TLS" : s.security === "starttls" ? "STARTTLS" : t("no encryption")}` : "—";
</script>

{#if manual}
  <button class="btn subtle back" onclick={() => (manual = null)}><Icon name="chevronLeft" size={16} />{t("Quick setup")}</button>
  <AccountEditor prefill={manual} initialPassword={password} />
{:else}
  <div class="wrap">
    <h2>{t("Add an email account")}</h2>
    <p class="muted lead">{t("Enter your email and password — Hermio finds the servers for you (like Thunderbird), including school/work mail on Google Workspace.")}</p>

    <div class="form">
      <button class="btn google big-google" onclick={() => google(true)} disabled={!!busy}>
        <svg width="20" height="20" viewBox="0 0 48 48" aria-hidden="true">
          <path fill="#FFC107" d="M43.6 20.5H42V20H24v8h11.3C33.7 32.7 29.2 36 24 36c-6.6 0-12-5.4-12-12s5.4-12 12-12c3.1 0 5.8 1.2 7.9 3.1l5.7-5.7C34 6.1 29.3 4 24 4 12.9 4 4 12.9 4 24s8.9 20 20 20 20-8.9 20-20c0-1.3-.1-2.4-.4-3.5z" />
          <path fill="#FF3D00" d="m6.3 14.7 6.6 4.8C14.7 15.1 19 12 24 12c3.1 0 5.8 1.2 7.9 3.1l5.7-5.7C34 6.1 29.3 4 24 4 16.3 4 9.7 8.3 6.3 14.7z" />
          <path fill="#4CAF50" d="M24 44c5.2 0 9.9-2 13.4-5.2l-6.2-5.2C29.2 35.1 26.7 36 24 36c-5.2 0-9.6-3.3-11.3-8l-6.5 5C9.5 39.6 16.2 44 24 44z" />
          <path fill="#1976D2" d="M43.6 20.5H42V20H24v8h11.3c-.8 2.2-2.2 4.2-4.1 5.6l6.2 5.2C37 39.2 44 34 44 24c0-1.3-.1-2.4-.4-3.5z" />
        </svg>
        <span class="grow">{busy === "google" ? t("Waiting for you to sign in in the Google window…") : t("Continue with Google")}</span>
        <span class="muted small">Gmail · Google Workspace</span>
      </button>
      {#if busy === "google"}
        <div class="muted small">{t("No window, or Google says “this browser may not be secure”?")}
          <button class="lnk" onclick={() => { busy = ""; google(false); }}>{t("Sign in with your browser instead")}</button>
        </div>
      {:else if !hasClient}
        <div class="muted small">{t("First use needs a one-time Google Client ID setup (Settings → Google sign-in).")}</div>
      {/if}
      <div class="or"><span>{t("or use email and password")}</span></div>
      <label class="field">
        <span>{t("Your name")}</span>
        <input class="input big" bind:value={name} placeholder={t("Display name on sent mail")} />
      </label>
      <label class="field">
        <span>{t("Email address")}</span>
        <input
          class="input big"
          bind:this={emailEl}
          bind:value={email}
          placeholder="ban@vidu.com"
          oninput={() => (found = null)}
          onkeydown={(e) => e.key === "Enter" && discover()}
          spellcheck="false"
        />
      </label>
      <label class="field">
        <span>{t("Password")} {found?.provider === "google" ? t("(App Password, if not using Google sign-in)") : ""}</span>
        <div class="pw">
          <input
            class="input big grow"
            type={showPw ? "text" : "password"}
            bind:value={password}
            autocomplete="new-password"
            onkeydown={(e) => e.key === "Enter" && done()}
          />
          <button class="icon-btn" title={showPw ? t("Hide") : t("Show")} onclick={() => (showPw = !showPw)}><Icon name={showPw ? "eyeOff" : "eye"} size={16} /></button>
        </div>
      </label>
      {#if store.spaces.length}
        <label class="field">
          <span>{t("Space")}</span>
          <select class="input big" bind:value={spaceId}>
            <option value={null}>{t("No space")}</option>
            {#each store.spaces as sp}<option value={sp.id}>{sp.name}</option>{/each}
          </select>
        </label>
      {/if}

      {#if busy === "discover"}
        <div class="status" transition:fade={{ duration: 140 }}><Icon name="sync" size={16} class="spin" />{t("Looking up settings for")} {email.split("@")[1]}…</div>
      {/if}

      {#if found}
        <div class="found card" in:fly={{ y: 8, duration: 220 }}>
          <div class="found-head">
            <Icon name="check" size={18} />
            <b>{t("Settings found")}</b>
            <span class="muted small">· {found.source}</span>
          </div>
          {#if found.imap && found.pop3}
            <div class="choose">
              <button class="opt" class:on={protocol === "imap"} onclick={() => (protocol = "imap")}>
                <span class="radio"></span>
                <span><b>IMAP</b> <span class="muted small">{t("— recommended, two-way sync with the server")}</span><br /><span class="mono">{srvText(found.imap)}</span></span>
              </button>
              <button class="opt" class:on={protocol === "pop3"} onclick={() => (protocol = "pop3")}>
                <span class="radio"></span>
                <span><b>POP3</b> <span class="muted small">{t("— downloads mail to this PC as .eml files")}</span><br /><span class="mono">{srvText(found.pop3)}</span></span>
              </button>
            </div>
          {:else}
            <div class="mono">{found.imap ? "IMAP" : "POP3"} {srvText(found.imap ?? found.pop3)}</div>
          {/if}
          <div class="mono muted">SMTP {srvText(found.smtp)}</div>
          {#if protocol === "pop3"}
            <div class="row" transition:slide={{ duration: 180 }}>
              <span class="muted small grow">{t("Store mail in:")} <span class="mono">{pop3Dir || t("default data folder")}</span></span>
              <button class="btn subtle" onclick={async () => (pop3Dir = (await pickFolder(t("Choose a folder for POP3 mail"))) ?? pop3Dir)}><Icon name="folder" size={16} />{t("Choose…")}</button>
            </div>
          {/if}
          {#if found.note}<div class="note small">{found.note}</div>{/if}
        </div>

        {#if found.provider === "google"}
          <button class="btn google" onclick={() => google(true)} disabled={!!busy}>
            <svg width="18" height="18" viewBox="0 0 48 48" aria-hidden="true">
              <path fill="#FFC107" d="M43.6 20.5H42V20H24v8h11.3C33.7 32.7 29.2 36 24 36c-6.6 0-12-5.4-12-12s5.4-12 12-12c3.1 0 5.8 1.2 7.9 3.1l5.7-5.7C34 6.1 29.3 4 24 4 12.9 4 4 12.9 4 24s8.9 20 20 20 20-8.9 20-20c0-1.3-.1-2.4-.4-3.5z" />
              <path fill="#FF3D00" d="m6.3 14.7 6.6 4.8C14.7 15.1 19 12 24 12c3.1 0 5.8 1.2 7.9 3.1l5.7-5.7C34 6.1 29.3 4 24 4 16.3 4 9.7 8.3 6.3 14.7z" />
              <path fill="#4CAF50" d="M24 44c5.2 0 9.9-2 13.4-5.2l-6.2-5.2C29.2 35.1 26.7 36 24 36c-5.2 0-9.6-3.3-11.3-8l-6.5 5C9.5 39.6 16.2 44 24 44z" />
              <path fill="#1976D2" d="M43.6 20.5H42V20H24v8h11.3c-.8 2.2-2.2 4.2-4.1 5.6l6.2 5.2C37 39.2 44 34 44 24c0-1.3-.1-2.4-.4-3.5z" />
            </svg>
            {busy === "google" ? t("Waiting for sign-in in your browser…") : hasClient ? t("Sign in with Google (recommended, also syncs calendar)") : t("Sign in with Google — needs a one-time setup")}
          </button>
        {/if}
      {/if}

      {#if error}<div class="err selectable" transition:slide={{ duration: 180 }}>{error}</div>{/if}

      <div class="row actions">
        <button class="btn subtle" onclick={toManual}><Icon name="settings" size={16} />{t("Advanced setup")}</button>
        <span class="grow"></span>
        {#if !found}
          <button class="btn primary big" onclick={discover} disabled={!valid || !!busy}>{t("Continue")}</button>
        {:else}
          <button class="btn primary big" onclick={done} disabled={!!busy || !password}>
            {#if busy === "login"}<Icon name="sync" size={16} class="spin" />{t("Signing in…")}{:else}{t("Done")}{/if}
          </button>
        {/if}
      </div>
    </div>
  </div>
{/if}

{#if gAsk}
  <Dialog title={t("Connect your Google account")} width={560} onclose={() => (gAsk = false)}>
    <div class="choices">
      <button class="choice" class:on={gProto === "imap"} onclick={() => (gProto = "imap")}>
        <span class="radio"></span>
        <span><b>IMAP</b> <span class="pill">{t("Recommended")}</span><br /><span class="muted small">{t("Two-way sync: folders, read status and deletions match Gmail on every device. Calendar sync included.")}</span></span>
      </button>
      <button class="choice" class:on={gProto === "pop3"} onclick={() => (gProto = "pop3")}>
        <span class="radio"></span>
        <span><b>POP3</b><br /><span class="muted small">{t("Downloads mail to this PC as .eml files. Changes here don't sync back to Gmail. POP must be enabled in Gmail → Settings → Forwarding and POP/IMAP.")}</span></span>
      </button>
    </div>
    {#if gProto === "pop3"}
      <div class="pop-opts">
        <div class="row">
          <span class="muted small grow">{t("Store mail in:")} <span class="mono">{pop3Dir || t("default data folder")}</span></span>
          <button class="btn subtle" onclick={async () => (pop3Dir = (await pickFolder(t("Choose a folder for POP3 mail"))) ?? pop3Dir)}><Icon name="folder" size={16} />{t("Choose…")}</button>
        </div>
        <div class="q">
          <b>{t("Use Gmail's “recent:” mode?")}</b>
          <p class="muted small">
            {t("Recent mode downloads the last 30 days of mail even if another device or app already downloaded it over POP. Without it, Gmail only hands out each message once to POP clients, following your Gmail POP setting (“all mail” or “from now on”), so mail another POP app already took won't arrive here.")}
          </p>
          <div class="choices two">
            <button class="choice" class:on={gRecent === true} onclick={() => (gRecent = true)}>
              <span class="radio"></span><span><b>{t("Yes, use recent mode")}</b><br /><span class="muted small">{t("Best if you also read this mailbox on a phone or another app.")}</span></span>
            </button>
            <button class="choice" class:on={gRecent === false} onclick={() => (gRecent = false)}>
              <span class="radio"></span><span><b>{t("No, normal POP")}</b><br /><span class="muted small">{t("Follows your Gmail POP setting; each message is downloaded once.")}</span></span>
            </button>
          </div>
        </div>
      </div>
    {/if}
    {#snippet footer()}
      <button class="btn" onclick={() => (gAsk = false)}>{t("Cancel")}</button>
      <button class="btn primary" onclick={googleGo} disabled={gProto === "pop3" && gRecent === null}>{t("Continue with Google")}</button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .choices {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .choice {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 12px;
    border-radius: 10px;
    border: 1px solid var(--border-strong);
    background: var(--surface);
    text-align: left;
    line-height: 1.45;
  }
  .choice.on {
    border-color: var(--accent);
    background: var(--selected);
  }
  .radio {
    flex: none;
    width: 16px;
    height: 16px;
    margin-top: 2px;
    border-radius: 50%;
    border: 1.5px solid var(--border-strong);
    transition: border var(--dur);
  }
  .choice.on .radio {
    border: 5px solid var(--accent);
  }
  .pill {
    font-size: 0.7rem;
    padding: 1px 7px;
    border-radius: 10px;
    background: var(--accent);
    color: white;
    font-weight: 700;
  }
  .pop-opts {
    margin-top: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    animation: slide-up 180ms var(--ease);
  }
  .q p {
    margin: 4px 0 8px;
    line-height: 1.5;
  }
  .wrap {
    max-width: 560px;
  }
  h2 {
    margin: 0 0 6px;
    font-family: var(--font-display);
    font-weight: 600;
    font-size: 1.45rem;
  }
  .lead {
    margin: 0 0 22px;
    line-height: 1.5;
  }
  .back {
    margin: -10px 0 8px -10px;
  }
  .form {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .input.big {
    height: 38px;
    font-size: 1rem;
  }
  .pw {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .status {
    display: flex;
    gap: 8px;
    align-items: center;
    color: var(--text-2);
  }
  .found {
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    animation: slide-up 200ms var(--ease);
  }
  .found-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .found-head :global(.icon) {
    color: var(--success);
  }
  .choose {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .opt {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 8px 10px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: transparent;
    text-align: left;
  }
  .opt.on {
    border-color: var(--accent);
    background: var(--selected);
  }
  .radio {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    border: 2px solid var(--text-3);
    margin-top: 2px;
    flex: none;
  }
  .opt.on .radio {
    border: 5px solid var(--accent);
  }
  .mono {
    font-family: Consolas, monospace;
    font-size: 0.85rem;
  }
  .note {
    padding: 8px 10px;
    border-radius: var(--radius);
    background: var(--surface-3);
    color: var(--text-2);
  }
  .big-google {
    height: 48px !important;
    font-size: 1rem;
    padding: 0 16px;
  }
  .or {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--text-3);
    font-size: 0.85rem;
    margin: 4px 0;
  }
  .or::before,
  .or::after {
    content: "";
    flex: 1;
    height: 1px;
    background: var(--border);
  }
  .lnk {
    border: none;
    background: none;
    padding: 0;
    color: var(--text-accent);
    font: inherit;
    text-decoration: underline;
  }
  .google {
    height: 42px;
    justify-content: flex-start;
    gap: 12px;
    font-weight: 600;
  }
  .err {
    padding: 10px 12px;
    border-radius: var(--radius);
    background: color-mix(in srgb, var(--danger) 12%, transparent);
    color: var(--danger);
    font-size: 0.9rem;
    white-space: pre-wrap;
  }
  .btn.big {
    height: 38px;
    padding: 0 22px;
  }
  .actions {
    padding-top: 4px;
  }
</style>