/**
 * Writing assistant (like Grammarly), ported from Themisio: the prompts, the parsing of the
 * model's answers, scoring and the shared settings. The UI lives in WritingAssistant.svelte
 * (underlines, cards, panel) and AssistPopup.svelte (Ask AI / Translate / Meaning).
 */
import { api } from "./api";
import { store } from "./store.svelte";
import type { Goals } from "./writing-types";
export { DEFAULT_GOALS, EMPTY_STATS, type Goals, type Stats } from "./writing-types";

export type Cat = "correctness" | "clarity" | "engagement" | "delivery";
export const CATS: Cat[] = ["correctness", "clarity", "engagement", "delivery"];
export const CAT_COLOR: Record<Cat, string> = {
  correctness: "#e5484d",
  clarity: "#1a8cf8",
  engagement: "#22a861",
  delivery: "#8b5cf6",
};
export const CAT_LABEL: Record<Cat, string> = { correctness: "Correctness", clarity: "Clarity", engagement: "Engagement", delivery: "Delivery" };
const WEIGHT: Record<Cat, number> = { correctness: 4, clarity: 2, engagement: 1, delivery: 1.5 };

export interface Issue {
  cat: Cat;
  kind: string;
  title: string;
  original: string;
  replacement: string;
  explanation: string;
}

export const DOMAINS = ["General", "Academic", "Business", "Email", "Technical", "Creative", "Casual"];
export const FORMALITY = ["Informal", "Neutral", "Formal"];
export const AUDIENCE = ["General", "Knowledgeable", "Expert"];
export const INTENT = ["Inform", "Describe", "Convince", "Tell a story"];
export const VARIANTS: [string, string][] = [
  ["auto", "Same as the text"],
  ["en-US", "American English"],
  ["en-GB", "British English"],
  ["en-CA", "Canadian English"],
  ["en-AU", "Australian English"],
  ["en-IN", "Indian English"],
  ["vi", "Vietnamese"],
];
export const LANGUAGES: [string, string][] = [
  ["auto", "Vietnamese ↔ English"],
  ["vi", "Vietnamese"],
  ["en", "English"],
  ["zh-CN", "Chinese (Simplified)"],
  ["zh-TW", "Chinese (Traditional)"],
  ["ja", "Japanese"],
  ["ko", "Korean"],
  ["fr", "French"],
  ["de", "German"],
  ["es", "Spanish"],
  ["pt", "Portuguese"],
  ["it", "Italian"],
  ["ru", "Russian"],
  ["th", "Thai"],
  ["id", "Indonesian"],
  ["ar", "Arabic"],
  ["hi", "Hindi"],
];

export function weekKey(d = new Date()): string {
  const t = new Date(Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()));
  const day = t.getUTCDay() || 7;
  t.setUTCDate(t.getUTCDate() + 4 - day);
  const y = t.getUTCFullYear();
  const w = Math.ceil(((t.getTime() - Date.UTC(y, 0, 1)) / 86400000 + 1) / 7);
  return `${y}-W${String(w).padStart(2, "0")}`;
}

const uiLang = () => (store.settings.lang === "vi" ? "Vietnamese" : "English");

/** Extra instructions the user keeps for the AI (Settings › AI assistant). */
function standing(): string {
  const s = (store.settings.writing_instructions ?? "").trim();
  return s ? `\n\nThe author's standing instructions:\n${s}` : "";
}

export function checkSystem(goals: Goals, dictionary: string[]): string {
  const variant = VARIANTS.find(([c]) => c === goals.variant)?.[1] ?? "the text's own language";
  const ui = uiLang();
  let p =
    `You are Hermio's writing assistant, an expert editor working like Grammarly. Review each numbered unit of the user's email against four criteria:\n` +
    `- correctness: spelling, grammar, punctuation, agreement, wrong or missing words (these are errors);\n` +
    `- clarity: wordy, vague or hard-to-follow phrasing, needless passive voice, overlong sentences — suggest a concise rewrite;\n` +
    `- engagement: repeated, flat or overused words and monotonous sentences — suggest livelier word choices (synonyms);\n` +
    `- delivery: tone and formality that miss the goals, hedging, impolite or overly casual phrasing.\n` +
    `Writing goals: domain ${goals.domain}; formality ${goals.formality}; audience ${goals.audience}; intent ${goals.intent}; language variety ${variant}.\n` +
    `Keep each unit's language (never translate) and the author's meaning. Do not flag what is correct and fits the goals.\n` +
    `Reply with JSON only, no prose and no code fence:\n` +
    `{"issues": [{"unit": 1, "category": "correctness|clarity|engagement|delivery", "type": "spelling|grammar|punctuation|word choice|wordiness|passive voice|repetition|tone|…", ` +
    `"title": "<a short action in ${ui}, e.g. Correct the spelling>", "original": "<exact substring of that unit>", "replacement": "<its replacement; empty to delete>", ` +
    `"explanation": "<one short sentence in ${ui}>"}], "tone": "<the email's overall tone in 2–4 words, in ${ui}>", "tone_fits": true}\n` +
    `"original" is copied character for character from the unit and is as short as possible while unique in it. At most 8 issues per unit, most important first.`;
  if (dictionary.length) p += `\nNever flag these words (the author's dictionary): ${dictionary.join(", ")}.`;
  return p + standing();
}

/** The first JSON object (or array) in a model's answer, tolerating code fences and prose. */
export function jsonIn(answer: string): any {
  const s = answer.replace(/```(?:json)?/gi, "");
  for (const [open, close] of [
    ["{", "}"],
    ["[", "]"],
  ]) {
    const a = s.indexOf(open);
    const b = s.lastIndexOf(close);
    if (a >= 0 && b > a) {
      try {
        return JSON.parse(s.slice(a, b + 1));
      } catch {}
    }
  }
  return null;
}

function parseCat(s: string): Cat {
  const v = (s || "").trim().toLowerCase();
  if (v === "clarity") return "clarity";
  if (v === "engagement") return "engagement";
  if (v === "delivery" || v === "tone") return "delivery";
  return "correctness";
}

/** Issues per unit; only those that quote their unit exactly are kept. */
export function parseBatch(answer: string, units: string[]): { issues: Issue[][]; tone: [string, boolean] | null } {
  const v = jsonIn(answer);
  if (!v) throw new Error("The model didn't answer in the expected form. Try again, or pick another model.");
  const out: Issue[][] = units.map(() => []);
  for (const i of v.issues ?? []) {
    const n = Number(i.unit);
    if (!(n >= 1 && n <= units.length)) continue;
    const original = String(i.original ?? "");
    const replacement = String(i.replacement ?? "");
    if (!original || original === replacement || !units[n - 1].includes(original)) continue;
    const list = out[n - 1];
    if (list.some((x) => x.original === original) || list.length >= 8) continue;
    list.push({ cat: parseCat(i.category), kind: String(i.type ?? ""), title: String(i.title ?? ""), original, replacement, explanation: String(i.explanation ?? "") });
  }
  const tone = typeof v.tone === "string" && v.tone.trim() ? ([v.tone, v.tone_fits !== false] as [string, boolean]) : null;
  return { issues: out, tone };
}

/** 0–100: weighted issues per 100 words, eased (4 → 91, 10 → 80, 40 → 50). */
export function score(issues: Issue[], words: number): number | null {
  if (!words) return null;
  const penalty = issues.reduce((a, i) => a + WEIGHT[i.cat], 0);
  const per100 = (penalty * 100) / Math.max(words, 40);
  return Math.round(Math.min(100, Math.max(0, 100 / (1 + per100 / 40))));
}

export function hash(text: string, goals: Goals): string {
  let h = 5381;
  const s = text + "\u0000" + JSON.stringify(goals);
  for (let i = 0; i < s.length; i++) h = ((h << 5) + h + s.charCodeAt(i)) | 0;
  return (h >>> 0).toString(36) + ":" + s.length;
}

// ---------- the model the assistant uses ----------

export interface AssistModel {
  provider: string;
  model: string;
  thinking: boolean;
}
export function assistModel(): AssistModel {
  const s = store.settings;
  return { provider: s.assist_provider || s.ai_default, model: s.assist_model || "", thinking: !!s.assist_thinking };
}

let seq = 0;
/** One completion; returns [promise, stop]. */
export function run(system: string, prompt: string, m = assistModel()): [Promise<string>, () => void] {
  const id = `write:${Date.now()}:${++seq}`;
  const p = (async () => {
    let provider = m.provider;
    // Nothing chosen yet: the first connected provider.
    if (!provider) provider = (await api.aiProviders())[0]?.id ?? "";
    if (!provider) throw new Error("Connect an AI provider in Settings › AI assistant first.");
    return api.aiComplete(provider, m.model || null, m.thinking, system, prompt, id);
  })();
  return [p, () => api.aiCancel(id).catch(() => {})];
}

export function askSystem(): string {
  return (
    "You are a skilled writing assistant inside an email composer. Do what the user asks with their text. " +
    "Reply with the resulting text only — no preamble, no quotes, no explanation, no Markdown unless the text already uses it. " +
    "Keep the text's language unless the user asks otherwise." +
    standing()
  );
}
export function writeSystem(goals: Goals): string {
  return (
    "You are a skilled writing assistant inside an email composer (like GrammarlyGO). Do what the user asks. Reply with the resulting text only — " +
    "no preamble, no quotes, no explanation. Keep the text's language unless asked otherwise." +
    `\nWriting goals: domain ${goals.domain}, formality ${goals.formality}, audience ${goals.audience}, intent ${goals.intent}.` +
    standing()
  );
}
export function defineSystem(): string {
  return (
    "You are a concise dictionary. Explain the meaning of the word or phrase the user gives, as used in its context. " +
    `Answer in ${uiLang()}: the meaning(s) with part of speech, pronunciation if useful, 2–5 synonyms, and one short example. Plain text.`
  );
}
export function translateSystem(to: string): string {
  const into = to === "auto" ? "English if the text is in Vietnamese, otherwise Vietnamese" : (LANGUAGES.find(([c]) => c === to)?.[1] ?? "English");
  return `You are a professional translator. Translate the user's text into ${into}. Keep the meaning, tone, formatting and line breaks. Reply with the translation only.` + standing();
}

export function preview(text: string, n: number): string {
  const one = text.split(/\s+/).filter(Boolean).join(" ");
  return one.length > n ? one.slice(0, n) + "…" : one;
}
