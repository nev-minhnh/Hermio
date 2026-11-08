<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import Icon from "../lib/Icon.svelte";
  import { store } from "../lib/store.svelte";
  import { fly } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { cubicOut } from "svelte/easing";

  let now = $state(Date.now());
  $effect(() => {
    if (!store.pendingSends.length) return;
    const h = setInterval(() => (now = Date.now()), 250);
    return () => clearInterval(h);
  });

  function left(deadline: number) {
    return Math.max(0, Math.ceil((deadline - now) / 1000));
  }
  function who(to: string[]) {
    const first = (to[0] ?? "").replace(/<.*>/, "").trim() || to[0] || "";
    return to.length > 1 ? `${first} +${to.length - 1}` : first;
  }
</script>

<div class="queue" aria-live="polite">
  {#each store.pendingSends as p (p.id)}
    <div class="card" in:fly={{ x: 40, duration: 260, easing: cubicOut }} out:fly={{ x: 40, duration: 200 }} animate:flip={{ duration: 220 }}>
      <div class="ring" style:--p={1 - left(p.deadline) / p.total}>
        {#if p.sending}<Icon name="sync" size={16} class="spin" />{:else}<span>{left(p.deadline)}</span>{/if}
      </div>
      <div class="txt">
        <div class="ttl">{p.sending ? t("Sending…") : t("Sending in {n} s", { n: left(p.deadline) })}</div>
        <div class="sub muted small" title={p.message.subject}>{p.message.subject || t("(No subject)")} · {who([...p.message.to, ...p.message.cc, ...p.message.bcc])}</div>
      </div>
      <div class="acts">
        <button class="btn subtle" onclick={() => store.cancelSend(p.id)} disabled={p.sending}>{t("Cancel")}</button>
        <button class="btn primary" onclick={() => store.sendNow(p.id)} disabled={p.sending}><Icon name="send" size={14} />{t("Send now")}</button>
      </div>
    </div>
  {/each}
</div>

<style>
  .queue {
    position: fixed;
    right: 18px;
    bottom: 18px;
    z-index: 60;
    display: flex;
    flex-direction: column;
    gap: 8px;
    pointer-events: none;
  }
  .card {
    pointer-events: auto;
    width: 380px;
    max-width: calc(100vw - 36px);
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: var(--shadow-l);
  }
  .ring {
    --p: 0;
    flex: none;
    width: 38px;
    height: 38px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    font-size: 0.86rem;
    color: var(--text-accent, var(--accent));
    background:
      radial-gradient(closest-side, var(--surface) 78%, transparent 80% 100%),
      conic-gradient(var(--accent) calc(var(--p) * 100%), var(--surface-3) 0);
    transition: background 250ms linear;
  }
  .txt {
    flex: 1;
    min-width: 0;
  }
  .ttl {
    font-weight: 600;
  }
  .sub {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .acts {
    display: flex;
    gap: 6px;
    flex: none;
  }
</style>
