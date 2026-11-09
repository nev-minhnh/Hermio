<script lang="ts">
  import { countUp } from "../lib/motion";
  import { scale } from "svelte/transition";
  import { t } from "../lib/i18n.svelte";
  import Icon from "../lib/Icon.svelte";
  import { store } from "../lib/store.svelte";
  import { api, type Folder, type Account, type Group } from "../lib/api";
  import { folderIcon } from "../lib/util";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";
  import { slide } from "svelte/transition";

  const globalGroups = $derived(store.groups.filter((g) => !g.account_id));
  const localGroupsOf = (a: Account) => store.groups.filter((g) => g.account_id === a.id);

  const hidden = new Set(["all", "flagged", "important"]);
  let dropOn = $state<string | null>(null);
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

  // Accounts start collapsed when there are many, so 10 mailboxes stay readable.
  const many = $derived(store.accounts.length > 4);
  const isOpen = (a: Account) => {
    const k = `acc:${a.id}`;
    return k in store.collapsed ? !store.collapsed[k] : !many;
  };

  function foldersOf(a: Account) {
    const all = store.folders.filter((f) => f.account_id === a.id);
    const paths = new Set(all.map((f) => f.path));
    return all
      .filter((f) => !hidden.has(f.role) && f.selectable)
      .map((f) => {
        let depth = 0;
        if (!f.role && f.delim) {
          const parts = f.path.split(f.delim);
          for (let i = 1; i < parts.length; i++) {
            const parent = parts.slice(0, i).join(f.delim);
            const p = all.find((x) => x.path === parent);
            if (paths.has(parent) && p && p.selectable && !hidden.has(p.role)) depth++;
          }
        }
        return { f, depth };
      });
  }
  const inboxOf = (a: Account) => store.folders.find((f) => f.account_id === a.id && f.role === "inbox");

  function dropIds(e: DragEvent): number[] | null {
    e.preventDefault();
    dropOn = null;
    const raw = e.dataTransfer?.getData("application/x-hermio-ids");
    return raw ? JSON.parse(raw) : null;
  }
  function onDropFolder(e: DragEvent, f: Folder) {
    const ids = dropIds(e);
    if (!ids) return;
    if (store.messages.filter((m) => ids.includes(m.id)).some((m) => m.account_id !== f.account_id)) {
      store.error(t("Messages can only be dragged to folders of the same account"));
      return;
    }
    store.moveTo(ids, f.id);
  }
  async function onDropGroup(e: DragEvent, g: Group) {
    const ids = dropIds(e);
    if (!ids) return;
    if (g.account_id && store.messages.filter((m) => ids.includes(m.id)).some((m) => m.account_id !== g.account_id)) {
      store.error(t("This group only takes mail from {email}", { email: store.accountById(g.account_id)?.email ?? "" }));
      return;
    }
    await api.addToGroup(ids, g.id);
    store.success(t("Moved {n} messages to “{name}”", { n: ids.length, name: g.name }));
    if (!store.view.startsWith("group:")) store.messages = store.messages.filter((m) => !ids.includes(m.id));
    await store.reloadAll();
  }

  function accountMenu(e: MouseEvent, a: Account) {
    e.preventDefault();
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { icon: "sync", label: t("Sync now"), run: () => store.syncNow(a.id) },
        { icon: "mail", label: t("Show all mail of this account"), run: () => store.open(`account:${a.id}`) },
        {
          icon: "plus",
          label: t("New folder…"),
          run: async () => {
            const name = prompt(t("New folder name"));
            if (!name?.trim()) return;
            try {
              await api.createFolder(a.id, name.trim());
              await store.loadFolders();
            } catch (err) {
              store.error(String(err));
            }
          },
        },
        { sep: true },
        { header: true, label: t("Move to space") },
        { icon: a.space_id ? undefined : "check", label: t("None"), run: () => moveSpace(a, null) },
        ...store.spaces.map((s) => ({ icon: a.space_id === s.id ? "check" : undefined, label: s.name, run: () => moveSpace(a, s.id) }) as MenuItem),
        { sep: true },
        { icon: "tag", label: t("New group for this mailbox…"), run: () => { store.settingsTab = "rules"; store.route = "settings"; } },
        { icon: "settings", label: t("Account settings"), run: () => { store.settingsTab = `account:${a.id}`; store.route = "settings"; } },
      ],
    };
  }

  async function moveSpace(a: Account, id: number | null) {
    await api.setAccountSpace(a.id, id);
    await store.loadAccounts();
    await store.loadFolders();
    store.success(id ? t("{email} moved to “{space}”", { email: a.email, space: store.spaces.find((s) => s.id === id)?.name ?? "" }) : t("{email} removed from its space", { email: a.email }));
  }

  function folderMenu(e: MouseEvent, f: Folder) {
    e.preventDefault();
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        {
          icon: "read",
          label: t("Mark all as read"),
          run: async () => {
            const rows = await api.messages({ view: `folder:${f.id}`, search: "", search_mode: "content", filter: "unread", sort_asc: false, limit: 10000, offset: 0 });
            store.markSeen(rows.map((r) => r.id), true);
            store.loadFolders();
          },
        },
        { icon: "sync", label: t("Sync account"), run: () => store.syncNow(f.account_id) },
        { icon: "sparkle", label: t("AI: summarize this folder"), run: () => { store.open(`folder:${f.id}`); store.ai("summarize"); } },
      ],
    };
  }

  function groupMenu(e: MouseEvent, g: Group) {
    e.preventDefault();
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { icon: "sparkle", label: t("AI: summarize this group"), run: () => { store.open(`group:${g.id}`); store.ai("summarize"); } },
        { icon: "settings", label: t("Edit groups and rules…"), run: () => { store.settingsTab = "rules"; store.route = "settings"; } },
      ],
    };
  }

  async function newSpace() {
    const name = prompt(t("New space name (e.g. Work, Study, Personal)"));
    if (!name?.trim()) return;
    const id = await api.saveSpace({ name: name.trim(), color: "#1d4fe0" });
    await store.loadSpaces();
    store.setSpace(id);
    store.info(t("Right-click an account → Move to space"));
  }
</script>

<div class="pane">
  <div class="top">
    <button class="btn primary new" onclick={() => store.startCompose("new")}>
      <Icon name="compose" size={18} /> {t("New mail")}
    </button>
  </div>

  {#if store.spaces.length || store.accounts.length > 3}
    <div class="spaces">
      <button class="sp" class:on={!store.settings.space} onclick={() => store.setSpace(0)}>{t("All")}</button>
      {#each store.spaces as sp (sp.id)}
        <button class="sp" class:on={store.settings.space === sp.id} onclick={() => store.setSpace(sp.id)} style:--c={sp.color}>{sp.name}</button>
      {/each}
      <button class="sp add" title={t("New space")} onclick={newSpace}><Icon name="plus" size={14} /></button>
    </div>
  {/if}

  <div class="tree scroll">
    <div class="section">{t("Favorites")}</div>
    <button class="node" class:on={store.view === "unified:inbox" && !store.search} onclick={() => store.open("unified:inbox")}>
      <Icon name="inbox" size={18} /><span class="name">{store.settings.space ? t("Space inbox") : t("All inboxes")}</span>
      {#if store.counts.inbox}<span class="badge" use:countUp={store.counts.inbox} transition:scale={{ duration: 160, start: 0.6 }}></span>{/if}
    </button>
    <button class="node" class:on={store.view === "unified:unread" && !store.search} onclick={() => store.open("unified:unread")}>
      <Icon name="unread" size={18} /><span class="name">{t("Unread")}</span>
    </button>
    <button class="node" class:on={store.view === "unified:flagged" && !store.search} onclick={() => store.open("unified:flagged")}>
      <Icon name="flag" size={18} /><span class="name">{t("Flagged")}</span>
      {#if store.counts.flagged}<span class="badge muted" use:countUp={store.counts.flagged} transition:scale={{ duration: 160, start: 0.6 }}></span>{/if}
    </button>

    {#if store.outbox.length}
      <button class="node" class:on={store.view === "outbox"} onclick={() => store.open("outbox")} transition:slide={{ duration: 160 }}>
        <Icon name="clock" size={18} /><span class="name">{t("Scheduled")}</span>
        <span class="badge muted" use:countUp={store.outbox.length} transition:scale={{ duration: 160, start: 0.6 }}></span>
      </button>
    {/if}

    <div class="section with-action">
      <span>{t("Groups")}</span>
      <button class="mini" title={t("Manage groups and rules")} onclick={() => { store.settingsTab = "rules"; store.route = "settings"; }}><Icon name="settings" size={13} /></button>
    </div>
    {#each globalGroups as g (g.id)}
      {@render groupNode(g, false)}
    {:else}
      {#if !store.groups.length}
        <button class="node hint" onclick={() => { store.settingsTab = "rules"; store.route = "settings"; }}>
          <Icon name="plus" size={16} /><span class="name">{t("Create a group to sort mail automatically")}</span>
        </button>
      {/if}
    {/each}

    <div class="section">{t("Accounts")}</div>
    {#each store.visibleAccounts as a (a.id)}
      {@const open = isOpen(a)}
      {@const inbox = inboxOf(a)}
      {@const unread = store.counts.accounts[a.id] ?? 0}
      <div class="account">
        <div class="acc-head" class:on={!!inbox && store.view === `folder:${inbox.id}` && !store.search} oncontextmenu={(e) => accountMenu(e, a)} role="group">
          <button class="chev" class:closed={!open} onclick={() => (store.collapsed[`acc:${a.id}`] = open)} title={open ? t("Collapse") : t("Expand")}>
            <Icon name="chevronDown" size={14} />
          </button>
          <button class="acc-main" onclick={() => inbox ? store.open(`folder:${inbox.id}`) : store.open(`account:${a.id}`)} title={t("{email} — open Inbox", { email: a.email })}>
            <span class="swatch" style:background={a.color}>{(a.name || a.email).slice(0, 1).toUpperCase()}</span>
            <span class="acc-text">
              <span class="acc-name">{a.name || a.email}</span>
              {#if a.name && a.name !== a.email}<span class="acc-mail">{a.email}</span>{/if}
            </span>
          </button>
          {#if store.syncing[a.id]}
            <Icon name="sync" size={14} class="spin" />
          {:else if store.errors[a.id]}
            <span class="err" title={store.errors[a.id]}><Icon name="warning" size={15} /></span>
          {/if}
          {#if unread}<span class="badge" use:countUp={unread} transition:scale={{ duration: 160, start: 0.6 }}></span>{/if}
        </div>
        {#if open}
          <div class="children" transition:slide={{ duration: 180 }}>
            {#each foldersOf(a) as { f, depth } (f.id)}
              <button
                class="node sub"
                class:on={store.view === `folder:${f.id}` && !store.search}
                class:drop={dropOn === `f${f.id}`}
                style:padding-left="{30 + depth * 16}px"
                onclick={() => store.open(`folder:${f.id}`)}
                oncontextmenu={(e) => folderMenu(e, f)}
                ondragover={(e) => { e.preventDefault(); dropOn = `f${f.id}`; }}
                ondragleave={() => (dropOn = null)}
                ondrop={(e) => onDropFolder(e, f)}
                title={f.path}
              >
                <Icon name={folderIcon(f.role)} size={17} />
                <span class="name">{f.name}</span>
                {#if f.unread && f.role !== "sent" && f.role !== "trash"}
                  <span class="badge" class:muted={f.role === "junk" || f.role === "drafts"} use:countUp={f.unread} transition:scale={{ duration: 160, start: 0.6 }}></span>
                {:else if f.role === "drafts" && f.total}
                  <span class="badge muted" use:countUp={f.total} transition:scale={{ duration: 160, start: 0.6 }}></span>
                {/if}
              </button>
            {/each}
            {#each localGroupsOf(a) as g (g.id)}
              {@render groupNode(g, true)}
            {/each}
            {#if store.errors[a.id]}
              <div class="acc-error">{store.errors[a.id]}</div>
            {/if}
          </div>
        {/if}
      </div>
    {/each}
    {#if store.settings.space && !store.visibleAccounts.length}
      <p class="muted small empty">{t("No accounts in this space yet. Right-click an account under “All” → Move to space.")}</p>
    {/if}
  </div>
</div>

{#snippet groupNode(g: Group, local: boolean)}
  {@const c = store.counts.groups[g.id]}
  <button
    class="node"
    class:sub={local}
    class:on={store.view === `group:${g.id}` && !store.search}
    class:drop={dropOn === `g${g.id}`}
    style:padding-left={local ? "30px" : null}
    onclick={() => store.open(`group:${g.id}`)}
    oncontextmenu={(e) => groupMenu(e, g)}
    ondragover={(e) => { e.preventDefault(); dropOn = `g${g.id}`; }}
    ondragleave={() => (dropOn = null)}
    ondrop={(e) => onDropGroup(e, g)}
    title={local ? t("Group for this mailbox") : t("Global group (all mailboxes)")}
  >
    <span class="gdot" style:background={g.color}></span>
    <span class="name">{g.name}</span>
    {#if c?.unread}<span class="badge" use:countUp={c.unread} transition:scale={{ duration: 160, start: 0.6 }}></span>{:else if c?.total}<span class="badge muted" use:countUp={c.total} transition:scale={{ duration: 160, start: 0.6 }}></span>{/if}
  </button>
{/snippet}

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

<style>
  .pane {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .top {
    padding: 4px 8px 8px 4px;
  }
  .new {
    width: 100%;
    height: 38px;
    border-radius: var(--radius-l);
    font-size: 0.92rem;
    letter-spacing: 0.01em;
    background: var(--brand-grad);
    background-size: 140% 140%;
    background-position: 0% 0%;
    box-shadow: 0 2px 8px color-mix(in srgb, var(--brand-2) 34%, transparent), inset 0 1px 0 rgba(255, 255, 255, 0.22);
    transition: background-position 400ms var(--ease), box-shadow var(--dur) var(--ease), transform 90ms var(--ease);
  }
  .new:hover {
    background: var(--brand-grad);
    background-size: 140% 140%;
    background-position: 100% 100%;
    box-shadow: 0 4px 16px color-mix(in srgb, var(--brand-2) 42%, transparent), inset 0 1px 0 rgba(255, 255, 255, 0.22);
  }
  .spaces {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
    padding: 0 8px 8px 4px;
  }
  .sp {
    height: 26px;
    padding: 0 10px;
    border-radius: 13px;
    border: 1px solid var(--border-strong);
    background: transparent;
    font-size: 0.8rem;
    font-weight: 600;
    color: var(--text-2);
    transition: background var(--dur), color var(--dur), border-color var(--dur);
  }
  .sp:hover {
    background: var(--hover);
  }
  .sp.on {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
    box-shadow: 0 1px 4px color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .sp.add {
    width: 26px;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .tree {
    flex: 1;
    padding: 0 6px 16px 2px;
  }
  .section {
    padding: 12px 10px 4px;
    font-size: 0.7rem;
    font-weight: 700;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .section:first-child {
    padding-top: 2px;
  }
  .with-action {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .mini {
    border: none;
    background: none;
    color: var(--text-3);
    display: flex;
    padding: 3px;
    border-radius: 4px;
  }
  .mini:hover {
    background: var(--hover);
    color: var(--text);
  }
  .node {
    position: relative;
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 32px;
    padding: 0 10px 0 14px;
    border: none;
    border-radius: var(--radius);
    background: transparent;
    color: var(--text);
    font-size: 0.92rem;
    text-align: left;
    transition: background var(--dur) var(--ease), color var(--dur) var(--ease);
  }
  .node.sub {
    height: 30px;
    font-size: 0.88rem;
  }
  .node.hint {
    color: var(--text-3);
    font-size: 0.86rem;
  }
  .node :global(.icon) {
    color: var(--text-2);
  }
  .node:hover {
    background: var(--hover);
  }
  .node.on {
    background: var(--selected);
    color: var(--text-accent);
    font-weight: 600;
  }
  .node.on :global(.icon) {
    color: var(--text-accent);
  }
  .node.on::before,
  .acc-head.on::before {
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
  .node.drop {
    background: var(--selected-strong);
    outline: 2px dashed var(--accent);
    outline-offset: -2px;
  }
  .gdot {
    width: 12px;
    height: 12px;
    border-radius: 4px;
    margin: 0 3px;
    flex: none;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .badge.muted {
    color: var(--text-3);
  }
  .account {
    margin-top: 2px;
  }
  .acc-head {
    position: relative;
    display: flex;
    align-items: center;
    gap: 4px;
    height: 42px;
    padding: 0 8px 0 0;
    border-radius: var(--radius);
    transition: background var(--dur);
  }
  .acc-head:hover {
    background: var(--hover);
  }
  .acc-head.on {
    background: var(--selected);
  }
  .chev {
    width: 22px;
    height: 30px;
    border: none;
    background: none;
    color: var(--text-3);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    transition: transform var(--dur) var(--ease);
  }
  .chev.closed {
    transform: rotate(-90deg);
  }
  .acc-main {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 9px;
    border: none;
    background: none;
    padding: 0;
    text-align: left;
    height: 100%;
  }
  .swatch {
    width: 24px;
    height: 24px;
    border-radius: 7px;
    flex: none;
    color: white;
    font-size: 0.74rem;
    font-weight: 700;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .acc-text {
    min-width: 0;
    display: flex;
    flex-direction: column;
    line-height: 1.2;
  }
  .acc-name {
    font-weight: 600;
    font-size: 0.92rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .acc-mail {
    font-size: 0.74rem;
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .err {
    color: var(--danger);
    display: flex;
  }
  .acc-error {
    margin: 4px 8px 4px 30px;
    padding: 6px 8px;
    border-radius: var(--radius);
    background: color-mix(in srgb, var(--danger) 10%, transparent);
    color: var(--danger);
    font-size: 0.78rem;
    user-select: text;
    word-break: break-word;
  }
  .children {
    animation: fade 160ms var(--ease);
    padding-bottom: 4px;
  }
  .empty {
    padding: 8px 12px;
    line-height: 1.5;
  }
</style>
