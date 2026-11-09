// Demo backend used only when the UI runs outside Tauri (browser preview).
import type { Account, Folder, Message, CalEvent, Calendar, Signature, Space, Group, Rule, AiProvider } from "./api";

const now = Math.floor(Date.now() / 1000);
const extra = { smtp_username: "", sync_limit: 2000, body_limit: 200, role_map: "{}" };
const accounts: Account[] = [
  { id: 1, name: "Cá nhân", email: "minh.demo@gmail.com", display_name: "Minh", protocol: "imap", auth: "oauth_google", provider: "google", in_host: "imap.gmail.com", in_port: 993, in_security: "ssl", out_host: "smtp.gmail.com", out_port: 465, out_security: "ssl", username: "minh.demo@gmail.com", pop3_dir: "", pop3_leave_on_server: true, pop3_recent_mode: false, color: "#0f6cbd", sort_order: 0, sig_new: 1, sig_reply: 1, calendar_sync: true, enabled: true, last_sync: now - 60, last_error: "", ...extra, space_id: 2 },
  { id: 2, name: "Lưu trữ POP3", email: "minh.archive@example.com", display_name: "Minh", protocol: "pop3", auth: "password", provider: "other", in_host: "pop.example.com", in_port: 995, in_security: "ssl", out_host: "smtp.example.com", out_port: 465, out_security: "ssl", username: "minh.archive@example.com", pop3_dir: "D:\\Mail\\Archive", pop3_leave_on_server: true, pop3_recent_mode: false, color: "#c239b3", sort_order: 1, sig_new: null, sig_reply: null, calendar_sync: false, enabled: true, last_sync: now - 300, last_error: "", ...extra, space_id: 2 },
];
const more = [
  ["Monash", "minh.n@student.monash.edu", 1, "#038387"],
  ["Lab NLP", "minh@nlp-lab.example.org", 1, "#ca5010"],
  ["Freelance", "hello@minh.dev", 3, "#8764b8"],
  ["Marisland", "support@marisland.app", 3, "#498205"],
];
more.forEach(([name, email, space, color], i) => {
  const id = 3 + i;
  accounts.push({ ...accounts[0], id, name: name as string, email: email as string, color: color as string, space_id: space as number, auth: "password", sort_order: 2 + i });
});
const folders: Folder[] = [
  { id: 1, account_id: 1, path: "INBOX", name: "Hộp thư đến", delim: "/", role: "inbox", selectable: true, unread: 4, total: 0 },
  { id: 2, account_id: 1, path: "[Gmail]/Drafts", name: "Thư nháp", delim: "/", role: "drafts", selectable: true, unread: 0, total: 1 },
  { id: 3, account_id: 1, path: "[Gmail]/Sent Mail", name: "Đã gửi", delim: "/", role: "sent", selectable: true, unread: 0, total: 0 },
  { id: 4, account_id: 1, path: "[Gmail]/Spam", name: "Thư rác", delim: "/", role: "junk", selectable: true, unread: 0, total: 0 },
  { id: 5, account_id: 1, path: "[Gmail]/Trash", name: "Thùng rác", delim: "/", role: "trash", selectable: true, unread: 0, total: 0 },
  { id: 6, account_id: 1, path: "Research", name: "Research", delim: "/", role: "", selectable: true, unread: 1, total: 0 },
  { id: 7, account_id: 1, path: "Research/Physica A", name: "Physica A", delim: "/", role: "", selectable: true, unread: 0, total: 0 },
  { id: 11, account_id: 2, path: "Inbox", name: "Hộp thư đến", delim: "/", role: "inbox", selectable: true, unread: 1, total: 0 },
  { id: 12, account_id: 2, path: "Sent", name: "Đã gửi", delim: "/", role: "sent", selectable: true, unread: 0, total: 0 },
  { id: 13, account_id: 2, path: "Archive", name: "Lưu trữ", delim: "/", role: "archive", selectable: true, unread: 0, total: 0 },
  { id: 14, account_id: 2, path: "Trash", name: "Thùng rác", delim: "/", role: "trash", selectable: true, unread: 0, total: 0 },
];
for (let a = 3; a <= 6; a++) {
  folders.push({ id: a * 10 + 1, account_id: a, path: "INBOX", name: "Hộp thư đến", delim: "/", role: "inbox", selectable: true, unread: 0, total: 0 });
  folders.push({ id: a * 10 + 2, account_id: a, path: "Sent", name: "Đã gửi", delim: "/", role: "sent", selectable: true, unread: 0, total: 0 });
  folders.push({ id: a * 10 + 3, account_id: a, path: "Trash", name: "Thùng rác", delim: "/", role: "trash", selectable: true, unread: 0, total: 0 });
}
const spaces: Space[] = [
  { id: 1, name: "Học tập", color: "#038387", sort_order: 1 },
  { id: 2, name: "Cá nhân", color: "#0f6cbd", sort_order: 2 },
  { id: 3, name: "Công việc", color: "#8764b8", sort_order: 3 },
];
const groups: Group[] = [
  { id: 1, name: "GitHub & CI", color: "#2b2b2b", hide_from_inbox: true, sort_order: 1, account_id: null },
  { id: 2, name: "Newsletters & arXiv", color: "#ca5010", hide_from_inbox: true, sort_order: 2, account_id: null },
  { id: 3, name: "Invoices", color: "#498205", hide_from_inbox: true, sort_order: 3, account_id: null },
  { id: 4, name: "Moodle", color: "#0f6cbd", hide_from_inbox: true, sort_order: 4, account_id: 1 },
];
const rules: Rule[] = [
  { id: 1, name: "GitHub", enabled: true, match_all: true, conditions: [{ field: "from", op: "domain", value: "github.com" }], account_ids: [], group_id: 1, mark_read: false, flag: false, stop: false, sort_order: 1 },
  { id: 2, name: "arXiv daily", enabled: true, match_all: false, conditions: [{ field: "from", op: "contains", value: "arxiv.org" }, { field: "subject", op: "contains", value: "daily, newsletter" }], account_ids: [], group_id: 2, mark_read: true, flag: false, stop: false, sort_order: 2 },
  { id: 3, name: "Hoá đơn & biên lai", enabled: true, match_all: false, conditions: [{ field: "subject", op: "contains", value: "receipt, hoá đơn, invoice" }], account_ids: [], group_id: 3, mark_read: false, flag: false, stop: false, sort_order: 3 },
];
// ?hw=intel | none | olddrv lets the preview show the other hardware cases.
const hw = typeof location !== "undefined" ? new URLSearchParams(location.search).get("hw") ?? "" : "";
const mockNpu =
  hw === "none"
    ? { present: false, vendor: "", device: "", driver: "", driver_ok: false, driver_missing: false, driver_min: "", cpu: "13th Gen Intel(R) Core(TM) i7-1365U", arch: "none", engine: "", platform: "", supported: false, ram_gb: 16, note: "" }
    : hw === "intel"
      ? { present: true, vendor: "intel", device: "Intel(R) AI Boost", driver: "32.0.100.3104", driver_ok: true, driver_missing: false, driver_min: "32.0.100.3104", cpu: "Intel(R) Core(TM) Ultra 7 258V", arch: "intel", engine: "ovms", platform: "", supported: true, ram_gb: 32, note: "" }
      : { present: true, vendor: "amd", device: "NPU Compute Accelerator Device", driver: hw === "olddrv" ? "32.0.201.204" : "32.0.203.314", driver_ok: hw !== "olddrv", driver_missing: false, driver_min: "32.0.203.311", cpu: "AMD Ryzen AI 7 PRO 360 w/ Radeon 880M", arch: "xdna2", engine: "flm", platform: "aie2p", supported: true, ram_gb: 32, note: "" };
const ov = (repo: string, name: string, params: string, size: number, rec: boolean, think = false, installed = false) => ({
  tag: repo, name, family: "", params, size, footprint: Math.round((size / 1e9) * 1.3 * 10) / 10, vision: false, audio: false, thinking: think, tools: false,
  labels: [], context: 4096, min_version: "", url: "", compatible: true, chat: true, installed, recommended: rec, note: "", repo, page: `https://huggingface.co/${repo}`,
});
const mockIntelAvail = {
  ram_gb: 32, version: "v2026.4.1", npu: mockNpu, collection: "https://huggingface.co/collections/OpenVINO/llms-optimized-for-npu-686e7f0bf7bc184bd71f8ba0", newer: [],
  models: [
    ov("OpenVINO/Phi-3.5-mini-instruct-int4-cw-ov", "Phi 3.5 mini instruct", "3.8B", 2.3e9, true, false, true),
    ov("OpenVINO/Qwen3-8B-int4-cw-ov", "Qwen3 8B", "8B", 4.9e9, true, true),
    ov("OpenVINO/DeepSeek-R1-Distill-Qwen-1.5B-int4-gq-ov", "DeepSeek R1 Distill Qwen 1.5B", "1.5B", 1.1e9, true, true),
    ov("OpenVINO/Qwen3-4B-int4-cw-ov", "Qwen3 4B", "4B", 2.5e9, false, true),
    ov("OpenVINO/Mistral-7B-Instruct-v0.3-int4-cw-ov", "Mistral 7B Instruct v0.3", "7B", 4.1e9, false),
    ov("OpenVINO/Qwen2.5-1.5B-Instruct-int4-cw-ov", "Qwen2.5 1.5B Instruct", "1.5B", 1.0e9, false),
  ],
};
if (typeof window !== "undefined" && new URLSearchParams(location.search).get("popup")) {
  setTimeout(() => window.dispatchEvent(new CustomEvent("mock:ai-suggestions", { detail: { count: 2, popup: true } })), 1800);
}
const aiProviders: AiProvider[] = [
  { id: "npu", kind: "npu", name: "On-device (NPU)", base_url: "http://127.0.0.1:52725/v1", model: "qwen3.5:4b", has_key: true },
  { id: "gemini-builtin", kind: "gemini", name: "Gemini (built-in)", base_url: "", model: "", has_key: true },
  { id: "a1", kind: "claude_cli", name: "Claude Code", base_url: "", model: "sonnet", has_key: false },
  { id: "a2", kind: "gemini", name: "Gemini", base_url: "", model: "gemini-2.5-flash", has_key: true },
  { id: "a3", kind: "local", name: "llama.cpp (máy nhà)", base_url: "http://127.0.0.1:8080/v1", model: "qwen3-14b", has_key: false },
];
const memberships: Record<number, number[]> = {};
const people = [
  ["GitHub", "noreply@github.com", "[hypergraph-group-voter] CI passed on main", "All checks have passed for commit 4f2a1c9. 3 workflows completed successfully in 2m 14s."],
  ["Lan Phạm", "lan.pham@example.edu", "Họp nhóm nghiên cứu thứ Năm", "Chào Minh, nhóm mình họp lúc 14:00 thứ Năm ở phòng 3.12 nhé. Em chuẩn bị slide kết quả GLiNER mới nhất giúp chị."],
  ["Elsevier Editorial", "physa@elsevier.com", "Submission received: JRNL-D-26-00421", "Dear Dr. Nguyen, thank you for submitting your manuscript to Physica A. Your submission has been assigned the reference number below."],
  ["Monash IT", "it.notices@monash.edu", "Scheduled maintenance this weekend", "Some services including Moodle and Allocate+ will be unavailable between 10pm Saturday and 6am Sunday (AEDT)."],
  ["Google Calendar", "calendar-notification@google.com", "Lời mời: Seminar NLP @ Thứ Sáu 15:00", "Bạn được mời tham dự Seminar NLP. Tham gia bằng Google Meet. Xem chi tiết trong lịch."],
  ["Anh Tuấn", "tuan.nguyen@example.com", "Re: Bản nháp Marisland 0.34", "Mình đã chạy thử bản mới trên laptop 2 — chế độ notch mượt hơn hẳn, chỉ còn lỗi nhỏ khi đổi màn hình."],
  ["Overleaf", "welcome@overleaf.com", "Your project was shared with you", "Lan Phạm has shared the project “Thesis — Chapter 4” with you. Click to open it in Overleaf."],
  ["Stripe", "receipts@stripe.com", "Your receipt from Claude.ai #2381-4410", "Receipt for your subscription. Amount paid: $100.00. Date paid: Nov 3, 2026."],
  ["Hà My", "hamy@example.com", "Ảnh chuyến đi Đà Lạt 📸", "Gửi cậu mấy tấm ảnh hôm đi Đà Lạt nè, có cả tấm chụp ở đồi chè. Xem tệp đính kèm nhé!"],
  ["arXiv", "no-reply@arxiv.org", "cs.CL daily: 142 new submissions", "New submissions for Fri, 6 Nov 26. Named entity recognition for low-resource extremist discourse detection..."],
];
let messages: Message[] = [];
let id = 1;
for (let i = 0; i < 160; i++) {
  const p = people[i % people.length];
  const age = i < 3 ? i * 1800 + 900 : i < 6 ? 86400 + i * 3600 : i * 5 * 3600 + 86400 * 2;
  messages.push({
    id: id++,
    account_id: i % 7 === 3 ? 2 : 1,
    folder_id: i % 7 === 3 ? 11 : i % 11 === 5 ? 6 : 1,
    uid: i,
    message_id: `m${i}@demo`,
    in_reply_to: "",
    subject: p[2] + (i >= people.length ? ` (${Math.floor(i / people.length) + 1})` : ""),
    from_name: p[0],
    from_addr: p[1],
    to_list: "Minh <minh.demo@gmail.com>",
    cc_list: "",
    date: now - age,
    snippet: p[3],
    seen: i > 4 && i % 5 !== 0,
    flagged: i === 1 || i === 9,
    answered: i === 5,
    has_attachments: i % 10 === 8 || i === 2,
    size: 20000,
  });
}

messages.forEach((m) => {
  if (m.from_addr.includes("github")) memberships[m.id] = [1];
  if (m.from_addr.includes("arxiv")) memberships[m.id] = [2];
  if (m.from_addr.includes("stripe")) memberships[m.id] = [3];
  if (m.id % 9 === 0) { m.account_id = 3 + (m.id % 4); m.folder_id = m.account_id * 10 + 1; }
});
const inGroup = (id: number, g: number) => (memberships[id] ?? []).includes(g);
const hiddenIds = (id: number) => (memberships[id] ?? []).some((g) => groups.find((x) => x.id === g)?.hide_from_inbox);

const calendars: Calendar[] = [
  { id: 1, account_id: null, remote_id: "local", name: "Lịch của tôi", color: "#0f6cbd", visible: true, writable: true, kind: "local", url: "", last_sync: 0, last_error: "" },
  { id: 2, account_id: 1, remote_id: "primary", name: "minh.demo@gmail.com", color: "#039be5", visible: true, writable: true, kind: "google", url: "", last_sync: 0, last_error: "" },
  { id: 3, account_id: 1, remote_id: "holidays", name: "Ngày lễ ở Việt Nam", color: "#0b8043", visible: true, writable: false, kind: "google", url: "", last_sync: 0, last_error: "" },
  { id: 4, account_id: null, remote_id: "ics:x", name: "Outlook – Lab", color: "#0078d4", visible: true, writable: false, kind: "ics", url: "https://outlook.office365.com/owa/calendar/abc/calendar.ics", last_sync: now - 600, last_error: "" },
];
const day = (offset: number, h: number, m = 0) => {
  const d = new Date();
  d.setHours(h, m, 0, 0);
  d.setDate(d.getDate() + offset);
  return Math.floor(d.getTime() / 1000);
};
let events: CalEvent[] = [
  { id: 1, calendar_id: 2, remote_id: "a", title: "Họp nhóm nghiên cứu", location: "Phòng 3.12", description: "", start_ts: day(0, 14), end_ts: day(0, 15, 30), all_day: false, recurring: false, meet_link: "", color: "#039be5" },
  { id: 2, calendar_id: 2, remote_id: "b", title: "Seminar NLP", location: "Google Meet", description: "", start_ts: day(1, 15), end_ts: day(1, 16), all_day: false, recurring: true, meet_link: "https://meet.google.com/abc-defg-hij", color: "#039be5" },
  { id: 3, calendar_id: 1, remote_id: "", title: "Gym", location: "", description: "", start_ts: day(-1, 7), end_ts: day(-1, 8), all_day: false, recurring: false, meet_link: "", color: "#0f6cbd" },
  { id: 4, calendar_id: 3, remote_id: "c", title: "Ngày Nhà giáo Việt Nam", location: "", description: "", start_ts: day(13, 0), end_ts: day(14, 0), all_day: true, recurring: false, meet_link: "", color: "#0b8043" },
  { id: 5, calendar_id: 2, remote_id: "d", title: "Deadline Physica A revision", location: "", description: "", start_ts: day(3, 9), end_ts: day(3, 10), all_day: false, recurring: false, meet_link: "", color: "#039be5" },
  { id: 6, calendar_id: 2, remote_id: "e", title: "Gọi với AFP", location: "Teams", description: "", start_ts: day(0, 14, 30), end_ts: day(0, 15), all_day: false, recurring: false, meet_link: "", color: "#039be5" },
];
const signatures: Signature[] = [{ id: 1, name: "Mặc định", html: "<div>Trân trọng,<br><b>Minh</b><br><span style=\"color:#666\">Master of Cybersecurity · Monash University</span></div>" }];
const settings: Record<string, unknown> = {};

function list(q: any): Message[] {
  let r = messages;
  if (q.space) r = r.filter((m) => accounts.find((a) => a.id === m.account_id)?.space_id === q.space);
  if (q.view === "unified:inbox") r = r.filter((m) => folders.find((f) => f.id === m.folder_id)?.role === "inbox" && !hiddenIds(m.id));
  else if (q.view.startsWith("group:")) r = r.filter((m) => inGroup(m.id, Number(q.view.slice(6))));
  else if (q.view.startsWith("ids:")) { const ids = q.view.slice(4).split(",").map(Number); r = r.filter((m) => ids.includes(m.id)); }
  else if (q.view === "unified:flagged") r = r.filter((m) => m.flagged);
  else if (q.view === "unified:unread") r = r.filter((m) => !m.seen);
  else if (q.view.startsWith("folder:")) r = r.filter((m) => m.folder_id === Number(q.view.slice(7)));
  else if (q.view.startsWith("account:")) r = r.filter((m) => m.account_id === Number(q.view.slice(8)));
  if (q.filter === "unread") r = r.filter((m) => !m.seen);
  if (q.filter === "flagged") r = r.filter((m) => m.flagged);
  if (q.filter === "attachments") r = r.filter((m) => m.has_attachments);
  const s = (q.search || "").trim();
  if (s) {
    // Rough imitation of the backend: from:/to:/cc: operators, People mode, hits with markers.
    const fold = (x: string) => x.normalize("NFD").replace(/\p{M}/gu, "").replace(/đ/gi, "d").toLowerCase();
    const ops: [string, string][] = [];
    const free: string[] = [];
    for (const m of s.matchAll(/(?:(from|to|cc):)?(?:"([^"]+)"|(\S+))/gi)) {
      const v = (m[2] ?? m[3]).toLowerCase();
      if (m[1]) ops.push([m[1].toLowerCase(), v]);
      else free.push(v);
    }
    const mark = (text: string, terms: string[]) => {
      const f = fold(text);
      let out = "", pos = 0, hit = false;
      const re = new RegExp(terms.map((t) => fold(t).replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|"), "g");
      for (const m of f.matchAll(re)) { out += text.slice(pos, m.index) + "\uE000" + text.slice(m.index!, m.index! + m[0].length) + "\uE001"; pos = m.index! + m[0].length; hit = true; }
      return hit ? out + text.slice(pos) : "";
    };
    const people = (m: Message) => ({ from: `${m.from_name} <${m.from_addr}>`, to: m.to_list, cc: (m as any).cc_list ?? "" });
    r = r
      .map((m): Message | null => {
        const p = people(m);
        for (const [k, v] of ops) if (!fold((p as any)[k]).includes(fold(v))) return null;
        let hit = "", hit_field: Message["hit_field"] = undefined;
        if (free.length) {
          if (q.search_mode === "people") {
            for (const k of ["from", "to", "cc"] as const) { const h = mark(p[k], free); if (h) { hit = h; hit_field = k; break; } }
            if (!hit) return null;
          } else {
            const hay = q.search_mode === "subject" ? m.subject : m.subject + " " + m.snippet + " " + m.from_name;
            if (!free.some((t) => fold(hay).includes(fold(t)))) return null;
            if (q.search_mode !== "subject") { const h = mark(m.snippet, free); if (h) { hit = "…" + h; hit_field = "body"; } }
          }
        }
        for (const [k, v] of ops) if (!hit) { const h = mark((p as any)[k], [v]); if (h) { hit = h; hit_field = k as any; } }
        return { ...m, hit: hit || undefined, hit_field };
      })
      .filter((m) => !!m) as Message[];
  }
  return r.slice(q.offset, q.offset + q.limit);
}

const msleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const mockLog: { ts: number; account_id: number; account: string; level: string; message: string }[] = [];
function mockSync(id: number, email: string) {
  const steps: [string, string][] = [
    ["info", "Sync started (IMAP · imap.gmail.com:993)"],
    ["info", "Connecting to imap.gmail.com:993 (ssl)…"],
    ["ok", "Signed in"],
    ["info", "11 folders to sync"],
    ["info", "Inbox: 2,318 messages on the server (UIDVALIDITY 1, last seen UID 48211)"],
    ["warn", "Inbox: skipped message UID 48215: unexpected end of FETCH response"],
    ["info", "Inbox: 3 new message(s), 1 skipped"],
    ["info", "Inbox: downloaded 3 message bodies for search"],
    ["ok", "Sync finished in 4.2s · 2 new unread"],
  ];
  steps.forEach(([level, message], i) =>
    setTimeout(() => {
      const l = { ts: Date.now(), account_id: id, account: email, level, message };
      mockLog.push(l);
      window.dispatchEvent(new CustomEvent("mock:sync-log", { detail: l }));
    }, 250 * i),
  );
}
const mockInvites: Record<number, string> = {};
export async function mockInvoke(cmd: string, a: any): Promise<unknown> {
  await new Promise((r) => setTimeout(r, 30));
  switch (cmd) {
    case "get_settings": return settings;
    case "set_setting": settings[a.key] = a.value; return null;
    case "google_config": return { client_id: "1234.apps.googleusercontent.com", has_secret: true };
    case "list_accounts": return accounts;
    case "list_folders": return folders.map((f) => ({ ...f, unread: messages.filter((m) => m.folder_id === f.id && !m.seen).length }));
    case "unread_counts": {
      const inboxUnread = messages.filter((m) => !m.seen && folders.find((f) => f.id === m.folder_id)?.role === "inbox" && !hiddenIds(m.id));
      const g: Record<string, { total: number; unread: number }> = {};
      for (const gr of groups) {
        const ms = messages.filter((m) => inGroup(m.id, gr.id));
        g[gr.id] = { total: ms.length, unread: ms.filter((m) => !m.seen).length };
      }
      const acc: Record<string, number> = {};
      for (const m of inboxUnread) acc[m.account_id] = (acc[m.account_id] ?? 0) + 1;
      const sp = a.space ? inboxUnread.filter((m) => accounts.find((x) => x.id === m.account_id)?.space_id === a.space) : inboxUnread;
      return { inbox: sp.length, flagged: messages.filter((m) => m.flagged).length, groups: g, accounts: acc };
    }
    case "list_spaces": return spaces;
    case "list_groups": return groups;
    case "list_rules": return rules;
    case "save_rule": { if (a.rule.id) Object.assign(rules.find((r) => r.id === a.rule.id)!, a.rule); else rules.push({ ...a.rule, id: rules.length + 1 }); return a.rule.id || rules.length; }
    case "run_rules": return 37;
    case "ai_providers": return aiProviders;
    case "ai_models": return ["gemini-2.5-flash", "gemini-2.5-pro"];
    case "list_groups_dummy": return null;
    case "ai_run": {
      const t = a.request.task;
      const sid = a.request.stream_id;
      const emit = (kind: string, extra: Record<string, unknown>) => window.dispatchEvent(new CustomEvent("mock:ai-delta", { detail: { id: sid, kind, ...extra } }));
      const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
      if (t === "cluster") return { text: "", used: [1, 2], thinking: "", sources: [], cancelled: false, clusters: [
        { title: "Research group meetings", summary: "Meeting times and GLiNER slides.", ids: [2, 12, 22], rule: { match_all: false, conditions: [{ field: "subject", op: "contains", value: "họp nhóm, meeting" }] } },
        { title: "Physica A submission", summary: "Manuscript confirmation and revision requests.", ids: [3, 13], rule: { match_all: false, conditions: [{ field: "from", op: "domain", value: "elsevier.com" }] } },
      ] };
      const thinking = a.request.thinking ? "The user wants the key points. Let me read the three emails: the meeting moved to Thursday 14:00, Elsevier confirmed the submission, Monash IT has maintenance. I should list actions first." : "";
      const text = t === "reply"
        ? "Hi Lan,\n\nThanks — I'll be there at 14:00 on Thursday in room 3.12 and will bring the latest GLiNER results.\n\nBest,\nMinh"
        : "## Summary\n- **Research meeting** moved to Thursday 14:00, room 3.12 — prepare GLiNER slides [E1]\n- **Physica A** received manuscript JRNL-D-26-00421 [E2]\n- Monash IT maintenance this weekend [E3]" + (a.request.web ? "\n\nAMD's Ryzen AI 300 NPUs deliver up to 50 TOPS [1], and FastFlowLM runs LLMs on them [2]." : "");
      if (sid) {
        emit("refs", { ids: [2, 3, 4] });
        if (a.request.web) {
          emit("status", { text: "web:" + (a.request.messages.at(-1)?.content ?? "") });
          await sleep(500);
          emit("source", { title: "AMD Ryzen AI 300 Series", url: "https://www.amd.com/en/products/processors/laptop/ryzen.html" });
          emit("source", { title: "FastFlowLM on GitHub", url: "https://github.com/FastFlowLM/FastFlowLM" });
        }
        for (let i = 0; i < thinking.length; i += 12) { emit("thinking", { text: thinking.slice(i, i + 12) }); await sleep(30); }
        // Bursty, like a real model behind a network: small and large chunks, uneven gaps.
        for (let i = 0; i < text.length; ) { const n = 2 + Math.floor(Math.random() * (Math.random() < 0.2 ? 60 : 10)); emit("text", { text: text.slice(i, i + n) }); i += n; await sleep(Math.random() < 0.15 ? 220 : 25); }
      }
      return { text, used: [2, 3, 4], clusters: [], thinking, sources: a.request.web ? [{ title: "AMD Ryzen AI 300 Series", url: "https://www.amd.com/en/products/processors/laptop/ryzen.html" }, { title: "FastFlowLM on GitHub", url: "https://github.com/FastFlowLM/FastFlowLM" }] : [], cancelled: false };
    }
    case "npu_info": return mockNpu;
    case "ai_suggest_queue": return 3;
    case "npu_open_dir": return null;
    case "npu_redownload": await msleep(800); return null;
    case "sync_log": return mockLog;
    case "sync_log_clear": mockLog.length = 0; return null;
    case "sync_log_open": return null;
    case "test_notification": return null;
    case "notification_pending": return null;
    case "invite_info": {
      const m = messages.find((x) => x.id === a.messageId);
      if (!m || !m.subject.startsWith("Lời mời")) return null;
      return { method: "REQUEST", title: "Seminar NLP", location: "Google Meet", start_ts: now + 86400, end_ts: now + 90000, all_day: false, recurring: false, organizer: m.from_addr, organizer_name: m.from_name, state: mockInvites[a.messageId] ?? "", in_calendar: false, can_reply: true, calendar: "minh.demo@gmail.com" };
    }
    case "invite_respond": mockInvites[a.messageId] = a.action === "add" ? "added" : a.action; await new Promise((r) => setTimeout(r, 700)); return { event_id: 1, added: a.action !== "declined" && a.action !== "dismiss", already: false, replied: a.action !== "add" && a.action !== "dismiss", title: "Event", start_ts: Math.floor(Date.now() / 1000) + 86400, all_day: false, calendar: "Calendar" };
    case "reminder_skip": return null;
    case "set_unread_badge": return null;
    case "npu_dirs": return { found: ["qwen3.5:4b", "gemma3:4b"], foreign: ["GGUF (llama.cpp / LM Studio / Ollama): Qwen3-8B-Q4_K_M.gguf, gemma-3-12b-it-Q4_0.gguf"], runtime: "C:\\Users\\Minh\\AppData\\Roaming\\app.nelo.mail\\runtime", runtime_default: "C:\\Users\\Minh\\AppData\\Roaming\\app.nelo.mail\\runtime", runtime_size: 41e6, models: "D:\\AI\\models", models_default: "C:\\Users\\Minh\\.flm\\models", models_size: 3.6e9 };
    case "npu_set_dirs": {
      for (let i = 1; i <= 10; i++) { window.dispatchEvent(new CustomEvent("mock:npu-progress", { detail: { kind: "move", tag: "models", percent: i * 10, done: i, total: 10, line: "", finished: false, error: "" } })); await msleep(120); }
      return { found: [], foreign: [], runtime: "C:\\Users\\Minh\\AppData\\Roaming\\app.nelo.mail\\runtime", runtime_default: "C:\\Users\\Minh\\AppData\\Roaming\\app.nelo.mail\\runtime", runtime_size: 41e6, models: String(a.models ?? "D:\\AI\\models"), models_default: "C:\\Users\\Minh\\.flm\\models", models_size: 3.6e9 };
    }
    case "npu_install_driver": {
      const ev = (tag: string, percent: number | null, done = 0, total = 0) => window.dispatchEvent(new CustomEvent("mock:npu-progress", { detail: { kind: "driver", tag, percent, done, total, line: "", finished: false, error: "" } }));
      ev("search", null); await msleep(700);
      for (let i = 1; i <= 8; i++) { ev("download", i * 12.5, i * 9e6, 72e6); await msleep(150); }
      ev("install", null); await msleep(900);
      return { method: "windows-update", ok: true, message: "AMD NPU driver", reboot: true, url: "" };
    }
    case "npu_sync_models": return ["qwen3.5:4b"];
    case "npu_available": if (hw === "intel") return mockIntelAvail; if (hw === "none") return { ram_gb: 16, version: "", npu: mockNpu, collection: "", newer: [], models: [] }; return { ram_gb: 32, version: "v1.0.12", npu: mockNpu, collection: "https://huggingface.co/collections/FastFlowLM/flm-models-xdna2", newer: ["Qwen3.9-4B-NPU2"], models: [
      { tag: "gemma4-it:e4b", name: "Gemma 4 E4B", family: "gemma4", params: "E4B", size: 5.1e9, footprint: 4.8, vision: true, audio: true, thinking: true, tools: true, labels: ["audio", "vision", "reasoning"], context: 32768, min_version: "1.0.0", url: "", compatible: true, chat: true, installed: false, recommended: false, note: "", repo: "Gemma4-E4B-IT-NPU2", page: "https://huggingface.co/FastFlowLM/Gemma4-E4B-IT-NPU2" },
      { tag: "qwen3.5:4b", name: "Qwen3.5 4B", family: "qwen3_5", params: "4B", size: 3.6e9, footprint: 3.2, vision: true, audio: false, thinking: true, tools: true, labels: ["vision", "reasoning"], context: 32768, min_version: "1.0.3", url: "https://huggingface.co/FastFlowLM/Qwen3.5-4B-NPU2", compatible: true, chat: true, installed: true, recommended: true, note: "", repo: "X-NPU2", page: "https://huggingface.co/FastFlowLM/X-NPU2" },
      { tag: "lfm3:1.7b", name: "LFM3 1.7B", family: "lfm3", params: "1.7B", size: 1.4e9, footprint: 1.5, vision: false, audio: false, thinking: true, tools: true, labels: ["reasoning", "tool-calling"], context: 32768, min_version: "1.0.11", url: "https://huggingface.co/FastFlowLM/LFM3-1.7B-NPU2", compatible: true, chat: true, installed: false, recommended: false, note: "", repo: "X-NPU2", page: "https://huggingface.co/FastFlowLM/X-NPU2" },
      { tag: "minicpm-v-4.7:1b", name: "MiniCPM V 4.7 1B", family: "minicpm", params: "1B", size: 1.1e9, footprint: 2.1, vision: true, audio: false, thinking: true, tools: true, labels: ["vision", "reasoning"], context: 32768, min_version: "1.0.8", url: "https://huggingface.co/FastFlowLM/MiniCPM-V-4.7-1B-NPU2", compatible: true, chat: true, installed: false, recommended: false, note: "", repo: "X-NPU2", page: "https://huggingface.co/FastFlowLM/X-NPU2" },
      { tag: "qwen3vl-it:4b", name: "Qwen3 VL 4B Instruct", family: "qwen3vl", params: "4B", size: 4.1e9, footprint: 3.9, vision: true, audio: false, thinking: false, tools: true, labels: ["vision", "tool-calling"], context: 32768, min_version: "0.9.22", url: "", compatible: true, chat: true, installed: false, recommended: false, note: "", repo: "X-NPU2", page: "https://huggingface.co/FastFlowLM/X-NPU2" },
      { tag: "qwen3.5:9b", name: "Qwen3.5 9B", family: "qwen3_5", params: "9B", size: 7.9e9, footprint: 7.6, vision: true, audio: false, thinking: true, tools: true, labels: ["vision"], context: 32768, min_version: "1.0.3", url: "", compatible: true, chat: true, installed: false, recommended: false, note: "", repo: "X-NPU2", page: "https://huggingface.co/FastFlowLM/X-NPU2" },
    ] };
    case "ai_read_bytes":
      if (a.name.toLowerCase().endsWith(".pdf")) return { name: a.name, mime: "application/pdf", text: null, image: null, pdf: a.data, size: Math.round(a.data.length * 0.75) };
      if (/\.(png|jpe?g)$/i.test(a.name)) return { name: a.name, mime: "image/png", text: null, image: a.data, size: Math.round(a.data.length * 0.75) };
      return { name: a.name, mime: "text/plain", text: atob(a.data), image: null, size: a.data.length };
    case "web_config": return { engine: "duckduckgo", searxng: "", has_brave_key: false };
    case "web_test": return [{ title: "Ryzen AI", url: "https://amd.com", snippet: "", content: "" }];
    case "list_messages": return list(a.query);
    case "get_message": {
      const m = messages.find((x) => x.id === a.id)!;
      return {
        id: a.id,
        html: `<div style="font-family:Segoe UI,Arial;font-size:14px;line-height:1.6"><p>${m.snippet}</p><p>Đây là nội dung xem trước trong chế độ demo của <b>Hermio</b>. Thư thật sẽ được hiển thị đầy đủ định dạng HTML, ảnh nội tuyến và liên kết.</p><ul><li>Hình ảnh bên ngoài bị chặn mặc định</li><li>Liên kết mở trong trình duyệt</li><li>Tệp đính kèm mở hoặc lưu bằng một cú nhấp</li></ul><p>Thân mến,<br>${m.from_name}</p><img src="https://example.com/tracker.png" width="1" height="1"></div>`,
        text: m.snippet,
        has_remote_images: true,
        attachments: m.has_attachments ? [{ index: 0, name: "ket-qua-thi-nghiem.pdf", mime: "application/pdf", size: 482113, inline: false }, { index: 1, name: "dalat.jpg", mime: "image/jpeg", size: 2213000, inline: false }] : [],
        to: m.to_list, cc: "", reply_to: "", references: "",
        ics: m.subject.startsWith("Lời mời") ? `BEGIN:VCALENDAR\nBEGIN:VEVENT\nSUMMARY:Seminar NLP\nDTSTART:${new Date((now + 86400) * 1000).toISOString().replace(/[-:]/g, "").slice(0, 15)}Z\nDTEND:${new Date((now + 90000) * 1000).toISOString().replace(/[-:]/g, "").slice(0, 15)}Z\nLOCATION:Google Meet\nEND:VEVENT\nEND:VCALENDAR` : null,
      };
    }
    case "set_seen": messages.forEach((m) => a.ids.includes(m.id) && (m.seen = a.seen)); return null;
    case "set_flagged": messages.forEach((m) => a.ids.includes(m.id) && (m.flagged = a.flagged)); return null;
    case "delete_messages": case "archive_messages": case "spam_messages": case "move_messages":
      messages = messages.filter((m) => !a.ids.includes(m.id)); return null;
    case "list_signatures": return signatures;
    case "save_signature": { if (a.signature.id) Object.assign(signatures.find((s) => s.id === a.signature.id)!, a.signature); else signatures.push({ ...a.signature, id: signatures.length + 1 }); return a.signature.id || signatures.length; }
    case "delete_signature": return null;
    case "suggest_contacts": return people.filter((p) => (p[0] + p[1]).toLowerCase().includes(a.prefix.toLowerCase())).slice(0, 6).map((p) => ({ name: p[0], addr: p[1] }));
    case "list_calendars": return calendars;
    case "set_calendar_visible": calendars.find((c) => c.id === a.id)!.visible = a.visible; return null;
    case "list_events": return events.filter((e) => calendars.find((c) => c.id === e.calendar_id)?.visible && e.end_ts > a.from && e.start_ts < a.to);
    case "save_event": { const ev = { ...a.event, color: calendars.find((c) => c.id === a.event.calendar_id)?.color }; if (ev.id) events = events.map((e) => (e.id === ev.id ? ev : e)); else events.push({ ...ev, id: events.length + 10 }); return ev; }
    case "delete_event": events = events.filter((e) => e.id !== a.id); return null;
    case "discover_account": return { provider: a.email.endsWith("monash.edu") ? "google" : "other", imap: { host: "imap.gmail.com", port: 993, security: "ssl", username: a.email }, pop3: { host: "pop.gmail.com", port: 995, security: "ssl", username: a.email }, smtp: { host: "smtp.gmail.com", port: 465, security: "ssl", username: a.email }, source: "Bản ghi MX (Google Workspace)", note: "Tên miền này dùng Google Workspace." };
    case "get_autostart": return true;
    case "start_hidden": return false;
    case "data_dir": return "C:\\Users\\Minh\\AppData\\Roaming\\app.nelo.mail";
    case "default_pop3_dir": return `C:\\Users\\Minh\\AppData\\Roaming\\app.nelo.mail\\pop3\\${a.accountId}`;
    case "guess_settings": return { provider: "other", imap: ["imap.example.com", 993, "ssl"], pop3: ["pop.example.com", 995, "ssl"], smtp: ["smtp.example.com", 465, "ssl"], note: "Đã đoán cấu hình." };
    case "npu_catalog": return [
      { tag: "qwen3.5:4b", name: "Qwen3.5 4B", params: "4B", note: "Best quality for mail: summaries, replies, sorting. 256k context, reads images.", vision: true, thinking: true, recommended: true },
      { tag: "gemma4-it:e4b", name: "Gemma 4 E4B", params: "≈4B effective", note: "Strong writing and multilingual; reads images and audio.", vision: true, thinking: true, recommended: false },
      { tag: "qwen3-it:4b", name: "Qwen3 4B Instruct 2507", params: "4B", note: "Fast, no thinking step; good at following instructions.", vision: false, thinking: false, recommended: false },
      { tag: "phi4-mini-it:4b", name: "Phi-4-mini", params: "3.8B", note: "Good reasoning for its size; English-first.", vision: false, thinking: false, recommended: false },
      { tag: "gemma4-it:e2b", name: "Gemma 4 E2B", params: "≈2B effective", note: "Light and quick; reads images and audio.", vision: true, thinking: true, recommended: false },
      { tag: "qwen3.5:2b", name: "Qwen3.5 2B", params: "2B", note: "Lightest capable model; reads images.", vision: true, thinking: true, recommended: false },
      { tag: "llama3.2:3b", name: "Llama 3.2 3B", params: "3B", note: "Classic small model, English-first.", vision: false, thinking: false, recommended: false },
    ];
    case "npu_status": if (hw === "intel") return { installed: true, path: "C:\\Users\\Minh\\AppData\\Roaming\\app.nelo.mail\\runtime\\ovms\\ovms.exe", version: "v2026.4.1", running: false, port: 52725, npu: "", models: ["OpenVINO/Phi-3.5-mini-instruct-int4-cw-ov"], pulling: [] };
      return { installed: true, path: "C:\\Users\\Minh\\AppData\\Roaming\\app.nelo.mail\\runtime\\flm\\flm.exe", version: "v1.0.12", running: false, port: 52725, npu: "", models: ["qwen3.5:4b"], pulling: [] };
    case "ai_skills": return [];
    case "ai_chats": return [{ id: 1, title: "Who is waiting for my reply this week?", entries: null, updated: now - 3600 }];
    case "ai_suggestions": return [{ message_id: 1, group_id: 1, new_group: "", reason: "CI notification from GitHub" }, { message_id: 5, group_id: null, new_group: "Conferences", reason: "Call for papers" }];
    case "list_outbox": return [{ id: 1, account_id: 1, subject: "Weekly report", recipients: "lan@monash.edu", send_at: now + 86400, status: "scheduled", error: "" }];
    case "attachment_data": return { name: "note.txt", mime: "text/plain", size: 120, data: btoa("Demo attachment preview."), text: null };
    case "sync_now": {
      const accs = (a.accountId ? accounts.filter((x) => x.id === a.accountId) : accounts.slice(0, 2));
      accs.forEach((x, i) => setTimeout(() => mockSync(x.id, x.email), i * 600));
      return null;
    }
    case "send_mail": case "save_draft": case "sync_calendars": case "open_url": return null;
    default: return null;
  }
}
