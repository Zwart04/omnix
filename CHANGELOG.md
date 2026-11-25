# Changelog

Notable changes to Omnix. Release notes for every tag are also on the
[releases page](https://github.com/Zwart04/omnix/releases).

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
