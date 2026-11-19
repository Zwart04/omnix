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
sudo apt install dist/linux/omnix_0.1.0_amd64.deb
```

Paket memasang `/usr/bin/omnix`, desktop entry, ikon, dan mendeklarasikan
`Depends: libwebkit2gtk-4.1-0, libgtk-3-0, libsoup-3.0-0`. Arsitektur diambil
dari `dpkg --print-architecture`.

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
./dist/windows/build-zip.ps1
```

Skrip memakai `target\release\omnix.exe`. Untuk build silang dari Linux
(mis. CI atau mesin tanpa Windows), jalankan
`cargo xwin build --release --target x86_64-pc-windows-msvc`; skrip otomatis
memakai `target\x86_64-pc-windows-msvc\release\omnix.exe` kalau berkas
release biasa tidak ada. Hasilnya `dist\windows\omnix-windows-x64.zip` berisi
`omnix.exe` dan `README.txt`.
Ekstrak, lalu jalankan `omnix.exe`. Runtime **Microsoft Edge WebView2** wajib
ada; Windows 11 sudah membawanya, Windows 10 mungkin perlu memasangnya dari
situs Microsoft. Build ini belum ditandatangani, jadi SmartScreen bisa meminta
konfirmasi.

### macOS

```bash
cd omnix
cargo build --release
sh dist/macos/build-app.sh
```

Hasilnya `Omnix.app` di `dist/macos/build/` dan arsip
`dist/macos/Omnix-0.1.0-macos.tar.gz` (atau `...-arm64.tar.gz` / `...-x64.tar.gz`
kalau `OMNIX_ARCH` diset). Bundle memakai
`CFBundleIdentifier dev.omnix.app` dan `LSMinimumSystemVersion 11.0`. Skrip
melakukan ad-hoc codesign kalau `codesign` tersedia; itu bukan notarization.
Gatekeeper di Mac lain tetap bisa menolak aplikasi yang belum dinotarisasi
(klik kanan → Open, atau `xattr -dr com.apple.quarantine Omnix.app`).

## Isi jendela

Menurut kontrak `docs/gui.md`, jendela Omnix adalah satu WebView UI ditambah N
WebView konten untuk tab browser, dan terminal asli lewat PTY. Tata letak yang
dituju:

- **Bar jendela** — satu baris penuh: brand `Omnix` di atas kolom sidebar,
  lalu tab, tombol `+`, dan tombol `Command` di kanan. Tab terminal memakai
  judul window dari shell (`user@host:path`) dan dipotong dengan ellipsis.
- **Sidebar kiri** — search field di paling atas. Mengetik memfilter daftar
  project; Enter membuka History Search dengan query yang sama, Escape
  mengosongkan filter. Di bawahnya satu grup workspace yang bisa dilipat dan
  judulnya bisa diganti langsung (double-click, Enter simpan, Escape batal).
  Baris judul workspace memuat aksi agents, refresh, dan `+ Project`.
  Grup itu hanya berisi folder yang kamu pin lewat `+ Project`; indexed history
  hanya memperkaya baris yang path-nya cocok. Baris project satu baris; klik
  membuka terminal di folder itu, twisty membuka branch dan daftar sesi agent
  yang dilacak, dan spinner kecil menandai pty yang masih hidup. `+ Project`
  memakai dialog folder bawaan OS, bukan file browser milik Omnix: GTK
  `FileChooserNative` di Linux, `FolderBrowserDialog` di Windows, `osascript
  choose folder` di macOS. Batal tidak menambah apa pun.
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

Saat ini pengiriman brief handoff belum masuk ke PTY. Karena itu outcome handoff
jujur dilaporkan `unconfirmed` atau `not-delivered`, bukan `handed-to-terminal`.

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
`127.0.0.1` dan mencetak alamat WebSocket-nya.

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
string, lalu status menjadi `unconfirmed` karena Omnix belum bisa membuktikan
prompt benar-benar masuk ke TUI agent.

Artefak ada di:

```
.omnix/handoffs/<epoch>-<nama>.json
.omnix/handoffs/<epoch>-<nama>.md
.omnix/worktrees/<nama>/
```

## Pintasan keyboard

Tabel ini adalah target UI dari kontrak jendela, **belum terverifikasi** karena
shell GUI belum berjalan di build ini:

| Pintasan | Aksi |
| --- | --- |
| `Ctrl+Shift+T` | tab terminal baru |
| `Ctrl+Shift+B` | tab browser baru |
| `Ctrl+Shift+D` | split pane terminal |
| `Ctrl+Shift+W` | tutup tab aktif |
| `Ctrl+Tab` | tab berikutnya |
| `Ctrl+L` | fokus ke bar alamat |
| `Ctrl+Shift+F` | fokus ke pencarian riwayat |
| `Ctrl+Shift+H` | buka dialog handoff |
| `Ctrl+Shift+S` | tandai workspace tidur |

Di macOS, ganti `Ctrl` dengan `Cmd`.

## Lokasi state

CLI memakai `OMNIX_HOME` sebagai root state; kalau tidak diset, root-nya adalah
direktori kerja saat perintah dijalankan. Di dalam root itu Omnix membuat
`.omnix/`:

```
.omnix/
  history.sqlite          index FTS5
  workspaces.json         sesi per pane dan status tidur
  handoffs/<id>.json      receipt handoff
  handoffs/<id>.md        brief untuk agent penerima
  worktrees/<nama>/       git worktree hasil handoff
  transcripts/            input indexing opsional
```

Shell GUI ditujukan memakai `~/.config/omnix` sebagai default, atau
`OMNIX_HOME` kalau variabel itu diset. Sampai kode jendela ada, yang berlaku
adalah aturan CLI di atas: `~/.config/omnix` belum otomatis dipakai. Untuk
memaksa state global dari CLI:

```bash
export OMNIX_HOME="$HOME/.config/omnix"
omnix history status
```

Semua isi `.omnix/` adalah data lokal. Jangan commit ke repo; `.gitignore`
seharusnya memuatnya.

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
Pasang Microsoft Edge WebView2 Runtime. Windows 11 umumnya sudah punya; Windows
10 sering belum. Setelah dipasang, jalankan `omnix.exe` lagi.

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

Langkah yang dicakup: `appInfo`, `status`, `historyIndex`, `historySearch`,
penolakan skema selain http/https, `ptySpawn`/`ptyWrite`/`ptyData`/`ptyKill`,
`browserFetch` lewat Lightpanda, `handoff`, `agentTrack`, `sleep`, `resume`.
Output berakhir dengan `selftest: PASS` (exit 0) atau `selftest: FAIL` (exit 1),
dengan timeout 90 detik. Direktori `OMNIX_HOME` harus berisi repo git dan
`.omnix/transcripts/**/*.jsonl` agar langkah history punya bahan.

## Batasan yang masih terbuka

- Jendela GUI baru diuji di Linux/X11 (lewat XWayland). Wayland native belum.
- Panel Docs hanya menampilkan tutorial statis, belum dokumentasi kontekstual.
- Pengiriman prompt ke TUI agent lewat PTY belum ada; outcome handoff tetap
  jujur `unconfirmed` atau `not-delivered`.
- Client CDP penuh untuk `click/fill/snapshot` belum ada; kontrak harness ada di
  `harness/`.
- Hanya jalur build dan paket Linux yang diuji di mesin ini. Windows dan macOS
  belum dijalankan.
