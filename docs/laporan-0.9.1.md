# Laporan Omnix 0.9.1 — 27 November 2026

> Lanjutan 0.9.2 (celah PID di browser/preview/locks): lihat
> [`laporan-0.9.2.md`](laporan-0.9.2.md).

Cabang `perbaikan/omnix-9-10`, tanpa push, tag, atau GitHub release.
Semua perintah di bawah dijalankan di `/home/zwart/Unduhan/Coding AI/Aplikasi & Games/omnix`
dengan `PATH=/usr/bin:/bin:/home/zwart/.cargo/bin`.

## Versi terpasang

```
$ omnix --version
omnix 0.9.1

$ omnix doctor
claude       /home/zwart/.local/bin/claude            2.1.332 (Claude Code)
codex        /home/zwart/.local/bin/codex             codex-cli 0.172.0
antigravity  /home/zwart/.local/bin/agy               1.5.18

Omnix 0.9.1 · control socket live · Lightpanda 1.0.4
```

`~/.local/bin/omnix` identik byte-per-byte dengan `target/release/omnix`
(`cmp` lulus). `.deb` rilis: `dist/linux/Omnix-0.9.1-Linux-Debian-Ubuntu-x64.deb`.

## Butir prompt 13 (stabilitas)

| Butir | Status | Bukti |
| --- | --- | --- |
| 1. A — isolasi tes dari data asli | Diperbaiki | `scripts/test-real-data-guard.sh`; sidik `~/.claude.json` (mode + sha256) dan seluruh pohon `~/.omnix` identik sebelum/sesudah `cargo test` |
| 2. B — laporan pekerja headless | Diperbaiki | pekerja headless claude/codex/antigravity berakhir `delivered-headless`, laporan `done`, receipt dan events konsisten; `--allowedTools "Bash(omnix handoff report *)"` |
| 3. `gc`, `close --force`, pane orkestrator | Diperbaiki | `gc` memangkas entri indeks yang rekamannya hilang; `close --force` menolak jalur berbahaya; pane orkestrator `sessionId` terisi dan `orchestrator: true` |
| 4. Commit per butir, tes gagal dulu | Dilakukan | lihat daftar commit; tiap perbaikan punya tes yang gagal sebelum kode diubah |
| 5. Pola Orca (worker_done, gerbang keputusan, status OSC/hook, peringatan kuota 80%) | **Belum** | sengaja ditunda oleh prompt 13b ("hentikan fitur lain") dan urutan prompt 13 (fase 2, cabang terpisah). Bukan bagian rilis 0.9.1. |

## Meniru pengaman Orca (prompt 13b)

Kode Orca (MIT, `github.com/stablyai/orca`) dibaca lewat `gh api`; salinannya di
`/tmp/opencode/orca-src/`. Berkas baru: `src/claude_trust.rs`.

### 1. Trust Claude — meniru `claude-folder-trust-file.ts`

- Berkas tujuan mengikuti env pane: `${CLAUDE_CONFIG_DIR}/.config.json` atau
  `~/.claude/.config.json` (warisan), lalu `${CLAUDE_CONFIG_DIR atau HOME}/.claude.json`
  dengan akhiran `-custom-oauth` bila `CLAUDE_CODE_CUSTOM_OAUTH_URL` di-set.
- Kunci trust: `resolve()` + `realpath`, dinormalisasi NFC; salah satu `true`
  berarti `unchanged` (tidak menulis).
- Berkas tidak ada → `missing` (tidak dibuat). Tidak terbaca / bukan objek /
  `projects` bukan objek → `unreadable` (tidak menulis).
- Symlink diikuti ke berkas asli, lalu berkas asli diganti, symlink tetap.
- Pengganti `<berkas>.omnix-trust-<id>.tmp` dengan `O_EXCL`, izin sama dengan
  asli, `chmod` ulang sesudah umask, tulis, `sync`, `rename`; berkas sementara
  dihapus lewat `Drop`.
- Kunci `<berkas>.lock` (mkdir atomik, 4 percobaan, jeda 50/100/200/250 ms).
  Gagal → `locked`, peluncuran pane tidak digagalkan. Baca ulang di dalam kunci.
  Penulis Omnix untuk berkas yang sama diantrekan (mutex).
- Hasil (`granted`/`unchanged`/`missing`/`unreadable`/`locked`) dicatat sebagai
  event `claude-trust`. `~/.omnix/trust-created.json` (nilai sebelumnya) tetap.

Tes (12, semua lulus di `src/claude_trust.rs`): izin 0600 tetap 0600, berkas
rusak utuh, `projects` bukan objek ditolak, berkas tidak ada tidak dibuat,
kunci dipegang pihak lain tidak menulis, 8 penulis bersamaan tidak saling
menimpa, `CLAUDE_CONFIG_DIR` + warisan + `-custom-oauth` dihormati, symlink
tetap symlink, kunci NFC, hanya `.omnix` yang di-trust.

Bukti asli bug (dari Orchestrator): `~/.claude.json` 664 setelah binary 11.37.
Binary 0.9.1 sekarang mempertahankan mode.

### 2. `close --force` — meniru `worktree-removal-safety.ts` + bukti yatim

`force_removal_problem` menolak: repo itu sendiri, `$HOME`, `/`, induk repo,
jalur di luar `<repo>/.omnix`, dan symlink. Folder yang tidak terdaftar di git
dihapus hanya bila `proves_orphan_worktree` membuktikan `gitdir` di bawah
`<repo>/.git/worktrees/` yang menunjuk balik ke `.git` folder itu. Folder yang
memuat worktree terdaftar lain tidak dihapus. Tes (5): `force_removal_refuses_*`
(repo, jalur berbahaya, folder asing, folder berisi worktree lain) dan
`force_removal_allows_its_own_record`.

Bukti jalur nyata:

```
$ omnix handoff close dang --repo /tmp/opencode/omnix-e2e --force
omnix: refusing to force-remove /tmp/opencode/omnix-e2e: it is the repository itself
rc=1
repo still exists: yes
```

### 3. Identitas proses sebelum sinyal — meniru `daemon-pid-identity` / `daemon-stale-kill`

Saat peluncuran disimpan `starttime` (`/proc/<pid>/stat` field 22) dan command
line (`/proc/<pid>/cmdline`). `/proc/<pid>/cmdline` bisa kosong sesaat setelah
`exec`, jadi dibaca ulang sampai 20×5 ms. Sebelum sinyal, identitas harus cocok;
tidak cocok → tidak ada sinyal. Sesudah `TERM`, tunggu sampai hilang (sinyal 0,
batas 3 detik), lalu `KILL`, lalu konfirmasi. Tes:
`close_never_kills_a_pid_whose_identity_does_not_match` (PID proses lain tidak
dibunuh) dan `close_stops_the_recorded_worker`.

### 4. Tes tidak menyentuh data asli

`scripts/test-real-data-guard.sh` mencatat mode + sha256 `~/.claude.json` dan
sidik pohon `~/.omnix` sebelum dan sesudah `cargo test`, lalu gagal bila salah
satu berubah.

### 5. Gerbang perintah (keluaran)

```
$ cargo fmt --check
FMT_RC=0

$ cargo clippy --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.71s
CLIPPY_RC=0

$ ./scripts/test-real-data-guard.sh
before: claude.json=mode=600 sha256=77fb94f8… omnix=0202ff29…
test result: ok. 158 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
after:  claude.json=mode=600 sha256=77fb94f8… omnix=0202ff29…
OK: ~/.claude.json and ~/.omnix are unchanged by the suite
```

Izin `~/.claude.json` sesudah uji ujung-ke-ujung:

```
$ stat -c '%a %n' ~/.claude.json
600 /home/zwart/.claude.json
```

Batasan yang diketahui: `browser.rs`, `preview.rs`, dan `locks.rs` masih
menghentikan proses dengan PID tanpa pencocokan identitas. Ketiganya mengurus
proses yang mereka sendiri luncurkan (browser, server pratinjau, pemegang
kunci) dan tidak termasuk berkas Orca yang diminta butir 3; belum diperbaiki di
rilis ini.

## Uji ujung-ke-ujung (repo uji `/tmp/opencode/omnix-e2e`, bukan repo CRM)

### Handoff Claude headless

```
$ omnix handoff e2e1 --repo /tmp/opencode/omnix-e2e --agent claude --run --headless
mode:     headless (detached)
outcome: working
worktree: /tmp/opencode/omnix-e2e/.omnix/worktrees/e2e1

$ omnix agent wait ho:e2e1 --until exited --timeout 300
ho:e2e1 exited

$ omnix agent status --handoff e2e1 --json
  "exitCode": 0,
  "sessionId": "64818649-72c2-47c3-b5ab-e7c566b1f1e6",
  "state": "exited",
  "status": "done"
```

`handoff list --json` → `outcome: delivered-headless`, receipt
`headless run finished -> …/handoff-e2e1.log`.

### Tiga handoff headless bersamaan (claude, codex, antigravity)

```
c-antigravity delivered-headless | headless run finished
c-codex       delivered-headless | headless run finished
c-claude      delivered-headless | headless run finished
```

Hitungan peristiwa mentah di `~/.omnix/events.jsonl` untuk ketiga handoff:
`handoff-created`, `handoff-headless`, `handoff-report` masing-masing **1**.
Tidak ada peristiwa ganda.

Catatan jujur: `e2e1` (skenario terpisah) sempat menghasilkan dua
`handoff-report` karena agen melapor dan pekerja juga melapor. Itu diperbaiki
di commit `8318184` (laporan kedua dengan status sama tidak menambah event) dan
diuji `a_repeated_report_with_the_same_status_keeps_one_event`.

### Handoff gagal (agen tidak dikenal)

```
$ omnix handoff uji-bad --repo … --agent tidakada --prompt x --run --headless
omnix: unknown agent 'tidakada' (…)
rc=1
worktrees now: e2e1        # tidak ada worktree baru
```

### `close` saat pekerja masih jalan

Codex dijalankan dengan `sleep 120`, lalu ditutup:

```
$ omnix handoff close c-stop --repo /tmp/opencode/omnix-e2e
closed c-stop (/tmp/opencode/omnix-e2e/.omnix/worktrees/c-stop)
leftover worker/agent?  no worker / no codex
zombies?               no zombies
```

### Pulih setelah aplikasi dimulai ulang

`handoff list --json` sebelum dan sesudah restart: 17 rekaman, daftar identik.
`events --limit 60` baris dan isinya identik; restart tidak menambah atau
menggandakan peristiwa.

### Trust nyata dari pane GUI

Pane Claude asli dibuka lewat `omnix agent adopt --agent claude --cwd
…/.omnix/worktrees/e2e1`:

```
before: mode=600  has key: False
after:  mode=600  has key: True  value: True
```

`~/.omnix/trust-created.json && previous: null` mencatat nilai sebelumnya untuk
pembatalan.

## Uji tahan 30 menit (3 tab agen + 1 terminal)

Aplikasi 0.9.1 dengan tab `claude`, `codex`, `antigravity`, dan `bash` di
`/tmp/opencode/omnix-e2e`; sampel tiap 5 menit.

| Menit | proses | RAM aplikasi | RAM pohon | CPU |
| --- | --- | --- | --- | --- |
| 0  | 7 | 180.5 MB | 1018.1 MB | 3.0% |
| 5  | 7 | 180.7 MB | 1015.5 MB | 2.5% |
| 10 | 7 | 180.8 MB | 1019.1 MB | 1.0% |
| 15 | 7 | 180.8 MB | 1020.4 MB | 0.5% |
| 20 | 7 | 180.8 MB | 1016.4 MB | 1.0% |
| 25 | 7 | 180.8 MB | 1016.3 MB | 2.0% |
| 30 | 7 | 180.8 MB | 1016.3 MB | 2.0% |

RAM aplikasi naik 0.3 MB (+0.17%) selama 30 menit lalu datar; RAM pohon turun
1.8 MB. Tidak ada proses baru yang menetap (`procs` tetap 7) dan tidak ada
kenaikan terus-menerus.

Kenaikan RAM aplikasi (proses `omnix` saja) dari sampel pertama ke terakhir
dihitung di atas. Tidak ada kenaikan terus-menerus (bocor). **Baseline 0.8.0
tidak bisa diukur** karena proses 0.8.0 sudah tidak berjalan; angka pembanding
yang tersedia hanyalah sampel 0.9.1 itu sendiri. Ini keputusan yang butuh boss
bila angka 5% terhadap 0.8.0 wajib.

## Commit (sejak 0.9.0)

```
8318184 fix(handoff): a repeated report with the same status makes one event
d0ff022 release: 0.9.1
af77c0e fix(handoff): read the full /proc cmdline as the process identity
b864e8e chore: apply cargo fmt and clear the baseline clippy warnings
50feeb8 test: guard the real ~/.claude.json around cargo test
5a01db9 fix(handoff): safe force-close and identity-checked stop
1ef1ef3 fix(claude): port Orca's trust-file safety
b3aba92 fix(handoff): headless Claude worker may run omnix handoff report
a38d12c fix(handoff): new worktrees start from origin/main like Orca
b0d3fdc feat(claude): trust Omnix worktrees so the pane does not exit
56faedc fix(handoff): a started headless worker drops the dry-run receipt
efdf894 test: keep handoff and preview tests out of the real events log
3756771 fix(events): list the handoff name for handoff-created rows
7fbfcb4 feat(handoff): --orchestrator marks the run as the orchestrator pane
f070267 fix(control): ptySpawn is reachable from the control socket
cd93038 fix(handoff): a queued handoff no longer reports the dry-run receipt
318016e fix(handoff,doctor): gc drops stale index entries; doctor warns on version mismatch
c5070dc fix(handoff): force close removes the folder first, then prunes git
e9891d2 fix(handoff): close --force for stray dirs; gc forgets records whose repo is gone
7c69407 fix(cli): every subcommand answers --help without running
533f23e fix(agent): fail wait fast when the handoff was rejected
f793cc8 fix(agent): a non-zero exit is not turn-done
```

## Keputusan yang butuh boss

1. **Baseline RAM 0.8.0 tidak terukur.** Proses 0.8.0 sudah mati sebelum
   pengukuran. Yang tersedia hanya angka 0.9.1 dan buktinya datar (tidak bocor).
2. **`~/.claude.json` sempat 664** akibat binary 0.9.0 yang dipasang 11.37;
   Orchestrator sudah mengembalikannya ke 600. 0.9.1 mempertahankannya (bukti
   di atas).
3. **Laporan `hasil-perbaikan-omnix-9-10.md` di folder CRM tidak diubah** karena
   prompt 13 melarang menyentuh repo CRM. Semua isi dipindah ke laporan ini.
4. **Tag `v0.9.0` tidak pernah dibuat** sebelumnya; rilis ini hanya menandai
   `v0.9.1`.

## Catatan data pengguna

- `gc` nyata (dua kali) memangkas `~/.omnix/handoffs.json` dari 27 → 8 entri,
  lalu membuang dua entri repo uji yang rekamannya hilang. Tidak ada rekaman
  yang hilang selain path yang berkasnya sudah tidak ada.
- `~/.omnix/events.jsonl` asli bertambah selama uji ujung-ke-ujung (event
  nyata dari handoff di repo uji). Tes unit tidak menulis ke sana (dijaga
  `test-real-data-guard.sh`).
- `~/.omnix/trust-created.json` mencatat dua worktree uji; nilai sebelumnya
  `null`.