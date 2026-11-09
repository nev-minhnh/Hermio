/** Writing-assistant settings types (kept apart from writing.ts to avoid an import cycle with the store). */

export interface Goals {
  domain: string;
  formality: string;
  audience: string;
  intent: string;
  /** "auto", "en-US", "en-GB", "en-CA", "en-AU", "en-IN", "vi". */
  variant: string;
}
export const DEFAULT_GOALS: Goals = { domain: "Email", formality: "Neutral", audience: "General", intent: "Inform", variant: "auto" };
export interface Stats {
  /** Accepted per category (correctness, clarity, engagement, delivery). */
  accepted: [number, number, number, number];
  dismissed: number;
  /** Accepted per ISO week ("2026-W45"). */
  weeks: Record<string, number>;
}
export const EMPTY_STATS: Stats = { accepted: [0, 0, 0, 0], dismissed: 0, weeks: {} };

