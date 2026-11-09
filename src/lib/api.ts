import { invoke } from "./tauri";

export interface Account {
  id: number;
  name: string;
  email: string;
  display_name: string;
  protocol: "imap" | "pop3";
  auth: "password" | "oauth_google";
  provider: string;
  in_host: string;
  in_port: number;
  in_security: string;
  out_host: string;
  out_port: number;
  out_security: string;
  username: string;
  pop3_dir: string;
  pop3_leave_on_server: boolean;
  pop3_recent_mode: boolean;
  color: string;
  sort_order: number;
  sig_new: number | null;
  sig_reply: number | null;
  calendar_sync: boolean;
  enabled: boolean;
  last_sync: number;
  last_error: string;
  smtp_username: string;
  space_id: number | null;
  sync_limit: number;
  body_limit: number;
  role_map: string;
}

export interface Space { id: number; name: string; color: string; sort_order: number }
export interface Group { id: number; name: string; color: string; hide_from_inbox: boolean; sort_order: number; account_id: number | null }
export interface Condition { field: string; op: string; value: string }
export interface Rule {
  id: number; name: string; enabled: boolean; match_all: boolean; conditions: Condition[]; account_ids: number[];
  group_id: number | null; mark_read: boolean; flag: boolean; stop: boolean; sort_order: number;
}
export interface Server { host: string; port: number; security: string; username: string }
export interface Discovery { provider: string; imap: Server | null; pop3: Server | null; smtp: Server | null; source: string; note: string }
export interface AiProvider { id: string; kind: string; name: string; base_url: string; model: string; has_key: boolean }
export interface AiImage { mime: string; data: string }
export interface ChatMsg { role: "user" | "assistant"; content: string; images?: AiImage[] }
export interface AiScope { kind: string; ids: number[] }
export interface RuleProposal { match_all: boolean; conditions: Condition[] }
export interface Cluster { title: string; summary: string; ids: number[]; rule: RuleProposal | null; senders?: number; rule_hits?: number; rule_extra?: number }
export interface Source { title: string; url: string }
export interface AiResponse { text: string; used: number[]; clusters: Cluster[]; thinking: string; sources: Source[]; cancelled: boolean; span?: string }
export interface AiFile { name: string; mime: string; text: string | null; image: string | null; pdf?: string | null; size: number }
export interface AiDelta { id: string; kind: "text" | "thinking" | "status" | "source" | "refs"; text?: string; title?: string; url?: string; ids?: number[] }
export interface WebHit { title: string; url: string; snippet: string; content: string }
export interface RuntimeModel {
  tag: string; name: string; family: string; params: string; size: number; footprint: number; vision: boolean; audio: boolean;
  thinking: boolean; tools: boolean; labels: string[]; context: number; min_version: string; url: string; compatible: boolean;
  chat: boolean; installed: boolean; recommended: boolean; note: string; repo: string; page: string;
}
export interface NpuInfo {
  present: boolean; vendor: "amd" | "intel" | ""; device: string; driver: string; driver_ok: boolean; driver_missing: boolean; driver_min: string;
  cpu: string; arch: string; engine: "flm" | "ovms" | ""; platform: string; supported: boolean; ram_gb: number; note: string;
}
export interface NpuDirs { found: string[]; foreign: string[]; runtime: string; runtime_default: string; runtime_size: number; models: string; models_default: string; models_size: number }
export interface InviteInfo {
  method: string; title: string; location: string; start_ts: number; end_ts: number; all_day: boolean; recurring: boolean;
  organizer: string; organizer_name: string; state: "" | "added" | "accepted" | "tentative" | "declined" | "dismissed";
  in_calendar: boolean; can_reply: boolean; calendar: string;
}
export interface InviteResult { event_id: number; added: boolean; already: boolean; replied: boolean; title: string; start_ts: number; all_day: boolean; calendar: string }
export interface Reminder { key: string; event_id: number; title: string; location: string; meet_link: string; start_ts: number; end_ts: number; left: number; last: boolean; color: string }
export interface SyncLogLine { ts: number; account_id: number; account: string; level: "info" | "ok" | "warn" | "error"; message: string }
export interface DriverOutcome { method: "intel-installer" | "windows-update" | "manual"; ok: boolean; message: string; reboot: boolean; url: string }
export interface NpuAvailable { models: RuntimeModel[]; ram_gb: number; version: string; npu: NpuInfo | null; collection: string; newer: string[] }
export interface AiRequest {
  provider_id: string;
  model?: string | null;
  thinking?: boolean;
  messages: ChatMsg[];
  scope: AiScope;
  days: number;
  from?: number | null;
  to?: number | null;
  task: string;
  files?: AiFile[];
  lang?: string;
  stream_id?: string | null;
  web?: boolean;
}
export interface Skill { id: string; name: string; description: string; content: string; enabled: boolean }
export interface ChatSummary { id: number; title: string; entries: unknown; updated: number }
export interface Suggestion { message_id: number; group_id: number | null; new_group: string; reason: string }
export interface OutboxItem { id: number; account_id: number; subject: string; recipients: string; send_at: number; status: string; error: string }
export interface NpuModel { tag: string; name: string; params: string; note: string; vision: boolean; thinking: boolean; recommended: boolean }
export interface NpuStatus { installed: boolean; path: string; version: string; running: boolean; port: number; npu: string; models: string[]; pulling: string[] }
export interface NpuProgress { kind: "runtime" | "model" | "driver" | "move"; tag: string; percent: number | null; done: number; total: number; rate?: number; line: string; finished: boolean; error: string }
export interface AttachmentData { name: string; mime: string; size: number; data: string; text: string | null }

export interface Folder {
  id: number;
  account_id: number;
  path: string;
  name: string;
  delim: string;
  role: string;
  selectable: boolean;
  unread: number;
  total: number;
}

export interface Message {
  /** Search results only: matching text, matches wrapped in U+E000 … U+E001. */
  hit?: string;
  /** Where `hit` comes from. */
  hit_field?: "body" | "from" | "to" | "cc";
  id: number;
  account_id: number;
  folder_id: number;
  uid: number;
  message_id: string;
  in_reply_to: string;
  subject: string;
  from_name: string;
  from_addr: string;
  to_list: string;
  cc_list: string;
  date: number;
  snippet: string;
  seen: boolean;
  flagged: boolean;
  answered: boolean;
  has_attachments: boolean;
  size: number;
}

export interface Attachment {
  index: number;
  name: string;
  mime: string;
  size: number;
  inline: boolean;
}

export interface MessageBody {
  id: number;
  html: string;
  text: string;
  has_remote_images: boolean;
  attachments: Attachment[];
  to: string;
  cc: string;
  reply_to: string;
  references: string;
  ics: string | null;
}

export interface Signature {
  id: number;
  name: string;
  html: string;
}

export interface Calendar {
  id: number;
  account_id: number | null;
  remote_id: string;
  name: string;
  color: string;
  visible: boolean;
  writable: boolean;
  kind: string;
  url: string;
  last_sync: number;
  last_error: string;
}

export interface CalEvent {
  id: number;
  calendar_id: number;
  remote_id: string;
  title: string;
  location: string;
  description: string;
  start_ts: number;
  end_ts: number;
  all_day: boolean;
  recurring: boolean;
  meet_link: string;
  color: string;
}

export interface Preset {
  provider: string;
  imap: [string, number, string];
  pop3: [string, number, string] | null;
  smtp: [string, number, string];
  note: string;
}

export interface Outgoing {
  account_id: number;
  to: string[];
  cc: string[];
  bcc: string[];
  subject: string;
  html: string;
  in_reply_to: string;
  references: string;
  attachments: { path?: string; from_message?: [number, number]; name: string }[];
  reply_of?: number | null;
  draft_id?: number | null;
}

export interface ListQuery {
  view: string;
  search: string;
  search_mode: "subject" | "content" | "people";
  filter: string;
  sort_asc: boolean;
  space?: number;
  limit: number;
  offset: number;
}

export const api = {
  settings: () => invoke<Record<string, unknown>>("get_settings"),
  setSetting: (key: string, value: unknown) => invoke<void>("set_setting", { key, value }),
  googleConfig: () => invoke<{ client_id: string; has_secret: boolean; builtin?: boolean }>("google_config"),
  setGoogleConfig: (clientId: string, clientSecret: string | null) =>
    invoke<void>("set_google_config", { clientId, clientSecret }),
  guess: (email: string) => invoke<Preset>("guess_settings", { email }),
  accounts: () => invoke<Account[]>("list_accounts"),
  defaultPop3Dir: (accountId: number) => invoke<string>("default_pop3_dir", { accountId }),
  saveAccount: (account: Partial<Account>, password: string | null, smtpPassword: string | null = null) =>
    invoke<number>("save_account", { account, password, smtpPassword }),
  discover: (email: string) => invoke<Discovery>("discover_account", { email }),
  quickAdd: (a: { email: string; password: string; displayName: string; protocol: string; pop3Dir?: string | null; spaceId?: number | null }) =>
    invoke<number>("quick_add_account", { ...a, pop3Dir: a.pop3Dir ?? null, spaceId: a.spaceId ?? null }),
  spaces: () => invoke<Space[]>("list_spaces"),
  saveSpace: (space: Partial<Space>) => invoke<number>("save_space", { space }),
  deleteSpace: (id: number) => invoke<void>("delete_space", { id }),
  setAccountSpace: (accountId: number, spaceId: number | null) => invoke<void>("set_account_space", { accountId, spaceId }),
  groups: () => invoke<Group[]>("list_groups"),
  saveGroup: (group: Partial<Group>) => invoke<number>("save_group", { group }),
  deleteGroup: (id: number) => invoke<void>("delete_group", { id }),
  addToGroup: (ids: number[], groupId: number) => invoke<void>("add_to_group", { ids, groupId }),
  removeFromGroup: (ids: number[], groupId: number) => invoke<void>("remove_from_group", { ids, groupId }),
  messageGroups: (id: number) => invoke<number[]>("message_groups", { id }),
  rules: () => invoke<Rule[]>("list_rules"),
  saveRule: (rule: Partial<Rule>) => invoke<number>("save_rule", { rule }),
  deleteRule: (id: number) => invoke<void>("delete_rule", { id }),
  runRules: (ruleId: number | null = null) => invoke<number>("run_rules", { ruleId }),
  aiProviders: () => invoke<AiProvider[]>("ai_providers"),
  aiSaveProvider: (provider: Partial<AiProvider>, apiKey: string | null) => invoke<AiProvider>("ai_save_provider", { provider, apiKey }),
  aiDeleteProvider: (id: string) => invoke<void>("ai_delete_provider", { id }),
  aiModels: (id: string) => invoke<string[]>("ai_models", { id }),
  aiTest: (id: string) => invoke<string>("ai_test", { id }),
  aiRun: (request: AiRequest) => invoke<AiResponse>("ai_run", { request }),
  aiSkills: () => invoke<Skill[]>("ai_skills"),
  aiSaveSkill: (skill: Partial<Skill>) => invoke<Skill>("ai_save_skill", { skill }),
  aiDeleteSkill: (id: string) => invoke<void>("ai_delete_skill", { id }),
  aiChats: () => invoke<ChatSummary[]>("ai_chats"),
  aiChat: (id: number) => invoke<ChatSummary>("ai_chat", { id }),
  aiSaveChat: (chat: { id: number; title: string; entries: unknown; updated?: number }) => invoke<number>("ai_save_chat", { chat: { updated: 0, ...chat } }),
  aiDeleteChat: (id: number) => invoke<void>("ai_delete_chat", { id }),
  aiReadFile: (path: string) => invoke<AiFile>("ai_read_file", { path }),
  aiReadBytes: (name: string, data: string) => invoke<AiFile>("ai_read_bytes", { name, data }),
  aiInstallSkillDir: (path: string) => invoke<Skill>("ai_install_skill_dir", { path }),
  aiCancel: (id: string) => invoke<void>("ai_cancel", { id }),
  webConfig: () => invoke<{ engine: string; searxng: string; has_brave_key: boolean }>("web_config"),
  setWebConfig: (engine: string, searxng: string, braveKey: string | null) => invoke<void>("set_web_config", { engine, searxng, braveKey }),
  webTest: (query: string) => invoke<WebHit[]>("web_test", { query }),
  npuAvailable: () => invoke<NpuAvailable>("npu_available"),
  npuInfo: (force = false) => invoke<NpuInfo>("npu_info", { force }),
  npuSyncModels: () => invoke<string[]>("npu_sync_models"),
  aiSuggestions: () => invoke<Suggestion[]>("ai_suggestions"),
  aiResolveSuggestion: (messageId: number, accept: boolean) => invoke<void>("ai_resolve_suggestion", { messageId, accept }),
  aiSuggestNow: () => invoke<number>("ai_suggest_now"),
  aiSuggestQueue: () => invoke<number>("ai_suggest_queue"),
  npuDirs: () => invoke<NpuDirs>("npu_dirs"),
  npuSetDirs: (runtime: string | null, models: string | null, moveFiles: boolean) => invoke<NpuDirs>("npu_set_dirs", { runtime, models, moveFiles }),
  npuOpenDir: (which: "runtime" | "models") => invoke<void>("npu_open_dir", { which }),
  npuInstallDriver: () => invoke<DriverOutcome>("npu_install_driver"),
  scheduleSend: (message: Outgoing, sendAt: number) => invoke<number>("schedule_send", { message, sendAt }),
  listOutbox: () => invoke<OutboxItem[]>("list_outbox"),
  cancelOutbox: (id: number) => invoke<Outgoing>("cancel_outbox", { id }),
  rescheduleOutbox: (id: number, sendAt: number) => invoke<void>("reschedule_outbox", { id, sendAt }),
  sendOutboxNow: (id: number) => invoke<void>("send_outbox_now", { id }),
  npuCatalog: () => invoke<NpuModel[]>("npu_catalog"),
  npuStatus: () => invoke<NpuStatus>("npu_status"),
  npuInstall: () => invoke<string>("npu_install"),
  npuPull: (tag: string) => invoke<void>("npu_pull", { tag }),
  npuCancelPull: (tag: string) => invoke<void>("npu_cancel_pull", { tag }),
  npuRemove: (tag: string) => invoke<void>("npu_remove", { tag }),
  npuStart: () => invoke<void>("npu_start"),
  npuStop: () => invoke<void>("npu_stop"),
  attachmentData: (id: number, index: number) => invoke<AttachmentData>("attachment_data", { id, index }),
  addIcs: (name: string, url: string, color: string) => invoke<number>("add_ics_calendar", { name, url, color }),
  importIcs: (path: string, calendarId: number) => invoke<number>("import_ics_file", { path, calendarId }),
  updateCalendar: (id: number, name: string, color: string) => invoke<void>("update_calendar", { id, name, color }),
  deleteCalendar: (id: number) => invoke<void>("delete_calendar", { id }),
  startHidden: () => invoke<boolean>("start_hidden"),
  getAutostart: () => invoke<boolean>("get_autostart"),
  setAutostart: (enabled: boolean) => invoke<void>("set_autostart", { enabled }),
  quit: () => invoke<void>("quit_app"),
  googleSignIn: (args: { accountId?: number | null; protocol?: string; loginHint?: string; pop3Dir?: string; popup?: boolean; recent?: boolean }) =>
    invoke<number>("google_sign_in", {
      accountId: args.accountId ?? null,
      protocol: args.protocol ?? null,
      loginHint: args.loginHint ?? null,
      pop3Dir: args.pop3Dir ?? null,
      popup: args.popup ?? true,
      recent: args.recent ?? null,
    }),
  testAccount: (accountId: number) => invoke<string>("test_account", { accountId }),
  deleteAccount: (accountId: number) => invoke<void>("delete_account", { accountId }),
  reorderAccounts: (ids: number[]) => invoke<void>("reorder_accounts", { ids }),
  relocatePop3: (accountId: number, newDir: string, moveFiles: boolean) =>
    invoke<number>("relocate_pop3", { accountId, newDir, moveFiles }),
  syncNow: (accountId: number | null = null) => invoke<void>("sync_now", { accountId }),
  dataBackup: (path: string, withSecrets: boolean) => invoke<number>("data_backup", { path, withSecrets }),
  dataStageRestore: (source: string) => invoke<string>("data_stage_restore", { source }),
  dataCancelRestore: () => invoke<void>("data_cancel_restore"),
  appRestart: () => invoke<void>("app_restart"),
  restoredNote: () => invoke<string | null>("restored_note"),
  configExport: (path: string, withSecrets: boolean, withChats: boolean) => invoke<void>("config_export", { path, withSecrets, withChats }),
  configImport: (path: string) =>
    invoke<{ accounts_added: number; accounts_updated: number; groups: number; rules: number; signatures: number; settings: number; secrets: number; notes: string[] }>("config_import", { path }),
  syncLog: () => invoke<SyncLogLine[]>("sync_log"),
  syncLogClear: () => invoke<void>("sync_log_clear"),
  syncLogOpen: () => invoke<void>("sync_log_open"),
  testNotification: () => invoke<void>("test_notification"),
  inviteInfo: (messageId: number) => invoke<InviteInfo | null>("invite_info", { messageId }),
  inviteRespond: (messageId: number, action: "add" | "accepted" | "tentative" | "declined" | "dismiss") => invoke<InviteResult>("invite_respond", { messageId, action }),
  reminderSkip: (key: string) => invoke<void>("reminder_skip", { key }),
  notificationPending: () => invoke<{ message_id: number; account_id: number } | null>("notification_pending"),
  npuRedownload: (tag: string) => invoke<void>("npu_redownload", { tag }),
  folders: () => invoke<Folder[]>("list_folders"),
  messages: (query: ListQuery) => invoke<Message[]>("list_messages", { query }),
  unreadCounts: (space = 0) =>
    invoke<{ inbox: number; flagged: number; groups: Record<string, { total: number; unread: number }>; accounts: Record<string, number> }>("unread_counts", { space }),
  message: (id: number) => invoke<MessageBody>("get_message", { id }),
  source: (id: number) => invoke<string>("message_source", { id }),
  setSeen: (ids: number[], seen: boolean) => invoke<void>("set_seen", { ids, seen }),
  setFlagged: (ids: number[], flagged: boolean) => invoke<void>("set_flagged", { ids, flagged }),
  move: (ids: number[], folderId: number) => invoke<void>("move_messages", { ids, folderId }),
  archive: (ids: number[]) => invoke<void>("archive_messages", { ids }),
  spam: (ids: number[]) => invoke<void>("spam_messages", { ids }),
  remove: (ids: number[], permanent = false) => invoke<void>("delete_messages", { ids, permanent }),
  createFolder: (accountId: number, name: string) => invoke<void>("create_folder", { accountId, name }),
  saveAttachment: (id: number, index: number, path: string) => invoke<void>("save_attachment", { id, index, path }),
  openAttachment: (id: number, index: number) => invoke<void>("open_attachment", { id, index }),
  contacts: (prefix: string) => invoke<{ addr: string; name: string }[]>("suggest_contacts", { prefix }),
  send: (message: Outgoing) => invoke<void>("send_mail", { message }),
  saveDraft: (message: Outgoing) => invoke<void>("save_draft", { message }),
  signatures: () => invoke<Signature[]>("list_signatures"),
  saveSignature: (signature: Signature) => invoke<number>("save_signature", { signature }),
  deleteSignature: (id: number) => invoke<void>("delete_signature", { id }),
  calendars: () => invoke<Calendar[]>("list_calendars"),
  setCalendarVisible: (id: number, visible: boolean) => invoke<void>("set_calendar_visible", { id, visible }),
  events: (from: number, to: number) => invoke<CalEvent[]>("list_events", { from, to }),
  saveEvent: (event: Partial<CalEvent>) => invoke<CalEvent>("save_event", { event }),
  deleteEvent: (id: number) => invoke<void>("delete_event", { id }),
  syncCalendars: () => invoke<void>("sync_calendars"),
  dataDir: () => invoke<string>("data_dir"),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
};
