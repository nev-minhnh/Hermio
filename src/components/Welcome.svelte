<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import Icon from "../lib/Icon.svelte";
  import Logo from "../lib/Logo.svelte";
  import { store } from "../lib/store.svelte";
</script>

<div class="welcome card">
  <div class="hero">
    <Logo size={96} />
    <h1>Hermio</h1>
    <div class="tagline">{t("Automate your inbox flow.")}</div>
    <p class="muted">{t("Add your first email account to get started. Gmail, Google Workspace, IMAP, POP3 and Google Calendar are supported.")}</p>
    <div class="row">
      <button class="btn primary big" onclick={() => { store.settingsTab = "add"; store.route = "settings"; }}>
        <Icon name="plus" size={18} /> {t("Add account")}
      </button>
      <button class="btn big" onclick={() => { store.settingsTab = "google"; store.route = "settings"; }}>
        <Icon name="key" size={18} /> {t("Set up Google sign-in")}
      </button>
    </div>
  </div>
</div>

<style>
  .welcome {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    background:
      radial-gradient(60% 50% at 50% 30%, color-mix(in srgb, var(--brand-1) 14%, transparent), transparent 70%),
      var(--surface);
  }
  .hero :global(img) { box-shadow: 0 14px 36px color-mix(in srgb, var(--brand-2) 38%, transparent); animation: float-in 600ms var(--ease-out); }
  @keyframes float-in { from { opacity: 0; transform: translateY(10px) scale(0.9); } }
  .hero { max-width: 520px; text-align: center; display: flex; flex-direction: column; align-items: center; gap: 10px; animation: slide-up 320ms var(--ease-out); }
  .tagline { font-family: var(--font-display); font-size: 1.1rem; color: var(--text-accent); font-weight: 600; margin-bottom: 6px; }
  h1 { margin: 14px 0 0; font-family: var(--font-display); font-weight: 700; font-size: 1.9rem; background: var(--brand-grad); -webkit-background-clip: text; background-clip: text; color: transparent; }
  :global([data-theme="dark"]) h1 { background: linear-gradient(135deg, #a9c4ff, #5b8cff); -webkit-background-clip: text; background-clip: text; }
  p { margin: 0 0 14px; line-height: 1.55; }
  .big { height: 40px; padding: 0 18px; font-size: 0.98rem; }
</style>
