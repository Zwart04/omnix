# Changelog

Notable changes to Omnix. Release notes for every tag are also on the
[releases page](https://github.com/Zwart04/omnix/releases).

## [Unreleased]

## [0.9.4] - 2026-11-28

### Fixed

- **`agent send` proves the turn instead of printing a fake success.** The
  message and Enter are separate writes, the CR waits for the paste to settle,
  and the CLI reports `sent` only after the PTY produces output beyond the
  echoed message. A shell, a read-only log pane, or a process that never reads
  stdin now fails with `not-accepted` and a reason. The same acceptance rule
  guards the initial handoff brief.
- **Handoff panels survive trust modals.** A Codex/Claude folder-trust prompt
  is answered once, then the brief waits for an idle composer. OSC 7501
  (`idle` / `working` / `done` / `approval` / `stalled` / `quota`) is parsed as
  the authoritative status, so a quiet turn no longer flickers between
  `agent-working` and `agent-turn-done`. `stalled` is emitted after
  `OMNIX_STALL_SECONDS` (default 600) without output.
- **A headless handoff is visible in the sidebar.** Rows show name, agent +
  model, live status (`working`/`stalled`/`done`/`failed`/`quota`/`queued`),
  start time, and **Buka log**; the log opens in a read-only `tail -F` pane.
  Subfolder repositories are placed under the parent project that owns them.
- **Queue conditions keep the GUI panel.** `--min-free-mb`, `--at`, and
  `--after` no longer silently force a detached run when the app is open: the
  pane opens with the wait reason and starts the agent when the condition
  clears. Only `--headless` or a missing GUI uses the detached worker.
- **Default branches no longer carry the app name.** The fallback prefix is
  `kerja/`; `worktree_for_pane` follows `.omnix/config` `branch_prefix` too.
  A stale test binary can no longer produce a false branch result: periksa
  rebuilds the CLI before its integration checks.
- **An exited pane keeps its final status and scrollback.** The old exit path
  removed the pane before writing `done`/`failed`, so the CLI and sidebar lost
  the truth. The pane record now survives until the tab is closed.

### Tests

- `docs/uji-prompt-14/periksa.sh` passes 15/15 (fmt, clippy, 169 Rust tests,
  frontend selftest, PTY protocol mock, branch CLI, home fingerprints).
- GUI integration under Xvfb: waiting pane stays queued and non-headless;
  `agent send` accepts a real mock reply and rejects a `sleep 60` echo; `--after`
  starts after the dependency reports; headless `agent status`, memory rows,
  and a read-only log pane were verified.
- Real TUI runs in `docs/uji-prompt-14/bukti-nyata-20261128.md`: Codex 6 ×
  `exec`, Claude 5 × `Bash`, and Agy 5 × `run_command`, each with a completed
  initial brief and an accepted `agent send` continuation.

## [0.9.3] - 2026-11-27

### Added

- **Split panes choose their layout.** Every terminal header carries an
  Auto / Vertical / Stacked picker shared by its group. Auto keeps the compact
  two-by-two grid at four panes; Vertical gives each pane a full-height column
  so four panes fit side by side without clipped text. The group stops at four
  panes and every split control disables at the cap.

## [0.9.2] - 2026-11-27

### Fixed

- **Every process stop now proves the launch identity.** The browser daemon,
  agent browser sessions, previews (including their process group), and lock
  owners all record `/proc/<pid>` start time and command line when they start.
  A stop sends `TERM` only when the identity still matches, waits up to three
  seconds, then `KILL`; a recycled or foreign PID is never signalled. A lock
  whose owner PID was recycled is stale instead of permanently held.

## [0.9.1] - 2026-11-27

### Fixed

- **Claude folder trust is safe like Orca's.** The config file follows the
  pane's `CLAUDE_CONFIG_DIR`, with the legacy `~/.claude/.config.json` and the
  `-custom-oauth` suffix. Omnix never creates a missing config and never writes
  an unreadable one; it rewrites through a same-mode `<file>.omnix-trust-<id>.tmp`
  file and a rename, so the owner-only `0600` mode, an existing symlink, and
  every unrelated key survive. Writers serialize on Claude Code's own
  `<file>.lock` directory, re-read under the lock, and log the outcome
  (granted / unchanged / missing / unreadable / locked).
- **`handoff close --force` refuses dangerous paths.** The repository itself,
  `$HOME`, `/`, the repository's parent, paths outside `<repo>/.omnix`, and
  symlinks are rejected; a leftover folder is removed only after its `.git`
  file proves an Omnix worktree of this repository, and a folder that holds
  another registered worktree is never deleted.
- **A process is signalled only when its identity still matches.** The launch
  records `/proc/<pid>` start time and command line; `close` sends `TERM`,
  waits up to three seconds, then `KILL` and confirms. A recycled or foreign
  PID is left alone.
- **The tests leave the real home data alone.** The suite is run through a
  guard that fingerprints the content and mode of `~/.claude.json` and the
  whole `~/.omnix` tree before and after, and fails if either changed.

## [0.9.0] - 2026-11-27

### Fixed

- **Claude handoffs work again** — the stored default `mode: "edit"` no longer
  leaks into `extra_args`, which rejected Claude with `--mode is only supported
  for antigravity`. Explicit `--mode plan|edit|full` maps to
  `claude --permission-mode plan|acceptEdits|bypassPermissions`, and `--effort`
  is forwarded to Claude.
- **Claude quota is no longer falsely exhausted** — official window data wins;
  output-text exhaustion is only used when no window exists and expires after
  ten minutes. Orchestrator panes are excluded from quota detection.
- **A failed handoff leaves nothing behind** — quota and resource gates run
  before the worktree is created, and any post-create failure removes the
  worktree, branch, brief, and record so retries no longer hit "already open"
  or "worktree already exists".
- **Tracked sessions are visible** — `omnix agent status` lists sessions
  registered with `omnix agent track` even when no live pane exists.
- **Session IDs are real** — Claude panes and headless runs get an in-process
  UUID v4 via `--session-id`, so `sessionId` is filled and `sleep`/`resume`
  work. No `uuidgen` dependency.
- **Orchestrator panes are recognized** — team orchestrator handoffs carry
  `orchestrator: true` into their pane, so the quota watcher and UI treat them
  correctly.
- **Headless Claude can be adopted** — `omnix agent adopt --agent claude
  --cwd <dir> [--session <id>]` opens a tab (resume when an id is given) or
  resumes detached when the app is not running.
- **Configurable branch names** — `handoff --branch <name>` wins; otherwise
  `.omnix/config` `branch_prefix` or the role (perbaikan → `fix/`, fitur →
  `fitur/`, desain → `desain/`, server → `server/`) sets the prefix. `omnix/`
  remains the fallback. Existing branches are reused.
- **Queued workers stop on close** — `handoff close` terminates the detached
  worker and agent, and a queued worker exits when its record is closed.
- **No zombie agents** — exited pane children are killed and awaited, so no
  `[agy] <defunct>` remains after close.
- **`agent wait` fails fast on a rejected handoff** — a handoff that was never
  delivered (or is blocked) returns immediately with its state instead of
  waiting out the full timeout.
- **`omnix --version`** prints the package version.

## [0.8.0] - 2026-11-26

### Added

- **Quota detection and orchestrator notice** — while a worker runs (interactive
  pane or headless), Omnix reads the output and recognizes an exhausted quota
  for Codex, Antigravity (per model group), and Claude Code, including the
  recovery time in WIB, the model, and the original message excerpt.
- **Quota events** — `agent-quota-exhausted`, `agent-stalled`, and
  `agent-quota-action` are written to `~/.omnix/events.jsonl`.
  `omnix events --follow --kind agent-quota-exhausted` streams one JSON line per
  event, and `omnix agent wait <name> --until ...,quota` exits with code 75 and
  prints the event JSON.
- **Never lose work** — on exhaustion Omnix commits a WIP commit on the handoff
  branch (`WIP [omnix] kuota <agent> habis <jam>`, without pushing unless
  `--push-wip`) and writes a blocked report with the original message, the
  changed files, and the WIP sha.
- **Per-handoff quota action** — `omnix handoff ... --on-quota stop|wait|fallback=A:M[,...]`.
  `wait` resends the brief after the reset time, `fallback` continues in the same
  worktree with the next provider while skipping ones `omnix usage` already shows
  as exhausted, and `stop` (the default) just reports and stops.
- **Honest `omnix usage`** — every provider and every window is shown with its
  reset time in WIB (Codex 5h and weekly, Antigravity per model group, Claude
  Code), unknown data reads "tidak diketahui" instead of being hidden, and
  `omnix usage --check AGENT[:MODEL]` exits 0 when available or 75 when exhausted.
- **Headless without the app** — `omnix handoff ... --run` now works with the app
  closed (detached process + log + events), and `--foreground` runs it in the
  terminal. The dry-run `launch:` preview prints the full argv, including model
  and effort.
- **Queue conditions** — `omnix handoff ... --after HANDOFF`, `--at HH:MM`, and
  `--min-free-mb N` hold a handoff until its condition is met.
- **Per-project Project Team** — a project can limit handoffs to a known set of
  agents/models, pick an orchestrator and worker roles, and choose the project's
  quota policy. The Team modal holds the fallback/on-quota configuration, so it
  is no longer part of the Hand Off Agent feature.

### Changed

- The agent guide now warns against leaving a dev server as an agent background
  task (start, probe, use, and stop it in one shell command) and asks workers to
  commit WIP after each finished step.

### Verification

- 115 Rust tests and `node app/selftest.cjs` pass.
- Release workflow builds Linux (`.deb`, Arch package, updater binary), Windows
  installer, and macOS universal DMG.

[0.8.0]: https://github.com/Zwart04/omnix/releases/tag/v0.8.0

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
