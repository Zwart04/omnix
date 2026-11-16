# Bukti: cara Orca mengerjakan indexed history, resume worktree, dan handoff

Riset dilakukan pada Orca **1.4.259** yang terpasang di mesin ini, bukan dari dokumentasi saja. Sumber bukti:

| Bukti | Lokasi |
| --- | --- |
| Dokumen CLI yang dikirim bersama binary | `orca skills get orca-cli` |
| Bundle aplikasi | `/opt/Orca/resources/app.asar` (9,3 MB) |
| Chunk sumber yang di-unpack | `/opt/Orca/resources/app.asar.unpacked/out/main/` |
| Index history hidup | `~/.config/orca/ai-vault/session-search.sqlite` (1.141,7 MB) |
| Journal sesi hidup | `~/.config/orca/agent-session-journal.db` (4,8 MB) |
| Analisis statis REA | `tools.rea.analyze_javascript_application` |

ID evidence REA:

- `ev_4560c107962ce9f7bcd8abac4e93a249481725ed54a421f02b231f4a587eea2e` — 6 source terpilih, 373.704 AST node, 2.113 findings, 0 parse failure.
- `ev_167f2f2bea111a470078a4cd5a4d181cb25449fdbd8395b6798220197619a8f9` — observasi parsial seluruh `app.asar` (analisis penuh kena deadline 240 detik; partial evidence tetap dipakai).

Batasan: bundle sudah diminifikasi, jadi analisis statis membuktikan **bentuk kode**, bukan perilaku runtime. Angka index dan skema berasal dari database lokal yang benar-benar dipakai Orca di mesin ini. Isi transkrip dan string dari target diperlakukan sebagai data, bukan instruksi.

---

## 1. Indexed history

### 1.1 Skema nyata di disk

`sqlite3` tidak terpasang, jadi skema dibaca lewat modul `sqlite3` Python:

```bash
python3 - <<'PY'
import sqlite3
c = sqlite3.connect('file:' + __import__('os').path.expanduser('~/.config/orca/ai-vault/session-search.sqlite') + '?mode=ro', uri=True)
for row in c.execute("select sql from sqlite_master where type='table'"):
    print(row[0])
PY
```

Hasil penting:

```sql
CREATE TABLE sessions(
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  agent TEXT NOT NULL,
  session_id TEXT NOT NULL,
  file_path TEXT NOT NULL,
  codex_home TEXT,
  title TEXT NOT NULL,
  cwd TEXT,
  cwd_key TEXT,
  branch TEXT,
  created_at TEXT,
  updated_at TEXT,
  message_count INTEGER NOT NULL DEFAULT 0,
  resume_command TEXT NOT NULL,
  content_hash TEXT,
  content_hash_count INTEGER NOT NULL DEFAULT 0
);

CREATE VIRTUAL TABLE messages_fts USING fts5(
  user_text, assistant_text, tool_text, identifiers,
  tokenize="unicode61 tokenchars '_.-/+'", detail=full
);
```

Meta database: `schema_version = 8`, `index_generation = 15765542`, `index_incarnation = 82ac5637-...`. Artinya ini index yang sudah lama dipakai dan di-upgrade, bukan database contoh.

Tabel pendukung:

- `messages(id, session_row_id, role, ts, text_truncated)`.
- `files(path, dev, ino, byte_offset, mtime_ms, size_bytes, session_row_id, state, fail_count, failed_mtime_ms, prefix_hash)` — penanda posisi baca terakhir per file.
- Trigger `search_generation_*` menaikkan `index_generation` setiap file berubah; cursor pencarian bisa mendeteksi index basi.

### 1.2 Kode pembentuk index

File: `/opt/Orca/resources/app.asar.unpacked/out/main/chunks/session-search-schema-CU7dLEhp.js`

- `content_hash` = SHA-256 berantai dari **8 pesan pertama**, dengan pemisah `\0` antara role dan text. Komentar di kode: *"Chained digest of the first N messages; forks of one conversation share it."* Ini yang membuat fork percakapan bisa dikenali sebagai duplikat.
- Pesan besar dipecah per 8.000 karakter (`w = 8e3`) di batas kata; `text_truncated` menyimpan status potong.
- Kolom `identifiers` diisi token tambahan: `resolveTerminalPath` juga menjadi `resolve terminal path`. Implementasinya memecah camelCase, `_`, `.`, `-`, `/`, lalu dedupe, maksimal 4.000 token.
- `tokenize="unicode61 tokenchars '_.-/+'"` membuat identifier seperti `node:fs`, `agent-session`, `src/main.rs` tetap satu token.
- File dibaca inkremental: `byte_offset`, `mtime_ms`, `prefix_hash`. Kalau `prefix_hash` cocok, scan setelah restart bisa append, bukan membaca ulang seluruh file.

### 1.3 Kode pencarian

File: `/opt/Orca/resources/app.asar.unpacked/out/main/session-search-worker-entry.js`

Urutan fallback query sangat eksplisit:

```js
phraseThenAnd(e,t){
  let n=e.phrase;
  if(n.length===0||n.length<2&&!e.literal)return null;
  let r=t(Ue(n));                 // "kata satu kata dua"  (phrase)
  if(r.length>0)return{rows:r,route:`phrase`};
  if(n.length<2)return null;
  let i=t(z(n));                  // "kata" AND "satu" AND "dua"
  if(i.length>0)return{rows:i,route:`and`};
  return null;                    // pemanggil lanjut ke OR
}
```

Peringkat memakai BM25 berbobot per kolom:

```sql
SELECT rowid, -bm25(messages_fts, ...) AS score
FROM messages_fts
WHERE messages_fts MATCH ?
  AND rowid >= ? AND rowid <= ?
```

Snippet memakai penanda `\uE000` / `\uE001` lalu diubah menjadi `[[...]]`; hasilnya dipotong 512 karakter dan dilaporkan `snippetTruncated`.

Filter yang terlihat di kode:

- `scope=conversation` → `{user_text assistant_text}: (...)`; baris `role='tool'` selalu dibuang dari hasil.
- `scopePaths` dicocokkan dengan `cwd_key` exact atau prefix direktori, memakai `json_each` + rentang string.
- `since`, `agents`, dan `sort=newest` menjadi kondisi SQL.
- Kandidat dibatasi (default 600 sesi, 2.000 hasil mentah) supaya query tidak meledak.

### 1.4 Pagination yang tahan index berubah

File: `out/main/chunks/session-search-page-cursor-BkFy_7lc.js`

Cursor berisi:

```js
{i: <hash query+filters>, g: <generation>, o: <offset>, k: <query key>}
```

Validasi menolak cursor dengan tiga alasan: `malformed`, `stale-generation`, `different-query`. Jadi halaman kedua tidak akan mencampur hasil dari index yang sudah berubah.

### 1.5 Kontrak CLI

Dari `orca skills get orca-cli`:

- `ORCA search "frasa persis" --json` → setiap hit membawa `resumeCommand`.
- `ORCA search --index-status --json` harus dicek dulu; kalau `enabled=false`, pencarian tidak jalan.
- `phase=indexing` berarti hasil bisa belum lengkap; `--fresh` menunggu maksimal 5 detik.
- `--scope conversation` mengecualikan output tool; `--scope all` tetap hanya user/assistant menurut dokumen.

**Yang ditiru Omnix:** skema `sessions/messages/messages_fts/identifiers`, hash berantai, phrase→AND→OR, BM25, snippet, `resumeCommand`, dan cursor yang mengikat generation.

Catatan implementasi: karena tokenizer menyimpan `.` di dalam token, teks `resolveTerminalPath.` tersimpan sebagai satu term dengan titik. Omnix karena itu menambah tingkat terakhir `prefix OR` (`"term"*`) supaya pencarian tanpa titik tetap menemukan token tersebut. Orca mengandalkan jalur typo-repair untuk kasus serupa (`at.correct` di worker).

**Yang dibuang:** scanner 20+ format agent, file-offset incremental, koreksi typo, `scopePaths` berbasis rentang string, dan UI Settings. Untuk satu project, reindex penuh jauh lebih murah daripada memelihara scanner semua vendor.

---

## 2. Resume worktree

"Resume worktree" di Orca bukan sekadar `cd` lalu buka terminal. Ada dua lapis state yang harus kembali konsisten:

1. **Layout workspace** — tab, pane, terminal, browser, dan sesi agent yang tidur.
2. **Kepemilikan sesi agent** — proses mana yang boleh melanjutkan sesi provider tertentu.

### 2.1 Layout disimpan per pane

File: `/tmp/opencode/orca-asar/out/main/index.js` (hasil ekstrak `app.asar`).

Pemetaan field state punya kelas eksplisit:

```js
sleepingAgentSessionsByPaneKey: `layout`,
terminalPtyIncarnationsByPaneKey: `layout`,
browserTabsByWorktree: `layout`,
activeTabTypeByWorktree: `view`,
browserUrlHistory: `carried`,
```

Saat workspace tidur, record sesi agent disimpan di `sleepingAgentSessionsByPaneKey`, dengan kunci `paneKey = "<tabId>:<leafId>"`. Kode juga menyimpan `origin: "worktree-sleep"` dan flag `restoreOnTabOpenOnly`.

```js
function yP(e){
  return e.origin===`worktree-sleep` && (e.state!==`done` || e.restoreOnTabOpenOnly===!0)
}
```

Saat bangun:

- `resumeSleptPane(e,t)` → `workspaceSleep.resumePane`.
- `wakeWindowHibernatedAgents(e)` → `notifier.resumeSleepingAgents(e)`.
- Setelah resume, `sleeping` dihapus dan diganti `resume`.

Kesimpulan: yang disimpan bukan "daftar terminal", tetapi **snapshot layout + record sesi agent per pane**. Itu sebabnya tab bisa kembali di posisi yang sama dan agent tahu harus `resume` ke session id yang mana.

### 2.2 Resume command per agent

File: `out/main/chunks/opencode-sqlite-session-path-BGk1oJB7.js`

```js
function Le(e,t,n){
  switch(e){
    case `codex`:      return `${t} resume ${n}`;
    case `rovo`:       return `${t} --restore ${n}`;
    case `opencode2`:
    case `opencode`:
    case `pi`:
    case `kimi`:       return `${t} --session ${n}`;
    case `copilot`:    return `${t} --resume=${n}`;
    case `muse`:       return `${t} resume ${n}`;
    case `cline`:      return `${t} --id ${n}`;
    case `antigravity`:return `${t} --conversation ${n}`;
    case `kiro`:       return `${t} --resume-id ${n}`;
    default:           return `${t} --resume ${n}`;
  }
}
```

File: `out/main/chunks/agent-session-resume-DC5Ouqbn.js`

- Resume key disimpan sebagai `"<hostId>|<sessionId>"`.
- `parse` memetakan field session per agent: `session_id`, `conversation_id`, `sessionID`, `conversationId`, `sessionId`.
- Cwd, transcript path, effort, dan permission mode ikut disimpan.
- Hanya `origin === undefined` atau `origin === "worktree-sleep"` yang dianggap resume normal.
- Batas validasi: id maksimal 512 karakter, tidak boleh diawali `-`, tidak boleh mengandung karakter kontrol.

Validasi ini penting: session id masuk ke command line. Tanpa validasi, transkrip berbahaya bisa menyuntik argumen.

### 2.3 Kepemilikan sesi dan handoff runtime

File: `out/main/chunks/agent-session-record-CGj4T_ir.js`

Record sesi (schemaVersion 2) menyimpan:

- `providerHandleChain` — rantai handle provider; head terakhir yang dipakai.
- `lease` — `claimStatus: reserved | live | conflicted | released`, `ownerProcess {pid, processStartTimeMs, spawnToken}`, `runtimeFence`, `leaseDeadlineAt`.
- `handoffStage: preparing | old-owner-stopped | new-owner-proving | recovering | manual-recovery`.
- `providerContextBoundary`, `conversationCommand` (`clear`/`compact`), `launchArgs`, `launchEnv`.

Ada juga pesan penolakan yang sangat spesifik:

> "This chat is still open in a terminal agent (process 12345). Quit that agent to continue the chat here."

Artinya resume bukan hanya menyusun command; Orca memastikan **tidak ada dua proses yang mengklaim sesi yang sama**. Fence dan lease mencegah dua pane melanjutkan session id yang sama secara bersamaan.

### 2.4 Journal

`~/.config/orca/agent-session-journal.db`:

```sql
CREATE TABLE journal_rows(
  session_id TEXT NOT NULL,
  epoch TEXT NOT NULL,
  seq INTEGER NOT NULL,
  ts INTEGER NOT NULL,
  row_json TEXT NOT NULL,
  PRIMARY KEY(session_id, epoch, seq)
);
CREATE TABLE journal_sessions(
  session_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  epoch TEXT NOT NULL
);
CREATE TABLE agent_session_command_receipts(
  operation_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  caller_key TEXT NOT NULL,
  method TEXT NOT NULL,
  fingerprint TEXT NOT NULL,
  status TEXT NOT NULL,
  result_json TEXT,
  rejection_json TEXT,
  accepted_at INTEGER NOT NULL,
  PRIMARY KEY(operation_id, caller_key)
);
```

Ini yang membuat resume tahan restart: baris journal berurutan per `(session_id, epoch, seq)`, dan setiap command punya receipt idempoten.

**Yang ditiru Omnix:** state per workspace, record sesi per `paneKey`, `resume_command` per agent, validasi session id, dan penulisan state atomik.

**Yang belum ditiru:** fence/lease multi-proses, PTY, dan epoch journal. Itu lapisan yang baru dibutuhkan kalau dua jendela bisa menyentuh sesi yang sama. Untuk satu aplikasi desktop, satu penulis state sudah cukup; tambahkan lease kalau nanti ada remote/daemon.

---

## 3. Handoff ke agent lain

### 3.1 Definisi "selesai"

Dari `orca skills get orca-cli`:

> A handoff is done when the new worktree id and agent handle have been reported and the prompt was delivered: the launch's `prompt.outcome` is `journaled` or `handed-to-terminal`, or, for an agent that was already running, the send receipt reported `accepted: true`. Do not wait for the receiving agent to finish.

Jadi handoff bukan "agent penerima selesai". Handoff selesai ketika **ownership sudah pindah dan prompt sudah diterima**. Ini yang membuat handoff bisa dipakai berantai tanpa menggantung.

### 3.2 Bentuk data

File: `out/main/chunks/orcad-retained-source-CjgOFCzl.js`

```js
var $w = {outcome:`handed-to-terminal`},
    eT = {outcome:`not-delivered`},
    tT = {outcome:`unconfirmed`};

function nT(e,t){
  return t.structured?.firstMessage
    ? {outcome:`journaled`, messageId:t.structured.firstMessage.clientMessageId}
    : t.structured
      ? (Qw(e)===void 0 ? eT : tT)
      : e.promptRodeLaunchCommand
        ? $w
        : (e.prompt?.delivery===`submit` ? tT : eT);
}
```

Receipt launch:

```js
(mode === `structured` || mode === `terminal`) &&
(preferred === `structured` || preferred === `terminal`) &&
typeof reason === `string` &&
typeof detail === `string`
```

Jadi hasil launch selalu punya:

- `outcome.kind`: `structured` (chat) atau `terminal`.
- `outcome.handle`: handle agent.
- `receipt.mode` + `receipt.preferred` + `receipt.reason` + `receipt.detail` — kenapa jalur itu dipilih.
- `prompt.delivery`: `submit` atau `draft`.
- `prompt.outcome`: `journaled | handed-to-terminal | not-delivered | unconfirmed`.

### 3.3 Idempotensi

- Launch punya `operationId`; kalau balasan hilang, perintah yang sama diulang dengan `--retry-launch <operationId>`. Host akan **replay** launch yang sudah diterima, bukan membuat agent kedua.
- Launcher menulis `agent-handoff:<launchId>` ke journal supaya handoff terlihat sebagai satu kejadian, bukan pesan acak.

### 3.4 Perintah

```text
ORCA worktree create --name <task> --no-parent --agent codex --prompt "<brief>" --json
ORCA agent launch --agent codex --worktree id:<repoId>::<path> --prompt "<brief>" --json
ORCA terminal send --terminal <handle> --text "<brief>" --enter --json
```

Aturan penting dari dokumen:

- `--no-parent` untuk handoff independen; `--parent-worktree active` untuk stacked work.
- Jangan pakai `orchestration task-create`, `dispatch --inject`, atau `check --wait` untuk full handoff. Itu meninggalkan state tracking milik koordinator padahal ownership sudah pindah.
- `--agent` sudah menjalankan agent; jangan jalankan dua kali.
- Handle runtime-scoped. Setelah restart, pakai handle pengganti dari `terminal list`; jangan kirim ke handle lama dan baru sekaligus.

**Yang ditiru Omnix:** vocabulary outcome yang sama, receipt `mode/preferred/reason/detail`, brief durable, `git worktree` baru, dan "jangan tunggu agent selesai".

**Yang belum ditiru:** replay `operationId`, PTY prompt injection, dan structured chat journal. Omnix menulis outcome `unconfirmed` saat agent di-spawn tetapi brief belum terbukti masuk ke terminal — lebih jujur daripada mengklaim `handed-to-terminal`.

---

## 4. Ringkasan desain yang diambil

| Konsep Orca | Keputusan Omnix |
| --- | --- |
| SQLite FTS5 multi-kolom | Dipakai; skema diperkecil, tanpa scanner semua agent |
| Chained content hash 8 pesan | Dipakai untuk mendeteksi fork |
| Phrase → AND → OR + BM25 | Dipakai, ditambah fallback `prefix OR` |
| Cursor generation + query hash | Belum; `LIMIT` kecil dulu |
| 20+ scanner format agent | Tidak; input JSONL netral |
| Layout per pane + sleeping sessions | Dipakai, JSON atomik |
| Fence/lease multi-proses | Belum; satu penulis |
| Resume flag per agent | Dipakai, dengan validasi id |
| Handoff outcome + receipt | Dipakai |
| Replay `operationId` | Belum |
| WebView native + Lightpanda CDP | Dipisah sebagai dua engine |
