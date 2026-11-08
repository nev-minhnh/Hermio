<script lang="ts">
  import { slide } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { pill } from "../lib/motion";
  import { t } from "../lib/i18n.svelte";
  import { onMount } from "svelte";
  import Icon from "../lib/Icon.svelte";
  import { api, type Group, type Rule, type Condition } from "../lib/api";
  import { store } from "../lib/store.svelte";
  import Dialog from "./Dialog.svelte";

  let rules = $state<Rule[]>([]);
  let editing = $state<Rule | null>(null);
  let groupEdit = $state<Group | null>(null);
  let running = $state(false);
  let askApply = $state<{ id: number; name: string } | null>(null);
  const colors = ["#8764b8", "#1d4fe0", "#038387", "#ca5010", "#c239b3", "#498205", "#d13438", "#986f0b"];

  const fields: [string, string][] = [
    ["from", t("Sender")],
    ["to", t("Recipients (To/Cc)")],
    ["subject", t("Subject")],
    ["body", t("Message body")],
    ["any", t("Anywhere (sender, recipients, subject, body)")],
    ["account", t("Receiving account")],
    ["attachment", t("Has attachments")],
  ];
  const ops: [string, string][] = [
    ["contains", t("contains")],
    ["not_contains", t("doesn't contain")],
    ["equals", t("is")],
    ["starts_with", t("starts with")],
    ["ends_with", t("ends with")],
    ["domain", t("is from domain")],
    ["regex", t("matches regex")],
  ];

  onMount(async () => {
    await load();
    if (store.ruleDraft) {
      const d = store.ruleDraft;
      store.ruleDraft = null;
      newRule(d.from ? [{ field: "from", op: "contains", value: d.from }] : d.subject ? [{ field: "subject", op: "contains", value: d.subject }] : undefined);
    }
  });

  async function load() {
    rules = await api.rules();
    await store.loadGroups();
  }

  function newRule(conds?: Condition[]) {
    editing = {
      id: 0, name: "", enabled: true, match_all: true,
      conditions: conds ?? [{ field: "from", op: "contains", value: "" }],
      account_ids: [], group_id: store.groups[0]?.id ?? null, mark_read: false, flag: false, stop: false, sort_order: 0,
    };
  }

  async function saveRule(runNow: boolean) {
    if (!editing) return;
    const r = $state.snapshot(editing);
    if (!r.conditions.some((c) => c.value.trim() || c.field === "attachment")) return store.error(t("Add at least one condition"));
    if (!r.group_id && !r.mark_read && !r.flag) return store.error(t("Choose a group or an action"));
    if (!r.name.trim()) {
      const c = r.conditions[0];
      r.name = `${fields.find((f) => f[0] === c.field)?.[1] ?? c.field} ${ops.find((o) => o[0] === c.op)?.[1] ?? ""} “${c.value}”`;
    }
    const isNew = !r.id;
    const id = await api.saveRule(r);
    editing = null;
    await load();
    store.success(t("Rule saved"));
    if (runNow) run(id);
    else if (isNew || r.enabled) askApply = { id, name: r.name };
  }

  async function run(id: number | null) {
    running = true;
    try {
      const n = await api.runRules(id);
      store.success(t("Sorted {n} existing messages", { n }));
      await store.reloadAll();
    } catch (e) {
      store.error(String(e));
    } finally {
      running = false;
    }
  }

  async function toggle(r: Rule) {
    await api.saveRule({ ...r, enabled: !r.enabled });
    await load();
  }

  async function removeRule(r: Rule) {
    if (!confirm(t("Delete rule “{name}”?", { name: r.name }))) return;
    await api.deleteRule(r.id);
    await load();
    store.success(t("Rule deleted"));
  }

  async function move(i: number, d: number) {
    const j = i + d;
    if (j < 0 || j >= rules.length) return;
    const a = rules[i];
    const b = rules[j];
    await api.saveRule({ ...a, sort_order: b.sort_order === a.sort_order ? j : b.sort_order });
    await api.saveRule({ ...b, sort_order: a.sort_order === b.sort_order ? i : a.sort_order });
    await load();
  }

  async function saveGroup() {
    if (!groupEdit || !groupEdit.name.trim()) return;
    const id = await api.saveGroup({ ...$state.snapshot(groupEdit), hide_from_inbox: true });
    if (editing && !editing.group_id) editing.group_id = id;
    groupEdit = null;
    await load();
    await store.reloadAll();
  }

  async function removeGroup(g: Group) {
    if (!confirm(t("Delete group “{name}”? Messages are not deleted, only removed from the group.", { name: g.name }))) return;
    await api.deleteGroup(g.id);
    await load();
    await store.reloadAll();
    if (store.view === `group:${g.id}`) store.open("unified:inbox");
  }

  const newGroup = (accountId: number | null = null) =>
    (groupEdit = { id: 0, name: "", color: colors[store.groups.length % colors.length], hide_from_inbox: true, sort_order: 0, account_id: accountId });
  const scopeOf = (g: Group) => (g.account_id ? (store.accountById(g.account_id)?.email ?? "?") : t("All mailboxes"));
  const globalGroups = $derived(store.groups.filter((g) => !g.account_id));
  const localGroups = $derived(
    store.accounts.map((a) => ({ a, groups: store.groups.filter((g) => g.account_id === a.id) })).filter((x) => x.groups.length),
  );
  const groupName = (id: number | null) => store.groups.find((g) => g.id === id)?.name ?? "—";
  const describe = (r: Rule) =>
    r.conditions
      .map((c) => `${fields.find((f) => f[0] === c.field)?.[1] ?? c.field} ${c.field === "attachment" ? "" : (ops.find((o) => o[0] === c.op)?.[1] ?? "") + " “" + c.value + "”"}`)
      .join(r.match_all ? t(" and ") : t(" or "));
</script>

<div class="head">
  <h2>{t("Groups & rules")}</h2>
</div>
<p class="muted lead">
  {t("Create groups (e.g. “Newsletters”, “Invoices”, “GitHub”) and rules that put incoming mail into them automatically. Grouped mail is")}
  <b>{t("hidden from the Inbox")}</b> {t("so your main inbox stays tidy — it still stays on the server.")}
</p>

<section>
  <div class="sec-head">
    <h3>{t("Groups")}</h3>
    <button class="btn" onclick={() => newGroup(null)}>
      <Icon name="plus" size={16} />{t("New group")}
    </button>
  </div>
  <div class="glevel">
    <div class="glabel"><Icon name="globe" size={14} />{t("Global groups")} <span class="muted small">— {t("collect mail from every mailbox")}</span></div>
    <div class="groups">
      {#each globalGroups as g (g.id)}
        {@render groupRow(g)}
      {:else}
        <p class="muted small">{t("No groups yet.")}</p>
      {/each}
    </div>
  </div>
  <div class="glevel">
    <div class="glabel"><Icon name="inbox" size={14} />{t("Mailbox groups")} <span class="muted small">— {t("only for one mailbox")}</span></div>
    {#each localGroups as { a, groups } (a.id)}
      <div class="acc-groups">
        <div class="acc-name"><i style:background={a.color}></i>{a.email}<button class="btn subtle sm" onclick={() => newGroup(a.id)}><Icon name="plus" size={14} />{t("Add")}</button></div>
        <div class="groups">
          {#each groups as g (g.id)}{@render groupRow(g)}{/each}
        </div>
      </div>
    {:else}
      <p class="muted small">{t("None yet. Create a group and choose a mailbox to make it local.")}</p>
    {/each}
  </div>
</section>

{#snippet groupRow(g: Group)}
  <div class="group card" transition:slide={{ duration: 180 }}>
    <span class="dot" style:background={g.color}></span>
    <b class="grow">{g.name}</b>
    <span class="muted small">{store.counts.groups[g.id]?.total ?? 0} {t("messages")}</span>
    <button class="icon-btn" title={t("Edit")} onclick={() => (groupEdit = { ...g })}><Icon name="settings" size={16} /></button>
    <button class="icon-btn" title={t("Delete")} onclick={() => removeGroup(g)}><Icon name="trash" size={16} /></button>
  </div>
{/snippet}

<section>
  <div class="sec-head">
    <h3>{t("Rules")}</h3>
    <div class="row">
      <button class="btn" onclick={() => run(null)} disabled={running || !rules.length}>
        {#if running}<Icon name="sync" size={16} class="spin" />{:else}<Icon name="sync" size={16} />{/if}{t("Run all on existing mail")}
      </button>
      <button class="btn primary" onclick={() => newRule()}><Icon name="plus" size={16} />{t("New rule")}</button>
    </div>
  </div>
  <div class="rules">
    {#each rules as r, i (r.id)}
      <div class="rule card" class:off={!r.enabled} animate:flip={{ duration: 220 }} transition:slide={{ duration: 180 }}>
        <button class="switch" class:on={r.enabled} onclick={() => toggle(r)} title={r.enabled ? t("Off") : t("On")} aria-label={t("Toggle")}></button>
        <div class="grow info">
          <b>{r.name}</b>
          <span class="muted small">{t("If")} {describe(r)}{r.account_ids.length ? t(" · {n} accounts", { n: r.account_ids.length }) : ""}</span>
          <span class="small">
            → {r.group_id ? t("group “{name}”", { name: groupName(r.group_id) }) : ""}{r.mark_read ? t(" · mark as read") : ""}{r.flag ? t(" · flag") : ""}{r.stop ? t(" · stop processing") : ""}
          </span>
        </div>
        <button class="icon-btn" title={t("Up")} disabled={i === 0} onclick={() => move(i, -1)}><Icon name="chevronUp" size={16} /></button>
        <button class="icon-btn" title={t("Down")} disabled={i === rules.length - 1} onclick={() => move(i, 1)}><Icon name="chevronDown" size={16} /></button>
        <button class="icon-btn" title={t("Run on existing mail")} onclick={() => run(r.id)}><Icon name="sync" size={16} /></button>
        <button class="icon-btn" title={t("Edit")} onclick={() => (editing = structuredClone($state.snapshot(r)))}><Icon name="settings" size={16} /></button>
        <button class="icon-btn" title={t("Delete")} onclick={() => removeRule(r)}><Icon name="trash" size={16} /></button>
      </div>
    {:else}
      <p class="muted small">{t("No rules yet. Tip: right-click a message → “Create rule from this sender”.")}</p>
    {/each}
  </div>
</section>

{#if editing}
  <Dialog title={editing.id ? t("Edit rule") : t("New rule")} width={720} onclose={() => (editing = null)}>
    <label class="field"><span>{t("Rule name")}</span><input class="input" bind:value={editing.name} placeholder={t("Named after the conditions if left empty")} /></label>
    <div class="row">
      <span>{t("When incoming mail matches")}</span>
      <div class="segmented" use:pill>
        <button class:on={editing.match_all} onclick={() => editing && (editing.match_all = true)}>{t("all")}</button>
        <button class:on={!editing.match_all} onclick={() => editing && (editing.match_all = false)}>{t("any")}</button>
      </div>
      <span>{t("of these conditions:")}</span>
    </div>
    <div class="conds">
      {#each editing.conditions as c, i}
        <div class="cond">
          <select class="input" bind:value={c.field}>
            {#each fields as [v, l]}<option value={v}>{l}</option>{/each}
          </select>
          {#if c.field === "attachment"}
            <select class="input grow" bind:value={c.value}>
              <option value="yes">{t("has")}</option>
              <option value="no">{t("doesn't have")}</option>
            </select>
          {:else}
            <select class="input" bind:value={c.op}>
              {#each ops as [v, l]}<option value={v}>{l}</option>{/each}
            </select>
            <input class="input grow" bind:value={c.value} placeholder={c.op === "domain" ? "github.com" : t("several values: separate with commas")} />
          {/if}
          <button class="icon-btn" title={t("Remove condition")} onclick={() => editing && (editing.conditions = editing.conditions.filter((_, j) => j !== i))} disabled={editing.conditions.length === 1}>
            <Icon name="close" size={14} />
          </button>
        </div>
      {/each}
      <button class="btn subtle add" onclick={() => editing?.conditions.push({ field: "subject", op: "contains", value: "" })}><Icon name="plus" size={14} />{t("Add condition")}</button>
    </div>

    <div class="field">
      <span>{t("Apply to accounts")}</span>
      <div class="accs">
        <button class="chip" class:on={editing.account_ids.length === 0} onclick={() => editing && (editing.account_ids = [])}>{t("All")}</button>
        {#each store.accounts as a}
          <button
            class="chip"
            class:on={editing.account_ids.includes(a.id)}
            onclick={() => {
              if (!editing) return;
              editing.account_ids = editing.account_ids.includes(a.id) ? editing.account_ids.filter((x) => x !== a.id) : [...editing.account_ids, a.id];
            }}
          ><i style:background={a.color}></i>{a.name || a.email}</button>
        {/each}
      </div>
    </div>

    <div class="field">
      <span>{t("Then")}</span>
      <div class="row">
        <span>{t("Move to group")}</span>
        <select class="input grow" bind:value={editing.group_id}>
          <option value={null}>{t("(none)")}</option>
          {#each store.groups as g}<option value={g.id}>{g.name} · {scopeOf(g)}</option>{/each}
        </select>
        <button class="btn subtle" onclick={() => newGroup(editing?.account_ids.length === 1 ? editing.account_ids[0] : null)}>
          <Icon name="plus" size={14} />{t("New group")}
        </button>
      </div>
      <button class="toggle" onclick={() => editing && (editing.mark_read = !editing.mark_read)}><span class="switch" class:on={editing.mark_read}></span>{t("Mark as read")}</button>
      <button class="toggle" onclick={() => editing && (editing.flag = !editing.flag)}><span class="switch" class:on={editing.flag}></span>{t("Flag")}</button>
      <button class="toggle" onclick={() => editing && (editing.stop = !editing.stop)}><span class="switch" class:on={editing.stop}></span>{t("Don't process later rules")}</button>
    </div>

    {#snippet footer()}
      <button class="btn" onclick={() => (editing = null)}>{t("Cancel")}</button>
      <button class="btn primary" onclick={() => saveRule(false)}>{t("Save")}</button>
    {/snippet}
  </Dialog>
{/if}

{#if askApply}
  <Dialog title={t("Apply to existing mail?")} width={460} onclose={() => (askApply = null)}>
    <p style="margin:0">{t("Rule “{name}” will sort new mail from now on. Do you also want to sort the mail you already have with it?", { name: askApply.name })}</p>
    {#snippet footer()}
      <button class="btn" onclick={() => (askApply = null)}>{t("Only new mail")}</button>
      <button class="btn primary" onclick={() => { const id = askApply!.id; askApply = null; run(id); }}><Icon name="sync" size={16} />{t("Apply to existing mail")}</button>
    {/snippet}
  </Dialog>
{/if}

{#if groupEdit}
  <Dialog title={groupEdit.id ? t("Edit group") : t("New group")} width={460} onclose={() => (groupEdit = null)}>
    <!-- svelte-ignore a11y_autofocus -->
    <label class="field"><span>{t("Group name")}</span><input class="input" bind:value={groupEdit.name} autofocus onkeydown={(e) => e.key === "Enter" && saveGroup()} /></label>
    <div class="field">
      <span>{t("Color")}</span>
      <div class="colors">
        {#each colors as c}<button class="sw" class:on={groupEdit.color === c} style:background={c} onclick={() => groupEdit && (groupEdit.color = c)} aria-label={c}></button>{/each}
      </div>
    </div>
    <label class="field">
      <span>{t("Level")}</span>
      <select class="input" bind:value={groupEdit.account_id}>
        <option value={null}>{t("Global — all mailboxes")}</option>
        {#each store.accounts as a}<option value={a.id}>{t("Local — {email}", { email: a.email })}</option>{/each}
      </select>
    </label>
    <p class="muted small" style="margin:0">{t("Mail in a group is hidden from the Inbox and shown under the group instead.")}</p>
    {#snippet footer()}
      <button class="btn" onclick={() => (groupEdit = null)}>{t("Cancel")}</button>
      <button class="btn primary" onclick={saveGroup}>{t("Save")}</button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .glevel {
    margin-bottom: 14px;
  }
  .glabel {
    display: flex;
    align-items: center;
    gap: 6px;
    font-weight: 600;
    margin: 4px 0 8px;
  }
  .acc-groups {
    margin: 0 0 10px 6px;
    padding-left: 10px;
    border-left: 2px solid var(--border);
  }
  .acc-name {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 0.88rem;
    color: var(--text-2);
    margin-bottom: 6px;
  }
  .acc-name i {
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }
  .btn.sm {
    height: 26px;
    padding: 0 8px;
  }
  .head h2 {
    margin: 0 0 6px;
    font-family: var(--font-display);
    font-weight: 600;
    font-size: 1.35rem;
  }
  .lead {
    line-height: 1.55;
    margin: 0 0 18px;
    max-width: 760px;
  }
  section {
    margin-bottom: 26px;
  }
  .sec-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 10px;
  }
  h3 {
    margin: 0;
    font-size: 1.02rem;
  }
  .groups,
  .rules {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .group,
  .rule {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 8px 8px 14px;
  }
  .rule.off {
    opacity: 0.6;
  }
  .dot {
    width: 12px;
    height: 12px;
    border-radius: 4px;
  }
  .pill {
    font-size: 0.74rem;
    padding: 1px 8px;
    border-radius: 10px;
    background: var(--surface-3);
    color: var(--text-2);
  }
  .info {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .info span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .conds {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .cond {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .cond select:first-child {
    width: 190px;
  }
  .add {
    align-self: flex-start;
  }
  .accs {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 10px;
    border-radius: 14px;
    border: 1px solid var(--border-strong);
    background: transparent;
    font-size: 0.85rem;
  }
  .chip.on {
    background: var(--selected);
    border-color: var(--accent);
    color: var(--text-accent);
    font-weight: 600;
  }
  .chip i {
    width: 8px;
    height: 8px;
    border-radius: 50%;
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
</style>
