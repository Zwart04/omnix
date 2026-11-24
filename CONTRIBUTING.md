# Contributing to Omnix

Thanks for taking the time to improve Omnix. This project stays deliberately
small: one Rust binary, a vanilla HTML/CSS/JS interface rendered by the system
WebView, and no Electron, Node daemon, or bundled Chromium.

By participating you agree to the [Code of Conduct](CODE_OF_CONDUCT.md).

## Before you start

- Check open issues and pull requests so we do not duplicate work.
- For a large feature, open an issue first and describe the invariant it
  changes. Omnix prefers a small, measured change over a broad rewrite.
- Security problems go to [SECURITY.md](SECURITY.md), never a public issue.

## Build

Linux needs `pkg-config` and the WebKitGTK 4.1 development packages:

```sh
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev
cargo build
```

Run the app without installing:

```sh
cargo run
```

If `cc` in `PATH` is not a real compiler (for example a zig wrapper):

```sh
CC=/usr/bin/cc RUSTFLAGS="-C linker=/usr/bin/cc" cargo build
```

Windows and macOS build notes live in `README.md` and `docs/app.md`.

## Test

Every change must keep the suite green:

```sh
cargo test
node app/selftest.cjs
```

`cargo test` covers git worktrees, SQLite FTS5, profiles, plugins, locks,
previews, and dev builds. `app/selftest.cjs` stubs the WebView and checks RPC
shape, rendering, palette, splits, and the new plugin/dev screens. Add a test
next to the code you change; a bug fix without a regression test is incomplete.

For UI changes, also run the app and check keyboard focus, narrow windows, and
the dark/light theme. Screenshots are welcome in the pull request.

## What to keep stable

- Old CLI commands and their output shape keep working.
- `.omnix/` state files remain backward compatible: new fields need
  `#[serde(default)]`.
- Agent arguments and environment are vectors, never shell strings.
- Profile names stay `[a-z0-9-]{1,48}` and use the CLI's official env var.
- Never fake `HOME`, read agent tokens, or store cookies.
- Plugin processes start only on demand; no always-on daemon or polling.
- Idle RAM/CPU regressions stay within 5% of the baseline in
  `docs/performance.md`.

## Code style

- Match the surrounding Rust and JavaScript style; do not reformat unrelated
  code.
- Prefer the standard library and existing dependencies. A new crate needs a
  short justification in the pull request.
- Keep UI copy in the same language as the surrounding screen (Indonesian for
  the app, English for API/developer docs).
- Handle errors with context; do not `unwrap()` user input or filesystem paths
  in library code.
- Comments explain *why*, not *what*.

## Plugins

Plugin changes must include:

- A manifest (`omnix-plugin.toml`) that passes `Manifest::parse`.
- A one-shot stdio runtime with a timeout and honest permissions.
- Tests for manifest parsing, permission denial, and crash/timeout handling
  when the host behavior changes.
- Documentation in `docs/plugin-api.md` (English) and `docs/plugin.md`
  (Indonesian) when the API changes.

Do not add a plugin that downloads Chromium, Chrome, Playwright, or Puppeteer
browsers. Browser tools connect to Lightpanda as clients.

## Commits and pull requests

- Keep commits focused and messages in the imperative mood.
- Do not commit build output, `target/`, `.omnix/`, screenshots of private
  data, tokens, client names, or personal absolute paths.
- Describe what changed, why, and which platforms were actually tested.
- Mention the test commands you ran and any measurement you took.
- Do not tag a release or publish a binary from a pull request; the owner does
  that explicitly.

## License

Omnix is licensed under GPL-3.0-or-later. By contributing you agree that your
contribution is licensed under the same terms.
