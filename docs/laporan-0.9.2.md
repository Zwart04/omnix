# Laporan Omnix 0.9.2 — 27 November 2026

Susulan Orchestrator setelah laporan 0.9.1: menutup celah PID di `browser.rs`,
`preview.rs`, dan `locks.rs`. Cabang `perbaikan/omnix-9-10`, tanpa push ke
GitHub. Semua perintah dijalankan di
`/home/zwart/Unduhan/Coding AI/Aplikasi & Games/omnix` dengan
`PATH=/usr/bin:/bin:/home/zwart/.cargo/bin`.

## Celah yang ditutup

0.9.1 sudah memeriksa identitas (`/proc/<pid>` starttime + command line) untuk
pekerja handoff, tetapi tiga tempat lain masih mengirim sinyal hanya dengan
nomor PID. Sesudah proses mati, nomor PID bisa dipakai program lain, jadi
`TERM`/`KILL` lama bisa mengenai proses yang salah.

Satu definisi sekarang ada di `src/procs.rs` dan dipakai semua pemanggil:

| Fungsi | Arti |
| --- | --- |
| `identity(pid)` | `(starttime, cmdline penuh)` dari `/proc`, retry 20×5 ms saat cmdline masih kosong sesudah `exec` |
| `identity_matches(pid, started, exe)` | benar hanya bila starttime dan cmdline masih sama |
| `alive(pid)` | hidup; zombie dihitung mati |
| `alive_and_ours(pid, started, exe)` | hidup **dan** identitas masih cocok |
| `stop(pid, started, exe)` | `TERM`, tunggu sampai 3 detik, cek identitas lagi, baru `KILL`; identitas tidak cocok = tidak ada sinyal |
| `stop_group(pid, started, exe)` | sama untuk process group (preview: npm/node/vite) |

Pemakaian:

- `src/browser.rs`: Lightpanda daemon (`browser.json` menyimpan `pidStarted`,
  `pidExe`) dan sesi agen (`Session.pid_started`, `pid_exe`). `session_active`
  memakai `alive_and_ours`, `kill`/`stop_pid` memakai `procs::stop`.
- `src/preview.rs`: `PreviewState` menyimpan `pid_started` dan `pid_exe`;
  `alive_state` memakai `alive_and_ours`; `stop_state` memakai `stop_group`.
  Identitas diambil tepat sesudah `spawn`, sebelum ada penundaan.
- `src/locks.rs`: `Lock` menyimpan `pid_started` dan `pid_exe`; `active` memakai
  `alive_and_ours`, jadi lock milik PID yang sudah didaur ulang menjadi basi.
- `src/handoff.rs`: `process_identity` sekarang men-delegasi ke `procs::identity`;
  `close_record_with` memakai `procs::stop` (perilaku 0.9.1 tidak berubah).

## Tes gagal dulu

Tes ditulis sebelum implementasi; `cargo test` gagal dikompilasi karena
memanggil `procs::identity`, `procs::stop`, `procs::alive_and_ours`,
`stop_pid(pid, started, exe)`, `alive_state`, `stop_state`, serta field
`pid_started`/`pid_exe` pada `Session`, `PreviewState`, dan `Lock` yang belum
ada (E0425/E0609). Sesudah implementasi:

```
$ cargo test --bin omnix -- procs::tests::
test procs::tests::stop_refuses_a_pid_that_no_longer_matches_its_identity ... ok
test procs::tests::stop_ends_the_process_whose_identity_matches ... ok
test procs::tests::a_recycled_pid_is_not_alive_and_ours ... ok
test result: ok. 5 passed; 0 failed

$ cargo test --bin omnix -- browser::tests::
test browser::tests::a_recycled_pid_is_never_signalled_and_the_session_is_stale ... ok
test result: ok. 5 passed; 0 failed

$ cargo test --bin omnix -- preview::tests::
test preview::tests::stop_never_signals_a_recycled_preview_pid ... ok
test result: ok. 7 passed; 0 failed

$ cargo test --bin omnix -- locks::tests::
test locks::tests::a_lock_whose_owner_pid_was_recycled_is_stale ... ok
test result: ok. 5 passed; 0 failed
```

Tes menyentuh proses nyata: anak `sleep 60` dibiarkan hidup saat identitas
sengaja salah (`started + 1`, cmdline lain), lalu dibunuh saat identitas cocok.
Tidak ada tes yang mengirim sinyal ke proses sesi lain.

## Gerbang

```
$ cargo fmt --check
FMT_RC=0

$ cargo clippy --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.15s
CLIPPY_RC=0

$ ./scripts/test-real-data-guard.sh
before: claude.json=mode=600 sha256=ce166be4... omnix=f5d633ff...
test result: ok. 164 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
after:  claude.json=mode=600 sha256=ce166be4... omnix=f5d633ff...
OK: ~/.claude.json and ~/.omnix are unchanged by the suite
GUARD_RC=0
```

Izin berkas asli sesudah semua uji:

```
$ stat -c '%a %n' ~/.claude.json
600 /home/zwart/.claude.json
```

## Rilis 0.9.2

```
$ cargo build --release -j 4
   Compiling omnix v0.9.2 (...)
    Finished `release` profile [optimized] target(s) in 42.75s

$ install -m755 target/release/omnix ~/.local/bin/omnix
installed identical          # cmp byte-per-byte

$ omnix --version
omnix 0.9.2

$ omnix doctor
Omnix 0.9.2 · control socket live · Lightpanda 1.0.4

$ dist/linux/build-deb.sh --binary target/release/omnix --version 0.9.2
-rw-r--r-- 1 zwart zwart 3023816 Nov 27 13:03 \
  dist/linux/Omnix-0.9.2-Linux-Debian-Ubuntu-x64.deb
```

Aplikasi 0.9.1 (pid 200903) dihentikan sebelum pemasangan; aplikasi 0.9.2
sekarang berjalan (pid 211316). Tag lokal `v0.9.2` dibuat; tidak ada push.

## Batasan / keputusan yang butuh boss

1. **State 0.9.1 tidak punya identitas.** Berkas `browser.json`, sesi browser,
   dan state preview yang ditulis versi lama tidak menyimpan starttime/cmdline.
   0.9.2 memperlakukannya sebagai bukan milik kita: state dibersihkan tanpa
   sinyal. Saat uji ini tidak ada daemon/preview lama yang hidup; bila nanti
   ada, hentikan sekali dengan 0.9.1 sebelum naik. Rekaman baru selalu
   menyimpan identitas.
2. **Baseline RAM 0.8.0 tetap tidak terukur** dan **push/tag GitHub tetap tidak
   dilakukan**; keduanya sudah ditulis di `docs/laporan-0.9.1.md`.
3. Uji tahan 30 menit 0.9.1 (RAM datar 180.5→180.8 MB, CPU 3.0%→2.0%) ada di
   laporan 0.9.1; 0.9.2 hanya menambah pemeriksaan identitas, tidak mengubah
   jalur render/terminal.

## Commit

```
release: 0.9.2   (versi, CHANGELOG, laporan)
fix(procs): prove the launch identity before every process stop
```