# Plugin Omnix

Plugin adalah program kecil yang **hanya jalan saat dipanggil**. Tidak ada
server plugin yang menunggu di latar belakang, tidak ada polling, dan plugin
yang tidak dipakai tidak memakan RAM atau CPU. Semua plugin berjalan sebagai
program biasa dengan hak user yang sama seperti terminal; Omnix tidak
mengklaim sandbox kernel.

Panduan pembuat plugin (bahasa Inggris) ada di [`plugin-api.md`](plugin-api.md).
Contoh siap pakai:

- `examples/plugins/hello-command/` — satu command.
- `examples/plugins/status-panel/` — panel HTML dengan `postMessage`.

## Pasang dan pakai

Lewat GUI: **Settings → Plugins**, atau menu **Plugin**. Lewat CLI:

```sh
omnix plugin search                 # lihat katalog publik
omnix plugin install hello-command  # nama katalog
omnix plugin install ./plugin-saya  # folder lokal
omnix plugin install ./plugin.tar.gz
omnix plugin list
omnix plugin enable hello-command
omnix plugin disable hello-command
omnix plugin update                 # semua yang terpasang
omnix plugin update hello-command
omnix plugin remove hello-command
```

Setelah dipasang, plugin **tidak otomatis dijalankan**. Kontribusinya muncul di
palette dan Settings, tetapi proses plugin baru start saat command, hook, panel,
atau tool itu benar-benar dipanggil. Pemilik bisa menonaktifkannya lewat
Settings atau `omnix plugin disable`; command lalu hilang sampai diaktifkan
lagi. Hook baru berjalan setelah plugin aktif dan event yang cocok terjadi.

Katalog default:

```text
https://raw.githubusercontent.com/Zwart04/omnix-plugins/main/index.json
```

Repo katalog itu belum dipublikasikan pada sumber ini, jadi `plugin search` dan
`plugin install <nama>` akan gagal 404 sampai pemilik membuatnya. Sebelum itu,
pasang dari folder/arsip lokal atau arahkan override ke `index.json` milikmu.

Untuk pengembangan, timpa dengan `OMNIX_PLUGIN_CATALOG`:

```sh
OMNIX_PLUGIN_CATALOG=file:///tmp/katalog-saya/index.json omnix plugin search
```

Hanya override ini yang boleh memakai `file://`; katalog default selalu HTTPS.

## Izin

Manifest menyatakan izin yang diminta: `read`, `write`, `network`, `agents`,
dan `history`. Omnix menampilkan izin itu saat instalasi dan di Settings.
Izin bisa dicabut tanpa menghapus plugin:

```sh
omnix plugin permissions hello-command
omnix plugin permissions hello-command --revoke network
omnix plugin permissions hello-command --grant read
```

Saat izin dicabut, panggilan yang membutuhkannya ditolak. Plugin tidak pernah
menerima token agent, cookie browser, atau environment lengkap user; hanya
variabel `OMNIX_PLUGIN_*` dan allowlist kecil yang diteruskan.

## Keamanan

- SHA-256 arsip katalog diverifikasi sebelum ekstraksi.
- Folder plugin harus berada di `~/.omnix/plugins/<nama>`; panel tidak boleh
  keluar folder lewat `..` atau symlink.
- Runtime diberi batas waktu (default 15 detik, maksimum 120 detik) dan batas
  RAM (default 256 MB, best-effort). Timeout, crash, atau output bukan JSON
  hanya menggagalkan panggilan itu; jendela Omnix tetap hidup.
- Plugin adalah program native. Kalau sumbernya tidak dipercaya, jangan
  pasang. Laporkan masalah ke `SECURITY.md`, bukan ke issue publik.

## Perintah pembuat

```sh
omnix plugin new plugin-saya --kind command
omnix plugin new panel-saya --kind panel
omnix plugin new hook-saya --kind hook
omnix plugin new agent-saya --kind agent
omnix plugin new tool-saya --kind browser-tool
```

Perintah ini membuat `omnix-plugin.toml`, runtime Python contoh, dan (untuk
panel) `panel.html`. Uji runtime langsung:

```sh
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"command","params":{"args":[]}}' \
  | python3 plugin.py
```

## Diagnosa

```sh
omnix plugin doctor
omnix plugin list --json
omnix plugin doctor --json
```

`doctor` memeriksa manifest, versi API, entry runtime, checksum folder, dan izin
yang diberikan. Kalau plugin hilang dari menu, pastikan statusnya `enabled` dan
runtime-nya bisa dieksekusi. Untuk panel kosong, cek `omnix plugin doctor` dan
konsol WebView; file panel harus berada di dalam folder plugin.

## Lokasi state

```
~/.omnix/plugins/<nama>/          folder plugin terpasang
~/.omnix/plugins/state.json       enabled, izin, versi, checksum
~/.omnix/plugins/data/<nama>/     data yang boleh ditulis plugin
```
