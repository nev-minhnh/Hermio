<script lang="ts">
  /**
   * Asked when an email with a calendar file is opened: add the event to the calendar, or, for
   * an invitation, answer Yes / Maybe / No (Yes and Maybe also add it to the calendar).
   */
  import { fade, scale } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { t, locale } from "../lib/i18n.svelte";
  import Icon from "../lib/Icon.svelte";
  import { api, type InviteInfo } from "../lib/api";
  import { inviteDone } from "../lib/invite";

  let {
    messageId,
    info,
    onclose,
    ondone,
  }: { messageId: number; info: InviteInfo; onclose: () => void; ondone: (state: InviteInfo["state"]) => void } = $props();

  type Act = "add" | "accepted" | "tentative" | "declined" | "dismiss";
  let busy = $state<Act | null>(null);
  let error = $state("");

  const invitation = $derived(info.method === "REQUEST" && info.can_reply);
  const when = $derived.by(() => {
    const s = new Date(info.start_ts * 1000);
    const e = new Date(info.end_ts * 1000);
    const day = s.toLocaleDateString(locale(), { weekday: "long", day: "numeric", month: "long", year: "numeric" });
    if (info.all_day) return day;
    const tm = (d: Date) => d.toLocaleTimeString(locale(), { hour: "numeric", minute: "2-digit" });
    return `${day} · ${tm(s)} – ${tm(e)}`;
  });

  async function act(a: Act) {
    if (busy) return;
    busy = a;
    error = "";
    try {
      const r = await api.inviteRespond(messageId, a);
      if (a !== "dismiss") inviteDone(a, r, info);
      ondone(a === "add" ? "added" : a === "dismiss" ? "dismissed" : a);
      onclose();
    } catch (e) {
      error = String(e).replace(/^Error: /, "");
    } finally {
      busy = null;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape" && !busy) act("dismiss");
  }
</script>

<svelte:window onkeydown={onKey} />
<div class="scrim" transition:fade={{ duration: 160 }} aria-hidden="true"></div>
<div class="dlg card" role="dialog" aria-modal="true" aria-label={t("Calendar event")} transition:scale={{ start: 0.96, duration: 220, easing: cubicOut }}>
  <div class="hero">
    <div class="ico"><Icon name="calendar" size={22} /></div>
    <div class="grow">
      <div class="kicker">{invitation ? t("You're invited") : t("This email contains a calendar event")}</div>
      <h2>{info.title || t("(No title)")}</h2>
    </div>
  </div>
  <div class="facts">
    <div><Icon name="clock" size={15} /><span>{when}</span></div>
    {#if info.location}<div><Icon name="location" size={15} /><span>{info.location}</span></div>{/if}
    {#if info.organizer}<div><Icon name="person" size={15} /><span>{info.organizer_name ? `${info.organizer_name} <${info.organizer}>` : info.organizer}</span></div>{/if}
    {#if info.recurring}<div class="muted small"><Icon name="info" size={15} /><span>{t("A repeating event: only the next occurrence is added.")}</span></div>{/if}
  </div>
  <p class="muted small q">
    {#if invitation}
      {t("Will you attend? Your answer is sent to the organizer; Yes and Maybe also add it to {cal}.", { cal: info.calendar || t("your calendar") })}
    {:else}
      {t("Add it to {cal}?", { cal: info.calendar || t("your calendar") })}
    {/if}
  </p>
  {#if error}<div class="err small" transition:fade={{ duration: 140 }}><Icon name="warning" size={15} />{error}</div>{/if}
  <div class="acts">
    <button class="btn subtle" onclick={() => act("dismiss")} disabled={!!busy}>{t("Not now")}</button>
    <span class="grow"></span>
    {#if invitation}
      <button class="btn" onclick={() => act("declined")} disabled={!!busy}>{#if busy === "declined"}<span class="spinner"></span>{:else}<Icon name="close" size={15} />{/if}{t("No")}</button>
      <button class="btn" onclick={() => act("tentative")} disabled={!!busy}>{#if busy === "tentative"}<span class="spinner"></span>{:else}<Icon name="help" size={15} />{/if}{t("Maybe")}</button>
      <button class="btn primary" onclick={() => act("accepted")} disabled={!!busy}>{#if busy === "accepted"}<span class="spinner light"></span>{:else}<Icon name="check" size={15} />{/if}{t("Yes")}</button>
    {:else}
      <button class="btn primary" onclick={() => act("add")} disabled={!!busy}>{#if busy === "add"}<span class="spinner light"></span>{:else}<Icon name="plus" size={15} />{/if}{t("Add to calendar")}</button>
    {/if}
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 900;
    background: rgba(8, 14, 40, 0.32);
    backdrop-filter: blur(2px);
  }
  .dlg {
    position: fixed;
    z-index: 901;
    left: 50%;
    top: 50%;
    translate: -50% -50%;
    width: min(460px, calc(100vw - 32px));
    padding: 0;
    overflow: hidden;
    box-shadow: var(--shadow-l);
  }
  .hero {
    display: flex;
    gap: 14px;
    align-items: center;
    padding: 18px 20px;
    color: white;
    background:
      radial-gradient(120% 160% at 0% 0%, rgba(255, 255, 255, 0.18), transparent 55%),
      var(--brand-grad);
  }
  .ico {
    width: 42px;
    height: 42px;
    border-radius: 12px;
    display: grid;
    place-items: center;
    background: rgba(255, 255, 255, 0.18);
    flex: none;
  }
  .kicker {
    font-size: 0.74rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    opacity: 0.85;
  }
  h2 {
    margin: 2px 0 0;
    font-size: 1.12rem;
    font-weight: 650;
    line-height: 1.3;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .facts {
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding: 16px 20px 4px;
  }
  .facts div {
    display: flex;
    gap: 9px;
    align-items: flex-start;
    color: var(--text-2);
    line-height: 1.35;
  }
  .facts :global(.icon) {
    margin-top: 2px;
    color: var(--text-accent);
  }
  .q {
    margin: 10px 20px 0;
  }
  .err {
    display: flex;
    gap: 7px;
    margin: 10px 20px 0;
    padding: 8px 10px;
    border-radius: var(--radius);
    color: var(--danger);
    background: color-mix(in srgb, var(--danger) 9%, transparent);
  }
  .acts {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 16px 20px 18px;
  }
</style>
