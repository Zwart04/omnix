# Omnix App — panduan pengguna

Dokumen ini untuk orang yang memakai Omnix, bukan yang mengubah kodenya.
Kontrak internal jendela ada di `docs/gui.md`; arsitektur data ada di
`docs/architecture.md`.

> **Catatan jujur.** Paket dan skrip di `dist/` ditulis untuk Linux, Windows,
> dan macOS, tetapi hanya jalur Linux yang dibangun dan diuji di mesin ini.
> Jalur Windows dan macOS belum dijalankan. Jendela aplikasi sudah berfungsi di
> Linux: `omnix` tanpa argumen membuka jendela, terminal memakai PTY asli, dan
> seluruh alur IPC lulus selftest otomatis (lihat bagian Selftest).

Tampilan memakai bahasa visual minimalis dengan kepadatan Orca: font sistem
per platform, chrome solid, satu aksen biru, ikon folder hangat, sudut kecil
yang konsisten, dan pemisah tipis. Tidak ada gradient dekoratif, label ALL CAPS, atau pill di
tombol dan chip. Aturan lengkapnya ada di `DESIGN.md`. Palet terminal tetap
palet ANSI masing-masing platform supaya warna TUI agent tidak berubah.
Overlay seperti menu dan palette memakai permukaan opaque; `backdrop-filter`
tidak dipakai karena WebKitGTK pernah salah memposisikan lapisan blur sehingga
menu tergambar di sidebar dan tidak menerima klik.

## Pemantauan RAM

Chip di status bar menunjukkan dua angka: total proses Omnix (`App`) dan total
semua terminal/agent yang dijalankan dari Omnix (`AI`). Klik chip atau buka
palette `Usage and Memory` untuk rincian:

- total process tree Omnix beserta jumlah proses;
- app shell + WebKit dihitung terpisah dari terminal;
- satu baris per pane terminal: label, PID, cwd, jumlah proses, dan nama proses
  anak yang paling banyak memakai RAM;
- memori sistem dan kuota provider yang menyimpan data lokal.

Angka AI menjumlahkan seluruh keturunan proses shell PTY, jadi agent, language
server, dan child process lain ikut masuk. Snapshot berhenti saat jendela
tersembunyi dan diperbarui tiap 3,5 detik di Linux atau 8 detik di macOS/Windows.

`OMNIX_AI_RAM_MB` menetapkan budget RAM untuk proses AI. Bila variabel itu
tidak diset, budget default-nya seperempat RAM sistem. Saat angka AI melewati
budget, chip berubah amber dan Omnix menampilkan satu toast per pelanggaran
(bukan berulang tiap polling). Ini pengingat, bukan pagar otomatis: Omnix tidak
membunuh agent tanpa izin. Untuk menurunkan angka:

1. tutup pane agent yang idle;
2. `omnix browser stop` untuk mematikan Lightpanda bersama;
3. cek `omnix diagnostics memory --json` sebelum membuka tool berat lain;
   dijalankan dari terminal Omnix, perintah ini mengukur seluruh tree app,
   bukan hanya proses CLI-nya.

Di Linux renderer DMA-BUF WebKit aktif secara default supaya compositing tetap
di GPU dan mengetik/scrolling mulus; fallback software bisa memakan CPU sekitar
10x lebih tinggi. `OMNIX_NO_DMABUF=1` mematikannya (hemat sekitar 100 MB saat
startup) bila driver GPU lama bermasalah dengan DMA-BUF.
Omnix tidak memaksa WebKit membuang cache saat idle: trim bawaan WebKit hanya
bereaksi di bawah tekanan memori sistem, dan menurunkan limitnya sendiri membuat
engine terus membuang cache (lebih lambat) demi penghematan yang kecil.

Setiap terminal juga mendapat `OMNIX_HARNESS_GUIDE` (path
`.omnix/agent-guide.md`), `OMNIX_BROWSER=lightpanda`, `OMNIX_LIGHTPANDA`, dan
`OMNIX_BROWSER_MCP` (binary Omnix untuk `browser mcp`).

## Instalasi

Omnix adalah satu binary Rust bernama `omnix`. Subcommand tetap berjalan di
terminal; menjalankan `omnix` tanpa argumen (atau `omnix app`) membuka jendela
aplikasi. Build tanpa GUI tetap mungkin: `cargo build --no-default-features`.

### Linux

Dependensi build (Debian/Ubuntu):

```bash
sudo apt update
sudo apt install -y build-essential pkg-config \
  libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev
```

Build:

```bash
cd omnix
cargo build --release
./target/release/omnix --help
```

Pasang untuk user sendiri (tanpa sudo):

```bash
sh dist/linux/install.sh
```

Skrip menyalin binary ke `~/.local/bin/omnix`, entri desktop ke
`~/.local/share/applications/omnix.desktop`, dan ikon ke
`~/.local/share/icons/hicolor/scalable/apps/omnix.svg`. Kalau
`~/.local/bin` belum ada di `PATH`, skrip mencetak baris `export PATH` yang
perlu ditambahkan. Perintah berikutnya juga dicetak, misalnya:

```bash
omnix --help
```

Untuk memasang system-wide di `/usr/local`:

```bash
sh dist/linux/install.sh --system
```

Itu memakai `sudo` hanya kalau kamu bukan root. Binary lain bisa dipilih dengan
`--binary /path/ke/omnix`.

Membuat paket `.deb` (butuh `dpkg-deb`, tanpa `cargo-deb`):

```bash
sh dist/linux/build-deb.sh
sudo apt install dist/linux/Omnix-<versi>-Linux-Debian-Ubuntu-x64.deb
```

Paket memasang `/usr/bin/omnix`, desktop entry, ikon, dan mendeklarasikan
`Depends: libwebkit2gtk-4.1-0, libgtk-3-0, libsoup-3.0-0`. Arsitektur diambil
dari `dpkg --print-architecture`.

Untuk Arch Linux, `dist/linux/build-arch.sh` membangun
`Omnix-<versi>-Linux-Arch-x64.pkg.tar.zst` dengan isi yang sama; pasang memakai
`sudo pacman -U dist/linux/Omnix-*-Linux-Arch-x64.pkg.tar.zst`.

Mencopot pemasangan manual:

```bash
rm -f ~/.local/bin/omnix \
  ~/.local/share/applications/omnix.desktop \
  ~/.local/share/icons/hicolor/scalable/apps/omnix.svg
```

### Windows

Build di PowerShell:

```powershell
cd omnix
cargo build --release
./dist/windows/build-installer.ps1
```

Skrip memakai `target\release\omnix.exe`. Untuk build silang dari Linux
(mis. CI atau mesin tanpa Windows), jalankan
`cargo xwin build --release --target x86_64-pc-windows-msvc`; skrip otomatis
memakai `target\x86_64-pc-windows-msvc\release\omnix.exe` kalau berkas
release biasa tidak ada. Hasilnya
`dist\windows\build\Omnix-<versi>-Windows-x64-Setup.exe`.

Installer memakai Inno Setup 6 (skrip memasangnya lewat Chocolatey bila belum
ada), memasang per-user tanpa prompt admin, dan menyertakan
`MicrosoftEdgeWebView2Setup.exe`. Runtime **Microsoft Edge WebView2** hanya
dipasang kalau registry belum melihat versi Evergreen; kalau unduhan runtime
gagal, installer berhenti dengan pesan yang jelas, bukan membiarkan aplikasi
gagal start. Build ini belum ditandatangani, jadi SmartScreen bisa meminta
konfirmasi.

### macOS

```bash
cd omnix
cargo build --release
sh dist/macos/build-app.sh --binary <arm64-binary> --binary <x64-binary>
```

Dua `--binary` digabung dengan `lipo` menjadi satu aplikasi universal. Hasilnya
`Omnix.app` di `dist/macos/build/` dan
`dist/macos/Omnix-<versi>-macOS-universal.dmg` berisi aplikasi plus pintasan
`Applications`. Satu binary tanpa `--binary` ganda menghasilkan
`...-macOS.dmg` (atau `...-macOS-arm64.dmg` / `...-macOS-x64.dmg` kalau
`OMNIX_ARCH` diset).
Bundle memakai
`CFBundleIdentifier dev.omnix.app` dan `LSMinimumSystemVersion 11.0`. Skrip
melakukan ad-hoc codesign kalau `codesign` tersedia; itu bukan notarization.
Gatekeeper di Mac lain tetap bisa menolak aplikasi yang belum dinotarisasi
(klik kanan → Open, atau `xattr -dr com.apple.quarantine Omnix.app`).

## Update

`updateCheck` memanggil `https://api.github.com/repos/Zwart04/omnix/releases/latest`
di thread latar. Kalau tag lebih baru dari `CARGO_PKG_VERSION`, status bar
menampilkan chip `Update <versi>`; klik chip memanggil `updateInstall`.
Pemeriksaan berjalan sekali saat start, tiap enam jam, dan saat versi di kiri
bawah sidebar diklik.

Aset yang dipilih mengikuti platform:

- Windows: `Omnix-<versi>-Windows-x64-Setup.exe`, dijalankan senyap dengan
  `/SILENT /CLOSEAPPLICATIONS /NORESTART`; installer menutup proses lama dan
  meluncurkan Omnix lagi.
- macOS: DMG dibuka dan aplikasi menutup diri; pengguna menyeret Omnix ke
  Applications.
- Linux: binary `Omnix-<versi>-Linux-x64` menggantikan executable yang sedang
  berjalan bila direktori itu bisa ditulis, lalu proses baru dijalankan. Kalau
  tidak (mis. `/usr/bin` milik root), paket `.deb` / `.pkg.tar.zst` dibuka lewat
  `xdg-open`.

Terminal juga bisa: `omnix update --check` dan `omnix update`. Tidak ada
signature selain TLS GitHub; paket tidak diverifikasi dengan checksum
terpisah.

## Pertama kali dibuka dan dukungan

Workspace baru membuka wizard empat langkah. Wizard ini bukan gerbang: semua
langkah boleh dilewati, dan Omnix tetap bisa dipakai sebagai terminal biasa.

1. **Welcome** — tiga hal yang membedakan Omnix: satu jendela, plugin/browser
   on demand, dan local-first tanpa akun.
2. **Open a project** — folder kerja opsional. Tombol **Browse** memakai dialog
   folder bawaan OS; path manual juga bisa diketik. Project yang dipilih
   langsung ditambahkan ke sidebar dan dibuka setelah wizard selesai.
3. **Choose your agents** — daftar CLI agent yang terdeteksi. Agent yang tidak
   terpasang tampil nonaktif; pilihan disimpan lewat `agentEnable` dan bisa
   diubah lagi di Settings → Agents.
4. **You are set** — ringkasan pintasan (`Ctrl K`, `Ctrl Shift D`, `Ctrl \``),
   kartu **Support Omnix**, dan lisensi GPL-3.0-or-later.

State wizard ada di `.omnix/setup.json` (`completed_at`, `version`). File itu
hanya ditulis saat wizard diselesaikan atau dilewati. Wizard muncul kalau file
tersebut belum ada **dan** `projects.json` masih kosong, jadi pengguna lama yang
sudah punya project tidak terganggu. Untuk membukanya lagi: **Settings → About &
Support → Run setup again**, atau palette **Setup Omnix Again**.

Pane **About & Support** juga menampilkan versi, lisensi, status setup, tombol
**GitHub**, dan tombol **Support on Tako**. Tautan dukungan selalu
`https://tako.id/Zwart04/gift`; RPC `supportOpen` membuka URL konstan itu lewat
`xdg-open` (Linux), `open` (macOS), atau `start` (Windows), bukan launcher
bebas. Tidak ada paywall dan tidak ada fitur yang dikunci.

## Akun agent (profil)

Satu CLI bisa punya beberapa akun. Tulis nama agent diikuti profil:
`codex:akun2`, `claude:akun2`, `antigravity:akun2`. Nama profil hanya huruf
kecil, angka, dan tanda hubung, 1–48 karakter. Tanpa titik dua, perilakunya
sama seperti sebelumnya.

Contoh dua akun Codex berjalan bersamaan, masing-masing dengan login sendiri:

```bash
# .omnix/agents.json boleh kosong; profil dibuat dari Settings
omnix handoff tugas-a --repo ~/proyek --prompt "Baca brief ini" \
  --agent codex:akun2 --run
omnix handoff tugas-b --repo ~/proyek --prompt "Baca brief ini" \
  --agent codex:akun3 --run
```

Codex menerima `CODEX_HOME` berbeda, Claude menerima `CLAUDE_CONFIG_DIR`
berbeda. Folder default:

```
~/.omnix/profiles/codex/akun2
~/.omnix/profiles/claude/akun2
```

Folder itu bisa diganti per profil lewat field `Login folder` di Settings atau
`configDir` di `.omnix/agents.json`. Profil mewarisi argumen dan environment
agent dasarnya; argumen/environment profil ditambahkan setelahnya.

Login dari GUI: Settings → Agents → `Add account profile`, lalu tombol
`Login profile`. Omnix membuka terminal dengan environment profil dan
menjalankan alur login resmi CLI:

- Codex: `codex login`
- Claude: `claude`, lalu Omnix mengetik `/login` setelah TUI siap
- Antigravity: `agy`, lalu layar login bawaannya

Omnix tidak membaca, menyalin, atau menyimpan token. Tab profil diberi badge
huruf berwarna; badge yang sama muncul di panel Agents dan pilihan handoff.
`omnix usage --json` menampilkan kuota per profil, dan `omnix history scan`
mengindeks transkrip tiap folder profil dengan nama `agent` berisi
`codex:akun2`.

Claude membagikan aturan lewat symlink: `~/.claude/skills` dan
`~/.claude/CLAUDE.md` ditautkan ke folder profil. Memori project
(`projects/`) terpisah secara default; centang `Share project memory` untuk
menautkannya ke akun utama.

**Batasan Antigravity.** `agy` belum punya flag/env resmi untuk memindah folder
config, jadi profil `antigravity:nama` hanya mengubah argumen/environment dan
tetap memakai login utama. Omnix tidak memalsukan `HOME`; Settings menampilkan
peringatan ini. Jangan menganggap profil Antigravity terisolasi.

## Isi jendela

Menurut kontrak `docs/gui.md`, jendela Omnix adalah satu WebView UI ditambah N
WebView konten untuk tab browser, dan terminal asli lewat PTY. Tata letak yang
dituju:

- **Bar jendela** — satu baris penuh: brand `Omnix` di atas kolom sidebar,
  diikuti tombol collapse dan Settings, lalu tab yang menempel pada batas
  kolom, tombol `+`, `Command`, toggle Agents, dan tiga kontrol jendela
  (minimize, maximize/restore, close) di ujung kanan. Sidebar bisa
  dilipat menjadi rail ikon 56px; panel Agents ditutup lewat tombol hide di
  kanan bawah dan dibuka lagi dari toggle di bar jendela. Tab milik project,
  bukan milik window: strip hanya menampilkan tab project yang aktif. Klik
  folder memindahkan scope ke folder itu, membuka tab baru bila belum ada, dan
  mengaktifkan tab lama hanya bila cwd-nya persis sama. Folder induk dan anak
  tidak berbagi tab. Tab terminal memakai nama project atau label pane
  (`Vesper`, `OpenCode`, `Terminal`), dipotong dengan ellipsis, dan tooltip
  membawa path lengkap. Panel Settings/Docs/Search/Browser juga dibuka per
  project, jadi tidak ikut terbawa ke project lain.
- **Sidebar kiri** — search field di paling atas. Mengetik memfilter daftar
  project; Enter membuka History Search dengan query yang sama, Escape
  mengosongkan filter. Di bawahnya satu grup workspace yang bisa dilipat dan
  judulnya bisa diganti langsung (double-click, Enter simpan, Escape batal).
  Baris judul workspace memuat aksi refresh dan `+ Project`; Settings dan
  toggle Agents tinggal di bar jendela.
  Grup itu hanya berisi folder yang kamu pin lewat `+ Project`; indexed history
  hanya memperkaya baris yang path-nya cocok. Nama project atau grup history
  yang kembar menambah nama folder induk (`omnix · packages`); bila masih
  kembar, dipakai path lengkap. Baris project satu baris; klik
  membuka terminal di folder itu, twisty membuka branch dan daftar sesi agent
  yang dilacak, dan status pty: dot hijau saat hidup, spinner saat pane masih
  mencetak output. Tab baru
  dan agent dari menu `+` mewarisi folder tab aktif, bukan home directory. `+ Project`
  memakai dialog folder bawaan OS, bukan file browser milik Omnix: GTK
  `FileChooserNative` di Linux, `FolderBrowserDialog` di Windows, `osascript
  choose folder` di macOS. Batal tidak menambah apa pun.
- **Panel Agents** — indexed history per project: segmented control
  `Workspace | Project | All`, search, status scan, sort `Last updated`, dan
  grup project yang bisa dilipat dengan count. Tombol header: collapse semua
  grup, filter agent, rescan. Kartu sesi bisa di-resume (klik) atau di-handoff
  (tombol panah). Panel mengikuti state terakhir yang disimpan; data sesi ada
  di `~/.omnix/history.sqlite`, jadi tetap ada setelah restart.
- **Area konten** — xterm untuk tab terminal; WebView konten hanya dibuat saat
  tab browser aktif dan ditutup saat tab lain dipilih. Tiap pane terminal punya
  tombol split dan close di sudut kanan atas (muncul saat hover/focus). Satu
  grup menampung sampai empat pane: 2 berdampingan, 3 dengan satu pane lebar di
  bawah, 4 dalam grid 2x2.
- **Bar status** — memori app dan agent, jumlah sesi/pesan index, dan toast
  singkat untuk hasil aksi.

WebView UI memakai protokol `omnix://`, bukan `file://`, dan asetnya ditanam di
binary. WebView konten tidak diberi IPC handler: hanya UI yang boleh mengirim
perintah ke Rust.

## Tab terminal

Tiap tab terminal adalah PTY sungguhan (`portable-pty`), bukan emulasi di
JavaScript. Data dari PTY dikirim ke UI sebagai base64 supaya byte non-UTF-8
selamat, lalu ditulis ke `xterm`.

Perintah yang dipakai UI: `ptySpawn`, `ptyWrite`, `ptyResize`, `ptyKill`.
Event yang diterima UI: `ptyData` dan `ptyExit` (berisi kode keluar).
Menutup tab mengirim `ptyKill`; proses tidak dibiarkan hidup tanpa tab.

Brief handoff dikirim ke PTY pane agent baru sebagai bracketed paste plus Enter
sekitar 3,5 detik setelah pane hidup. Kalau pane tidak bisa dibuka, outcome
tetap `not-delivered`; kalau pane terbuka tetapi GUI tidak mengonfirmasi
pengiriman, outcome jujur `unconfirmed`.

## Ganti engine browser

Omnix memisahkan browser yang dilihat manusia dari browser yang dipakai agent.

- **native** — WebView bawaan aplikasi (WebKitGTK di Linux, WebView2 di Windows,
  WKWebView di macOS, lewat `wry`). Ini yang menggambar piksel dan bisa dipakai
  manusia.
- **lightpanda** — browser headless berbicara CDP untuk agent, crawling, dan
  automation. Ringan, tetapi tidak menggambar layar; `Page.captureScreenshot`
  hanya placeholder.

Lewat CLI, engine dipilih per perintah:

```bash
omnix browser fetch https://example.com --engine lightpanda --dump markdown
omnix browser doctor
```

`--dump` menerima `markdown`, `html`, atau `semantic_tree_text`. `--engine
native` di CLI sengaja ditolak: WebView native hanya hidup di dalam jendela app,
CLI tidak bisa menggambarnya.

Lokasi binary Lightpanda bisa diatur:

```bash
OMNIX_LIGHTPANDA=/opt/lightpanda/lightpanda omnix browser doctor
```

`omnix browser serve --port 9222` menjalankan Lightpanda sebagai server CDP di
`127.0.0.1` dan mencetak alamat WebSocket-nya. `omnix browser mcp` membuka MCP
stdio untuk agent, sedangkan `omnix browser stop` menghentikan daemon bersama
dan menghapus `.omnix/browser.json`.

## Pencarian riwayat

Transkrip agent di-index ke SQLite FTS5. Default direktori input adalah
`.omnix/transcripts` (relatif ke root state), dan tiap file JSONL berisi pesan
`{"role","text","ts"}`.

```bash
# index; --agent membatasi ke satu agent
omnix history index --agent codex --cwd "$PWD"

# cari frasa, satu hasil terbaik per sesi
omnix history search "blank restore"

# tampilkan resume command dan keluaran JSON
omnix history search "blank restore" --conversation --json

# urutkan dari yang terbaru, batasi hasil
omnix history search deploy --newest --limit 5

omnix history status
```

Pencarian mencoba berurutan: frasa, AND, OR, lalu prefix OR. Hasil kosong di
satu tingkat tidak menghentikan tingkat berikutnya. Pesan dengan `role=tool`
tetap di-index untuk pencarian identifier, tetapi selalu dibuang dari hasil
pencarian supaya output tool tidak dianggap jawaban.

## Tidur dan bangun

`sleep` hanya menandai workspace tidur dan menyimpan sesi agent per pane. Proses
agent tidak dimatikan oleh Omnix; shell atau aplikasi yang menutupnya.

```bash
# catat sesi agent ke pane tertentu
omnix agent track --agent codex --session abc-123 --pane tab:1 --cwd "$PWD"

# tandai workspace tidur
omnix sleep

# lihat workspace dan status tidurnya
omnix status

# bangun: cetak perintah resume per sesi
omnix resume

# bangun dan langsung spawn agent
omnix resume --run
```

Satu pane hanya menyimpan satu sesi; mencatat ulang pane yang sama mengganti
sesi lama. `workspaces.json` ditulis atomik (tmp lalu rename).

## Handoff

Handoff membuat worktree git baru, branch `omnix/<nama>`, brief markdown, dan
receipt JSON. Nama hanya boleh `[a-z0-9-]`, 1–48 karakter.

```bash
omnix handoff fix-login \
  --agent claude \
  --prompt "Perbaiki redirect login yang berputar" \
  --base main
```

Tanpa `--run`, brief ditulis dan receipt berstatus `not-delivered`; perintah
`cd` dan launch dicetak. Dengan `--run`, agent penerima di-spawn tanpa shell
string, brief dikirim sebagai bracketed paste, lalu status menjadi `unconfirmed`
sampai pane melaporkan hasil.

Receipt bisa diurus dari repo mana pun karena indeks global disimpan di
`~/.omnix/handoffs.json`:

```bash
omnix handoff list --open
omnix handoff report fix-login --status done --text "PR #42 siap"
omnix handoff report fix-login --status blocked --file catatan.md
omnix handoff close fix-login
```

`report` menerima `done`, `blocked`, atau `failed`. `gc` menutup handoff yang
branch-nya sudah merged dan mengarsipkan brief lama.

Artefak ada di:

```
.omnix/handoffs/<epoch>-<nama>.json
.omnix/handoffs/<epoch>-<nama>.md
.omnix/worktrees/<nama>/
```

Handoff juga mengambil lease untuk worktree, branch, dan setiap pola
`--paths`. Lease itu disimpan di record dan dilepas saat `handoff close`.
Kalau pembuatan worktree gagal di tengah jalan, semua lease yang sudah diambil
dilepas kembali; tidak ada resource yang tertinggal terkunci.

## Kerja bersama

Omnix bisa menjalankan beberapa agent sekaligus, tetapi folder, path, port,
login, dan worktree bisa bertabrakan. Tiga mekanisme menjaganya:

1. **Lock/lease.** Setiap resource punya satu file JSON di
   `.omnix/locks/` dengan owner, PID, waktu dibuat, dan TTL.

   ```bash
   omnix lock acquire "port:3000" --owner claude:riset --ttl 3600 --meta task=web
   omnix lock list
   omnix lock release "port:3000" --owner claude:riset
   omnix lock list --prune
   ```

   Lease hanya aktif selama TTL belum habis dan PID pemilik masih hidup. Tidak
   ada monitor latar; daftar dan acquire membersihkan lease mati. Acquire ulang
   oleh owner yang sama memperbarui TTL. `handoff` dan `preview start` memakai
   mekanisme ini otomatis.

2. **Sesi browser per terminal.** Setiap PTY mendapat `OMNIX_OWNER` dan
   `OMNIX_BROWSER_SESSION`. `omnix browser mcp` mendaftarkan sesi selama
   panggilan hidup. `omnix browser stop` hanya menutup sesi terminal pemanggil;
   `omnix browser stop --all` menghentikan daemon Lightpanda bersama. Lihat
   pemilik dengan `omnix browser sessions`.

3. **Peringatan GUI.** Kalau dua agent hidup di folder yang sama, atau dua
   worker memakai akun/profil yang sama, banner **Kerja bersama** muncul di atas
   pane. Untuk konflik folder, tombol **Pindahkan ke worktree baru** membuat
   worktree dari repo, membuka ulang agent di sana dengan `sessionId` yang sama,
   lalu menutup pane lama. Peringatan bersifat saran; tidak memblokir kerja.

Preview juga memilih port berikutnya yang bebas kalau port di
`.claude/launch.json` sedang dipakai atau di-lease, menulis ulang argumen
`--port`/`-p` yang menunjuk port lama, mengisi `PORT` bila config belum
menyetelnya, dan mencatat lease `port:<n>` sampai preview berhenti.

### Contoh: dua Codex dan satu Claude

Tiga worker, tiga worktree, tiga login:

```bash
omnix handoff api --repo . --prompt-file tasks/api.md \
  --agent codex:akun2 --paths "src/api/**" --run
omnix handoff ui --repo . --prompt-file tasks/ui.md \
  --agent codex:akun3 --paths "src/ui/**" --run
omnix handoff review --repo . --prompt-file tasks/review.md \
  --agent claude:riset --paths "docs/**" --run
```

Yang dicek sebelum mulai:

- `omnix lock list` menunjukkan pemilik worktree, branch, path, dan port.
- `omnix handoff list --open` menunjukkan status tiap worker.
- `omnix browser sessions` menunjukkan sesi browser per terminal.
- `omnix preview start web` memilih port bebas dan mencatat lease-nya.

Jangan membuka dua worker di folder yang sama. Kalau itu terjadi, banner
**Kerja bersama** muncul; pindahkan salah satu lewat **Pindahkan ke worktree
baru** atau tutup pane-nya. Saat selesai, jalankan
`omnix handoff report <nama> --status done --text "..."` lalu
`omnix handoff close <nama>`; lease worktree, branch, dan path dilepas.
`omnix browser stop` hanya menutup sesi terminal pemanggil, jadi worker lain
tetap punya halamannya.

## Plugin

Plugin adalah program native yang hanya dijalankan saat dipanggil. Tidak ada
daemon plugin dan plugin yang tidak dipakai tidak memakai RAM/CPU.

```bash
omnix plugin search
omnix plugin install hello-command
omnix plugin enable hello-command
omnix hello-command:hello
omnix plugin permissions hello-command
omnix plugin doctor
```

Instalasi memverifikasi SHA-256, menampilkan izin, dan **tidak** menjalankan
plugin. Izin bisa dicabut tanpa menghapus plugin. Plugin tidak pernah menerima
token agent atau cookie browser. Omnix tidak mengklaim sandbox kernel: pasang
hanya plugin yang sumbernya dipercaya. Panduan pengguna ada di
`docs/plugin.md`; API, manifest, JSON-RPC, panel, dan format katalog ada di
`docs/plugin-api.md`. Contoh ada di `examples/plugins/`.

## Kembangkan Omnix

Settings → **Kembangkan Omnix** membuat worktree dari repo Omnix, menulis
brief bersama `CONTRIBUTING.md` dan `docs/architecture.md`, lalu membuka agent
(`claude`, `codex`, atau `antigravity`) di worktree itu.

```bash
omnix dev task --prompt-file brief.md --agent claude
omnix dev build
omnix dev test
omnix dev run
omnix dev status
```

Build disalin ke `~/.omnix/builds/<id>/`. `dev run` memakai
`OMNIX_HOME=~/.omnix/dev/home`, jadi state pemilik tidak tersentuh. Binary yang
sedang dipakai hanya diganti setelah pemilik menekan **Pasang build ini**
(atau `omnix dev install --yes`); binary lama disimpan di
`~/.omnix/builds/previous/` dan bisa dipulihkan lewat **Kembalikan**
(`omnix dev restore --yes`). Agent tidak menandai tag, mempublikasikan rilis,
atau memasang binary sendiri. Detail: `docs/dev.md`.

## Kendali dari terminal

Selama jendela Omnix terbuka, CLI bisa membaca dan mengirim ke pane yang
dikelola Omnix. GUI menulis registry pane ke `~/.omnix/run/panes.json` dan
membuka socket loopback dengan token 0600; CLI tidak bisa menyentuh pane milik
terminal lain. Tidak ada daemon latar: semua fitur ini hidup hanya saat
dipanggil.

```bash
omnix agent status
omnix agent wait ho:fix-login --until turn-done,needs-approval --timeout 300
omnix agent send ho:fix-login --text "lanjutkan test"
omnix agent send ho:fix-login --file brief.md
```

`agent send` menolak pane yang sedang `needs-approval` kecuali diberi
`--force`. Pesan yang memuat pola di `OMNIX_HOLD` (dipisah koma) atau
`~/.omnix/hold.txt` (satu pola per baris) juga ditolak tanpa `--force`. Fitur
ini mati secara default.

Event worker ditulis append-only ke `~/.omnix/events.jsonl`:

```bash
omnix events --limit 30
omnix events --kind agent-started --json
```

Notifikasi desktop opsional: set `OMNIX_NOTIFY_DESKTOP=1` untuk `notify-send`
saat worker selesai atau minta persetujuan. Webhook opsional lewat
`OMNIX_NOTIFY_WEBHOOK=https://...`; hanya URL http/https yang diterima dan
payload dikirim di thread sekali pakai, tanpa polling.

Pembersihan satu kali:

```bash
omnix gc --dry-run
omnix gc
```

`gc` menutup handoff yang branch-nya sudah merged, mengarsipkan receipt lama,
memangkas sesi yang transkripnya hilang, dan menghentikan Lightpanda hanya
kalau tidak ada jendela Omnix yang memegang control socket.

## Pintasan keyboard

Pintasan yang benar-benar terpasang di `app/app.js`:

| Pintasan | Aksi |
| --- | --- |
| `Ctrl/Cmd+K` | buka/tutup command palette |
| `Ctrl/Cmd+Shift+P` | buka command palette |
| `Ctrl/Cmd+Shift+D` | split pane terminal aktif |
| `Escape` | tutup palette, modal, atau menu yang terbuka |

Aksi lain (tab baru, browser, handoff, settings) tersedia lewat tombol `+`,
menu `Command`, dan rail. Di macOS, `Ctrl` diganti `Cmd`.

## Lokasi state

State aplikasi berada di `.omnix/` pada folder kerja saat GUI dijalankan, atau
folder tempat perintah CLI dijalankan. `OMNIX_HOME` memindahkan state itu ke
folder lain:

```
.omnix/
  history.sqlite          index FTS5
  setup.json              status wizard pertama kali
  agents.json             argumen, environment, profil per agent
  agent-defaults.json     permissions default
  projects.json           folder sidebar
  workspaces.json         sesi per pane dan status tidur
  handoffs/<id>.json      receipt handoff + lease ids
  handoffs/<id>.md        brief untuk agent penerima
  worktrees/<nama>/       git worktree hasil handoff/move
  locks/*.json            lease worktree, branch, path, port
  previews/<nama>.json    pid, port, log, lease port preview
  browser/sessions/       lease Lightpanda per terminal
  agent-guide.md          aturan harness untuk agent
  browser.json            alamat Lightpanda bersama
  transcripts/            input indexing opsional
```

State global pengguna berada di `~/.omnix` (atau `OMNIX_HOME` kalau diset),
karena dipakai dari repo mana pun:

```
~/.omnix/
  profiles/<agent>/<profil>/   login/config tiap akun
  plugins/<nama>/              plugin terpasang + state.json
  builds/<id>/                 build dev; previous/ untuk rollback
  dev/home/                    OMNIX_HOME terpisah untuk dev run
  handoffs.json                indeks semua handoff
  events.jsonl                 event worker append-only
  hold.txt                     pola pesan yang ditahan (opsional)
  run/panes.json               registry pane milik GUI
  run/control.json             port + token socket loopback
```

`OMNIX_RUN_DIR` memindahkan folder `run/` bila perlu. Semua isi `.omnix/`
adalah data lokal. Jangan commit ke repo; `.gitignore` seharusnya memuatnya.

## Troubleshooting

**`Package webkit2gtk-4.1 was not found` atau `pkg-config` gagal saat build.**
Paket dev WebKitGTK belum ada. Di Debian/Ubuntu:

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev
```

Pastikan `pkg-config --modversion webkit2gtk-4.1` mengeluarkan versi. Di distro
lain, nama paketnya bisa `webkit2gtk4.1-devel`, `gtk3-devel`, dan
`libsoup3-devel`.

**Aplikasi tidak mau start karena WebView2 tidak ditemukan (Windows).**
Jalankan ulang `Omnix-<versi>-Windows-x64-Setup.exe`; installer memasang
WebView2 otomatis.
Kalau mesin sedang offline, unduh Evergreen Bootstrapper dari
<https://go.microsoft.com/fwlink/p/?LinkId=2124703>, jalankan, lalu buka Omnix.

**`could not run 'lightpanda'. Install Lightpanda or set OMNIX_LIGHTPANDA`.**
Engine Lightpanda tidak ada di `PATH`. Pasang binary-nya, atau tunjuk lokasinya:

```bash
OMNIX_LIGHTPANDA=/path/ke/lightpanda omnix browser doctor
```

Engine native tetap bisa dipakai di jendela walau Lightpanda tidak terpasang;
`omnix browser doctor` akan melaporkan versinya sebagai `not found`.

**`omnix: unknown command` atau `omnix` hanya mencetak bantuan.**
Binary dibangun dengan `--no-default-features` (tanpa `gui`). Build ulang dengan
`cargo build --release` biasa untuk mendapat jendela aplikasi.

**Jendela tidak terbuka: `the window handle kind is not supported`.**
Sesi Wayland dilaporkan tidak didukung oleh `wry`. Omnix otomatis menyetel
`GDK_BACKEND=x11` kalau `WAYLAND_DISPLAY` ada dan `GDK_BACKEND` belum diisi;
kalau kamu menyetel `GDK_BACKEND=wayland` sendiri, hapus dulu.

**Jendela terbuka tapi kosong.**
Pasang `libwebkit2gtk-4.1-0` dan jalankan ulang. Log muncul di stderr; untuk
membuktikan UI termuat, jalankan `OMNIX_GUI_PROBE=1 omnix`.

**`omnix` tidak ditemukan setelah `install.sh`.**
`~/.local/bin` belum masuk `PATH`. Tambahkan `export PATH="$PATH:$HOME/.local/bin"`
ke profil shell, buka terminal baru, lalu jalankan `omnix --help`.

**Paket `.deb` menolak dipasang karena dependensi.**
`apt` akan menyebut paket yang kurang. Pasang dependensi yang disebut, atau
`sudo apt install -f` untuk memperbaikinya. Isi `Depends` paket ini adalah
`libwebkit2gtk-4.1-0, libgtk-3-0, libsoup-3.0-0`.

**State terasa “hilang”.**
Periksa dari direktori mana perintah dijalankan dan apakah `OMNIX_HOME` diset.
State CLI relatif ke root itu, bukan ke `~/.config/omnix`, kecuali kamu
menyetelnya sendiri.

## Selftest

Ada pemeriksaan end-to-end tanpa screenshot: UI benar-benar dimuat ke WebView,
lalu setiap perintah IPC dijalankan dari JavaScript dan hasilnya dicetak.

```sh
OMNIX_HOME=/tmp/omnix-selftest OMNIX_SELFTEST=1 omnix app
```

Langkah yang dicakup: `appInfo`, `layers`, `status`, indeks/scan/sesi/project
history dan pencarian FTS5, penolakan skema selain http/https,
`ptySpawn`/`memory`/`ptyWrite`/`ptyData`/`ptyKill`, `browserFetch` lewat
Lightpanda, `browserNav`/`browserNavClose`, lalu `browserOpen`/`browserClose`.
Output berakhir dengan `selftest: PASS` (exit 0) atau `selftest: FAIL` (exit 1),
dengan timeout 240 detik. Pemeriksaan menulis sendiri satu transkrip contoh ke
`OMNIX_HOME/.omnix/transcripts/`, jadi tidak perlu menyiapkan data history.

Untuk perubahan UI yang lebih cepat, stub WebView dipakai oleh
`node app/selftest.cjs`; perintah itu memeriksa RPC, render sidebar, picker,
split pane, palette, tab, plugin, dev, dan peringatan kerja bersama tanpa
membuka jendela.

## Batasan yang masih terbuka

- Jendela GUI baru diuji di Linux/X11 (lewat XWayland). Wayland native belum.
- Profil agent sudah diuji end-to-end lewat CLI dengan dua akun Codex palsu;
  klik `Add account profile`, `Login profile`, badge, dan dialog Settings belum
  diuji otomatis di jendela sungguhan.
- Profil Antigravity belum terisolasi karena `agy` tidak punya knob config resmi.
- Panel Docs hanya menampilkan tutorial statis, belum dokumentasi kontekstual.
- Client CDP penuh untuk `click/fill/snapshot` belum ada; kontrak harness ada di
  `harness/`.
- Hanya jalur build dan paket Linux yang diuji di mesin ini. Windows dan macOS
  belum dijalankan.
