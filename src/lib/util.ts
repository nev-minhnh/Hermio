import { t, locale } from "./i18n.svelte";
const dayMs = 86400000;

export function startOfDay(d: Date) {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate());
}

export function listTime(ts: number): string {
  if (!ts) return "";
  const d = new Date(ts * 1000);
  const now = new Date();
  const today = startOfDay(now).getTime();
  if (d.getTime() >= today) return d.toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit" });
  if (d.getTime() >= today - 6 * dayMs)
    return d.toLocaleDateString(locale(), { weekday: "short" }) + " " + d.toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit" });
  if (d.getFullYear() === now.getFullYear()) return d.toLocaleDateString(locale(), { day: "2-digit", month: "2-digit" });
  return d.toLocaleDateString(locale(), { day: "2-digit", month: "2-digit", year: "numeric" });
}

export function fullTime(ts: number): string {
  if (!ts) return "";
  return new Date(ts * 1000).toLocaleString(locale(), {
    weekday: "long", day: "2-digit", month: "2-digit", year: "numeric", hour: "2-digit", minute: "2-digit",
  });
}

/** Month name in the UI language, e.g. "March". */
export function monthName(d: Date): string {
  const s = d.toLocaleDateString(locale(), { month: "long" });
  return s.charAt(0).toUpperCase() + s.slice(1);
}

/** Group label for the message list ("Today", "Yesterday", ...). */
export function groupLabel(ts: number): string {
  const now = new Date();
  const today = startOfDay(now).getTime();
  const ms = ts * 1000;
  if (ms >= today) return t("Today");
  if (ms >= today - dayMs) return t("Yesterday");
  const dow = (now.getDay() + 6) % 7; // Monday = 0
  if (ms >= today - dow * dayMs) return t("This week");
  if (ms >= today - (dow + 7) * dayMs) return t("Last week");
  const d = new Date(ms);
  if (d.getFullYear() === now.getFullYear() && d.getMonth() === now.getMonth()) return t("This month");
  if (d.getFullYear() === now.getFullYear()) return monthName(d);
  return t("Year {y}", { y: d.getFullYear() });
}

export function initials(name: string, addr = ""): string {
  const src = (name || addr.split("@")[0] || "?").replace(/["'()]/g, "").trim();
  const parts = src.split(/[\s._-]+/).filter(Boolean);
  if (parts.length >= 2) return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
  return src.slice(0, 2).toUpperCase();
}

const palette = ["#1d4fe0", "#c239b3", "#038387", "#ca5010", "#8764b8", "#498205", "#d13438", "#986f0b", "#005b70", "#5c2e91", "#e3008c", "#107c10"];
export function avatarColor(key: string): string {
  let h = 0;
  for (let i = 0; i < key.length; i++) h = (h * 31 + key.charCodeAt(i)) >>> 0;
  return palette[h % palette.length];
}

export function fileSize(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

export function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

/** Split "A <a@x>, b@y" into individual address strings. */
export function splitAddresses(s: string): string[] {
  const out: string[] = [];
  let cur = "";
  let inQuote = false;
  let inAngle = false;
  for (const ch of s) {
    if (ch === '"') inQuote = !inQuote;
    if (ch === "<") inAngle = true;
    if (ch === ">") inAngle = false;
    if ((ch === "," || ch === ";") && !inQuote && !inAngle) {
      if (cur.trim()) out.push(cur.trim());
      cur = "";
    } else cur += ch;
  }
  if (cur.trim()) out.push(cur.trim());
  return out;
}

export function addrOnly(s: string): string {
  const m = s.match(/<([^>]+)>/);
  return (m ? m[1] : s).trim().toLowerCase();
}

export function displayOf(s: string): string {
  const m = s.match(/^\s*"?([^"<]*?)"?\s*<([^>]+)>/);
  if (m && m[1].trim()) return m[1].trim();
  return addrOnly(s);
}

export function isEmail(s: string): boolean {
  return /^[^\s@<>]+@[^\s@<>]+\.[^\s@<>]+$/.test(addrOnly(s));
}

export function debounce<T extends (...a: any[]) => void>(fn: T, ms: number) {
  let t: ReturnType<typeof setTimeout> | undefined;
  return (...a: Parameters<T>) => {
    clearTimeout(t);
    t = setTimeout(() => fn(...a), ms);
  };
}

/** Minimal iCalendar VEVENT reader (for meeting invitations). */
export function parseIcs(text: string): { title: string; location: string; description: string; start: number; end: number; allDay: boolean } | null {
  const unfolded = text.replace(/\r?\n[ \t]/g, "");
  const ev = unfolded.split(/BEGIN:VEVENT/)[1];
  if (!ev) return null;
  const get = (k: string) => {
    const m = ev.match(new RegExp(`^${k}(;[^:]*)?:(.*)$`, "m"));
    return m ? { params: m[1] ?? "", value: m[2].trim() } : null;
  };
  const unescape = (s: string) => s.replace(/\\n/gi, "\n").replace(/\\,/g, ",").replace(/\\;/g, ";").replace(/\\\\/g, "\\");
  const toTs = (p: { params: string; value: string } | null): [number, boolean] => {
    if (!p) return [0, false];
    const v = p.value;
    if (/^\d{8}$/.test(v)) {
      return [new Date(+v.slice(0, 4), +v.slice(4, 6) - 1, +v.slice(6, 8)).getTime() / 1000, true];
    }
    const m = v.match(/^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})(Z?)$/);
    if (!m) return [0, false];
    const [, y, mo, d, h, mi, s, z] = m;
    const date = z ? new Date(Date.UTC(+y, +mo - 1, +d, +h, +mi, +s)) : new Date(+y, +mo - 1, +d, +h, +mi, +s);
    return [Math.floor(date.getTime() / 1000), false];
  };
  const [start, allDay] = toTs(get("DTSTART"));
  let [end] = toTs(get("DTEND"));
  if (!start) return null;
  if (!end) end = start + (allDay ? 86400 : 3600);
  return {
    title: unescape(get("SUMMARY")?.value ?? t("Event")),
    location: unescape(get("LOCATION")?.value ?? ""),
    description: unescape(get("DESCRIPTION")?.value ?? ""),
    start, end, allDay,
  };
}

export function folderIcon(role: string): string {
  switch (role) {
    case "inbox": return "inbox";
    case "sent": return "send";
    case "drafts": return "draft";
    case "trash": return "trash";
    case "junk": return "junk";
    case "archive": return "archive";
    default: return "folder";
  }
}

// ---------------- search highlighting ----------------

/** Search terms like the backend: words are OR-ed; "quoted phrases" stay whole; the
 *  from: / to: / cc: operators only contribute their value. */
export function searchTerms(q: string): string[] {
  const out: string[] = [];
  const re = /(?:(?:from|to|cc|từ|tu|đến|den):)?(?:"([^"]+)"|(\S+))/gi;
  let m: RegExpExecArray | null;
  while ((m = re.exec(q))) {
    const clean = m[1] !== undefined ? m[1].trim().replace(/\s+/g, " ") : m[2].replace(/["*()]/g, "");
    if (clean && !/^(or|and|not)$/i.test(clean)) out.push(clean);
  }
  return out;
}

/** HTML for a backend search hit: matches arrive wrapped in U+E000 … U+E001. Falls back to
 *  highlighting `terms` when the text has no markers. */
export function hitHtml(hit: string, terms: string[]): string {
  if (!hit.includes("\uE000")) return highlight(hit, terms);
  return escapeHtml(hit).replace(/\uE000/g, '<mark class="hl">').replace(/\uE001/g, "</mark>");
}

function foldChar(c: string): string {
  if (c === "đ" || c === "Đ") return "d";
  return c.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
}

/** Diacritic/case-insensitive matches of `terms` in `text`, as [start, end) ranges of the original text. */
export function matchRanges(text: string, terms: string[]): [number, number][] {
  if (!terms.length || !text) return [];
  let folded = "";
  const map: number[] = [];
  for (let i = 0; i < text.length; i++) {
    const f = foldChar(text[i]);
    for (let k = 0; k < f.length; k++) map.push(i);
    folded += f;
  }
  map.push(text.length);
  const ranges: [number, number][] = [];
  for (const term of terms) {
    const ft = [...term].map(foldChar).join("").replace(/\s+/g, " ");
    if (!ft) continue;
    // phrases match across any whitespace
    const pat = new RegExp(ft.split(" ").map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("\\s+"), "g");
    let m: RegExpExecArray | null;
    while ((m = pat.exec(folded))) {
      if (!m[0].length) {
        pat.lastIndex++;
        continue;
      }
      ranges.push([map[m.index], map[m.index + m[0].length - 1] + 1]);
    }
  }
  ranges.sort((a, b) => a[0] - b[0]);
  const merged: [number, number][] = [];
  for (const r of ranges) {
    const last = merged[merged.length - 1];
    if (last && r[0] <= last[1]) last[1] = Math.max(last[1], r[1]);
    else merged.push([...r]);
  }
  return merged;
}

/** Escape `text` for HTML and wrap search matches in <mark class="hl">. */
export function highlight(text: string, terms: string[]): string {
  const ranges = matchRanges(text, terms);
  if (!ranges.length) return escapeHtml(text);
  let out = "";
  let pos = 0;
  for (const [a, b] of ranges) {
    out += escapeHtml(text.slice(pos, a)) + '<mark class="hl">' + escapeHtml(text.slice(a, b)) + "</mark>";
    pos = b;
  }
  return out + escapeHtml(text.slice(pos));
}

/** Highlight matches inside a document (e.g. the message iframe). Returns the first mark. */
export function highlightDom(root: HTMLElement, terms: string[], cls = "hermio-hl"): HTMLElement | null {
  if (!terms.length) return null;
  const doc = root.ownerDocument;
  const walker = doc.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode: (n) => {
      const p = n.parentElement;
      if (!p || /^(SCRIPT|STYLE|TEXTAREA|NOSCRIPT)$/.test(p.tagName)) return NodeFilter.FILTER_REJECT;
      return n.nodeValue && n.nodeValue.trim() ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT;
    },
  });
  const nodes: Text[] = [];
  while (walker.nextNode()) nodes.push(walker.currentNode as Text);
  let first: HTMLElement | null = null;
  for (const node of nodes) {
    const text = node.nodeValue ?? "";
    const ranges = matchRanges(text, terms);
    if (!ranges.length) continue;
    const frag = doc.createDocumentFragment();
    let pos = 0;
    for (const [a, b] of ranges) {
      if (a > pos) frag.appendChild(doc.createTextNode(text.slice(pos, a)));
      const mark = doc.createElement("mark");
      mark.className = cls;
      mark.textContent = text.slice(a, b);
      frag.appendChild(mark);
      first ??= mark;
      pos = b;
    }
    if (pos < text.length) frag.appendChild(doc.createTextNode(text.slice(pos)));
    node.parentNode?.replaceChild(frag, node);
  }
  return first;
}
