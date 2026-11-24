#!/usr/bin/env python3
"""Omnix plugin example: answer a panel request with JSON.

The panel only receives this JSON over the narrow postMessage bridge. The
plugin cannot read Omnix UI state, agent tokens, or cookies.
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
    result = {
        "ok": True,
        "method": method,
        "plugin": os.environ.get("OMNIX_PLUGIN_NAME", "status-panel"),
        "api": os.environ.get("OMNIX_PLUGIN_API", "1"),
        "dataDir": os.environ.get("OMNIX_PLUGIN_DATA", ""),
        "received": params,
    }
    print(json.dumps({"jsonrpc": "2.0", "id": request.get("id", 1), "result": result}), flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
