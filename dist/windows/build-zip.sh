#!/bin/sh
# Package a cross-built Windows x64 binary for recipients without PowerShell.
set -eu

BINARY=${1:-target/x86_64-pc-windows-msvc/release/omnix.exe}
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ROOT_DIR=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)

cd "$ROOT_DIR"
[ -f "$BINARY" ] || { printf 'build-zip: binary not found: %s\n' "$BINARY" >&2; exit 1; }
command -v zip >/dev/null 2>&1 || command -v python3 >/dev/null 2>&1 || {
    printf 'build-zip: need zip or python3\n' >&2
    exit 1
}

STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT HUP INT TERM
cp "$BINARY" "$STAGE/omnix.exe"
cat >"$STAGE/README.txt" <<'EOF'
Omnix for Windows x64

Run omnix.exe. No installation is needed.
Requires the Microsoft Edge WebView2 Runtime, which ships with current
Windows 10 and Windows 11. If it is missing, install the Evergreen runtime
from Microsoft once; Omnix itself needs no other dependency.
EOF

OUT=$SCRIPT_DIR/omnix-windows-x64.zip
rm -f "$OUT"
if command -v zip >/dev/null 2>&1; then
    (cd "$STAGE" && zip -q -9 -r "$OUT" omnix.exe README.txt)
else
    python3 - "$STAGE" "$OUT" <<'PY'
import os, sys, zipfile
stage, out = sys.argv[1], sys.argv[2]
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as zf:
    for name in ("omnix.exe", "README.txt"):
        zf.write(os.path.join(stage, name), name)
PY
fi
printf 'Built %s\n' "$OUT"
