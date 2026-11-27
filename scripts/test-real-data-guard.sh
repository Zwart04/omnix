#!/usr/bin/env bash
# Run the test suite while proving the real Claude config is untouched.
# Records the mode and content hash of ~/.claude.json before and after.
set -euo pipefail
cd "$(dirname "$0")/.."
CLAUDE_JSON="${HOME}/.claude.json"

fingerprint() {
  if [ -e "$CLAUDE_JSON" ]; then
    printf 'mode=%s sha256=%s\n' \
      "$(stat -c '%a' "$CLAUDE_JSON")" \
      "$(sha256sum "$CLAUDE_JSON" | awk '{print $1}')"
  else
    printf 'absent\n'
  fi
}

export PATH="/usr/bin:/bin:${HOME}/.cargo/bin"
before="$(fingerprint)"
echo "before: $before"
cargo test -- --test-threads=1
after="$(fingerprint)"
echo "after:  $after"
if [ "$before" != "$after" ]; then
  echo "FAIL: cargo test changed $CLAUDE_JSON" >&2
  exit 1
fi
echo "OK: $CLAUDE_JSON is unchanged"