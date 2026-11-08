<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import Icon from "../lib/Icon.svelte";
  import { store } from "../lib/store.svelte";
  import { folderIcon } from "../lib/util";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";

  const sel = $derived(store.selected);
  const has = $derived(sel.length > 0);
  const cur = $derived(store.current);
  const selMsgs = $derived(store.messages.filter((m) => sel.includes(m.id)));
  const allRead = $derived(selMsgs.length > 0 && selMsgs.every((m) => m.seen));
  const allFlagged = $derived(selMsgs.length > 0 && selMsgs.every((m) => m.flagged));
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

  function at(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    return { x: r.left, y: r.bottom + 4 };
  }

  function moveMenu(e: MouseEvent) {
    const accounts = new Set(selMsgs.map((m) => m.account_id));
    if (accounts.size !== 1) {
      store.error(t("Select messages from one account to move"));
      return;
    }
    const aid = [...accounts][0];
    const items: MenuItem[] = [{ header: true, label: t("Move to") }];
    for (const f of store.folders.filter((f) => f.account_id === aid && f.selectable && !["all", "flagged", "important"].includes(f.role))) {
      if (selMsgs.every((m) => m.folder_id === f.id)) continue;
      items.push({ icon: folderIcon(f.role), label: f.name, run: () => store.moveTo(sel, f.id) });
    }
    menu = { ...at(e), items };
  }

  function filterMenu(e: MouseEvent) {
    const opt = (v: string, label: string, icon: string): MenuItem => ({
      icon: store.filter === v ? "check" : icon,
      label,
      run: () => {
        store.filter = v;
        store.loadMessages(true);
      },
    });
    menu = {
      ...at(e),
      items: [
        { header: true, label: t("Filter") },
        opt("", t("All"), "mail"),
        opt("unread", t("Unread"), "unread"),
        opt("flagged", t("Flagged"), "flag"),
        opt("attachments", t("Has attachments"), "attach"),
      ],
    };
  }

  function layoutMenu(e: MouseEvent) {
    const opt = (v: "right" | "bottom" | "off", label: string): MenuItem => ({
      icon: store.settings.reading_pane === v ? "check" : undefined,
      label,
      run: () => store.set("reading_pane", v),
    });
    menu = {
      ...at(e),
      items: [
        { header: true, label: t("Reading pane") },
        opt("right", t("Show on the right")),
        opt("bottom", t("Show at the bottom")),
        opt("off", t("Hide")),
        { header: true, label: t("Density") },
        { icon: store.settings.density === "comfortable" ? "check" : undefined, label: t("Comfortable"), run: () => store.set("density", "comfortable") },
        { icon: store.settings.density === "compact" ? "check" : undefined, label: t("Compact"), run: () => store.set("density", "compact") },
      ],
    };
  }
</script>

<div class="toolbar">
  <div class="group">
    <button class="tb" disabled={!has} onclick={() => store.remove(sel)} title={t("Delete (Delete)")}>
      <Icon name="trash" /><span>{t("Delete")}</span>
    </button>
    <button class="tb" disabled={!has} onclick={() => store.archive(sel)} title={t("Archive (Backspace)")}>
      <Icon name="archive" /><span>{t("Archive")}</span>
    </button>
    <button class="tb" disabled={!has} onclick={() => store.spam(sel)} title={t("Report spam")}>
      <Icon name="junk" /><span class="hide-s">{t("Junk")}</span>
    </button>
    <button class="tb" disabled={!has} onclick={moveMenu} title={t("Move")}>
      <Icon name="move" /><span class="hide-s">{t("Move")}</span><Icon name="chevronDown" size={12} />
    </button>
  </div>
  <div class="sep"></div>
  <div class="group">
    <button class="tb" disabled={!cur} onclick={() => cur && store.startCompose("reply", cur)} title={t("Reply (Ctrl+R)")}>
      <Icon name="reply" /><span>{t("Reply")}</span>
    </button>
    <button class="tb" disabled={!cur} onclick={() => cur && store.startCompose("replyAll", cur)} title={t("Reply all (Ctrl+Shift+R)")}>
      <Icon name="replyAll" /><span class="hide-m">{t("Reply all")}</span>
    </button>
    <button class="tb" disabled={!cur} onclick={() => cur && store.startCompose("forward", cur)} title={t("Forward (Ctrl+F)")}>
      <Icon name="forward" /><span class="hide-m">{t("Forward")}</span>
    </button>
  </div>
  <div class="sep"></div>
  <div class="group">
    <button class="tb" disabled={!has} onclick={() => store.markSeen(sel, !allRead)} title={allRead ? t("Mark as unread (Ctrl+U)") : t("Mark as read (Ctrl+Q)")}>
      <Icon name={allRead ? "unread" : "read"} /><span class="hide-m">{allRead ? t("Unread") : t("Read")}</span>
    </button>
    <button class="tb" class:active={allFlagged} disabled={!has} onclick={() => store.toggleFlag(sel)} title={t("Flag (Insert)")}>
      <Icon name="flag" fill={allFlagged} /><span class="hide-m">{allFlagged ? t("Unflag") : t("Flag")}</span>
    </button>
  </div>
  <div class="spacer"></div>
  <button class="tb" class:active={!!store.filter} onclick={filterMenu} title={t("Filter")}>
    <Icon name="filter" /><span class="hide-s">{t("Filter")}</span>
  </button>
  <button class="tb" onclick={layoutMenu} title={t("Layout")}>
    <Icon name="layout" /><span class="hide-s">{t("Layout")}</span>
  </button>
</div>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

<style>
  .toolbar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 2px;
    height: 48px;
    padding: 0 8px;
    border-bottom: 1px solid var(--border);
    overflow: hidden;
  }
  .group {
    display: flex;
    gap: 2px;
  }
  .sep {
    width: 1px;
    height: 22px;
    background: var(--border);
    margin: 0 6px;
  }
  .spacer {
    flex: 1;
  }
  .tb {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 34px;
    padding: 0 9px;
    border: none;
    border-radius: var(--radius);
    background: transparent;
    color: var(--text);
    font-size: 0.92rem;
    white-space: nowrap;
    transition: background var(--dur) var(--ease), opacity var(--dur);
  }
  .tb :global(.icon) {
    color: var(--text-accent);
  }
  .tb:hover {
    background: var(--hover);
  }
  .tb:active {
    background: var(--press);
  }
  .tb:disabled {
    opacity: 0.38;
    pointer-events: none;
  }
  .tb.active {
    background: var(--selected);
  }
  @media (max-width: 1350px) {
    .hide-m {
      display: none;
    }
  }
  @media (max-width: 1100px) {
    .hide-s {
      display: none;
    }
  }
</style>
