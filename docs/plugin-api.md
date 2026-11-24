# Omnix plugin API 1

Omnix plugins are small native programs started on demand. There is no plugin
daemon: each call starts the runtime, writes one JSON-RPC request, reads one
JSON-RPC response, and lets the process exit. When no plugin is called, no
plugin process exists and no plugin code is loaded.

This document describes the stable API 1 surface. The Indonesian user guide is
in [`plugin.md`](plugin.md); two working examples live in
[`../examples/plugins/`](../examples/plugins/).

> **Security model.** A plugin is a normal program running as your user. Omnix
> verifies checksums, gates contributions behind permissions, and never gives a
> plugin agent tokens or browser cookies. Omnix does **not** kernel-sandbox a
> plugin. Install only plugins whose source you trust.

## Install layout

| Path | Meaning |
| --- | --- |
| `~/.omnix/plugins/<name>/` | Installed plugin folder |
| `~/.omnix/plugins/state.json` | Enabled flag, grants, installed version, checksum |
| `~/.omnix/plugins/data/<name>/` | Writable data directory (`OMNIX_PLUGIN_DATA`) |
| `<repo>/.omnix/plugins/` | Not used; installs are per user |

Installing never runs the plugin. It copies the files and enables the
contributions, but no plugin process exists until a command, hook, panel, or
tool is actually called. Disabling hides contributions but keeps files. Removing
deletes the install and its data directory.

## Manifest: `omnix-plugin.toml`

```toml
name = "hello-command"          # [a-z0-9-]{1,48}
version = "0.1.0"               # semver MAJOR.MINOR.PATCH
author = "Your Name"
license = "GPL-3.0-or-later"
description = "Says hello from Omnix"
api = 1                         # must equal the running API

[runtime]
entry = ["python3", "plugin.py"] # argv vector, no shell
timeout_ms = 15000               # 1..120000, default 15000
ram_mb = 256                     # best-effort address-space cap, default 256

[permissions]
read = ["/path/to/allow"]        # documented paths the plugin may read
write = ["/path/to/allow"]       # documented paths the plugin may write
network = ["api.example.com"]    # documented hosts
agents = false                   # may register an agent contribution
history = false                  # may ask the host for history data

[[contributions]]
kind = "command"
name = "hello"
title = "Hello"
description = "Print a greeting"
```

Required top-level fields: `name`, `version`, `author`, `license`,
`description`, `api`. `runtime.entry` is required and must be a non-empty argv
vector. Unknown contribution kinds are rejected, so a newer plugin cannot
silently claim compatibility.

### Contributions

| Kind | Required fields | What Omnix does |
| --- | --- | --- |
| `command` | `name` (defaults to plugin name) | Adds `omnix <plugin>:<command>` and a palette/menu entry |
| `agent` | `name`, `bin` | Adds a launchable agent; optional `args`, `env`, `resume` |
| `browser-tool` | `name` | Exposes a tool next to `omnix browser mcp`; optional `input_schema` |
| `hook` | `events` | Runs after matching events (`*` or a kind such as `handoff-created`) |
| `panel` | `name`, `entry` | Serves a sandboxed iframe from the plugin folder |

`agent.resume` supports `{session}` substitution, for example
`resume = ["--resume", "{session}"]`.

### Permissions

Permissions are declarations plus an owner-controlled grant. A revoked
permission blocks the matching call; it does not uninstall the plugin.

- `read` / `write`: path allowlists. They document intent and are checked when
  the host performs a matching operation.
- `network`: host allowlist.
- `agents`: required for an `agent` contribution.
- `history`: required for history access.

Plugins are launched with a stripped environment. Only a small set of safe
variables plus these are present:

| Variable | Meaning |
| --- | --- |
| `OMNIX_PLUGIN_NAME` | Manifest name |
| `OMNIX_PLUGIN_API` | `1` |
| `OMNIX_PLUGIN_DIR` | Installed plugin folder |
| `OMNIX_PLUGIN_DATA` | Per-plugin writable folder |
| `OMNIX_PLUGIN_PERMISSIONS` | Comma-separated granted keys |

Agent tokens, cookies, browser profiles, and the user's full environment are
never passed to a plugin.

## Runtime protocol

One request line on **stdin**, one response line on **stdout**. Diagnostics go
to stderr and are captured for errors.

Request:

```json
{"jsonrpc":"2.0","id":1,"method":"command","params":{"plugin":"hello","args":[],"cwd":"/repo"}}
```

Success:

```json
{"jsonrpc":"2.0","id":1,"result":{"message":"hello"}}
```

Failure:

```json
{"jsonrpc":"2.0","id":1,"error":{"code":"failed","message":"why"}}
```

Methods:

| Method | Params | Used by |
| --- | --- | --- |
| `command` | `plugin`, `args`, `cwd` | CLI and palette commands |
| `hook` | `event`, `name`, `at` | Event hooks |
| `browser-tool` | `arguments`, `browser` | Browser tool registry |
| `panel` | free-form object from the panel | Panel host |

The plugin may log to stderr; the last lines are included in timeout or crash
errors. Exit before a response, invalid JSON, a non-zero exit, or exceeding the
timeout/RAM cap all fail the call without affecting the Omnix process. Omnix
kills the child and its process group after the response or timeout.

### Panel host

A `panel` contribution is served from
`/plugin-panel/<plugin>/<entry>` inside a sandboxed iframe with a strict CSP.
Relative assets stay under the plugin folder; symlinks and path escapes are
refused. The panel talks to Omnix with `postMessage`:

```js
parent.postMessage({ omnixPanel: true, ready: true }, "*");
parent.postMessage({ omnixPanel: true, method: "status", params: {} }, "*");
window.addEventListener("message", function (event) {
  if (!event.data || !event.data.omnixPanelReply) return;
  render(event.data.result);
});
```

The host answers with `{omnixPanelReply: true, id, ok, result}` and calls the
`panel` JSON-RPC method through the normal one-shot runtime. Panels cannot read
cookies or agent tokens.

For a browser tool, Omnix adds:

```json
{"browser":{"endpoint":"http://127.0.0.1:9222","engine":"lightpanda"}}
```

The plugin may connect to that CDP endpoint as a client. It must not start or
download its own Chromium.

## Catalog

The default catalog is:

```text
https://raw.githubusercontent.com/Zwart04/omnix-plugins/main/index.json
```

Override it with `OMNIX_PLUGIN_CATALOG`. `file://` catalogs are accepted only
through that override, which keeps the default install path HTTPS-only.

```json
{
  "api": 1,
  "plugins": [
    {
      "name": "hello-command",
      "version": "0.1.0",
      "author": "Zwart04",
      "license": "GPL-3.0-or-later",
      "description": "Says hello from Omnix",
      "url": "https://github.com/Zwart04/omnix-plugins/releases/download/hello-command-0.1.0/hello-command.tar.gz",
      "sha256": "…64 hex characters…",
      "kinds": ["command"],
      "permissions": {"read": [], "write": [], "network": [], "agents": false, "history": false}
    }
  ]
}
```

The archive is verified against `sha256` before extraction. Local folders and
local `.tar.gz` files are accepted for development; a directory checksum is
computed from relative paths and file contents.

## CLI

```sh
omnix plugin list [--json]
omnix plugin search [query] [--json]
omnix plugin install <catalog-name|folder|archive.tar.gz>
omnix plugin update [name...]
omnix plugin remove <name>
omnix plugin enable <name>
omnix plugin disable <name>
omnix plugin doctor [--json]
omnix plugin permissions <name> [--grant read] [--revoke network]
omnix plugin new <name> --kind command|panel|hook|agent|browser-tool [--dir PATH]
```

A command contribution is invoked as `omnix <plugin>:<command> [args...]`. A
bare plugin name works when the plugin has exactly one command.

## Tests expected from a plugin

At minimum, test that the manifest parses, every contribution has its required
fields, the runtime answers one request and exits, and errors are reported as
`{"error":{"message":…}}` rather than a crash. Omnix's own suite covers manifest
parsing, permission gates, sha256 verification, crash isolation, and call
timeouts (`cargo test`).
