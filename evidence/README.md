# Evidence

Riset penuh ada di `docs/orca-evidence.md`. Folder ini menyimpan cara mereproduksi buktinya, bukan salinan kode Orca.

## Artefak yang diperiksa

| Artefak | SHA-256 |
| --- | --- |
| `/opt/Orca/resources/app.asar` | `df35b52e9d4fdf278f73b0bac34758817b89f88ce58fe5f2e8d108bf0d128f47` |
| `~/.config/orca/ai-vault/session-search.sqlite` | dibaca langsung; `schema_version=8`, `index_generation=15765542` |
| `~/.config/orca/agent-session-journal.db` | dibaca langsung; tabel `journal_rows`, `agent_session_command_receipts` |

## REA evidence

- `ev_4560c107962ce9f7bcd8abac4e93a249481725ed54a421f02b231f4a587eea2e` — analisis 6 source terpilih: schema index, cursor, limit, resume, session record, operation ledger.
- `ev_167f2f2bea111a470078a4cd5a4d181cb25449fdbd8395b6798220197619a8f9` — partial analysis seluruh `app.asar` (deadline 240 detik).
- `ev_a2d0a096b7a04888acc75e596b28e470d8d85461b6186be789f7bb07f504b7b0` — view summary turunan.

Catatan: export lengkap menghasilkan 386 MB karena membawa seluruh graph. Tidak disimpan di repo. Untuk mendapatkannya lagi, jalankan ulang langkah di bawah.

## Reproduksi

```bash
# 1. Dokumen CLI
orca skills get orca-cli > /tmp/orca-cli.md

# 2. Skema index hidup
python3 - <<'PY'
import sqlite3, os
p = os.path.expanduser('~/.config/orca/ai-vault/session-search.sqlite')
c = sqlite3.connect('file:' + p + '?mode=ro', uri=True)
for (sql,) in c.execute("select sql from sqlite_master where type='table'"):
    print(sql)
PY

# 3. Kode yang di-unpack
ls /opt/Orca/resources/app.asar.unpacked/out/main/chunks/ | grep -E 'session|search|resume'
grep -o -E '.{300}resumeCommand.{300}' \
  /tmp/opencode/orca-asar/out/main/index.js | head

# 4. Analisis statis ulang lewat REA
# tools.rea.analyze_javascript_application({
#   input_path: "/opt/Orca/resources/app.asar",
#   source_paths: [
#     "out/main/chunks/session-search-schema-CU7dLEhp.js",
#     "out/main/chunks/session-search-page-cursor-BkFy_7lc.js",
#     "out/main/chunks/ai-vault-search-limit-B45ItwQd.js",
#     "out/main/chunks/agent-session-resume-DC5Ouqbn.js",
#     "out/main/chunks/agent-session-record-CGj4T_ir.js",
#     "out/main/chunks/agent-session-operation-ledger-CNvGRy7X.js"
#   ],
#   detail: "summary"
# })
```

## Batasan

- Bundle diminifikasi; analisis statis membuktikan bentuk kode, bukan perilaku runtime.
- Database lokal adalah state mesin ini, bukan fixture.
- Potongan kode di laporan adalah kutipan pendek untuk menjelaskan mekanisme, bukan salinan implementasi.
