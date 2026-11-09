import { t, locale } from "./i18n.svelte";
import { store } from "./store.svelte";
import type { InviteInfo, InviteResult } from "./api";

/** The confirmation popup after answering / adding a calendar event from an email. */
export function inviteDone(action: string, r: InviteResult, info: Pick<InviteInfo, "organizer" | "organizer_name">) {
  const d = new Date(r.start_ts * 1000);
  const when = r.all_day
    ? d.toLocaleDateString(locale(), { weekday: "short", day: "numeric", month: "short" })
    : d.toLocaleString(locale(), { weekday: "short", day: "numeric", month: "short", hour: "numeric", minute: "2-digit" });
  const who = info.organizer_name || info.organizer;
  if (action === "declined") {
    store.success(r.replied ? t("Declined. {who} has been told.", { who }) : t("Declined"));
    return;
  }
  const what = r.already ? t("Already in your calendar: {title} · {when}", { title: r.title, when }) : t("Added to your calendar: {title} · {when}", { title: r.title, when });
  const reply = r.replied ? " " + (action === "accepted" ? t("Your Yes was sent to {who}.", { who }) : t("Your Maybe was sent to {who}.", { who })) : "";
  store.toast("success", what + reply, { label: t("Open calendar"), run: () => store.showEvent(r.event_id, r.start_ts) });
}
