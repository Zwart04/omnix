# Perch

Tempat bertengger: worktree, sesi agent, dan browser dalam satu workbench Rust yang ringan.

Perch mengambil tiga inti Orca yang paling berguna, lalu membuang sisa bobotnya:

1. **Indexed history** — semua transkrip agent masuk SQLite FTS5, bisa dicari per frasa dan langsung memberi `resume` command.
2. **Sleep / resume worktree** — layout dan sesi agent disimpan per `paneKey`; workspace bisa tidur, lalu bangun dengan sesi agent yang sama.
3. **Handoff** — worktree baru + branch + brief + catatan outcome yang tahan crash, tanpa menunggu agent penerima selesai.

Browser dibuat bisa diganti:

- `native` — WebView bawaan aplikasi (WebKitGTK / WebView2 / WKWebView lewat `wry`). Ini yang menggambar piksel.
- `lightpanda` — headless CDP untuk agent, crawling, dan automation. Ringan, tapi **tidak menggambar layar**; `Page.captureScreenshot` hanya placeholder.

Alasannya sederhana: Lightpanda bukan pengganti browser visual. Ia pengganti *automation browser*. Jadi Perch memisahkan **engine yang dilihat manusia** dari **engine yang dipakai agent**.

## Status

Yang sudah jalan dan dites di repo ini:

- `history index/search/status` dengan SQLite FTS5, skema mirip Orca (`sessions`, `messages`, `messages_fts`, `identifiers`).
- `agent track` + `sleep` + `resume` dengan state JSON per workspace.
- `handoff` yang membuat `git worktree`, branch, brief, dan receipt `journaled | handed-to-terminal | not-delivered | unconfirmed`.
- `browser fetch/serve/doctor` untuk backend Lightpanda.
- 10 unit test, termasuk test yang membuat git repo dan worktree sungguhan.

Belum dibangun (sengaja, biar tidak jadi kerangka kosong):

- Shell GUI `wry` + multi-tab. Butuh `webkit2gtk-4.1` di mesin build.
- PTY injection untuk benar-benar mengirim prompt ke TUI agent. Sekarang brief ditulis ke file dan outcome jujur dilaporkan `unconfirmed`.
- CDP client penuh untuk `click/fill/snapshot`. Kontrak harness sudah ada di `harness/`, implementasinya menyusul.

## Install

Build dulu, lalu pasang. Linux butuh `pkg-config` dan paket dev WebKitGTK 4.1:

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev
cargo build --release

# user-only, ke ~/.local/bin
sh dist/linux/install.sh

# atau paket Debian
sh dist/linux/build-deb.sh
sudo apt install dist/linux/perch_*.deb
```

Windows memakai `dist/windows/build-zip.ps1` (butuh WebView2 Runtime), macOS
memakai `dist/macos/build-app.sh`. Paket `.deb`, zip, dan `.app` dibangun oleh
GitHub Actions pada tag `v*`. Panduan lengkap, termasuk state, troubleshooting,
dan batasan: `docs/app.md`.

Jalankan `perch` tanpa argumen untuk membuka jendela aplikasi. Verifikasi
end-to-end tanpa screenshot:

```bash
PERCH_HOME=/tmp/perch-selftest PERCH_SELFTEST=1 perch app   # berakhir "selftest: PASS"
```

Hanya jalur Linux yang sudah dibangun dan diuji di mesin ini; Windows dan macOS
belum dijalankan. Kalau `cc` di `PATH` bukan compiler sungguhan (mis. wrapper
zig), build dengan `CC=/usr/bin/cc RUSTFLAGS="-C linker=/usr/bin/cc"`.

## Quickstart

```bash
cd perch
cargo build --release
./target/release/perch --help

# 1. Index transkrip JSONL
./target/release/perch history index .perch/transcripts --agent codex --cwd "$PWD"

# 2. Cari dan dapatkan resume command
./target/release/perch history search "blank restore" --conversation
# codex  abc-123  score 4.21
#   resume: codex resume abc-123

# 3. Catat sesi agent ke workspace, lalu tidurkan
./target/release/perch agent track --agent codex --session abc-123 --pane tab:1
./target/release/perch sleep

# 4. Bangun lagi (cetak perintah; --run untuk spawn)
./target/release/perch resume
./target/release/perch resume --run

# 5. Handoff ke worktree + agent lain
./target/release/perch handoff fix-login --agent claude --prompt "Fix the login redirect"

# 6. Browser
PERCH_LIGHTPANDA=lightpanda ./target/release/perch browser fetch https://example.com
./target/release/perch browser serve --port 9222
```

Format transkrip yang dibaca (`*.jsonl`):

```json
{"role":"user","text":"where is resolveTerminalPath?","ts":"2026-01-01T00:00:00Z"}
{"role":"assistant","text":"It lives in terminal path resolution.","ts":"2026-01-01T00:00:01Z"}
{"role":"tool","text":"output...","ts":"2026-01-01T00:00:02Z"}
```

Semua state ada di `.perch/` di root project (atau `PERCH_HOME`):

```
.perch/
  history.sqlite          FTS5 index
  workspaces.json         sleeping sessions, per pane
  handoffs/<id>.json      receipt + brief path
  handoffs/<id>.md        prompt untuk agent penerima
  worktrees/<name>/       git worktree hasil handoff
  transcripts/            input indexing (opsional)
```

## Docs

- `docs/app.md` — panduan pengguna: instalasi per OS, isi jendela, terminal, browser, state, troubleshooting.
- `docs/architecture.md` — model data, invariant, dan roadmap.
- `docs/browser.md` — tutorial lengkap browser native + Lightpanda, command, harness, keamanan.
- `docs/orca-evidence.md` — bukti kode Orca untuk indexed history, resume worktree, handoff.
- `harness/browser-protocol.md` — kontrak tool browser untuk agent.
- `tutorial/index.html` — onboarding yang bisa dibuka di browser bawaan aplikasi.
- `evidence/rea-session.json` — bundle bukti REA (kalau diekspor).

## Test

```bash
cargo test
```

Test tidak memakai framework tambahan. Satu test membuat git repo, commit, worktree, brief, dan receipt; satu lagi menguji FTS5 phrase fallback + exclusion kolom tool.

## Nama

`Perch` = tempat burung bertengger. Pendek, bahasa Inggris, tidak metaforis berlebihan: worktree adalah tempat kerja bertengger, sesi bisa tidur dan kembali ke ranting yang sama.
