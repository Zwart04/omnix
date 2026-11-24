# hello-command

Minimal **command** plugin for Omnix. It reads one JSON-RPC request from stdin,
prints one response, and exits.

```sh
omnix plugin install examples/plugins/hello-command
omnix plugin run hello-command:hello -- Ada
omnix plugin remove hello-command
```

The same command also appears in the Omnix command palette after the plugin is
installed and enabled.
