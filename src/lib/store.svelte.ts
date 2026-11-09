import { t } from "./i18n.svelte";
import { setInvokeHooks } from "./tauri";
import { updateBadge, type BadgeMode } from "./badge";
import type { NpuStatus, NpuInfo, NpuProgress, NpuAvailable, NpuDirs } from "./api";
import { api, type Account, type Folder, type Message, type Signature, type Space, type Group, type Outgoing, type OutboxItem, type Suggestion, type Reminder } from "./api";
import { i18n, syncLang, type Lang } from "./i18n.svelte";

export interface Settings {
  theme: "system" | "light" | "dark";
  accent: string;
  density: "comfortable" | "compact";
  font_scale: number;
  reading_pane: "right" | "bottom" | "off";
  preview_lines: number;
  show_avatars: boolean;
  sync_interval: number;
  mark_read_delay: number; // seconds, -1 = never automatically
  remote_images: "ask" | "always" | "never";
  notifications: boolean;
  /** Also notify for new mail that goes straight into a group hidden from the inbox. */
  notify_groups: boolean;
  /** Unread count on the taskbar / tray icon: number, a dot, or nothing. */
  taskbar_badge: BadgeMode;
  /** The navigation rail (Mail / Calendar / Settings) is hidden behind a line at the left edge. */
  rail_collapsed: boolean;
  /** Calendar reminders (12 h … 10 min before). */
  reminders: boolean;
  confirm_delete: boolean;
  search_mode: "subject" | "content" | "people";
  folder_pane_width: number;
  list_width: number;
  week_start: number; // 0 = Sunday, 1 = Monday
  trusted_senders: string[];
  close_to_tray: boolean;
  ai_default: string;
  ai_allowed_accounts: number[] | null;
  ai_days: number;
  space: number;
  lang: Lang;
  /** Seconds a sent message waits in the corner (undo window), 15–300. */
  send_delay: number;
  /** Zoom of the message body and attachment preview (1 = 100%). */
  reading_zoom: number;
  ai_autosuggest: boolean;
  ai_thinking: boolean;
  /** Let the assistant search the web (toggle in the chat box). */
  ai_web: boolean;
  /** Ids of AI suggestion chips that were used and should stay hidden. */
  ai_hidden_hints: string[];
}

export const defaults: Settings = {
  theme: "system",
  accent: "#1d4fe0",
  density: "comfortable",
  font_scale: 1,
  reading_pane: "right",
  preview_lines: 1,
  show_avatars: true,
  sync_interval: 5,
  mark_read_delay: 1,
  remote_images: "ask",
  notifications: true,
  notify_groups: false,
  reminders: true,
  rail_collapsed: false,
  taskbar_badge: "count",
  confirm_delete: false,
  search_mode: "content",
  folder_pane_width: 248,
  list_width: 400,
  week_start: 1,
  trusted_senders: [],
  close_to_tray: true,
  ai_default: "",
  ai_allowed_accounts: null,
  ai_days: 30,
  space: 0,
  lang: "en",
  send_delay: 60,
  reading_zoom: 1,
  ai_autosuggest: false,
  ai_thinking: false,
  ai_web: false,
  ai_hidden_hints: [],
};

export type ComposeMode = "new" | "reply" | "replyAll" | "forward" | "draft";
export interface ComposeInit {
  key: number;
  mode: ComposeMode;
  source?: Message;
  to?: string[];
  aiText?: string;
  /** Re-open a message that was cancelled from the send queue or the scheduled list. */
  restore?: Outgoing;
}

export interface PendingSend {
  id: number;
  message: Outgoing;
  deadline: number;
  total: number;
  sending: boolean;
}

/** A task the AI panel should start (from the reading pane, list menus…). */
export interface AiIntent {
  key: number;
  task: "summarize" | "reply" | "cluster" | "report" | "chat" | "signature";
  ids?: number[];
  prompt?: string;
  /** Called with the AI result (e.g. generated signature HTML). */
  done?: (text: string) => void;
}

export interface Toast {
  id: number;
  kind: "info" | "error" | "success";
  text: string;
  action?: { label: string; run: () => void };
}

class Store {
  route = $state<"mail" | "calendar" | "settings">("mail");
  settingsTab = $state("accounts");
  settings = $state<Settings>({ ...defaults });
  accounts = $state<Account[]>([]);
  folders = $state<Folder[]>([]);
  signatures = $state<Signature[]>([]);
  counts = $state<{ inbox: number; flagged: number; groups: Record<string, { total: number; unread: number }>; accounts: Record<string, number> }>({ inbox: 0, flagged: 0, groups: {}, accounts: {} });
  spaces = $state<Space[]>([]);
  groups = $state<Group[]>([]);
  aiOpen = $state(false);
  aiIntent = $state<AiIntent | null>(null);
  ruleDraft = $state<{ from?: string; subject?: string } | null>(null);
  view = $state("unified:inbox");
  filter = $state("");
  search = $state("");
  messages = $state<Message[]>([]);
  /** Calendar: jump to this day and open this event (from a reminder, an invitation…). */
  calFocus = $state<{ id: number; ts: number; seq: number } | null>(null);
  private focusSeq = 0;
  /** Calendar reminders shown as cards in the window. */
  reminders = $state<Reminder[]>([]);
  /** Operations in flight that the user is waiting for (drives the progress bar). */
  busy = $state(0);
  /** Bumped to make the list scroll the selected email into view. */
  reveal = $state(0);
  hasMore = $state(true);
  loading = $state(false);
  selected = $state<number[]>([]);
  anchor = $state<number | null>(null);
  compose = $state<ComposeInit | null>(null);
  syncing = $state<Record<number, boolean>>({});
  errors = $state<Record<number, string>>({});
  toasts = $state<Toast[]>([]);
  collapsed = $state<Record<string, boolean>>({});
  ready = $state(false);

  idsTitle = $state("");
  /** On-device AI (NPU): lives in the store so progress survives leaving the settings page. */
  npu = $state<{
    status: NpuStatus | null;
    info: NpuInfo | null;
    avail: NpuAvailable | null;
    installing: boolean;
    runtime: NpuProgress | null;
    pulls: Record<string, NpuProgress>;
    loading: boolean;
    /** Driver install in progress (phase: search / download / install). */
    driver: NpuProgress | null;
    dirs: NpuDirs | null;
    /** Folder move in progress. */
    moving: NpuProgress | null;
  }>({ status: null, info: null, avail: null, installing: false, runtime: null, pulls: {}, loading: false, driver: null, dirs: null, moving: null });
  /** New mail waiting for the NPU to wake before it gets group suggestions. */
  suggestQueue = $state(0);
  /** Corner popup after the queue was processed. */
  suggestPopup = $state(0);
  /** Citation numbers ([E1] → 1) of the emails shown in an "ids:" list opened from an AI answer. */
  cite = $state<Map<number, number>>(new Map());
  /** Background work on the NPU (sorting queued mail), shown so the NPU never looks busy for nothing. */
  npuBg = $state<{ active: boolean; count: number } | null>(null);
  /** Suggestions bar expanded (opened from the popup). */
  suggestOpen = $state(false);
  pendingSends = $state<PendingSend[]>([]);
  outbox = $state<OutboxItem[]>([]);
  suggestions = $state<Suggestion[]>([]);
  /** Message ids selected via checkboxes (multi-select mode). */
  checkMode = $state(false);
  private reqSeq = 0;
  private toastSeq = 0;
  private composeSeq = 0;

  get current(): Message | null {
    if (this.selected.length !== 1) return null;
    return this.messages.find((m) => m.id === this.selected[0]) ?? null;
  }

  folderById(id: number) {
    return this.folders.find((f) => f.id === id);
  }
  accountById(id: number) {
    return this.accounts.find((a) => a.id === id);
  }

  viewTitle(): string {
    if (this.search.trim()) return t("Search results");
    switch (this.view) {
      case "unified:inbox": return t("All inboxes");
      case "unified:flagged": return t("Flagged");
      case "unified:unread": return t("Unread");
      case "outbox": return t("Scheduled");
      case "suggestions": return t("AI suggestions");
    }
    if (this.view.startsWith("folder:")) {
      const f = this.folderById(Number(this.view.slice(7)));
      return f?.name ?? t("Folder");
    }
    if (this.view.startsWith("account:")) {
      return this.accountById(Number(this.view.slice(8)))?.email ?? "";
    }
    if (this.view.startsWith("group:")) {
      return this.groups.find((g) => g.id === Number(this.view.slice(6)))?.name ?? t("Group");
    }
    if (this.view.startsWith("ids:")) return this.idsTitle || t("Results");
    return "";
  }

  // ---------- settings ----------
  async loadSettings() {
    const raw = await api.settings();
    const s = { ...defaults } as Record<string, unknown>;
    for (const [k, v] of Object.entries(raw)) if (k in defaults) s[k] = v;
    this.settings = s as unknown as Settings;
    if (!("lang" in raw)) this.settings.lang = i18n.lang;
    else syncLang(this.settings.lang);
    this.settings.send_delay = Math.min(300, Math.max(15, Number(this.settings.send_delay) || 60));
    // The old default accent (Nelo blue) follows the new Hermio blue; a colour you picked stays.
    if (String(this.settings.accent).toLowerCase() === "#0f6cbd") this.settings.accent = defaults.accent;
    this.applyTheme();
  }

  async set<K extends keyof Settings>(key: K, value: Settings[K]) {
    this.settings[key] = value;
    this.applyTheme();
    await api.setSetting(key, value);
    if (key === "taskbar_badge") this.updateBadge();
  }

  applyTheme() {
    const root = document.documentElement;
    const s = this.settings;
    const dark = s.theme === "dark" || (s.theme === "system" && matchMedia("(prefers-color-scheme: dark)").matches);
    root.dataset.theme = dark ? "dark" : "light";
    root.dataset.density = s.density;
    root.style.setProperty("--accent", s.accent);
    root.style.setProperty("--font-scale", String(s.font_scale));
    root.style.fontSize = `${14 * s.font_scale}px`;
  }

  // ---------- data ----------
  async loadAccounts() {
    this.accounts = await api.accounts();
    for (const a of this.accounts) if (a.last_error) this.errors[a.id] = a.last_error;
  }
  async loadFolders() {
    this.folders = await api.folders();
    this.counts = await api.unreadCounts(this.settings.space);
    this.updateBadge();
  }
  async loadSpaces() {
    this.spaces = await api.spaces();
    if (this.settings.space && !this.spaces.some((s) => s.id === this.settings.space)) this.settings.space = 0;
  }
  async loadGroups() {
    this.groups = await api.groups();
  }
  /** Accounts visible in the current space. */
  get visibleAccounts() {
    const sp = this.settings.space;
    return sp ? this.accounts.filter((a) => a.space_id === sp) : this.accounts;
  }
  async setSpace(id: number) {
    await this.set("space", id);
    this.selected = [];
    await this.loadFolders();
    if (this.view.startsWith("unified")) await this.loadMessages(true);
  }
  ai(task: AiIntent["task"], ids?: number[], prompt?: string) {
    this.aiOpen = true;
    this.aiIntent = { key: Date.now(), task, ids, prompt };
  }
  async loadSignatures() {
    this.signatures = await api.signatures();
  }

  async loadMessages(reset = true) {
    if (this.view === "outbox") return this.loadOutbox();
    const seq = ++this.reqSeq;
    const offset = reset ? 0 : this.messages.length;
    const page = 300;
    if (reset) this.loading = true;
    try {
      const rows = await api.messages({
        view: this.search.trim() && this.view.startsWith("unified") ? "all" : this.view,
        search: this.search,
        search_mode: this.settings.search_mode,
        filter: this.filter,
        sort_asc: false,
        space: this.view.startsWith("unified") || this.view === "all" ? this.settings.space : 0,
        limit: page,
        offset,
      });
      if (seq !== this.reqSeq) return;
      this.messages = reset ? rows : [...this.messages, ...rows];
      this.hasMore = rows.length === page;
      if (reset) {
        const ids = new Set(this.messages.map((m) => m.id));
        this.selected = this.selected.filter((id) => ids.has(id));
      }
    } catch (e) {
      this.error(String(e));
    } finally {
      if (seq === this.reqSeq) this.loading = false;
    }
  }

  /** Refresh after background changes without losing scroll position or selection. */
  async refresh() {
    if (this.view === "outbox") return this.loadOutbox();
    const seq = ++this.reqSeq;
    const n = Math.max(300, this.messages.length);
    const rows = await api.messages({
      view: this.search.trim() && this.view.startsWith("unified") ? "all" : this.view,
      search: this.search,
      search_mode: this.settings.search_mode,
      filter: this.filter,
      sort_asc: false,
      space: this.view.startsWith("unified") || this.view === "all" ? this.settings.space : 0,
      limit: n,
      offset: 0,
    });
    if (seq !== this.reqSeq) return;
    this.messages = this.merge(rows);
    this.hasMore = rows.length === n;
    const ids = new Set(rows.map((m) => m.id));
    this.selected = this.selected.filter((id) => ids.has(id));
  }

  /** Reuse existing row objects so unchanged rows (and the open message) don't re-render. */
  private merge(rows: Message[]): Message[] {
    const old = new Map(this.messages.map((m) => [m.id, m]));
    let same = rows.length === this.messages.length;
    const out = rows.map((r, i) => {
      const o = old.get(r.id);
      if (!o) {
        same = false;
        return r;
      }
      for (const k of Object.keys(r) as (keyof Message)[]) {
        if (o[k] !== r[k]) (o as unknown as Record<string, unknown>)[k] = r[k];
      }
      if (same && this.messages[i] !== o) same = false;
      return o;
    });
    return same ? this.messages : out;
  }

  /** Reload lists after something changed elsewhere (settings, rules, groups, sync…). */
  async reloadAll() {
    await Promise.all([this.loadGroups(), this.loadFolders(), this.loadOutbox(), this.loadSuggestions()]);
    await this.refresh();
  }
  // ---------- on-device AI (NPU) ----------
  async loadNpu(full = true) {
    this.npu.loading = true;
    try {
      this.npu.status = await api.npuStatus();
      if (!this.npu.info) api.npuInfo().then((i) => (this.npu.info = i)).catch(() => {});
      if (full && this.npu.status.installed) {
        this.npu.avail = await api.npuAvailable();
        if (this.npu.avail.npu) this.npu.info = this.npu.avail.npu;
      }
    } catch (e) {
      this.error(String(e));
    } finally {
      this.npu.loading = false;
    }
  }
  async syncNpuModels() {
    try {
      const models = await api.npuSyncModels();
      if (this.npu.status) this.npu.status.models = models;
    } catch {}
  }
  onNpuProgress(p: NpuProgress) {
    if (p.kind === "driver") {
      this.npu.driver = p.finished ? null : p;
      return;
    }
    if (p.kind === "move") {
      this.npu.moving = p.finished ? null : p;
      if (p.finished && p.error) this.error(p.error);
      return;
    }
    if (p.kind === "runtime") {
      if (p.finished) {
        this.npu.runtime = null;
        this.npu.installing = false;
        if (p.error) this.error(p.error);
        this.loadNpu();
      } else {
        this.npu.installing = true;
        this.npu.runtime = p;
      }
    } else if (p.finished) {
      const { [p.tag]: _, ...rest } = this.npu.pulls;
      this.npu.pulls = rest;
      if (p.error) this.error(t("Download of {name} failed", { name: p.tag }) + `: ${p.error.split("\n").pop()}`);
      else this.success(t("{name} is ready", { name: p.tag }));
      this.loadNpu();
    } else if (p.tag in this.npu.pulls || this.npu.status?.pulling.includes(p.tag)) {
      this.npu.pulls = { ...this.npu.pulls, [p.tag]: p };
    }
  }
  async installNpu() {
    if (this.npu.installing) return;
    this.npu.installing = true;
    this.npu.runtime = { kind: "runtime", tag: "check", percent: null, done: 0, total: 0, line: "", finished: false, error: "" };
    try {
      const v = await api.npuInstall();
      // Show the new state right away; the full model list follows.
      if (this.npu.status) {
        this.npu.status.installed = true;
        this.npu.status.version = v;
      }
      this.success(t("NPU runtime installed"));
    } catch (e) {
      this.error(String(e));
    } finally {
      this.npu.installing = false;
      this.npu.runtime = null;
      await this.loadNpu();
    }
  }
  async installDriver() {
    if (this.npu.driver) return;
    this.npu.driver = { kind: "driver", tag: "search", percent: null, done: 0, total: 0, line: "", finished: false, error: "" };
    try {
      const r = await api.npuInstallDriver();
      if (r.ok) {
        this.success(r.reboot ? t("NPU driver installed. Restart Windows to finish.") : t("NPU driver installed"));
      } else if (r.url) {
        this.error(t("Hermio couldn't install the driver automatically. Opening the download page…"));
        api.openUrl(r.url).catch(() => {});
      }
      this.npu.info = await api.npuInfo(true);
    } catch (e) {
      this.error(String(e));
    } finally {
      this.npu.driver = null;
    }
  }
  async loadDirs() {
    try {
      this.npu.dirs = await api.npuDirs();
    } catch {}
  }
  async setNpuDirs(runtime: string | null, models: string | null, moveFiles: boolean) {
    this.npu.moving = { kind: "move", tag: "", percent: null, done: 0, total: 0, line: "", finished: false, error: "" };
    try {
      this.npu.dirs = await api.npuSetDirs(runtime, models, moveFiles);
      this.success(t("Storage folder changed"));
      await this.loadNpu();
    } catch (e) {
      this.error(String(e));
    } finally {
      this.npu.moving = null;
    }
  }
  async pullNpu(tag: string, fresh = false) {
    this.npu.pulls = {
      ...this.npu.pulls,
      [tag]: { kind: "model", tag, percent: null, done: 0, total: 0, line: fresh ? t("Deleting the old files…") : t("Starting download…"), finished: false, error: "" },
    };
    try {
      if (fresh) await api.npuRedownload(tag);
      else await api.npuPull(tag);
      if (!this.settings.ai_default) this.set("ai_default", "npu");
    } catch {
      // reported through the progress event
    } finally {
      const { [tag]: _, ...rest } = this.npu.pulls;
      this.npu.pulls = rest;
      this.loadNpu();
    }
  }

  async loadOutbox() {
    try {
      this.outbox = await api.listOutbox();
    } catch {}
  }
  async loadSuggestions() {
    try {
      this.suggestions = await api.aiSuggestions();
    } catch {}
  }

  /**
   * A new-mail toast was clicked: show the email. It is opened in the inbox it arrived in (the
   * unified inbox when the account is in the current space) so the user lands in a normal list;
   * mail that went straight into a hidden group is shown on its own.
   */
  async openNotified(messageId: number, accountId: number) {
    if (!messageId) return;
    this.route = "mail";
    this.search = "";
    this.compose = null;
    const inSpace = this.visibleAccounts?.some((a) => a.id === accountId) ?? true;
    const inbox = this.folders.find((f) => f.account_id === accountId && f.role === "inbox");
    const view = inSpace ? "unified:inbox" : inbox ? `folder:${inbox.id}` : `account:${accountId}`;
    this.cite = new Map();
    this.view = view;
    this.checkMode = false;
    await this.loadMessages(true);
    if (!this.messages.some((m) => m.id === messageId)) {
      this.idsTitle = t("New mail");
      this.view = `ids:${messageId}`;
      await this.loadMessages(true);
    }
    this.selected = [messageId];
    this.anchor = messageId;
    this.reveal++;
  }

  // ---------- sync started by the user: report how it went ----------
  private manualSync = new Map<number, string>();
  private manualAll = false;
  private manualFailed: string[] = [];
  async syncNow(accountId: number | null) {
    const ids = accountId === null ? this.accounts.filter((a) => a.enabled).map((a) => a.id) : [accountId];
    if (!ids.length) return this.info(t("No mailbox to sync"));
    for (const id of ids) this.manualSync.set(id, this.accountById(id)?.email ?? "");
    this.manualAll = accountId === null && ids.length > 1;
    this.manualFailed = [];
    try {
      await api.syncNow(accountId);
    } catch (e) {
      this.manualSync.clear();
      this.error(t("Sync failed: {err}", { err: String(e) }));
    }
  }
  /** Called when a mailbox finished syncing. */
  syncFinished(accountId: number, error?: string) {
    const email = this.manualSync.get(accountId);
    if (email === undefined) return;
    this.manualSync.delete(accountId);
    if (error) this.manualFailed.push(email);
    if (!this.manualAll) {
      if (error) this.error(t("Sync of {email} failed: {err}", { email, err: error }));
      else this.success(t("{email} is up to date", { email }));
    } else if (!this.manualSync.size) {
      if (this.manualFailed.length) this.error(t("Sync failed for {list}. Open the sync status for details.", { list: this.manualFailed.join(", ") }));
      else this.success(t("All mailboxes are up to date"));
      this.manualAll = false;
    }
  }

  /** Unread mail in the inboxes of every signed-in mailbox (all spaces), on the taskbar icon. */
  updateBadge() {
    const per = (this.counts as { accounts?: Record<string, number> })?.accounts ?? {};
    const total = Object.values(per).reduce((a, b) => a + (Number(b) || 0), 0);
    updateBadge(total, this.settings.taskbar_badge ?? "count");
  }

  showEvent(id: number, ts: number) {
    this.route = "calendar";
    this.calFocus = { id, ts, seq: ++this.focusSeq };
  }

  /** From the suggestion popup: show the inbox with the suggestions list expanded. */
  reviewSuggestions() {
    this.suggestPopup = 0;
    this.aiOpen = false;
    this.open("unified:inbox");
    this.suggestOpen = true;
  }

  open(view: string) {
    this.route = "mail";
    if (!view.startsWith("ids:")) this.cite = new Map();
    if (this.view === view && !this.search) return;
    this.view = view;
    this.selected = [];
    this.checkMode = false;
    if (view === "outbox") {
      this.messages = [];
      this.loadOutbox();
      return;
    }
    this.loadMessages(true);
  }

  // ---------- actions ----------
  private patch(ids: number[], f: (m: Message) => void) {
    const set = new Set(ids);
    for (const m of this.messages) if (set.has(m.id)) f(m);
  }

  /** `announce`: confirm with a popup (user actions; not the automatic mark-as-read). */
  async markSeen(ids: number[], seen: boolean, announce = true) {
    if (!ids.length) return;
    this.patch(ids, (m) => (m.seen = seen));
    try {
      await api.setSeen(ids, seen);
      if (announce) this.info(seen ? t("Marked {n} as read", { n: ids.length }) : t("Marked {n} as unread", { n: ids.length }));
    } catch (e) {
      this.patch(ids, (m) => (m.seen = !seen));
      this.error(String(e));
    }
  }
  async toggleFlag(ids: number[]) {
    if (!ids.length) return;
    const msgs = this.messages.filter((m) => ids.includes(m.id));
    const on = !msgs.every((m) => m.flagged);
    this.patch(ids, (m) => (m.flagged = on));
    try {
      await api.setFlagged(ids, on);
      this.info(on ? t("Flagged") : t("Flag removed"));
    } catch (e) {
      this.patch(ids, (m) => (m.flagged = !on));
      this.error(String(e));
    }
  }

  /** Remove rows from the list and move the selection to the next message. */
  private dropRows(ids: number[]) {
    const set = new Set(ids);
    const idx = this.messages.findIndex((m) => set.has(m.id));
    this.messages = this.messages.filter((m) => !set.has(m.id));
    if (this.selected.some((id) => set.has(id))) {
      const next = this.messages[Math.min(idx, this.messages.length - 1)];
      this.selected = next && this.selected.length === 1 ? [next.id] : [];
    }
  }

  async remove(ids: number[], permanent = false) {
    if (!ids.length) return;
    if (this.settings.confirm_delete || permanent) {
      if (!confirm(permanent ? t("Permanently delete {n} messages?", { n: ids.length }) : t("Delete {n} messages?", { n: ids.length }))) return;
    }
    this.dropRows(ids);
    try {
      await api.remove(ids, permanent);
      this.info(ids.length > 1 ? t("Deleted {n} messages", { n: ids.length }) : t("Message deleted"));
    } catch (e) {
      this.error(String(e));
      this.refresh();
    }
  }
  async archive(ids: number[]) {
    if (!ids.length) return;
    this.dropRows(ids);
    try {
      await api.archive(ids);
      this.info(t("Archived"));
    } catch (e) {
      this.error(String(e));
      this.refresh();
    }
  }
  async spam(ids: number[]) {
    if (!ids.length) return;
    this.dropRows(ids);
    try {
      await api.spam(ids);
      this.info(t("Moved to Junk"));
    } catch (e) {
      this.error(String(e));
      this.refresh();
    }
  }
  async moveTo(ids: number[], folderId: number) {
    if (!ids.length) return;
    const f = this.folderById(folderId);
    this.dropRows(ids);
    try {
      await api.move(ids, folderId);
      this.info(t("Moved to {name}", { name: f?.name ?? t("folder") }));
    } catch (e) {
      this.error(String(e));
      this.refresh();
    }
  }

  startCompose(mode: ComposeMode, source?: Message, to?: string[], aiText?: string) {
    this.route = "mail";
    if (source && !source.seen && mode !== "draft") this.markSeen([source.id], true, false);
    this.compose = { key: ++this.composeSeq, mode, source, to, aiText };
  }
  restoreCompose(message: Outgoing) {
    this.route = "mail";
    this.compose = { key: ++this.composeSeq, mode: "new", restore: message };
  }

  // ---------- send queue (undo window) ----------
  private sendSeq = 0;
  private timers = new Map<number, ReturnType<typeof setTimeout>>();
  queueSend(message: Outgoing) {
    const secs = Math.min(300, Math.max(15, Number(this.settings.send_delay) || 60));
    const id = ++this.sendSeq;
    this.pendingSends = [...this.pendingSends, { id, message, deadline: Date.now() + secs * 1000, total: secs, sending: false }];
    this.timers.set(id, setTimeout(() => this.sendNow(id), secs * 1000));
  }
  async sendNow(id: number) {
    const p = this.pendingSends.find((x) => x.id === id);
    if (!p || p.sending) return;
    clearTimeout(this.timers.get(id));
    this.timers.delete(id);
    p.sending = true;
    try {
      await api.send($state.snapshot(p.message) as Outgoing);
      this.pendingSends = this.pendingSends.filter((x) => x.id !== id);
      this.success(t("Message sent"));
    } catch (e) {
      this.pendingSends = this.pendingSends.filter((x) => x.id !== id);
      this.toast("error", String(e).replace(/^Error: /, ""), { label: t("Edit"), run: () => this.restoreCompose(p.message) });
    }
  }
  cancelSend(id: number) {
    const p = this.pendingSends.find((x) => x.id === id);
    if (!p || p.sending) return;
    clearTimeout(this.timers.get(id));
    this.timers.delete(id);
    this.pendingSends = this.pendingSends.filter((x) => x.id !== id);
    this.restoreCompose($state.snapshot(p.message) as Outgoing);
  }

  // ---------- toasts ----------
  toast(kind: Toast["kind"], text: string, action?: Toast["action"]) {
    // The same message twice (e.g. a failure reported by both the action and the app): show it once.
    const same = this.toasts.find((x) => x.kind === kind && (x.text === text || (kind === "error" && (x.text.includes(text) || text.includes(x.text)))));
    if (same) return;
    const id = ++this.toastSeq;
    this.toasts = [...this.toasts, { id, kind, text, action }];
    setTimeout(() => this.dismiss(id), kind === "error" ? 8000 : 3500);
  }
  info(t: string) { this.toast("info", t); }
  success(t: string) { this.toast("success", t); }
  error(t: string) { this.toast("error", t.replace(/^Error: /, "")); }
  dismiss(id: number) {
    this.toasts = this.toasts.filter((t) => t.id !== id);
  }
}

export const store = new Store();

// ---------- every backend call: progress bar + failure popups ----------
// Calls with their own progress/streaming UI, or that run in the background, stay silent.
const QUIET = /^(ai_(run|chat|chats|suggestions|suggest_queue|providers|skills|models)$|npu_(available|info|status|catalog|dirs|pull|install|cancel_pull)$|sync_log|notification_pending|invite_info|get_message|list_messages|list_folders|list_events|list_calendars|unread|message_source|start_hidden|restored_note|suggest_contacts|attachment_data|message_groups|set_setting|get_autostart|web_config|google_config|data_dir|guess_settings|discover_account)/;
// Failures of these are reported where they happen (inline), or are background reads.
const NO_POPUP = /^(list_|get_|ai_(run|test|chat|chats|suggestions|suggest_queue|providers|skills|models|read_file|read_bytes|cancel)$|npu_|sync_log|notification_|invite_info|message_source|start_hidden|restored_note|suggest_contacts|attachment_data|message_groups|unread|web_test|test_account|guess_settings|discover_account|google_config|web_config|data_dir)/;
let seq = 0;
const timers = new Map<number, ReturnType<typeof setTimeout> | "shown">();
setInvokeHooks({
  start(cmd) {
    const id = ++seq;
    if (!QUIET.test(cmd)) {
      timers.set(
        id,
        setTimeout(() => {
          timers.set(id, "shown");
          store.busy++;
        }, 300),
      );
    }
    return id;
  },
  end(id, cmd, error) {
    const tm = timers.get(id);
    if (tm === "shown") store.busy = Math.max(0, store.busy - 1);
    else if (tm) clearTimeout(tm);
    timers.delete(id);
    if (error !== undefined && !NO_POPUP.test(cmd)) store.error(String(error));
  },
});
