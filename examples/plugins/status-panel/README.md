# status-panel

Minimal **panel** plugin. The HTML runs in a sandboxed iframe (`allow-scripts`,
no `allow-same-origin`) with a strict CSP. It talks to the plugin process only
through the host's JSON `postMessage` bridge.

```sh
omnix plugin install examples/plugins/status-panel
```

Then open **Settings / Plugins** and press **Buka panel: Status panel**.

The panel script must be inline or shipped in the same plugin folder. External
network requests are blocked by CSP and by the missing `allow-same-origin`
sandbox token.
