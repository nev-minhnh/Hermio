# Contributing to Hermio

Thanks for helping! Hermio is a Tauri 2 (Rust) + Svelte 5 app for Windows 11.

## Getting started

```powershell
git clone https://github.com/nev-minhnh/Hermio
cd Hermio
npm ci
npm run tauri dev
```

The UI can also be worked on in a plain browser with mock data: `npm run dev`, then open
`http://localhost:5173` (add `?hw=intel`, `?hw=none` or `?popup=1` to simulate hardware cases).

## Before you open a pull request

- `npx svelte-check --threshold error` reports no errors.
- `cargo test --manifest-path src-tauri/Cargo.toml` passes.
- `cargo fmt` for Rust; keep the existing formatting for Svelte/TypeScript.
- New user-facing text goes through `t("…")`; add the Vietnamese string to `src/lib/vi.ts`.
- Respect "reduce motion" for new animations, and both light and dark themes.
- Never commit credentials. `secrets.local.env` is git-ignored; CI reads repository secrets.

## Commit messages

Short imperative subject (“Fix POP3 resume after timeout”), with details in the body when useful.

## Licence

By contributing you agree that your contribution is dual-licensed under MIT OR Apache-2.0, as
described in the README.
