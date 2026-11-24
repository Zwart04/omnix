# Performa Omnix

Dokumen ini mencatat pengukuran yang benar-benar dijalankan di mesin Linux ini,
bukan target atau klaim pemasaran. Angka CPU sangat bergantung pada compositor
dan isi layar, jadi rentangnya lebar.

## Metode baseline Part 3

- Binary lama: build `0.6.9` sebelum fitur profil akun, control socket, handoff
  list/report/close, events, dan GC (disimpan di luar repo).
- Binary baru: `target/release/omnix` pada commit kerja yang sama dengan
  dokumen ini.
- Tiap percobaan memakai `OMNIX_HOME` baru, menunggu proses WebKit anak pertama,
  lalu mengukur setelah 12 detik idle.
- RAM = jumlah `VmRSS` proses Omnix + WebKitNetworkProcess + WebKitWebProcess.
- Waktu buka = waktu dari `exec` sampai proses anak pertama muncul.
- CPU = sampel 5 detik dari `/proc/<pid>/stat` proses utama.

Tiga percobaan per binary, dijalankan bergantian supaya suhu mesin tidak
menguntungkan salah satu pihak.

## Hasil baseline Part 3

| Percobaan | Binary | Waktu buka | RAM idle | CPU sampel |
| --- | --- | ---: | ---: | ---: |
| 1 | lama | 0,20 s | 487,9 MiB | 132,2% |
| 1 | baru | 0,20 s | 488,5 MiB | 144,2% |
| 2 | lama | 0,36 s | 488,6 MiB | 156,4% |
| 2 | baru | 0,26 s | 488,8 MiB | 156,8% |
| 3 | lama | 0,25 s | 488,0 MiB | 145,0% |
| 3 | baru | 0,25 s | 488,2 MiB | 123,6% |

Rata-rata RAM: lama 488,2 MiB, baru 488,5 MiB (+0,06%, di dalam noise).
Waktu buka: 0,20–0,36 s untuk keduanya. CPU tidak bisa dipakai sebagai bukti
regresi: rentang lama 132–156%, baru 124–157%. Selisihnya lebih kecil dari
variasi antar-percobaan.

## Plugin dan kerja bersama (Part 5/6)

Putaran ini mengukur biaya **plugin terpasang tetapi tidak pernah dipanggil**,
ditambah fitur kerja bersama (lock, sesi browser, preview, peringatan GUI).
Tanggal pengukuran: 24 November 2026.

Metode:

- Binary `target/release/omnix` 0.6.9 dengan seluruh fitur Part 3–6.
- Dua `OMNIX_HOME` terpisah: satu kosong, satu berisi **5 plugin command**
  yang dipasang dan `enabled` (hasil `omnix plugin new` + `plugin install`).
- Tiap percobaan: jalankan `omnix app`, tunggu proses anak WebKit pertama,
  diamkan 12 detik, jumlahkan `VmRSS` seluruh pohon proses (Omnix +
  WebKitNetworkProcess + WebKitWebProcess), sampel CPU proses utama 5 detik,
  lalu tutup jendela.
- 5 percobaan per kondisi, dijalankan bergantian. Tidak ada plugin yang
  dipanggil selama pengukuran.

| Percobaan | Plugin | Waktu buka | RAM idle | CPU utama | Proses plugin |
| --- | --- | ---: | ---: | ---: | ---: |
| 1 | 0 | 319 ms | 487,20 MiB | 130% | 0 |
| 2 | 0 | 217 ms | 501,14 MiB | 52% | 0 |
| 3 | 0 | 213 ms | 487,05 MiB | 0% | 0 |
| 4 | 0 | 324 ms | 488,41 MiB | 0% | 0 |
| 5 | 0 | 216 ms | 484,99 MiB | 0% | 0 |
| 1 | 5 | 217 ms | 476,74 MiB | 124% | 0 |
| 2 | 5 | 325 ms | 501,41 MiB | 40% | 0 |
| 3 | 5 | 320 ms | 487,27 MiB | 4% | 0 |
| 4 | 5 | 216 ms | 487,07 MiB | 0% | 0 |
| 5 | 5 | 215 ms | 483,92 MiB | 0% | 0 |

| Metrik | 0 plugin | 5 plugin | Selisih |
| --- | ---: | ---: | ---: |
| Median RAM | 498.744 KiB (487,05 MiB) | 498.756 KiB (487,07 MiB) | +12 KiB (+0,002%) |
| Rata-rata RAM | 501.514 KiB (489,76 MiB) | 498.977 KiB (487,28 MiB) | −2.538 KiB (−0,51%) |
| Median waktu buka | 217 ms | 217 ms | 0 ms |
| Rata-rata waktu buka | 257,8 ms | 258,6 ms | +0,3% |
| Proses plugin saat idle | 0 | 0 | 0 |

Kesimpulan: lima plugin terpasang **tidak menambah RAM, CPU, atau waktu buka**
di luar noise. Median RAM sama sampai 12 KiB, rata-rata justru 0,51% lebih
rendah, dan median waktu buka identik. Semua 10 percobaan melaporkan 0 proses
plugin. Ini jauh di bawah batas regresi 5%.

CPU tetap tidak bisa dipakai sebagai bukti: kedua kondisi sama-sama sesekali
0% dan sesekali >100% karena compositor dan pengecekan update. Yang penting,
tidak ada proses atau polling baru; plugin hanya start saat dipanggil.

Dibanding baseline Part 3 (rata-rata 488,2–488,5 MiB, buka 0,20–0,36 s),
kondisi 0 plugin sekarang 489,76 MiB dan 0,26 s rata-rata — masih di dalam
noise dan di bawah 5%. Lock hanya file yang dibaca saat dipanggil, sesi browser
dan preview tidak punya proses sampai `browser serve/mcp` atau
`preview start`, dan tidak ada monitor latar belakang.

## Kenapa CPU idle tinggi?

Pada kedua binary, satu thread GTK/WebKit (nama thread `omnix`, stack di
`libglib`/`libgobject`/`libgtk-3`) berputar sekitar 98% dan thread utama sekitar
36–43%. Ini bukan thread yang ditambahkan fitur profil: baseline tanpa kode
profil menunjukkan pola yang sama. Pengukuran lebih awal yang terlihat seperti
regresi (77% vs 122%) adalah noise compositor, bukan tambahan polling.

Tidak ada polling baru saat idle. Yang berjalan hanya:

- sampler RAM yang sudah ada (3,5 detik di Linux, berhenti saat jendela
  tersembunyi);
- pengecekan update tiap enam jam;
- socket control yang menunggu `accept`, tanpa timer.

Fitur yang sering dianggap "daemon" semuanya one-shot: `omnix gc` hanya bekerja
saat dipanggil, webhook dikirim sekali di thread sementara, `omnix agent
wait` hanya berjalan di proses CLI yang menunggu, plugin start saat dipanggil
lalu keluar, dan lock/lease hanya berupa file yang dibaca saat ada operasi.
Tidak ada monitor lock, sesi browser, atau plugin yang berjalan di latar.

## Catatan pengukuran

- Angka RAM dan CPU di atas berasal dari satu mesin dengan WebKitGTK dan
  compositor X11/XWayland. Windows dan macOS belum diukur.
- `strace` tidak tersedia di mesin ini; identifikasi thread panas memakai
  `/proc`, `ps -L`, dan pembacaan stack lewat `ptrace`.
- Untuk mengulang:

```bash
# ukur manual, binary bebas
OMNIX_HOME=/tmp/omnix-perf ./target/release/omnix app &
ps -L -p $! -o tid=,comm=,pcpu= | sort -k3 -nr | head
```

Perhatikan bahwa compositor dapat mengubah angka CPU puluhan persen tanpa ada
perubahan kode.
