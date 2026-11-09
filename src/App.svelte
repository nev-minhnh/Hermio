<script lang="ts">
  import type { NpuProgress } from "./lib/api";
  import { t } from "./lib/i18n.svelte";
  import { onMount } from "svelte";
  import { listen } from "./lib/tauri";
  import { store } from "./lib/store.svelte";
  import { api } from "./lib/api";
  import { debounce } from "./lib/util";
  import TitleBar from "./components/TitleBar.svelte";
  import NavRail from "./components/NavRail.svelte";
  import MailView from "./components/MailView.svelte";
  import CalendarView from "./components/CalendarView.svelte";
  import SettingsView from "./components/SettingsView.svelte";
  import Toasts from "./components/Toasts.svelte";
  import Welcome from "./components/Welcome.svelte";
  import SendQueue from "./components/SendQueue.svelte";
  import { showWindow } from "./lib/tauri";
  import SuggestPopup from "./components/SuggestPopup.svelte";

  /** The page being left fades out underneath the new one and no longer takes clicks. */
  function leave(node: HTMLElement) {
    node.style.pointerEvents = "none";
    node.style.zIndex = "0";
    return { duration: 110, css: (k: number) => `opacity:${k}` };
  }

  const refreshAll = debounce(async () => {
    await Promise.all([store.loadFolders(), store.loadAccounts(), store.loadGroups(), store.loadOutbox(), store.loadSuggestions()]);
    if (store.route === "mail") await store.refresh();
  }, 250);

  onMount(() => {
    const unsubs: Array<() => void> = [];
    (async () => {
      await store.loadSettings();
      await Promise.all([store.loadAccounts(), store.loadSpaces(), store.loadGroups(), store.loadSignatures()]);
      await store.loadFolders();
      await store.loadMessages(true);
      store.ready = true;
      store.loadOutbox();
      store.loadSuggestions();
      if (!(await api.startHidden().catch(() => false))) showWindow();
      unsubs.push(await listen("tray-compose", () => store.startCompose("new")));

      unsubs.push(await listen("mail-changed", () => refreshAll()));
      unsubs.push(
        await listen<{ account_id: number; syncing: boolean; error?: string }>("sync-state", (p) => {
          store.syncing[p.account_id] = p.syncing;
          if (!p.syncing) {
            const acc = store.accounts.find((x) => x.id === p.account_id);
            if (acc && !p.error) acc.last_sync = Math.floor(Date.now() / 1000);
            if (p.error) store.errors[p.account_id] = p.error;
            else delete store.errors[p.account_id];
          }
        }),
      );
      unsubs.push(await listen<{ message: string }>("op-error", (p) => store.error(p.message)));
      unsubs.push(await listen("outbox-changed", () => store.loadOutbox()));
      unsubs.push(
        await listen<{ subject: string }>("scheduled-sent", (p) => {
          store.success(t("Scheduled message sent: {subject}", { subject: p?.subject || t("(No subject)") }));
          store.loadOutbox();
        }),
      );
      unsubs.push(
        await listen<{ count: number; popup?: boolean }>("ai-suggestions", async (p) => {
          await store.loadSuggestions();
          // Suggestions for mail that waited for the NPU: a small non-blocking popup.
          if (p?.popup && p.count > 0) store.suggestPopup = p.count;
        }),
      );
      unsubs.push(await listen<{ active: boolean; count?: number }>("npu-bg", (p) => (store.npuBg = p?.active ? { active: true, count: p.count ?? 0 } : null)));
      unsubs.push(await listen<{ count: number }>("ai-suggest-queue", (p) => (store.suggestQueue = p?.count ?? 0)));
      // Click on a new-mail toast: open that email (also a click made while this page was loading).
      unsubs.push(
        await listen<{ message_id: number; account_id: number }>("open-message", async () => {
          const p = await api.notificationPending().catch(() => null);
          if (p) store.openNotified(p.message_id, p.account_id);
        }),
      );
      api.notificationPending().then((p) => p && store.openNotified(p.message_id, p.account_id)).catch(() => {});
      api.restoredNote().then((p) => p && store.success(t("Data restored. Your previous data was kept in {path}", { path: p }))).catch(() => {});
      api.aiSuggestQueue().then((n) => (store.suggestQueue = n)).catch(() => {});
      unsubs.push(await listen<NpuProgress>("npu-progress", (p) => store.onNpuProgress(p)));
      unsubs.push(await listen("npu-state", () => store.loadNpu(false)));
      // NPU details are gathered in the background at start-up.
      api.npuInfo().then((i) => (store.npu.info = i)).catch(() => {});
      unsubs.push(await listen<{ message: string }>("sync-warning", (p) => console.warn(p.message)));
      // New-mail toasts are shown by the sync itself (also while this window is hidden).
    })();

    const mq = matchMedia("(prefers-color-scheme: dark)");
    const onScheme = () => store.applyTheme();
    mq.addEventListener("change", onScheme);
    return () => {
      unsubs.forEach((u) => u());
      mq.removeEventListener("change", onScheme);
    };
  });

  function onKey(e: KeyboardEvent) {
    const t = e.target as HTMLElement;
    const typing = t && (t.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(t.tagName));
    const ctrl = e.ctrlKey || e.metaKey;
    if (ctrl && e.key.toLowerCase() === "n") {
      e.preventDefault();
      store.startCompose("new");
      return;
    }
    if (ctrl && e.key.toLowerCase() === "e") {
      e.preventDefault();
      document.getElementById("global-search")?.focus();
      return;
    }
    if (ctrl && e.key.toLowerCase() === "j") {
      e.preventDefault();
      store.route = "mail";
      store.aiOpen = !store.aiOpen;
      return;
    }
    if (e.key === "F5") {
      e.preventDefault();
      api.syncNow(null);
      return;
    }
    if (typing || store.route !== "mail" || store.compose) return;
    const sel = store.selected;
    const cur = store.current;
    if (e.key === "Delete") {
      e.preventDefault();
      store.remove(sel, e.shiftKey);
    } else if (ctrl && e.key.toLowerCase() === "r" && cur) {
      e.preventDefault();
      store.startCompose(e.shiftKey ? "replyAll" : "reply", cur);
    } else if (ctrl && e.key.toLowerCase() === "f" && cur) {
      e.preventDefault();
      store.startCompose("forward", cur);
    } else if (ctrl && e.key.toLowerCase() === "q") {
      e.preventDefault();
      store.markSeen(sel, true);
    } else if (ctrl && e.key.toLowerCase() === "u") {
      e.preventDefault();
      store.markSeen(sel, false);
    } else if (e.key === "Insert") {
      e.preventDefault();
      store.toggleFlag(sel);
    } else if (e.key === "Backspace" && !ctrl) {
      e.preventDefault();
      store.archive(sel);
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="shell">
  <TitleBar />
  <div class="body">
    <NavRail />
    <main>
      {#key store.ready && store.accounts.length === 0 && store.route !== "settings" ? "welcome" : store.route}
        <div class="route" out:leave>
          {#if store.ready && store.accounts.length === 0 && store.route !== "settings"}
            <Welcome />
          {:else if store.route === "mail"}
            <MailView />
          {:else if store.route === "calendar"}
            <CalendarView />
          {:else}
            <SettingsView />
          {/if}
        </div>
      {/key}
    </main>
  </div>
  <Toasts />
  <SendQueue />
  <SuggestPopup />
</div>

<style>
  .shell {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--bg-app);
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  /* Routes are stacked in one grid cell: while the old page plays its exit transitions it sits
     underneath the new one instead of squeezing it side by side. */
  main {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: grid;
    grid-template: minmax(0, 1fr) / minmax(0, 1fr);
    padding: 0 8px 8px 0;
  }
  .route {
    grid-area: 1 / 1;
    position: relative;
    z-index: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    animation: scale-in 220ms var(--ease);
  }

</style>
