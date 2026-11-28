#!/usr/bin/env bash
# Skrip pemeriksaan otomatis Prompt 14 (dijalankan sesudah OpenCode selesai).
# - Cek RAM tersedia >= 1500 MB sebelum build/test
# - Penjaga sidik + izin ~/.claude.json dan ~/.omnix (sebelum & sesudah)
# - cargo fmt --check
# - cargo clippy --all-targets -- -D warnings
# - cargo test -- --test-threads=1 (+ app/selftest.cjs bila node ada)
# - Cek otomatis non-GUI untuk 4 butir Prompt 14 (repo sementara terisolasi)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO_ROOT"

export PATH="/usr/bin:/bin:${HOME}/.local/bin:${HOME}/.cargo/bin:${PATH}"

PASS_COUNT=0
FAIL_COUNT=0
WARN_COUNT=0

ok()   { echo "[PASS] $*"; PASS_COUNT=$((PASS_COUNT + 1)); }
fail() { echo "[FAIL] $*" >&2; FAIL_COUNT=$((FAIL_COUNT + 1)); }
warn() { echo "[WARN] $*"; WARN_COUNT=$((WARN_COUNT + 1)); }

echo "=== Omnix Prompt 14 — periksa.sh ==="
echo "Waktu : $(date)"
echo "Repo  : $REPO_ROOT"
echo "HEAD  : $(git rev-parse --short HEAD) ($(git log -1 --pretty=%s))"
echo ""

# 1. Penjaga RAM minimal 1500 MB
AVAIL_MB="$(free -m | awk '/^Mem:/ {print $7}')"
echo "RAM tersedia: ${AVAIL_MB} MB"
if [ "${OMNIX_SKIP_RAM_CHECK:-0}" != "1" ] && [ "${AVAIL_MB:-0}" -lt 1500 ]; then
  echo "ABORT: RAM tersedia ${AVAIL_MB} MB < 1500 MB. Tutup proses lain dulu." >&2
  exit 2
fi
ok "RAM tersedia ${AVAIL_MB} MB >= 1500 MB"

# 2. Sidik + izin ~/.claude.json dan ~/.omnix sebelum semua pengujian
CLAUDE_JSON="${HOME}/.claude.json"
OMNIX_HOME_REAL="${HOME}/.omnix"

fingerprint_file() {
  if [ -e "$1" ]; then
    printf 'mode=%s sha256=%s' \
      "$(stat -c '%a' "$1")" \
      "$(sha256sum "$1" | awk '{print $1}')"
  else
    printf 'absent'
  fi
}

fingerprint_tree() {
  if [ ! -e "$1" ]; then
    printf 'absent'
    return
  fi
  {
    find "$1" -mindepth 0 -printf '%P %y %m\n' | LC_ALL=C sort
    find "$1" -type f -print0 | LC_ALL=C sort -z | xargs -0 -r sha256sum
  } | sha256sum | awk '{print $1}'
}

CLAUDE_BEFORE="$(fingerprint_file "$CLAUDE_JSON")"
OMNIX_BEFORE="$(fingerprint_tree "$OMNIX_HOME_REAL")"
echo "Guard sebelum: claude.json=[$CLAUDE_BEFORE] omnix=[$OMNIX_BEFORE]"

# 3. cargo fmt --check
echo ""
echo "--- [1/6] cargo fmt --check ---"
if cargo fmt --check; then
  ok "cargo fmt --check"
else
  fail "cargo fmt --check"
fi

# 4. cargo clippy --all-targets -- -D warnings
echo ""
echo "--- [2/6] cargo clippy --all-targets -- -D warnings ---"
if cargo clippy --all-targets -- -D warnings; then
  ok "cargo clippy --all-targets -- -D warnings"
else
  fail "cargo clippy --all-targets -- -D warnings"
fi

# 5. cargo test
echo ""
echo "--- [3/6] cargo test -- --test-threads=1 ---"
if cargo test -- --test-threads=1; then
  ok "cargo test -- --test-threads=1"
else
  fail "cargo test -- --test-threads=1"
fi

# 5b. Frontend JS selftest (non-GUI, via Node)
if command -v node >/dev/null 2>&1 && [ -f app/selftest.cjs ]; then
  echo ""
  echo "--- [3b/6] node app/selftest.cjs ---"
  if node app/selftest.cjs; then
    ok "node app/selftest.cjs"
  else
    fail "node app/selftest.cjs"
  fi
else
  warn "node tidak tersedia; lewati app/selftest.cjs"
fi

# 6. Uji protokol PTY mock (tanpa kuota Codex/Claude/Agy)
echo ""
echo "--- [4/6] Uji protokol PTY mock (uji-pty-mock.py) ---"
if python3 "$REPO_ROOT/docs/uji-prompt-14/uji-pty-mock.py"; then
  ok "uji-pty-mock.py"
else
  fail "uji-pty-mock.py"
fi

# 7. Cek statis + integrasi CLI non-GUI terhadap 4 butir Prompt 14
echo ""
echo "--- [5/6] Cek statis & integrasi CLI Prompt 14 (tanpa GUI) ---"

# 7a. Butir 1: Tidak boleh ada single-write `\x1b[201~\r` sekaligus di src/gui.rs
if grep -n '\\x1b\[201~\\r' src/gui.rs; then
  fail "Butir 1: src/gui.rs masih menulis \\x1b[201~\\r dalam satu write_all (tempelan & Enter belum dipisah)"
else
  ok "Butir 1: tidak ada single-write \\x1b[201~\\r di src/gui.rs"
fi

# 7b. Butir 3: force_headless tidak boleh memaksa headless untuk --after/--at/--min-free-mb saat GUI hidup
if grep -n -A6 'let force_headless =' src/main.rs | grep -q -E 'record\.(after|start_at|min_free_mb)\.is_some'; then
  fail "Butir 3: src/main.rs masih memasukkan after/start_at/min_free_mb ke force_headless"
else
  ok "Butir 3: force_headless tidak lagi memaksa headless untuk after/start_at/min_free_mb"
fi

# 7c. Butir 4: fallback cabang bawaan tidak boleh lagi "omnix/"
if grep -n '"omnix/"' src/handoff.rs; then
  fail "Butir 4: src/handoff.rs masih memiliki fallback cabang \"omnix/\""
else
  ok "Butir 4: src/handoff.rs tidak memakai fallback \"omnix/\""
fi
if grep -n 'format!("omnix/' src/handoff.rs; then
  fail "Butir 4: src/handoff.rs masih hardcode format!(\"omnix/...\")"
else
  ok "Butir 4: tidak ada hardcode format!(\"omnix/...\") di src/handoff.rs"
fi

# 7d. Integrasi CLI di temp dir terisolasi (OMNIX_HOME & HOME palsu tidak menyentuh home asli)
TMP_WORK="$(mktemp -d /tmp/omnix-uji-p14-XXXXXX)"
trap 'rm -rf "$TMP_WORK"' EXIT

BIN="$REPO_ROOT/target/debug/omnix"
# Always rebuild: a stale binary would test the old branch prefix and report a
# false failure (or worse, a false pass).
echo "Membangun binary debug untuk uji CLI..."
cargo build --bin omnix

if [ -x "$BIN" ]; then
  FAKE_HOME="$TMP_WORK/home"
  FAKE_RUN="$TMP_WORK/run"
  TEST_REPO="$TMP_WORK/repo"
  mkdir -p "$FAKE_HOME" "$FAKE_RUN" "$TEST_REPO"
  git -C "$TEST_REPO" init -q
  git -C "$TEST_REPO" config user.email "uji@example.com"
  git -C "$TEST_REPO" config user.name "Uji Omnix"
  echo "hello" > "$TEST_REPO/README.md"
  git -C "$TEST_REPO" add README.md
  git -C "$TEST_REPO" commit -q -m "init"

  # Uji 4a: cabang bawaan tanpa --branch / --role / config
  OUT_DEF="$(HOME="$FAKE_HOME" OMNIX_HOME="$TEST_REPO" OMNIX_RUN_DIR="$FAKE_RUN" \
    "$BIN" handoff uji-bawaan --repo "$TEST_REPO" --base HEAD --agent codex --prompt "tes bawaan" 2>&1 || true)"
  BR_DEF="$(printf '%s\n' "$OUT_DEF" | awk '/^branch:/ {print $2}')"
  if [ -n "$BR_DEF" ] && [[ "$BR_DEF" != omnix/* ]]; then
    ok "Butir 4 (CLI): cabang bawaan tanpa kata omnix -> $BR_DEF"
  else
    fail "Butir 4 (CLI): cabang bawaan masih '$BR_DEF' (keluaran: $OUT_DEF)"
  fi

  # Uji 4b: --branch eksplisit
  OUT_BR="$(HOME="$FAKE_HOME" OMNIX_HOME="$TEST_REPO" OMNIX_RUN_DIR="$FAKE_RUN" \
    "$BIN" handoff uji-kustom --repo "$TEST_REPO" --base HEAD --branch fix/tautan-uji --agent codex --prompt "tes kustom" 2>&1 || true)"
  BR_KUSTOM="$(printf '%s\n' "$OUT_BR" | awk '/^branch:/ {print $2}')"
  if [ "$BR_KUSTOM" = "fix/tautan-uji" ]; then
    ok "Butir 4 (CLI): --branch fix/tautan-uji dihormati"
  else
    fail "Butir 4 (CLI): --branch gagal (dapat '$BR_KUSTOM')"
  fi

  # Uji 4c: .omnix/config branch_prefix
  mkdir -p "$TEST_REPO/.omnix"
  printf 'branch_prefix = tim/\n' > "$TEST_REPO/.omnix/config"
  OUT_CFG="$(HOME="$FAKE_HOME" OMNIX_HOME="$TEST_REPO" OMNIX_RUN_DIR="$FAKE_RUN" \
    "$BIN" handoff uji-cfg --repo "$TEST_REPO" --base HEAD --agent codex --prompt "tes cfg" 2>&1 || true)"
  BR_CFG="$(printf '%s\n' "$OUT_CFG" | awk '/^branch:/ {print $2}')"
  if [ "$BR_CFG" = "tim/uji-cfg" ]; then
    ok "Butir 4 (CLI): .omnix/config branch_prefix=tim/ -> $BR_CFG"
  else
    fail "Butir 4 (CLI): .omnix/config branch_prefix gagal (dapat '$BR_CFG')"
  fi
else
  fail "Binary $BIN tidak ditemukan"
fi

# 8. Penjaga sidik + izin ~/.claude.json dan ~/.omnix sesudah semua pengujian
echo ""
echo "--- [6/6] Penjaga ~/.claude.json dan ~/.omnix ---"
CLAUDE_AFTER="$(fingerprint_file "$CLAUDE_JSON")"
OMNIX_AFTER="$(fingerprint_tree "$OMNIX_HOME_REAL")"
echo "Guard sesudah: claude.json=[$CLAUDE_AFTER] omnix=[$OMNIX_AFTER]"

if [ "$CLAUDE_BEFORE" = "$CLAUDE_AFTER" ]; then
  ok "~/.claude.json utuh (sidik + izin identik)"
else
  fail "~/.claude.json BERUBAH! sebelum=[$CLAUDE_BEFORE] sesudah=[$CLAUDE_AFTER]"
fi

if [ "$OMNIX_BEFORE" = "$OMNIX_AFTER" ]; then
  ok "~/.omnix utuh (sidik pohon identik)"
else
  fail "~/.omnix BERUBAH! sebelum=[$OMNIX_BEFORE] sesudah=[$OMNIX_AFTER]"
fi

echo ""
echo "========================================"
echo "Ringkasan: PASS=$PASS_COUNT FAIL=$FAIL_COUNT WARN=$WARN_COUNT"
echo "========================================"
if [ "$FAIL_COUNT" -gt 0 ]; then
  exit 1
fi
exit 0
