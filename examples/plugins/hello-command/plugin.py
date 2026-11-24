#!/usr/bin/env python3
"""Omnix plugin example: one stdin request, one stdout response, then exit.

This process is started only when the owner runs the command. It never stays
alive, opens a port, or reads Omnix state.
"""
import json
import os
import sys


def main():
    line = sys.stdin.readline()
    if not line:
        return 1
    request = json.loads(line)
    method = request.get("method", "")
    params = request.get("params", {}) or {}
    name = os.environ.get("OMNIX_PLUGIN_NAME", "hello-command")
    args = params.get("args", [])
    if method == "command":
        who = " ".join(str(arg) for arg in args).strip() or "Omnix"
        result = {
            "stdout": "Hello, " + who + "!\n",
            "plugin": name,
            "permissions": os.environ.get("OMNIX_PLUGIN_PERMISSIONS", ""),
        }
    else:
        result = {"ok": True, "method": method, "plugin": name}
    print(json.dumps({"jsonrpc": "2.0", "id": request.get("id", 1), "result": result}), flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
