# Security policy

## Reporting a vulnerability

Please report security problems privately through GitHub Security Advisories:

<https://github.com/Zwart04/omnix/security/advisories/new>

Do **not** open a public issue for a vulnerability, and do not include live
tokens, cookies, or private transcripts in a report. Describe the impact,
affected version, reproduction steps, and any mitigation you found.

You can expect an acknowledgement within a few days. If the report is valid we
will prepare a fix and credit you in the advisory unless you prefer to stay
anonymous.

## Supported versions

Security fixes target the latest release on the `main` branch. Older releases
may not receive backports; include the exact version from `omnix --version`
(or the About/version chip) in your report.

## Scope

Omnix is a local desktop workbench. In scope:

- Command injection or argument confusion in agent, terminal, handoff, plugin,
  preview, or browser commands.
- Path traversal in plugin panels, worktrees, handoffs, previews, or state
  files.
- Reading or exposing agent tokens, cookies, browser profiles, or secrets that
  Omnix should not touch.
- Permission bypass in the plugin host, revoked permissions that still work, or
  panel escapes from the plugin sandbox iframe.
- Unsafe archive extraction, checksum bypass, or catalog downgrade.
- Lock or lease bugs that let one worker release another worker's resources.
- Crashes reachable from untrusted page content or a malicious plugin manifest.

Out of scope:

- A malicious plugin or agent that already runs as your user. Omnix does not
  kernel-sandbox native plugins; review the source before installing.
- Vulnerabilities in the upstream CLI of Codex, Claude, Antigravity, Lightpanda,
  WebKitGTK, or another agent that Omnix merely launches.
- Escalation that requires an attacker to already control your local account,
  shell, or `~/.omnix` directory.
- Denial of service by intentionally starting many terminals or browsers.

## How Omnix handles secrets

- Agent login is delegated to each CLI. Omnix does not read or store tokens.
- Browser cookies stay in the browser profile. `browser session import` writes
  a temporary handoff only when the user asks; it is not passed to plugins.
- Plugins receive a stripped environment plus `OMNIX_PLUGIN_*`; never agent
  tokens, cookies, or the user's full environment.
- State under `.omnix/` can contain prompts and paths. Treat it as private and
  do not attach it to a public issue without redaction.

## Dependency and supply-chain issues

Omnix vendors no browser and downloads no Chromium. Plugin archives from the
catalog are verified with SHA-256 before extraction; local installs are
explicit owner actions. Report a compromised catalog entry or checksum bypass
through the private advisory link above.
