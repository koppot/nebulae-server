# Agent coordination notes — 2026-10-14

Two agents shipped the first public betas today under the owner's direct
instruction: **Droid** (Factory) and **Antigravity**. This note documents
who changed what and why. Feature work from both sides was preserved
throughout; do not revert pipeline fixes without checking the releases
below.

## koppot/nebulae-server (this repo) — SERVER RELEASE IS GREEN

- Droid: repo created from the shared codebase (owner instruction:
  server deployment lives only here). Dockerfile frontend stage switched
  from corepack to `npm i -g pnpm@12` (corepack signature keys break in
  image builds); `pnpm-workspace.yaml` with
  `onlyBuiltDependencies: [esbuild]` added (pnpm 12 no longer reads the
  `pnpm` field in package.json and hard-fails on unapproved build
  scripts — ERR_PNPM_IGNORED_BUILDS).
- Antigravity: copied `pnpm-workspace.yaml*` into the Docker build
  context, added `--fetch-retries=5` to the pnpm install, bumped the
  builder to `rust:1-slim` for edition2024 crates, upgraded
  pnpm/action-setup to v4, and made rqbit pause/resume idempotent
  (007575e) — all verified by CI and the release run.
- Release `nebulae-server-v0.1.0-beta.1` (tag = 007575e): binary +
  SHA-256 attached; image `ghcr.io/koppot/nebulae-server:beta` pushed and
  verified anonymously pullable (HTTP 200). main is one docs commit ahead
  of the tag; the next `nebulae-server-v*` tag ships the media-engine
  work.
- Droid: `AGENT-COORDINATION.md` (this file), provider-neutral signing
  documentation.

## koppot/Nebulae-torrent_client — DESKTOP BETA.8 RELEASE IS GREEN

- Droid: SignPath-decline documentation (application declined
  2026-10-14, insufficient project history; Windows betas are unsigned by
  design); provider-neutral signing gate (`WINDOWS_SIGN_PROVIDER`) +
  per-release `WINDOWS-SIGNING-STATUS.txt`; Tauri updater signing secrets
  wired into the release build (beta.8 originally failed all four OS legs
  without them); client/server split at the owner's instruction (server
  deployment removed here; `nebulae-server` branch deleted; pointer stub
  at docs/nebulae-server.md); VPN test fixture made scanner-safe;
  Unraid install guides (plain + Discord edition).
- Antigravity: Windows compile fixes (os::unix scgi gate, Manager import),
  rqbit engine improvements, media-station features, action-setup v4.

## Standing owner instructions

- SignPath is NOT an active signing provider; Windows betas ship unsigned
  unless another provider is configured. Do not disable SmartScreen or
  bypass security controls in docs or scripts.
- Server deployment exists only in koppot/nebulae-server.
