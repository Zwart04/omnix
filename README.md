# Omnix

Tempat bertengger: worktree, sesi agent, dan browser dalam satu workbench Rust yang ringan.

Omnix mengambil tiga inti Orca yang paling berguna, lalu membuang sisa bobotnya:

1. **Indexed history** — semua transkrip agent masuk SQLite FTS5, bisa dicari per frasa dan langsung memberi `resume` command.
2. **Sleep / resume worktree** — layout dan sesi agent disimpan per `paneKey`; workspace bisa tidur, lalu bangun dengan sesi agent yang sama.
3. **Handoff** — worktree baru + branch + brief + catatan outcome yang tahan crash, tanpa menunggu agent penerima selesai.

Browser dibuat bisa diganti:

- `native` — WebView bawaan aplikasi (WebKitGTK / WebView2 / WKWebView lewat `wry`). Ini yang menggambar piksel.
- `lightpanda` — headless CDP untuk agent, crawling, dan automation. Ringan, tapi **tidak menggambar layar**; `Page.captureScreenshot` hanya placeholder.

Alasannya sederhana: Lightpanda bukan pengganti browser visual. Ia pengganti *automation browser*. Jadi Omnix memisahkan **engine yang dilihat manusia** dari **engine yang dipakai agent**.

## Unduh

Rilis membawa tiga paket siap pasang (dibangun CI pada tag `v*`):

| Platform | Berkas | Cara pasang |
| --- | --- | --- |
| Linux x86_64 | `omnix_0.4.1_amd64.deb` | `sudo apt install ./omnix_0.4.1_amd64.deb` |
| Windows x64 | `omnix-windows-x64.zip` | ekstrak, jalankan `omnix.exe` (butuh WebView2 Runtime) |
| macOS 11+ arm64 | `Omnix-0.4.1-macos-arm64.tar.gz` | ekstrak, pindahkan `Omnix.app` ke `/Applications` |
| macOS 11+ Intel | `Omnix-0.4.1-macos-x64.tar.gz` | sama, pakai arsitektur yang sesuai |

<https://github.com/Zwart04/omnix/releases>

## Status

Yang sudah jalan dan dites di repo ini:

- `history index/search/status` dengan SQLite FTS5, skema mirip Orca (`sessions`, `messages`, `messages_fts`, `identifiers`).
- `agent track` + `sleep` + `resume` dengan state JSON per workspace.
- `handoff` yang membuat `git worktree`, branch, brief, dan receipt `journaled | handed-to-terminal | not-delivered | unconfirmed`.
- `history scan` yang membaca langsung transkrip di disk: `~/.codex/sessions`,
  `~/.claude/projects` (tanpa `subagents/`), dan `~/.local/share/opencode/opencode.db`
  read-only. Rescan hanya mem-parse berkas yang mtime-nya berubah.
- `browser fetch/serve/doctor` untuk backend Lightpanda.
- GUI satu jendela bergaya Orca: sidebar workspace dengan brand `Omnix`,
  satu baris ikon (Search, Handoff, Docs, Agents), lalu satu grup workspace
  yang bisa dilipat dan judulnya bisa diganti langsung. Grup itu hanya berisi
  folder yang kamu pin lewat `+ Project`; indexed history cuma memperkaya baris
  yang path-nya cocok (jumlah sesi, logo agent, waktu), tidak menambah folder.
  Baris proyek bisa dibuka menjadi daftar sesi agent, dan spinner kecil muncul
  selama ada pty hidup dengan cwd di folder itu. Tab strip di atas, panel
  `Agents` berisi indexed history semua agent, command palette (Ctrl+K).
  Tidak ada rail ganda; command palette dan setelan ada di header sidebar.
  Tab strip punya menu `+` (Terminal, Browser, Custom CLI, lalu **agent yang
  terdeteksi dan enabled saja** dengan logo aslinya), dan terminal memakai
  palet/font per OS.
- `+ Project` membuka folder picker bawaan Omnix, bukan dialog OS: rail Places
  (Home, Desktop, Documents, Downloads, Computer), breadcrumb yang bisa diklik,
  kolom Name/Size/Type/Modified dengan sort, dan tombol `Select folder`.
  Single click memilih, double click atau Enter masuk folder, Backspace naik,
  Escape batal; daftar dibatasi 400 entri agar tetap ringan. Ini membuat
  tampilan dan perilakunya sama di Linux, Windows, dan macOS. Folder disimpan di
  `~/.omnix/projects.json`, klik satu baris membuka terminal dengan cwd di sana,
  dan `×` di baris menghapusnya dari sidebar. Path manual tetap tersedia lewat
  `Add Project by Path` di palette. Indexed history hanya mengisi jumlah sesi
  dan logo agent untuk path yang cocok, tidak menambah baris.
- Terminal bisa di-split sampai empat pane dalam satu grup: `Ctrl+Shift+D`,
  menu `+`, palette `Split Terminal`, atau tombol split di sudut kanan atas
  pane. Dua pane tampil berdampingan, tiga pane jadi dua di atas plus satu
  lebar di bawah, empat pane jadi grid 2x2. Tiap pane punya pty sendiri; tombol
  close menutup pty itu dan pane terakhir kembali menjadi satu pane penuh.
  Tombol pane muncul saat hover atau focus-within supaya tidak menutupi teks,
  dan tombol split baru dinonaktifkan saat grup sudah berisi empat pane.
- Status bar menampilkan RAM terpakai dan window kuota agent yang paling ketat.
  Klik chip itu (atau palette `Usage and memory`) untuk modal usage: memori
  sistem plus per-provider plan, progress bar, dan waktu reset. Saat ini kuota
  Codex dibaca dari snapshot `rate_limits` terakhir di rollout
  `~/.codex/sessions`; provider yang tidak menyimpan data lokal tidak
  ditampilkan supaya tidak ada angka karangan. Data dipoll tiap 30 detik.
- Handoff modal ala Orca (`Focused handoff` / `Full session transcript`): brief
  ditulis ke `<cwd>/.omnix/handoffs/`, lalu sesi agent target dibuka di cwd yang
  sama. Panel `Agents` default tertutup dan index baru dipindai saat dibuka.
  Index disimpan di `~/.omnix/history.sqlite`; rescan hanya mem-parse transkrip
  yang baru atau berubah, dan baris meta menampilkan
  `N updated · M unchanged · X new messages` supaya penghematannya terlihat.
- Terminal fokus otomatis saat tab dibuka (tanpa klik), font UI 14px, dan
  terminal 14px / line-height 1.2 (metrik Orca) dengan cursor block, fontWeight
  500/700, font chain DejaVu Sans Mono lalu Nerd Font, dan renderer WebGL supaya
  glyph tetap tajam; fallback ke renderer DOM kalau konteks GL tidak ada.
- Setiap pty dipaksa `TERM=xterm-256color`, `COLORTERM=truecolor`,
  `TERM_PROGRAM=omnix` supaya TUI agent (opencode, codex, antigravity) tampil
  berwarna, bukan monokrom.
- Settings / Agents ala Orca (gear di header sidebar atau menu `+`): nav kiri,
  pencarian, breadcrumb `Settings / Agents`, pilihan `New agents permissions`,
  lalu daftar `Installed` dan `Available to install`. Tiap baris punya command,
  tag `CLI`, toggle enable, dan menu launch options untuk argumen/environment.
  Nilai tersimpan otomatis di `.omnix/agents.json` (per agent) dan
  `.omnix/agent-defaults.json` (permissions default), lalu dipakai saat agent
  diluncurkan atau di-resume. Full access memakai flag resmi tiap CLI
  (`claude`, `codex`, `gemini`/`qwen-code`, `copilot`); CLI yang tidak punya
  flag dikenal tidak diberi tebakan. Deteksi mencari `PATH` lalu direktori
  install per-user (`~/.opencode/bin`, `~/.local/bin`, `~/.bun/bin`,
  `~/.cargo/bin`, nvm, npm global).
- WebView mengambil fokus keyboard sendiri saat jendela aktif (WebKitGTK di
  compositor tertentu sempat membiarkan fokus di body, jadi ketikan pty hilang).
- 20 unit test, termasuk test yang membuat git repo dan worktree sungguhan.

Belum dibangun (sengaja, biar tidak jadi kerangka kosong):

- Prompt otomatis ke TUI agent penerima: sekarang brief ditulis ke file dan
  path-nya dicetak di tab agent baru; agent tetap mulai dari prompt kosong.
- Headless screenshot Lightpanda: `Page.captureScreenshot` masih placeholder.
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
sudo apt install dist/linux/omnix_*.deb
```

Windows memakai `dist/windows/build-zip.ps1` (butuh WebView2 Runtime), macOS
memakai `dist/macos/build-app.sh`. Paket `.deb`, zip, dan `.app` dibangun oleh
GitHub Actions pada tag `v*`. Panduan lengkap, termasuk state, troubleshooting,
dan batasan: `docs/app.md`.

Jalankan `omnix` tanpa argumen untuk membuka jendela aplikasi. Verifikasi
end-to-end tanpa screenshot:

```bash
OMNIX_HOME=/tmp/omnix-selftest OMNIX_SELFTEST=1 omnix app   # berakhir "selftest: PASS"
```

Linux: dibangun, dipasang dari `.deb`, dan diuji penuh di mesin ini. Windows:
`.exe` di-cross-compile (`cargo xwin build --release --target
x86_64-pc-windows-msvc`) dan diperiksa sebagai PE32+ GUI, tetapi belum
dijalankan di Windows sungguhan. macOS: dibangun oleh CI, belum dijalankan. Kalau `cc` di `PATH` bukan compiler sungguhan (mis. wrapper
zig), build dengan `CC=/usr/bin/cc RUSTFLAGS="-C linker=/usr/bin/cc"`.

## Quickstart

```bash
cd omnix
cargo build --release
./target/release/omnix --help

# 1. Index transkrip JSONL
./target/release/omnix history index .omnix/transcripts --agent codex --cwd "$PWD"

# 2. Cari dan dapatkan resume command
./target/release/omnix history search "blank restore" --conversation
# codex  abc-123  score 4.21
#   resume: codex resume abc-123

# 3. Catat sesi agent ke workspace, lalu tidurkan
./target/release/omnix agent track --agent codex --session abc-123 --pane tab:1
./target/release/omnix sleep

# 4. Bangun lagi (cetak perintah; --run untuk spawn)
./target/release/omnix resume
./target/release/omnix resume --run

# 5. Handoff ke worktree + agent lain
./target/release/omnix handoff fix-login --agent claude --prompt "Fix the login redirect"

# 6. Browser
OMNIX_LIGHTPANDA=lightpanda ./target/release/omnix browser fetch https://example.com
./target/release/omnix browser serve --port 9222
```

Format transkrip yang dibaca (`*.jsonl`):

```json
{"role":"user","text":"where is resolveTerminalPath?","ts":"2026-01-01T00:00:00Z"}
{"role":"assistant","text":"It lives in terminal path resolution.","ts":"2026-01-01T00:00:01Z"}
{"role":"tool","text":"output...","ts":"2026-01-01T00:00:02Z"}
```

Semua state ada di `.omnix/` di root project (atau `OMNIX_HOME`):

```
.omnix/
  history.sqlite          FTS5 index
  projects.json           folder yang ditampilkan di sidebar
  agents.json             argumen, environment, enabled per agent
  agent-defaults.json     permissions default (full access) untuk agent baru
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
node app/selftest.cjs
```

Test tidak memakai framework tambahan. Satu test membuat git repo, commit, worktree, brief, dan receipt; satu lagi menguji FTS5 phrase fallback + exclusion kolom tool. `app/selftest.cjs` men-stub WebView lalu memeriksa RPC, render sidebar/picker/history, split pane, dan resume session tanpa membuka jendela.

## Nama

`Omnix` = tempat burung bertengger. Pendek, bahasa Inggris, tidak metaforis berlebihan: worktree adalah tempat kerja bertengger, sesi bisa tidur dan kembali ke ranting yang sama.
