# Perch App — panduan pengguna

Dokumen ini untuk orang yang memakai Perch, bukan yang mengubah kodenya.
Kontrak internal jendela ada di `docs/gui.md`; arsitektur data ada di
`docs/architecture.md`.

> **Catatan jujur.** Paket dan skrip di `dist/` ditulis untuk Linux, Windows,
> dan macOS, tetapi hanya jalur Linux yang dibangun dan diuji di mesin ini.
> Jalur Windows dan macOS belum dijalankan. Jendela aplikasi sudah berfungsi di
> Linux: `perch` tanpa argumen membuka jendela, terminal memakai PTY asli, dan
> seluruh alur IPC lulus selftest otomatis (lihat bagian Selftest).

## Instalasi

Perch adalah satu binary Rust bernama `perch`. Subcommand tetap berjalan di
terminal; menjalankan `perch` tanpa argumen (atau `perch app`) membuka jendela
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
cd perch
cargo build --release
./target/release/perch --help
```

Pasang untuk user sendiri (tanpa sudo):

```bash
sh dist/linux/install.sh
```

Skrip menyalin binary ke `~/.local/bin/perch`, entri desktop ke
`~/.local/share/applications/perch.desktop`, dan ikon ke
`~/.local/share/icons/hicolor/scalable/apps/perch.svg`. Kalau
`~/.local/bin` belum ada di `PATH`, skrip mencetak baris `export PATH` yang
perlu ditambahkan. Perintah berikutnya juga dicetak, misalnya:

```bash
perch --help
```

Untuk memasang system-wide di `/usr/local`:

```bash
sh dist/linux/install.sh --system
```

Itu memakai `sudo` hanya kalau kamu bukan root. Binary lain bisa dipilih dengan
`--binary /path/ke/perch`.

Membuat paket `.deb` (butuh `dpkg-deb`, tanpa `cargo-deb`):

```bash
sh dist/linux/build-deb.sh
sudo apt install dist/linux/perch_0.1.0_amd64.deb
```

Paket memasang `/usr/bin/perch`, desktop entry, ikon, dan mendeklarasikan
`Depends: libwebkit2gtk-4.1-0, libgtk-3-0, libsoup-3.0-0`. Arsitektur diambil
dari `dpkg --print-architecture`.

Mencopot pemasangan manual:

```bash
rm -f ~/.local/bin/perch \
  ~/.local/share/applications/perch.desktop \
  ~/.local/share/icons/hicolor/scalable/apps/perch.svg
```

### Windows

Build di PowerShell:

```powershell
cd perch
cargo build --release
./dist/windows/build-zip.ps1
```

Hasilnya `dist\windows\perch-windows-x64.zip` berisi `perch.exe` dan `README.txt`.
Ekstrak, lalu jalankan `perch.exe`. Runtime **Microsoft Edge WebView2** wajib
ada; Windows 11 sudah membawanya, Windows 10 mungkin perlu memasangnya dari
situs Microsoft. Build ini belum ditandatangani, jadi SmartScreen bisa meminta
konfirmasi.

### macOS

```bash
cd perch
cargo build --release
sh dist/macos/build-app.sh
```

Hasilnya `Perch.app` di `dist/macos/build/` dan arsip
`dist/macos/Perch-0.1.0-macos.tar.gz`. Bundle memakai
`CFBundleIdentifier dev.perch.app` dan `LSMinimumSystemVersion 11.0`. Skrip
melakukan ad-hoc codesign kalau `codesign` tersedia; itu bukan notarization.
Gatekeeper di Mac lain tetap bisa menolak aplikasi yang belum dinotarisasi
(klik kanan → Open, atau `xattr -dr com.apple.quarantine Perch.app`).

## Isi jendela

Menurut kontrak `docs/gui.md`, jendela Perch adalah satu WebView UI ditambah N
WebView konten untuk tab browser, dan terminal asli lewat PTY. Tata letak yang
dituju:

- **Sidebar kiri** — daftar workspace yang dilacak, termasuk yang sedang tidur.
- **Tab bar** — tab terminal dan tab browser dalam satu jendela.
- **Area konten** — xterm untuk tab terminal; WebView konten hanya dibuat saat
  tab browser aktif dan ditutup saat tab lain dipilih.
- **Bar status/URL** — alamat halaman, indikator engine browser, dan toast
  singkat untuk hasil aksi.

WebView UI memakai protokol `perch://`, bukan `file://`, dan asetnya ditanam di
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

Perch memisahkan browser yang dilihat manusia dari browser yang dipakai agent.

- **native** — WebView bawaan aplikasi (WebKitGTK di Linux, WebView2 di Windows,
  WKWebView di macOS, lewat `wry`). Ini yang menggambar piksel dan bisa dipakai
  manusia.
- **lightpanda** — browser headless berbicara CDP untuk agent, crawling, dan
  automation. Ringan, tetapi tidak menggambar layar; `Page.captureScreenshot`
  hanya placeholder.

Lewat CLI, engine dipilih per perintah:

```bash
perch browser fetch https://example.com --engine lightpanda --dump markdown
perch browser doctor
```

`--dump` menerima `markdown`, `html`, atau `semantic_tree_text`. `--engine
native` di CLI sengaja ditolak: WebView native hanya hidup di dalam jendela app,
CLI tidak bisa menggambarnya.

Lokasi binary Lightpanda bisa diatur:

```bash
PERCH_LIGHTPANDA=/opt/lightpanda/lightpanda perch browser doctor
```

`perch browser serve --port 9222` menjalankan Lightpanda sebagai server CDP di
`127.0.0.1` dan mencetak alamat WebSocket-nya.

## Pencarian riwayat

Transkrip agent di-index ke SQLite FTS5. Default direktori input adalah
`.perch/transcripts` (relatif ke root state), dan tiap file JSONL berisi pesan
`{"role","text","ts"}`.

```bash
# index; --agent membatasi ke satu agent
perch history index --agent codex --cwd "$PWD"

# cari frasa, satu hasil terbaik per sesi
perch history search "blank restore"

# tampilkan resume command dan keluaran JSON
perch history search "blank restore" --conversation --json

# urutkan dari yang terbaru, batasi hasil
perch history search deploy --newest --limit 5

perch history status
```

Pencarian mencoba berurutan: frasa, AND, OR, lalu prefix OR. Hasil kosong di
satu tingkat tidak menghentikan tingkat berikutnya. Pesan dengan `role=tool`
tetap di-index untuk pencarian identifier, tetapi selalu dibuang dari hasil
pencarian supaya output tool tidak dianggap jawaban.

## Tidur dan bangun

`sleep` hanya menandai workspace tidur dan menyimpan sesi agent per pane. Proses
agent tidak dimatikan oleh Perch; shell atau aplikasi yang menutupnya.

```bash
# catat sesi agent ke pane tertentu
perch agent track --agent codex --session abc-123 --pane tab:1 --cwd "$PWD"

# tandai workspace tidur
perch sleep

# lihat workspace dan status tidurnya
perch status

# bangun: cetak perintah resume per sesi
perch resume

# bangun dan langsung spawn agent
perch resume --run
```

Satu pane hanya menyimpan satu sesi; mencatat ulang pane yang sama mengganti
sesi lama. `workspaces.json` ditulis atomik (tmp lalu rename).

## Handoff

Handoff membuat worktree git baru, branch `perch/<nama>`, brief markdown, dan
receipt JSON. Nama hanya boleh `[a-z0-9-]`, 1–48 karakter.

```bash
perch handoff fix-login \
  --agent claude \
  --prompt "Perbaiki redirect login yang berputar" \
  --base main
```

Tanpa `--run`, brief ditulis dan receipt berstatus `not-delivered`; perintah
`cd` dan launch dicetak. Dengan `--run`, agent penerima di-spawn tanpa shell
string, lalu status menjadi `unconfirmed` karena Perch belum bisa membuktikan
prompt benar-benar masuk ke TUI agent.

Artefak ada di:

```
.perch/handoffs/<epoch>-<nama>.json
.perch/handoffs/<epoch>-<nama>.md
.perch/worktrees/<nama>/
```

## Pintasan keyboard

Tabel ini adalah target UI dari kontrak jendela, **belum terverifikasi** karena
shell GUI belum berjalan di build ini:

| Pintasan | Aksi |
| --- | --- |
| `Ctrl+Shift+T` | tab terminal baru |
| `Ctrl+Shift+B` | tab browser baru |
| `Ctrl+Shift+W` | tutup tab aktif |
| `Ctrl+Tab` | tab berikutnya |
| `Ctrl+L` | fokus ke bar alamat |
| `Ctrl+Shift+F` | fokus ke pencarian riwayat |
| `Ctrl+Shift+H` | buka dialog handoff |
| `Ctrl+Shift+S` | tandai workspace tidur |

Di macOS, ganti `Ctrl` dengan `Cmd`.

## Lokasi state

CLI memakai `PERCH_HOME` sebagai root state; kalau tidak diset, root-nya adalah
direktori kerja saat perintah dijalankan. Di dalam root itu Perch membuat
`.perch/`:

```
.perch/
  history.sqlite          index FTS5
  workspaces.json         sesi per pane dan status tidur
  handoffs/<id>.json      receipt handoff
  handoffs/<id>.md        brief untuk agent penerima
  worktrees/<nama>/       git worktree hasil handoff
  transcripts/            input indexing opsional
```

Shell GUI ditujukan memakai `~/.config/perch` sebagai default, atau
`PERCH_HOME` kalau variabel itu diset. Sampai kode jendela ada, yang berlaku
adalah aturan CLI di atas: `~/.config/perch` belum otomatis dipakai. Untuk
memaksa state global dari CLI:

```bash
export PERCH_HOME="$HOME/.config/perch"
perch history status
```

Semua isi `.perch/` adalah data lokal. Jangan commit ke repo; `.gitignore`
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
10 sering belum. Setelah dipasang, jalankan `perch.exe` lagi.

**`could not run 'lightpanda'. Install Lightpanda or set PERCH_LIGHTPANDA`.**
Engine Lightpanda tidak ada di `PATH`. Pasang binary-nya, atau tunjuk lokasinya:

```bash
PERCH_LIGHTPANDA=/path/ke/lightpanda perch browser doctor
```

Engine native tetap bisa dipakai di jendela walau Lightpanda tidak terpasang;
`perch browser doctor` akan melaporkan versinya sebagai `not found`.

**`perch: unknown command` atau `perch` hanya mencetak bantuan.**
Binary dibangun dengan `--no-default-features` (tanpa `gui`). Build ulang dengan
`cargo build --release` biasa untuk mendapat jendela aplikasi.

**Jendela tidak terbuka: `the window handle kind is not supported`.**
Sesi Wayland dilaporkan tidak didukung oleh `wry`. Perch otomatis menyetel
`GDK_BACKEND=x11` kalau `WAYLAND_DISPLAY` ada dan `GDK_BACKEND` belum diisi;
kalau kamu menyetel `GDK_BACKEND=wayland` sendiri, hapus dulu.

**Jendela terbuka tapi kosong.**
Pasang `libwebkit2gtk-4.1-0` dan jalankan ulang. Log muncul di stderr; untuk
membuktikan UI termuat, jalankan `PERCH_GUI_PROBE=1 perch`.

**`perch` tidak ditemukan setelah `install.sh`.**
`~/.local/bin` belum masuk `PATH`. Tambahkan `export PATH="$PATH:$HOME/.local/bin"`
ke profil shell, buka terminal baru, lalu jalankan `perch --help`.

**Paket `.deb` menolak dipasang karena dependensi.**
`apt` akan menyebut paket yang kurang. Pasang dependensi yang disebut, atau
`sudo apt install -f` untuk memperbaikinya. Isi `Depends` paket ini adalah
`libwebkit2gtk-4.1-0, libgtk-3-0, libsoup-3.0-0`.

**State terasa “hilang”.**
Periksa dari direktori mana perintah dijalankan dan apakah `PERCH_HOME` diset.
State CLI relatif ke root itu, bukan ke `~/.config/perch`, kecuali kamu
menyetelnya sendiri.

## Selftest

Ada pemeriksaan end-to-end tanpa screenshot: UI benar-benar dimuat ke WebView,
lalu setiap perintah IPC dijalankan dari JavaScript dan hasilnya dicetak.

```sh
PERCH_HOME=/tmp/perch-selftest PERCH_SELFTEST=1 perch app
```

Langkah yang dicakup: `appInfo`, `status`, `historyIndex`, `historySearch`,
penolakan skema selain http/https, `ptySpawn`/`ptyWrite`/`ptyData`/`ptyKill`,
`browserFetch` lewat Lightpanda, `handoff`, `agentTrack`, `sleep`, `resume`.
Output berakhir dengan `selftest: PASS` (exit 0) atau `selftest: FAIL` (exit 1),
dengan timeout 90 detik. Direktori `PERCH_HOME` harus berisi repo git dan
`.perch/transcripts/**/*.jsonl` agar langkah history punya bahan.

## Batasan yang masih terbuka

- Jendela GUI baru diuji di Linux/X11 (lewat XWayland). Wayland native belum.
- Panel Docs hanya menampilkan tutorial statis, belum dokumentasi kontekstual.
- Pengiriman prompt ke TUI agent lewat PTY belum ada; outcome handoff tetap
  jujur `unconfirmed` atau `not-delivered`.
- Client CDP penuh untuk `click/fill/snapshot` belum ada; kontrak harness ada di
  `harness/`.
- Hanya jalur build dan paket Linux yang diuji di mesin ini. Windows dan macOS
  belum dijalankan.
