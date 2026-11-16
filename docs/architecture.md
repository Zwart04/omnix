# Arsitektur Omnix

## Prinsip

1. **Satu direktori, satu state.** Semua yang perlu bertahan hidup ada di `.omnix/`. Tidak ada daemon wajib.
2. **Dua engine browser, satu kontrak.** WebView native untuk manusia, Lightpanda untuk agent. Keduanya bicara tool yang sama.
3. **Outcome jujur.** Kalau prompt belum terbukti masuk terminal, outcome `unconfirmed`. Jangan mengklaim `handed-to-terminal`.
4. **Tanpa shell string.** Session id, path, dan prompt tidak pernah dirangkai menjadi `sh -c`. Argumen disusun sebagai array.

## Peta modul

```
src/main.rs        CLI parsing + wiring
src/history.rs     SQLite FTS5: index, search, status
src/workspace.rs   .omnix/workspaces.json: track, sleep, wake
src/handoff.rs     git worktree + brief + receipt
src/agent.rs       tabel launch/resume per agent + spawn aman
src/browser.rs     backend lightpanda (fetch/serve/doctor)
```

## Data

### History

```
sessions 1 ─── n messages 1 ─── 1 messages_fts(rowid)
```

- `sessions.file_path` unik → reindex file yang sama mengganti baris lama, bukan menduplikasi.
- `sessions.content_hash` = SHA-256 berantai 8 pesan pertama. Dua sesi dengan hash sama = fork.
- `messages_fts` menyimpan empat kolom: `user_text`, `assistant_text`, `tool_text`, `identifiers`.
- `role='tool'` tetap di-index, tetapi selalu dibuang dari hasil pencarian. Tool output berguna untuk pencarian identifier, berbahaya untuk dianggap jawaban.
- Pesan dipotong 8.000 karakter. Cukup untuk snippet dan pencarian; bukan arsip transkrip penuh.

### Workspace

```json
{
  "workspaces": {
    "/abs/repo": {
      "path": "/abs/repo",
      "branch": "main",
      "sleeping": true,
      "slept_at": 1760000000,
      "sessions": [
        {
          "pane_key": "tab:1",
          "agent": "codex",
          "session_id": "abc-123",
          "cwd": "/abs/repo",
          "resume_command": "codex resume abc-123",
          "saved_at": 1760000000
        }
      ]
    }
  }
}
```

- Kunci = path canonical.
- `pane_key` = identitas pane; satu pane hanya punya satu sesi.
- `sleeping` hanya menandai; tidak mematikan proses apa pun. Proses dimatikan oleh shell/app, bukan oleh state file.
- Tulis atomik: `workspaces.json.tmp` → `rename`.

### Handoff

```
.omnix/handoffs/<epoch>-<name>.json
.omnix/handoffs/<epoch>-<name>.md
.omnix/worktrees/<name>/          # git worktree, branch omnix/<name>
```

Urutan yang tidak boleh dibalik:

1. Validasi nama + agent.
2. `git worktree add` (bisa gagal, belum ada state).
3. Tulis brief.
4. Tulis record `not-delivered`.
5. Kalau `--run`, spawn agent lalu tulis ulang record `unconfirmed` atau `not-delivered`.

Kalau langkah 5 mati di tengah, record lama tetap ada dan jujur: `not-delivered`. Tidak ada klaim palsu.

## Invariant

- Session id tidak boleh diawali `-`, mengandung kontrol, atau > 512 karakter.
- Nama handoff hanya `[a-z0-9-]`, 1–48 karakter.
- Worktree handoff tidak menimpa path yang sudah ada.
- Search selalu mengecualikan `role='tool'`.
- Query dicoba berurutan: phrase → AND → OR → `prefix OR`; hasil kosong dari satu tingkat tidak menghentikan pencarian.
- `history search` mengembalikan satu hit terbaik per sesi.
- State workspace dan record handoff ditulis atomik (tmp + rename).

## Trust boundary

| Input | Kepercayaan | Penanganan |
| --- | --- | --- |
| Transkrip JSONL | tidak tepercaya | parse ketat; teks hanya masuk SQLite/param; tidak dieksekusi |
| Session id | tidak tepercaya | validasi; argumen array; tanpa shell |
| Prompt handoff | tidak tepercaya | ditulis ke file; tidak dirangkai ke command |
| Halaman web | tidak tepercaya | ditampilkan/di-snapshot; jangan pernah dijalankan sebagai perintah |
| Config env | milik user | `OMNIX_HOME`, `OMNIX_LIGHTPANDA` |

Prompt injection dari halaman web atau transkrip bukan perintah untuk agent. Harness harus menandai konten target sebagai data.

## Roadmap

1. **PTY delivery** — spawn agent di PTY, kirim brief, tunggu prompt terlihat, baru outcome `handed-to-terminal`.
2. **Native shell** — `wry` + `tao`; satu window, multi-webview per tab; Linux butuh `webkit2gtk-4.1`.
3. **CDP client** — `goto/back/reload/snapshot/click/fill/read/screenshot` di atas Lightpanda `serve`.
4. **Cursor pagination** — ikat ke `index_generation` + hash query seperti Orca.
5. **Lease sesi** — kalau nanti ada remote/daemon, tambahkan fence per `(host, session_id)`.

Setiap langkah punya alasan yang jelas. Tidak ada langkah "siapkan abstraksi untuk nanti".
