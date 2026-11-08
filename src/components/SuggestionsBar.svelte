<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import { slide } from "svelte/transition";
  import Icon from "../lib/Icon.svelte";
  import { store } from "../lib/store.svelte";
  import { api, type Suggestion } from "../lib/api";

  const open = $derived(store.suggestOpen);
  let busy = $state(false);

  const items = $derived(
    store.suggestions
      .map((s) => ({ s, m: store.messages.find((m) => m.id === s.message_id) }))
      .filter((x) => x.m),
  );
  const target = (s: Suggestion) =>
    s.group_id ? (store.groups.find((g) => g.id === s.group_id)?.name ?? "?") : t("new group “{name}”", { name: s.new_group });

  async function resolve(ids: number[], accept: boolean) {
    busy = true;
    try {
      for (const id of ids) await api.aiResolveSuggestion(id, accept);
      if (accept) {
        store.messages = store.messages.filter((m) => !ids.includes(m.id));
        store.selected = store.selected.filter((x) => !ids.includes(x));
        store.success(t("Sorted {n} messages", { n: ids.length }));
      }
      await store.reloadAll();
    } catch (e) {
      store.error(String(e));
    } finally {
      busy = false;
    }
  }
</script>

{#if items.length}
  <div class="sugg" transition:slide={{ duration: 180 }}>
    <button class="bar" onclick={() => (store.suggestOpen = !store.suggestOpen)}>
      <Icon name="sparkle" size={16} />
      <span class="grow">{t("AI suggests sorting {n} new messages", { n: items.length })}</span>
      <Icon name={open ? "chevronUp" : "chevronDown"} size={14} />
    </button>
    {#if open}
      <div class="list" transition:slide={{ duration: 180 }}>
        {#each items as { s, m } (s.message_id)}
          <div class="item" transition:slide={{ duration: 160 }}>
            <div class="grow txt">
              <button class="subj" onclick={() => (store.selected = [m!.id])}>{m!.subject || t("(No subject)")}</button>
              <span class="muted small">→ <b>{target(s)}</b>{s.reason ? ` · ${s.reason}` : ""}</span>
            </div>
            <button class="icon-btn ok" title={t("Accept")} disabled={busy} onclick={() => resolve([s.message_id], true)}><Icon name="check" size={16} /></button>
            <button class="icon-btn" title={t("Dismiss")} disabled={busy} onclick={() => resolve([s.message_id], false)}><Icon name="close" size={14} /></button>
          </div>
        {/each}
        <div class="all">
          <button class="btn subtle" disabled={busy} onclick={() => resolve(items.map((x) => x.s.message_id), false)}>{t("Dismiss all")}</button>
          <button class="btn primary" disabled={busy} onclick={() => resolve(items.map((x) => x.s.message_id), true)}><Icon name="check" size={15} />{t("Accept all")}</button>
        </div>
      </div>
    {/if}
  </div>
{/if}

<style>
  .sugg {
    flex: none;
    margin: 0 8px 6px;
    border-radius: 10px;
    border: 1px solid color-mix(in srgb, var(--ai) 30%, var(--border));
    background: color-mix(in srgb, var(--ai) 6%, var(--surface));
    overflow: hidden;
  }
  .bar {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    border: none;
    background: transparent;
    text-align: left;
    font-size: 0.86rem;
    font-weight: 600;
  }
  .bar :global(svg:first-child) {
    color: var(--ai);
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .list {
    max-height: 260px;
    overflow-y: auto;
    padding: 0 6px 6px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 4px 4px 8px;
    border-radius: 8px;
  }
  .item:hover {
    background: var(--hover);
  }
  .txt {
    display: flex;
    flex-direction: column;
  }
  .subj {
    border: none;
    background: none;
    padding: 0;
    text-align: left;
    font-size: 0.86rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ok {
    color: var(--success);
  }
  .all {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
    padding-top: 6px;
  }
</style>
