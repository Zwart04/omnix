## What changed

<!-- One short paragraph. Link the issue with "Closes #123" when applicable. -->

## Why

<!-- The problem, not just the implementation. -->

## Checks

- [ ] `cargo test` passes
- [ ] `node app/selftest.cjs` passes
- [ ] Old commands and `.omnix/` state still work (or the break is documented)
- [ ] No always-on daemon, polling, bundled Chromium, or new agent token storage
- [ ] Plugin installs remain on demand and permissions are enforced
- [ ] Docs updated (`README.md`, `docs/plugin*.md`, `docs/dev.md`, `harness/` as relevant)
- [ ] No secrets, cookies, tokens, private paths, or client names in code, docs, or screenshots

## Measurements

<!-- For startup, idle RAM/CPU, browser, or plugin changes: before/after numbers
and the method. Otherwise write "not applicable". -->

## Platforms tested

<!-- Linux / Windows / macOS, and which desktop or compositor. Say what was not run. -->
