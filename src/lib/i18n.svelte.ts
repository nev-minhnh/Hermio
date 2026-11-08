// Tiny i18n: English strings are the keys; other languages map them to translations.
// Changing the language reloads the window, so strings computed once at start-up are refreshed too.
import { vi } from "./vi";

export type Lang = "en" | "vi";
export const languages: { id: Lang; name: string }[] = [
  { id: "en", name: "English" },
  { id: "vi", name: "Tiếng Việt" },
];

function initial(): Lang {
  try {
    const v = localStorage.getItem("hermio.lang");
    if (v === "vi" || v === "en") return v;
  } catch {}
  return "en";
}

export const i18n = $state<{ lang: Lang }>({ lang: initial() });

const dicts: Record<Lang, Record<string, string> | null> = { en: null, vi };

export function t(key: string, vars?: Record<string, string | number>): string {
  const d = dicts[i18n.lang];
  let s = (d && d[key]) || key;
  if (vars) s = s.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? String(vars[k]) : m));
  return s;
}

/** BCP-47 locale for dates and numbers. */
export function locale(): string {
  return i18n.lang === "vi" ? "vi-VN" : "en-GB";
}

/** Remember the language locally (read synchronously at start-up) and reload. */
export function setLang(lang: Lang, reload = true) {
  try {
    localStorage.setItem("hermio.lang", lang);
  } catch {}
  if (lang === i18n.lang) return;
  i18n.lang = lang;
  if (reload) location.reload();
}

/** Keep localStorage in sync with the language stored in settings (e.g. after reinstall). */
export function syncLang(lang: unknown) {
  if ((lang === "en" || lang === "vi") && lang !== i18n.lang) setLang(lang, true);
}
