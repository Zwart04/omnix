# Bukti uji nyata — 2026-11-28 (Omnix 0.9.4)

Uji ini menjalankan TUI asli **Codex**, **Claude Code**, dan **Antigravity (`agy`)** di
dalam panel Omnix. Repo uji `/tmp/omnix-real-20261128/repo` bersifat sementara,
baca-saja, dan sudah dihapus setelah uji. HOME memakai akun asli; kuota saat mulai:
Codex 41%, Claude 7%, Antigravity 37.8%.

## Hasil

| Agen | Handoff awal | Panggilan alat | Penanda awal | `agent send` | Penanda lanjutan |
|---|---|---:|---|---|---|
| Codex CLI 0.172.0 | `real-codex`, `--effort low` | 6 × `exec` | `UJI-CODEX-ASLI-SELESAI` | diterima | `CODEX-FINAL-OK` |
| Claude Code 2.1.332 | `real-claude`, `--mode full` | 5 × `Bash` | `UJI-CLAUDE-ASLI-SELESAI` | diterima | `CLAUDE-FINAL-OK` |
| Antigravity 1.5.18 | `real-agy`, `--mode full` | 5 × `run_command` | `UJI-AGY-ASLI-SELESAI` | diterima | `AGY-FINAL-OK` |

Ketiga handoff membuka panel `ho:<name>`, mengirim brief otomatis, berpindah
`agent-working` → `agent-turn-done`, dan menerima pesan lanjutan tanpa
menempelkan prompt dua kali. Brief meminta lima perintah baca-saja terpisah;
tidak ada agen yang mengubah berkas repo.

## Transkrip sumber

- Codex:
  `/home/zwart/.codex/sessions/2026/11/28/rollout-2026-11-28T10-26-53-01a22031-079a-7d12-8a03-ca9950936cd7.jsonl`
- Claude:
  `/home/zwart/.claude/projects/-tmp-omnix-real-20261128-repo--omnix-worktrees-real-claude/f0e1bc0f-9f03-41eb-a401-82ca16b67122.jsonl`
- Antigravity:
  `/home/zwart/.gemini/antigravity-cli/brain/43293c99-0259-4b90-886b-38f6fe815e20/.system_generated/logs/transcript.jsonl`

## Perbaikan yang ditemukan oleh uji nyata

- Prompt awal Codex dikirim ke composer kosong sebelum modal *Trust this folder*
  selesai. Sekarang trust dijawab sekali, lalu composer harus stabil lima detik.
- Prompt awal Agy hilang bila dikirim terlalu awal. Batas fallback diperpanjang
  ke 60 detik dan marker composer `? for shortcuts` dipakai bila ada.
- Claude menyisipkan escape CSI di tengah teks composer (`Try<CSI>"...`).
  Pencocokan marker sekarang membuang escape dan mengubahnya menjadi pemisah
  kata, sehingga `Try "..."`, `shift+tab`, dan trust modal tetap terdeteksi.

## Catatan lingkungan

Uji nyata menulis state normal Omnix: `~/.omnix/events.jsonl`, sesi Codex/Claude/
Antigravity, dan trust worktree Claude. Repo, runtime pane, serta Xvfb uji
diisolasi/dibersihkan. `periksa.sh` dijalankan terpisah setelah uji ini dan hanya
memeriksa perubahan selama prosesnya sendiri.
