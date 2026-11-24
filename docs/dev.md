# Kembangkan Omnix

Omnix bisa mengerjakan dirinya sendiri dengan aman: agent mendapat worktree
terpisah, build diuji di folder sendiri, dan binary yang sedang dipakai tidak
pernah diganti tanpa klik pemilik.

## Alur singkat

1. Tulis brief di Settings → **Kembangkan Omnix**, atau simpan ke berkas.
2. Pilih agent: `claude`, `codex`, atau `antigravity`.
3. Klik **Buat tugas**. Omnix membuat worktree dan branch `omnix/dev-*`, lalu
   membuka agent di worktree itu. Prompt berisi brief, `CONTRIBUTING.md`, dan
   `docs/architecture.md`.
4. Setelah agent selesai, buka **Build** untuk `cargo build --release`,
   **Test** untuk `cargo test`, dan **Jalankan** untuk mencoba binary di
   `OMNIX_HOME` terpisah.
5. Kalau hasilnya bagus, pemilik menekan **Pasang build ini**. Binary lama
   disimpan untuk **Kembalikan**.

CLI yang setara:

```sh
omnix dev task --prompt-file brief.md --agent claude
omnix dev task --prompt-file brief.md --agent codex --name dev-login
omnix dev task --prompt-file brief.md --agent antigravity --no-run
omnix dev status [--json]
omnix dev build [--workspace PATH] [--debug]
omnix dev test [--workspace PATH] [--offline]
omnix dev run [--build ID]
omnix dev install [--build ID] --yes
omnix dev restore --yes
```

`dev task` tetap membuat worktree dan brief walau `--no-run` dipakai; delivery
diputuskan pemanggil. Di GUI, pane agent langsung dibuka dan brief dikirim.

## Isolasi

- Worktree: `<repo>/.omnix/worktrees/dev-*` dengan branch `omnix/dev-*`.
- Build: `~/.omnix/builds/<id>/omnix`, plus `installed.json`.
- Binary sebelumnya: `~/.omnix/builds/previous/`.
- `dev run` memakai `OMNIX_HOME=~/.omnix/dev/home`, jadi state, history, dan
  plugin pemilik tidak tersentuh.
- Agent tidak diminta menandai tag, mempublikasikan rilis, atau memasang
  binary. Prompt-nya melarang itu; pemasangan hanya lewat perintah pemilik.

## Build dan test

```sh
omnix dev build --workspace .omnix/worktrees/dev-123
omnix dev test --workspace .omnix/worktrees/dev-123
omnix dev run --build 20260101-120000
```

`dev build` menyalin binary ke folder build baru. `dev run` menjalankan build
itu sebagai proses terpisah dengan state dev; jendela utama tidak terganggu.
`dev test` menjalankan `cargo test` (dan bisa `--offline` di mesin tanpa
jaringan).

## Pasang dan kembalikan

Linux:

```sh
omnix dev install --build <id> --yes
omnix dev restore --yes
```

`--yes` adalah pengganti klik **Pasang build ini**; tanpa itu perintah menolak.
Pemasangan mengganti binary di lokasi yang sama dengan instalasi biasa,
menyimpan binary lama ke `~/.omnix/builds/previous/`, dan tidak me-restart
aplikasi secara diam-diam. Restart dilakukan pemilik. `dev restore` menukar
kembali binary sebelumnya.

Platform selain Linux menolak `dev install` dan memakai installer biasa.

## Aturan kerja

- Satu tugas, satu worktree. Jangan memakai worktree milik agent lain.
- Jalankan `cargo test` dan `node app/selftest.cjs` sebelum melapor.
- Jangan mengubah command lama, format state, atau perilaku single-agent tanpa
  alasan yang dijelaskan di brief.
- Ukur RAM/CPU idle kalau perubahan menyentuh startup atau loop GUI.
- Laporkan platform yang belum diuji dengan jujur.
- Jangan commit token, path pribadi, nama klien, atau kredensial.

## Troubleshooting

- **"not the Omnix repository"**: jalankan dari repo yang punya `Cargo.toml`
  dengan `name = "omnix"` dan `docs/architecture.md`.
- **Build gagal**: buka output build di GUI; error compiler ditampilkan utuh.
- **Binari hasil build tidak mau jalan**: cek `omnix dev status` dan pastikan
  build tidak memakai state pemilik. `dev run` selalu memakai home terpisah.
- **Ingin kembali**: `omnix dev restore --yes`, lalu restart Omnix.
