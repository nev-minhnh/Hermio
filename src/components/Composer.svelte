<script lang="ts">
  import { slide, fade, scale } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { t, locale } from "../lib/i18n.svelte";
  import { onDestroy, onMount } from "svelte";
  import Icon from "../lib/Icon.svelte";
  import { store } from "../lib/store.svelte";
  import { api, type Outgoing, type MessageBody } from "../lib/api";
  import { addrOnly, escapeHtml, fileSize, fullTime, splitAddresses } from "../lib/util";
  import { pickFiles } from "../lib/tauri";
  import RichEditor from "./RichEditor.svelte";
  import RecipientInput from "./RecipientInput.svelte";
  import WritingAssistant from "./WritingAssistant.svelte";

  let wa = $state<ReturnType<typeof WritingAssistant>>();
  let aiMenu = $state(false);
  function aiPick(e: MouseEvent, what: "check" | "ask" | "translate" | "define") {
    aiMenu = false;
    if (what === "check") return wa?.checkWriting();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    wa?.assist(what, r.left, r.bottom);
  }

  const init = store.compose!;
  const src = init.source;

  function defaultAccount(): number {
    if (src) return src.account_id;
    const v = store.view;
    if (v.startsWith("folder:")) {
      const f = store.folderById(Number(v.slice(7)));
      if (f) return f.account_id;
    }
    if (v.startsWith("account:")) return Number(v.slice(8));
    return store.accounts[0]?.id ?? 0;
  }

  let accountId = $state(defaultAccount());
  let to = $state<string[]>(init.to ?? []);
  let cc = $state<string[]>([]);
  let bcc = $state<string[]>([]);
  let showCc = $state(false);
  let showBcc = $state(false);
  let subject = $state("");
  let editorEl = $state<HTMLDivElement>();
  let initialHtml = $state<string | null>(null);
  let attachments = $state<{ path?: string; from_message?: [number, number]; name: string; size?: number }[]>([]);
  let sending = $state(false);
  let dirty = $state(false);
  // ---- autosave to the Drafts folder ----
  /** One key for all saved versions of this message (each save replaces the previous one). */
  const draftKey: string = init.restore?.draft_key || (crypto.randomUUID?.() ?? `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`);
  /** The draft to remove on the next save: the one this was opened from, then (POP3) the last autosave. */
  let draftId: number | null = init.restore ? (init.restore.draft_id ?? null) : init.mode === "draft" ? (init.source?.id ?? null) : null;
  let savedAccount: number | null = null;
  let saveState = $state<"idle" | "saving" | "saved" | "error">("idle");
  let savedAt = $state(0);
  let saveError = $state("");
  let lastSave = 0;
  let autoTimer: ReturnType<typeof setTimeout> | undefined;
  let closed = false;
  let inReplyTo = "";
  let references = "";
  let sigId = $state<number | null>(null);

  const account = $derived(store.accountById(accountId));
  const title = $derived(
    { new: t("New mail"), reply: t("Reply"), replyAll: t("Reply all"), forward: t("Forward"), draft: t("Drafts") }[init.mode],
  );

  /** Strip anything that could escape the editor (styles, scripts, handlers). */
  function sanitize(html: string): string {
    const doc = new DOMParser().parseFromString(html, "text/html");
    doc.querySelectorAll("script,style,link,meta,iframe,object,embed,base,title,form").forEach((n) => n.remove());
    doc.querySelectorAll("*").forEach((n) => {
      for (const a of [...n.attributes]) if (/^on/i.test(a.name) || (a.name === "href" && /^\s*javascript:/i.test(a.value))) n.removeAttribute(a.name);
    });
    return doc.body.innerHTML;
  }

  function sigHtml(id: number | null) {
    const s = store.signatures.find((x) => x.id === id);
    return s ? `<div class="hermio-signature"><br>${s.html}</div>` : `<div class="hermio-signature"></div>`;
  }

  function header(b: MessageBody | null) {
    if (!src) return "";
    const line = (k: string, v: string) => (v ? `<b>${k}:</b> ${escapeHtml(v)}<br>` : "");
    return `<hr style="border:none;border-top:1px solid #ccc;margin:16px 0 8px">
<div style="font-size:13px">${line(t("From"), src.from_name ? `${src.from_name} <${src.from_addr}>` : src.from_addr)}${line(t("Sent"), fullTime(src.date))}${line(t("To"), b?.to || src.to_list)}${line("Cc", b?.cc || src.cc_list)}${line(t("Subject"), src.subject)}</div>`;
  }

  onMount(async () => {
    if (init.restore) {
      const r = init.restore;
      accountId = r.account_id;
      to = [...r.to];
      cc = [...r.cc];
      bcc = [...r.bcc];
      showCc = cc.length > 0;
      showBcc = bcc.length > 0;
      subject = r.subject;
      inReplyTo = r.in_reply_to;
      references = r.references;
      attachments = r.attachments.map((a) => ({ ...a }));
      initialHtml = r.html;
      dirty = true;
      return;
    }
    const me = (id: number) => (store.accountById(id)?.email ?? "").toLowerCase();
    let body: MessageBody | null = null;
    if (src) {
      try {
        body = await api.message(src.id);
      } catch (e) {
        store.error(String(e));
      }
    }
    const a = store.accountById(accountId);
    const replying = init.mode === "reply" || init.mode === "replyAll";
    sigId = replying || init.mode === "forward" ? (a?.sig_reply ?? null) : (a?.sig_new ?? null);
    const quoted = body ? sanitize(body.html) : "";

    if (src && replying) {
      const sender = body?.reply_to ? splitAddresses(body.reply_to) : [src.from_name ? `${src.from_name} <${src.from_addr}>` : src.from_addr];
      to = sender;
      if (init.mode === "replyAll") {
        const self = me(src.account_id);
        const seen = new Set([...to.map(addrOnly), self]);
        const more = splitAddresses(body?.to || src.to_list).filter((x) => !seen.has(addrOnly(x)));
        more.forEach((x) => seen.add(addrOnly(x)));
        to = [...to, ...more];
        cc = splitAddresses(body?.cc || src.cc_list).filter((x) => !seen.has(addrOnly(x)));
        showCc = cc.length > 0;
      }
      subject = /^(re|tl|trả lời)\s*:/i.test(src.subject) ? src.subject : `Re: ${src.subject}`;
      if (src.message_id) {
        inReplyTo = `<${src.message_id.replace(/[<>]/g, "")}>`;
        references = [body?.references ?? "", inReplyTo].join(" ").trim();
      }
      initialHtml = `${aiHtml()}${sigHtml(sigId)}<div class="hermio-quote">${header(body)}<blockquote>${quoted}</blockquote></div>`;
    } else if (src && init.mode === "forward") {
      subject = /^(fw|fwd|chuyển tiếp)\s*:/i.test(src.subject) ? src.subject : `Fw: ${src.subject}`;
      attachments = (body?.attachments ?? []).filter((x) => !x.inline).map((x) => ({ from_message: [src.id, x.index] as [number, number], name: x.name, size: x.size }));
      initialHtml = `${aiHtml()}${sigHtml(sigId)}<div class="hermio-quote">${header(body)}${quoted}</div>`;
    } else if (src && init.mode === "draft") {
      to = splitAddresses(body?.to || src.to_list);
      cc = splitAddresses(body?.cc || src.cc_list);
      showCc = cc.length > 0;
      subject = src.subject;
      attachments = (body?.attachments ?? []).filter((x) => !x.inline).map((x) => ({ from_message: [src.id, x.index] as [number, number], name: x.name, size: x.size }));
      initialHtml = quoted;
    } else {
      initialHtml = `${aiHtml()}${sigHtml(sigId)}`;
    }
  });

  /** AI-drafted text (human-in-the-loop: it lands in the editor for review, never sent directly). */
  function aiHtml() {
    if (!init.aiText?.trim()) return `<div><br></div>`;
    return init.aiText
      .trim()
      .split(/\n{2,}/)
      .map((p) => `<div>${escapeHtml(p).replace(/\n/g, "<br>")}</div><div><br></div>`)
      .join("");
  }

  function changeSignature(id: number | null) {
    sigId = id;
    const host = editorEl?.querySelector(".hermio-signature");
    const html = store.signatures.find((x) => x.id === id)?.html;
    if (host) host.innerHTML = html ? `<br>${html}` : "";
    else if (editorEl && html) editorEl.insertAdjacentHTML("beforeend", sigHtml(id));
  }

  function outgoing(): Outgoing {
    return {
      account_id: accountId,
      to: [...to],
      cc: [...cc],
      bcc: [...bcc],
      subject,
      html: editorEl?.innerHTML ?? "",
      in_reply_to: inReplyTo,
      references,
      attachments: attachments.map(({ path, from_message, name }) => ({ path, from_message, name })),
      reply_of: init.restore ? (init.restore.reply_of ?? null) : init.mode === "reply" || init.mode === "replyAll" ? (src?.id ?? null) : null,
      draft_id: draftId,
      draft_key: draftKey,
    };
  }

  /** Something worth keeping: text, a subject, recipients or attachments. */
  function hasContent() {
    const text = (editorEl?.innerText ?? "").replace(/\s+/g, "");
    return !!(subject.trim() || to.length || cc.length || bcc.length || attachments.length || text.length > 0);
  }
  /** Save a new version to the Drafts folder (replacing the previous one). */
  async function autosave(force = false) {
    clearTimeout(autoTimer);
    if (closed && !force) return;
    if (!dirty && !force) return;
    if (!hasContent() || saveState === "saving") return;
    const wait = 8000 - (Date.now() - lastSave);
    if (!force && wait > 0) {
      autoTimer = setTimeout(() => autosave(), wait);
      return;
    }
    saveState = "saving";
    dirty = false;
    lastSave = Date.now();
    const acc = accountId;
    try {
      // Moved to another mailbox: its versions in the previous one go.
      if (savedAccount !== null && savedAccount !== acc) await api.discardDraft(savedAccount, draftKey, null).catch(() => {});
      const r = await api.saveDraft(outgoing(), true);
      savedAccount = acc;
      draftId = r.local_id ?? null;
      saveState = "saved";
      savedAt = Date.now();
      saveError = "";
    } catch (e) {
      saveState = "error";
      saveError = String(e).replace(/^Error: /, "");
      dirty = true;
    }
  }
  function scheduleAutosave() {
    dirty = true;
    clearTimeout(autoTimer);
    autoTimer = setTimeout(() => autosave(), 2500);
  }
  // Recipients, subject and attachments count as edits too.
  let seenFields = "";
  $effect(() => {
    const f = JSON.stringify([to, cc, bcc, subject, attachments.length, accountId]);
    if (seenFields && f !== seenFields) scheduleAutosave();
    seenFields = f;
  });
  // Leaving the composer some other way (opening another email…): keep the latest version.
  onDestroy(() => {
    clearTimeout(autoTimer);
    if (!closed && dirty && hasContent()) api.saveDraft(outgoing(), true).catch(() => {});
  });

  function checks(): boolean {
    if (!to.length && !cc.length && !bcc.length) {
      store.error(t("Add at least one recipient"));
      return false;
    }
    if (!subject.trim() && !confirm(t("Send without a subject?"))) return false;
    const text = editorEl?.innerText.toLowerCase() ?? "";
    if (!attachments.length && /(đính kèm|attached|attachment|tệp kèm)/.test(text.split("\n").slice(0, 40).join(" ")) && !confirm(t("The message mentions an attachment but none is attached. Send anyway?"))) return false;
    return true;
  }

  /** Send goes through the undo window: the message waits in the corner for a few seconds. */
  function send() {
    if (!checks()) return;
    closed = true;
    clearTimeout(autoTimer);
    store.queueSend(outgoing());
    store.compose = null;
  }

  // ----- scheduled send -----
  let schedOpen = $state(false);
  let schedCustom = $state("");
  function at(daysAhead: number, hour: number, weekday?: number) {
    const d = new Date();
    if (weekday !== undefined) {
      const diff = (weekday - d.getDay() + 7) % 7 || 7;
      d.setDate(d.getDate() + diff);
    } else d.setDate(d.getDate() + daysAhead);
    d.setHours(hour, 0, 0, 0);
    return d;
  }
  const presets = $derived.by(() => {
    const now = new Date();
    const list: [string, Date][] = [];
    if (now.getHours() < 16) list.push([t("This afternoon"), at(0, 17)]);
    list.push([t("Tomorrow morning"), at(1, 8)]);
    list.push([t("Tomorrow afternoon"), at(1, 13)]);
    list.push([t("Monday morning"), at(0, 8, 1)]);
    return list;
  });
  function localInput(d: Date) {
    const p = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`;
  }
  async function schedule(when: Date) {
    if (when.getTime() < Date.now() + 30_000) {
      store.error(t("Choose a time in the future"));
      return;
    }
    if (!checks()) return;
    sending = true;
    try {
      await api.scheduleSend({ ...outgoing(), draft_key: "" }, Math.floor(when.getTime() / 1000));
      closed = true;
      clearTimeout(autoTimer);
      if (savedAccount !== null || draftId) api.discardDraft(savedAccount ?? accountId, draftKey, draftId).catch(() => {});
      store.success(t("Scheduled for {time}", { time: when.toLocaleString(locale(), { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" }) }));
      store.compose = null;
      store.loadOutbox();
    } catch (e) {
      store.error(String(e));
    } finally {
      sending = false;
      schedOpen = false;
    }
  }

  async function saveDraft(close = true) {
    clearTimeout(autoTimer);
    saveState = "saving";
    // Closing: the editor goes away at once and the save finishes in the background.
    const snapshot = close ? ($state.snapshot(outgoing()) as Outgoing) : null;
    if (close) {
      closed = true;
      store.compose = null;
      store.info(t("Saving draft…"));
    }
    try {
      if (savedAccount !== null && savedAccount !== accountId) await api.discardDraft(savedAccount, draftKey, null).catch(() => {});
      const r = await api.saveDraft(outgoing());
      savedAccount = accountId;
      draftId = r.local_id ?? null;
      dirty = false;
      saveState = "saved";
      savedAt = Date.now();
      store.success(t("Draft saved"));
    } catch (e) {
      saveState = "error";
      saveError = String(e).replace(/^Error: /, "");
      if (snapshot) store.toast("error", t("Couldn't save the draft: {err}", { err: saveError }), { label: t("Edit"), run: () => store.restoreCompose(snapshot) });
      else store.error(t("Couldn't save the draft: {err}", { err: saveError }));
    }
  }


  async function discard() {
    if ((dirty || savedAccount !== null) && hasContent() && !confirm(t("Discard this message?"))) return;
    closed = true;
    clearTimeout(autoTimer);
    store.compose = null;
    // Its autosaved versions (and the draft it was opened from) go too.
    if (savedAccount !== null || draftId) {
      try {
        await api.discardDraft(savedAccount ?? accountId, draftKey, draftId);
        store.info(t("Draft discarded"));
      } catch (e) {
        store.error(t("Couldn't delete the saved draft: {err}", { err: String(e) }));
      }
    }
  }

  async function attach() {
    const files = await pickFiles();
    for (const p of files) attachments = [...attachments, { path: p, name: p.split(/[\\/]/).pop() ?? p }];
  }

  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
      e.preventDefault();
      send();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      saveDraft(false);
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="composer" onkeydown={onKey} oninput={scheduleAutosave}>
  <div class="bar">
    <div class="split">
      <button class="btn primary main" onclick={send} disabled={sending}>
        {#if sending}<Icon name="sync" size={16} class="spin" />{:else}<Icon name="send" size={16} />{/if}
        {t("Send")}
      </button>
      <button class="btn primary drop" onclick={() => { schedOpen = !schedOpen; schedCustom = localInput(at(1, 8)); }} disabled={sending} title={t("Schedule send")} aria-label={t("Schedule send")}>
        <Icon name="chevronDown" size={14} />
      </button>
      {#if schedOpen}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div class="sched-backdrop" onclick={() => (schedOpen = false)}></div>
        <div class="sched pop" transition:scale={{ start: 0.96, duration: 150, opacity: 0 }}>
          <div class="sched-title"><Icon name="clock" size={16} />{t("Schedule send")}</div>
          {#each presets as [label, d]}
            <button class="sched-item" onclick={() => schedule(d)}>
              <span>{label}</span>
              <span class="muted small">{d.toLocaleString(locale(), { weekday: "short", hour: "2-digit", minute: "2-digit" })}</span>
            </button>
          {/each}
          <div class="sched-custom">
            <input class="input" type="datetime-local" bind:value={schedCustom} />
            <button class="btn" onclick={() => schedCustom && schedule(new Date(schedCustom))}>{t("Schedule")}</button>
          </div>
        </div>
      {/if}
    </div>
    <button class="btn subtle" onclick={attach}><Icon name="attach" size={16} />{t("Attach")}</button>
    <div class="ai-wrap">
      <button class="btn subtle ai-btn" onclick={() => (aiMenu = !aiMenu)} title={t("Writing assistant, Ask AI, Translate, Meaning")}><span class="spark">✦</span>Hermio AI<Icon name="chevronDown" size={13} /></button>
      {#if aiMenu}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div class="sched-backdrop" onclick={() => (aiMenu = false)}></div>
        <div class="sched pop ai-menu" transition:scale={{ start: 0.96, duration: 150, opacity: 0 }}>
          <button class="sched-item" onclick={(e) => aiPick(e, "check")}><span>{t("Check writing with AI")}</span><kbd>Ctrl+Alt+G</kbd></button>
          <button class="sched-item" onclick={(e) => aiPick(e, "ask")}><span>{t("Ask AI…")}</span><kbd>Ctrl+Alt+K</kbd></button>
          <button class="sched-item" onclick={(e) => aiPick(e, "translate")}><span>{t("Translate")}</span><kbd>Ctrl+Alt+T</kbd></button>
          <button class="sched-item" onclick={(e) => aiPick(e, "define")}><span>{t("Look up meaning")}</span><kbd>Ctrl+Alt+D</kbd></button>
        </div>
      {/if}
    </div>
    <label class="sig">
      <Icon name="signature" size={16} />
      <select class="input" value={sigId ?? ""} onchange={(e) => changeSignature((e.target as HTMLSelectElement).value ? Number((e.target as HTMLSelectElement).value) : null)}>
        <option value="">{t("No signature")}</option>
        {#each store.signatures as s}<option value={s.id}>{s.name}</option>{/each}
      </select>
    </label>
    <span class="grow"></span>
    <span class="save-state small" class:err={saveState === "error"} title={saveState === "error" ? saveError : t("Your message is saved to Drafts as you write")}>
      {#if saveState === "saving"}<span class="spinner"></span>{t("Saving…")}
      {:else if saveState === "saved"}<Icon name="check" size={13} />{t("Saved to Drafts {time}", { time: new Date(savedAt).toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit" }) })}
      {:else if saveState === "error"}<Icon name="warning" size={13} />{t("Not saved")}
      {:else}<span class="muted">{t("Ctrl+Enter to send")}</span>{/if}
    </span>
    <button class="btn subtle" onclick={() => saveDraft(true)} title={t("Save draft (Ctrl+S)")}><Icon name="draft" size={16} />{t("Save draft")}</button>
    <button class="icon-btn" onclick={discard} title={t("Discard")}><Icon name="trash" /></button>
  </div>

  <div class="scroll body">
    <div class="title">{title}{#if init.aiText}<span class="aitag">{t("✨ AI draft — review it before sending")}</span>{/if}</div>
    <div class="fromrow">
      <span class="lbl">{t("From")}</span>
      <select class="input from" bind:value={accountId}>
        {#each store.accounts as a}
          <option value={a.id}>{a.display_name || a.name} &lt;{a.email}&gt;</option>
        {/each}
      </select>
      <span class="grow"></span>
      {#if !showCc}<button class="link" onclick={() => (showCc = true)} out:fade={{ duration: 100 }}>Cc</button>{/if}
      {#if !showBcc}<button class="link" onclick={() => (showBcc = true)} out:fade={{ duration: 100 }}>Bcc</button>{/if}
    </div>
    <RecipientInput label={t("To")} bind:values={to} autofocus={init.mode === "new" || init.mode === "forward"} />
    {#if showCc}<div transition:slide={{ duration: 180 }}><RecipientInput label="Cc" bind:values={cc} autofocus /></div>{/if}
    {#if showBcc}<div transition:slide={{ duration: 180 }}><RecipientInput label="Bcc" bind:values={bcc} autofocus /></div>{/if}
    <input class="subject" placeholder={t("Add a subject")} bind:value={subject} spellcheck="true" />

    {#if attachments.length}
      <div class="atts" transition:slide={{ duration: 180 }}>
        {#each attachments as a, i (a)}
          <span class="att" animate:flip={{ duration: 200 }} in:scale={{ start: 0.85, duration: 180 }} out:scale={{ start: 0.85, duration: 140 }}>
            <Icon name="attach" size={14} />
            <span class="nm">{a.name}</span>
            {#if a.size}<span class="muted small">{fileSize(a.size)}</span>{/if}
            <button onclick={() => (attachments = attachments.filter((_, j) => j !== i))} title={t("Remove")}><Icon name="close" size={12} /></button>
          </span>
        {/each}
      </div>
    {/if}

    {#if initialHtml !== null}
      <RichEditor html={initialHtml} bind:el={editorEl} placeholder={t("Write your message…")} minHeight={320} onchange={scheduleAutosave} />
    {:else}
      <div class="muted" style="padding:16px 4px" out:fade={{ duration: 100 }}>{t("Preparing…")}</div>
    {/if}
    {#if editorEl}<WritingAssistant el={editorEl} bind:this={wa} />{/if}
    {#if account?.protocol === "pop3"}
      <div class="muted small foot">{t("A copy of sent mail is saved in this account's POP3 data folder.")}</div>
    {/if}
  </div>
</div>

<style>
  .composer {
    position: relative;
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    animation: slide-up 200ms var(--ease);
  }
  .split {
    position: relative;
    display: flex;
  }
  .split .main {
    border-top-right-radius: 0;
    border-bottom-right-radius: 0;
  }
  .split .drop {
    border-top-left-radius: 0;
    border-bottom-left-radius: 0;
    border-left: 1px solid color-mix(in srgb, #fff 35%, transparent);
    padding: 0 8px;
    min-width: 0;
  }
  .save-state {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--text-3);
    white-space: nowrap;
  }
  .save-state.err {
    color: var(--danger);
  }
  .ai-wrap {
    position: relative;
  }
  .ai-btn .spark {
    color: #e8b931;
  }
  .ai-menu {
    width: 260px;
  }
  .ai-menu kbd {
    font: 0.72rem var(--mono, ui-monospace, Consolas, monospace);
    color: var(--text-3);
  }
  .sched-backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
  }
  .sched {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    z-index: 41;
    width: 290px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-l);
    box-shadow: var(--shadow-l);
    padding: 6px;
  }
  .sched-title {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 600;
    padding: 8px 10px;
  }
  .sched-item {
    width: 100%;
    display: flex;
    justify-content: space-between;
    align-items: center;
    border: none;
    background: transparent;
    padding: 8px 10px;
    border-radius: 6px;
    text-align: left;
  }
  .sched-item:hover {
    background: var(--hover);
  }
  .sched-custom {
    display: flex;
    gap: 6px;
    padding: 8px 4px 4px;
    border-top: 1px solid var(--border);
    margin-top: 4px;
  }
  .sched-custom .input {
    flex: 1;
    min-width: 0;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
  }
  .sig {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-2);
    margin-left: 4px;
  }
  .sig select {
    height: 30px;
    max-width: 180px;
  }
  .hint {
    margin-right: 6px;
  }
  @media (max-width: 1300px) {
    .hint {
      display: none;
    }
  }
  .body {
    flex: 1;
    padding: 12px 22px 24px;
    display: flex;
    flex-direction: column;
  }
  .title {
    font-family: var(--font-display);
    font-weight: 600;
    font-size: 1.05rem;
    margin-bottom: 6px;
  }
  .aitag {
    margin-left: 10px;
    font-size: 0.78rem;
    font-weight: 600;
    padding: 2px 8px;
    border-radius: 10px;
    background: color-mix(in srgb, var(--ai) 18%, transparent);
    color: var(--ai);
  }
  .fromrow {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 0;
    border-bottom: 1px solid var(--border);
  }
  .lbl {
    width: 42px;
    color: var(--text-2);
    font-size: 0.9rem;
  }
  .from {
    border: none;
    background: transparent;
    padding: 0 4px;
    max-width: 420px;
    box-shadow: none !important;
  }
  .link {
    border: none;
    background: none;
    color: var(--text-accent);
    font-weight: 600;
    padding: 4px 8px;
    border-radius: 4px;
  }
  .link:hover {
    background: var(--hover);
  }
  .subject {
    height: 42px;
    border: none;
    border-bottom: 1px solid var(--border);
    outline: none;
    background: transparent;
    font-size: 1.05rem;
    font-weight: 600;
    padding: 0 2px;
    user-select: text;
  }
  .subject:focus {
    border-bottom-color: var(--accent);
  }
  .atts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 10px 0 2px;
  }
  .att {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 4px 0 10px;
    border-radius: var(--radius);
    border: 1px solid var(--border-strong);
    font-size: 0.86rem;
    max-width: 280px;
  }
  .att .nm {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .att button {
    border: none;
    background: none;
    padding: 4px;
    border-radius: 4px;
    display: flex;
    color: var(--text-3);
  }
  .att button:hover {
    background: var(--hover);
  }
  .body :global(.editor) {
    margin-top: 10px;
  }
  .foot {
    margin-top: 10px;
  }
</style>
