# Agent coordination notes

## 2026-10-14 — Droid (Factory) session

Modifications in this repository were made by **Droid** (Factory's coding
agent), working at the direct instruction of the repository owner (koppot).
This note is left so Antigravity — and any other agent or contributor —
can see what was changed here and why.

### Changes on koppot/nebulae-server (this repo)

- `server/Dockerfile` (commit 481ca2a): the frontend build stage no longer
  uses corepack (its bundled signature keys fail inside the image build);
  it installs pnpm 12 from npm on node:22-slim, matching the pnpm version
  the release workflows pin via pnpm/action-setup.
- `package.json` (this commit): added `pnpm.onlyBuiltDependencies: ["esbuild"]`.
  pnpm >= 10 refuses to run non-allowlisted dependency build scripts, and the
  container build failed hard with `ERR_PNPM_IGNORED_BUILDS` ("Ignored build
  scripts: esbuild@0.21.5"), leaving vite without a working esbuild binary.
  This is exactly what `pnpm approve-builds` would write; esbuild's
  postinstall only provisions the platform binary vite needs.
- Tag `nebulae-server-v0.1.0-beta.1` re-pointed to the fixed commit so the
  server-release workflow builds a working image.
- `AGENT-COORDINATION.md` (this file).

### Context you should not undo

- SignPath Foundation DECLINED the project's OSS code-signing application
  (insufficient project history). Windows artifacts are intentionally
  unsigned beta builds; the release workflow's signing gate is
  provider-neutral (repo variable `WINDOWS_SIGN_PROVIDER`, currently unset).
  See docs/code-signing-policy.md and docs/signpath-application.md.
- The client/server repository split was done at the owner's instruction:
  server deployment lives ONLY here; the desktop client repo
  (koppot/Nebulae-torrent_client) keeps the client-side Server mode and a
  pointer stub.

### Changes on koppot/Nebulae-torrent_client (for reference)

- SignPath-decline documentation pass, provider-neutral signing gate,
  WINDOWS-SIGNING-STATUS.txt per release, desktop release workflow now
  passes the Tauri updater signing secrets to tauri build (beta.8 failed
  without them), client-side VPN test fixture made scanner-safe, and the
  server deployment files were removed there per the owner's instruction.
- Antigravity's own Windows compile fixes (139029d) are preserved as-is.

Questions or conflicts: coordinate here or in the Factory Droid session;
do not revert the changes above without updating the tag/pipeline that
depends on them.
