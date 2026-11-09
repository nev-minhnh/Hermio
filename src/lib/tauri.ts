import { t } from "./i18n.svelte";
// Thin wrappers over the Tauri APIs, with a browser fallback (demo data) so the UI can be
// previewed in a normal browser during development.

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

let mock: Promise<typeof import("./mock")> | null = null;

/** Observers of every backend call (progress bar, failure popups); set by the app. */
export interface InvokeHooks {
  start(cmd: string): number;
  end(token: number, cmd: string, error?: unknown): void;
}
let hooks: InvokeHooks | null = null;
export function setInvokeHooks(h: InvokeHooks) {
  hooks = h;
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (inTauri) return tauriInvoke<T>(cmd, args);
  mock ??= import("./mock");
  return (await mock).mockInvoke(cmd, args ?? {}) as T;
}

export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const token = hooks?.start(cmd) ?? 0;
  try {
    const r = await call<T>(cmd, args);
    hooks?.end(token, cmd);
    return r;
  } catch (e) {
    hooks?.end(token, cmd, e);
    throw e;
  }
}

export async function listen<T>(event: string, cb: (payload: T) => void): Promise<() => void> {
  if (!inTauri) {
    // Browser preview: the mock backend dispatches DOM events instead.
    const h = (e: Event) => cb((e as CustomEvent).detail as T);
    window.addEventListener(`mock:${event}`, h);
    return () => window.removeEventListener(`mock:${event}`, h);
  }
  return tauriListen<T>(event, (e) => cb(e.payload));
}

export function showWindow() {
  if (!inTauri) return;
  const w = getCurrentWindow();
  w.show().then(() => w.setFocus());
}

export const win = {
  minimize: () => inTauri && getCurrentWindow().minimize(),
  toggleMaximize: () => inTauri && getCurrentWindow().toggleMaximize(),
  close: () => inTauri && getCurrentWindow().close(),
  isMaximized: async () => (inTauri ? getCurrentWindow().isMaximized() : false),
  onResized: async (cb: () => void) => (inTauri ? getCurrentWindow().onResized(cb) : () => {}),
};

export async function notify(title: string, body: string) {
  if (!inTauri) return;
  const n = await import("@tauri-apps/plugin-notification");
  let ok = await n.isPermissionGranted();
  if (!ok) ok = (await n.requestPermission()) === "granted";
  if (ok) n.sendNotification({ title, body });
}

export async function pickFolder(title = t("Choose folder")): Promise<string | null> {
  if (!inTauri) return prompt(title, "D:\\Mail") || null;
  const d = await import("@tauri-apps/plugin-dialog");
  const r = await d.open({ directory: true, title });
  return typeof r === "string" ? r : null;
}

export async function pickFiles(): Promise<string[]> {
  if (!inTauri) return [];
  const d = await import("@tauri-apps/plugin-dialog");
  const r = await d.open({ multiple: true, title: t("Attach files") });
  if (!r) return [];
  return Array.isArray(r) ? r : [r];
}

export async function pickSavePath(name: string): Promise<string | null> {
  if (!inTauri) return null;
  const d = await import("@tauri-apps/plugin-dialog");
  return await d.save({ defaultPath: name, title: t("Save attachment") });
}

export async function pickConfigSave(name: string): Promise<string | null> {
  if (!inTauri) return null;
  const d = await import("@tauri-apps/plugin-dialog");
  return await d.save({ defaultPath: name, title: t("Export settings"), filters: [{ name: "Hermio settings", extensions: ["json"] }] });
}

export async function pickConfigOpen(): Promise<string | null> {
  if (!inTauri) return null;
  const d = await import("@tauri-apps/plugin-dialog");
  const r = await d.open({ title: t("Import settings"), filters: [{ name: "Hermio settings", extensions: ["json"] }] });
  return typeof r === "string" ? r : null;
}

export async function pickBackupSave(name: string): Promise<string | null> {
  if (!inTauri) return null;
  const d = await import("@tauri-apps/plugin-dialog");
  return await d.save({ defaultPath: name, title: t("Save a full backup"), filters: [{ name: "Hermio backup", extensions: ["zip"] }] });
}

export async function pickBackupOpen(): Promise<string | null> {
  if (!inTauri) return null;
  const d = await import("@tauri-apps/plugin-dialog");
  const r = await d.open({ title: t("Choose a Hermio backup"), filters: [{ name: "Hermio backup", extensions: ["zip"] }] });
  return typeof r === "string" ? r : null;
}

export async function pickIcsFile(): Promise<string | null> {
  if (!inTauri) return null;
  const d = await import("@tauri-apps/plugin-dialog");
  const r = await d.open({ title: t("Choose a calendar file"), filters: [{ name: "iCalendar", extensions: ["ics", "ical", "ifb", "icalendar"] }] });
  return typeof r === "string" ? r : null;
}

export async function hideWindow() {
  if (!inTauri) return;
  await getCurrentWindow().hide();
}
