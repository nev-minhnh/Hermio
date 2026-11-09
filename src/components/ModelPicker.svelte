<script lang="ts">
  /** Provider + model (and Thinking) for the writing assistant; remembered in the settings. */
  import { onMount } from "svelte";
  import { t } from "../lib/i18n.svelte";
  import { api, type AiProvider } from "../lib/api";
  import { store } from "../lib/store.svelte";

  let { google = false, compact = false }: { google?: boolean; compact?: boolean } = $props();

  let providers = $state<AiProvider[]>([]);
  let models = $state<string[]>([]);

  // Nothing chosen yet: the assistant's default, else the first connected provider.
  const provider = $derived(google ? store.settings.translate_engine || "google" : store.settings.assist_provider || store.settings.ai_default || providers[0]?.id || "");
  const isGoogle = $derived(google && provider === "google");

  onMount(async () => {
    providers = await api.aiProviders().catch(() => []);
    loadModels();
  });

  async function loadModels() {
    const p = providers.find((x) => x.id === provider);
    models = p?.model ? [p.model] : [];
    if (!p) return;
    try {
      const list = await api.aiModels(p.id);
      if (list.length) models = [...new Set([p.model, ...list].filter(Boolean))];
    } catch {}
  }
  function setProvider(id: string) {
    if (google) store.set("translate_engine", id);
    else {
      store.set("assist_provider", id);
      store.set("assist_model", "");
    }
    loadModels();
  }
</script>

<div class="mp" class:compact>
  <select class="input" value={provider} onchange={(e) => setProvider((e.target as HTMLSelectElement).value)} title={t("AI provider")}>
    {#if google}<option value="google" selected={provider === "google"}>Google Translate</option>{/if}
    {#each providers as p (p.id)}<option value={p.id} selected={p.id === provider}>{p.name}</option>{/each}
  </select>
  {#if !isGoogle && !google}
    <select class="input" value={store.settings.assist_model || models[0] || ""} onchange={(e) => store.set("assist_model", (e.target as HTMLSelectElement).value)} title={t("Model")}>
      {#each models as m, i}<option value={m} selected={store.settings.assist_model ? m === store.settings.assist_model : i === 0}>{m}</option>{/each}
    </select>
  {/if}
  {#if !isGoogle}
    <label class="think small"><input type="checkbox" checked={store.settings.assist_thinking} onchange={(e) => store.set("assist_thinking", (e.target as HTMLInputElement).checked)} />{t("Thinking")}</label>
  {/if}
</div>

<style>
  .mp {
    display: flex;
    gap: 6px;
    align-items: center;
    min-width: 0;
  }
  .mp select {
    min-width: 0;
    flex: 1 1 0;
    padding: 4px 8px;
    font-size: 0.82rem;
  }
  .think {
    display: inline-flex;
    gap: 5px;
    align-items: center;
    white-space: nowrap;
    color: var(--text-2);
  }
</style>
