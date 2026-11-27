#!/usr/bin/env bash
# Run the test suite while proving the real home data is untouched.
# Records the mode and content of ~/.claude.json, and a fingerprint of the whole
# ~/.omnix tree, before and after `cargo test`; fails if anything moved.
set -euo pipefail
cd "$(dirname "$0")/.."

CLAUDE_JSON="${HOME}/.claude.json"
OMNIX_HOME="${HOME}/.omnix"

fingerprint_file() {
  if [ -e "$1" ]; then
    printf 'mode=%s sha256=%s\n' \
      "$(stat -c '%a' "$1")" \
      "$(sha256sum "$1" | awk '{print $1}')"
  else
    printf 'absent\n'
  fi
}

# Path, type and mode of every entry plus the content of every file.
fingerprint_tree() {
  if [ ! -e "$1" ]; then
    printf 'absent\n'
    return
  fi
  {
    find "$1" -mindepth 0 -printf '%P %y %m\n' | LC_ALL=C sort
    find "$1" -type f -print0 | LC_ALL=C sort -z | xargs -0 -r sha256sum
  } | sha256sum | awk '{print $1}'
}

fingerprint() {
  printf 'claude.json=%s omnix=%s\n' \
    "$(fingerprint_file "$CLAUDE_JSON")" \
    "$(fingerprint_tree "$OMNIX_HOME")"
}

export PATH="/usr/bin:/bin:${HOME}/.cargo/bin"
before="$(fingerprint)"
echo "before: $before"
cargo test -- --test-threads=1
after="$(fingerprint)"
echo "after:  $after"
if [ "$before" != "$after" ]; then
  echo "FAIL: cargo test changed real home data" >&2
  exit 1
fi
echo "OK: ~/.claude.json and ~/.omnix are unchanged by the suite"
