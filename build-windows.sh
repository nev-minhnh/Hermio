#!/usr/bin/env bash
# Cross-build Hermio for Windows from Linux. On Windows just run:  npm install && npm run tauri build
#
# Linux prerequisites: mingw-w64, nsis, Node 20+, Rust stable with the std *source* available at
# $(rustc --print sysroot)/lib/rustlib/src/rust, because the Windows std is built from source
# (-Zbuild-std). If `rustup target add x86_64-pc-windows-gnu` works for you, drop -Zbuild-std and the shim.
set -euo pipefail
cd "$(dirname "$0")"
export RUSTC_BOOTSTRAP=1
# Built-in Google OAuth client / Gemini key (optional; see secrets.example.env)
[ -f secrets.local.env ] && . ./secrets.local.env
export PATH="$PWD/scripts/cross-shim:$PATH"
mkdir -p "$(rustc --print sysroot)/lib/rustlib/x86_64-pc-windows-gnu/lib"
npx tauri build --target x86_64-pc-windows-gnu --bundles nsis -- -Zbuild-std=std,panic_abort
