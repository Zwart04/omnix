# Changelog

Notable changes to Omnix. Release notes for every tag are also on the
[releases page](https://github.com/Zwart04/omnix/releases).

## [0.7.2] - 2026-11-25

### Added

- **Live AI usage panel** — status-bar chips and a full panel with compact and
  detailed views, sort and refresh, plan/account labels, and per-agent rows.
  Claude quota is read live through the CLI OAuth token when present, with a
  cached/local fallback so the panel still works offline.
- **Memory chip** — process memory and system RAM are visible in the status bar
  and explained in a modal instead of hiding inside a log.
- **Update notice** — a Tako support card that offers the update, links to the
  support page, and can be dismissed per version.
- **Faster project scanning** — opencode scan failures are logged instead of
  disappearing silently, and the history selftest now covers the usage panel,
  memory chip, and update notice.

### Fixed

- Includes the 0.7.1 handoff fix: the brief is pasted first, then Enter is sent,
  so the target agent receives the whole prompt.
- Conflict copy is explicit that two panes are sharing one folder/worktree.

### Verification

- 92 Rust tests and `node app/selftest.cjs` pass.
- Release workflow builds Linux (`.deb`, Arch package, updater binary), Windows
  installer, and macOS universal DMG.

[0.7.2]: https://github.com/Zwart04/omnix/releases/tag/v0.7.2

## [0.7.1] - 2026-11-24

### Fixed

- Handoff brief is submitted only after the paste settles, so the target agent
  receives the whole prompt instead of an Enter keystroke racing the paste.
- README demo and hero refreshed from a real opencode → OpenCode 2 run; image
  links carry a cache-buster so GitHub serves the new files.

### Verification

- 91 Rust tests and `node app/selftest.cjs` pass.
- Release workflow builds Linux (`.deb`, Arch package, updater binary), Windows
  installer, and macOS universal DMG.

[0.7.1]: https://github.com/Zwart04/omnix/releases/tag/v0.7.1

## [0.7.0] - 2026-11-24

### Added

- **Plugin API 1** — `omnix-plugin.toml` manifests, `command | agent |
  browser-tool | hook | panel` contributions, revocable permissions, SHA-256
  verification, and one-shot stdio JSON-RPC. Installing a plugin never runs it.
- **Self-development workflow** — `omnix dev task/build/test/run/install/restore`
  builds Omnix in a worktree, runs the result with a separate `OMNIX_HOME`, and
  replaces the installed binary only after owner approval.
- **Shared-work guards** — cooperative leases for worktrees, branches, preview
  ports, and paths, with PID, TTL, and owner metadata. The GUI warns when two
  agents share a folder or profile and can move a pane to a new worktree.
- **Per-terminal browser sessions** — `browser stop` closes only the caller's
  session, with `--all` for the shared daemon. Preview auto-port, native
  screenshots, preview proxy, cookie handoff, host allowlist, and MCP proxy are
  wired into the app.
- **Agent history & handoff panel** — indexed sessions show their CLI, model,
  branch, message count, and age, with resume in a worktree or new chat, copy
  resume command, hand off to another agent, reveal log, open working directory,
  and delete. Handoff starts the target CLI in a fresh terminal with a focused
  brief or the full transcript.
- **First-run setup wizard** — four steps persisted in `.omnix/setup.json`,
  shown only for a fresh workspace, replayable from Settings.
- **About & Support** — version, license, setup state, GitHub link, and the Tako
  support link.

### Changed

- README rewritten in Indonesian and English with a workspace screenshot and a
  short demo GIF.
- New user, plugin, development, and performance documentation, plus issue and
  pull-request templates, contribution guide, security policy, and code of
  conduct.
- License aligned to GPL-3.0-or-later across manifests, examples, and docs.
- Lower CPU on 2x displays: DMA-BUF stays on by default, the working spinner
  steps at 4 Hz, and wheel scrolling repaints once per gesture.

### Verification

- 91 Rust tests and `node app/selftest.cjs` pass.
- Release workflow builds Linux (`.deb`, Arch package, updater binary), Windows
  installer, and macOS universal DMG.

[0.7.0]: https://github.com/Zwart04/omnix/releases/tag/v0.7.0
