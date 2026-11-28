# Daftar Pengecekan Ujung-ke-Ujung — Omnix Prompt 14 (0.9.4)

Panduan uji lengkap untuk memverifikasi ke-4 perbaikan Prompt 14 pada **Codex**, **Claude**, dan **Antigravity (`agy`)**.

> **Aturan keamanan sebelum mulai:**
> - Cek RAM tersedia minimal 1.500 MB: `free -m` (kolom `available`).
> - Jangan mengubah `~/.claude.json`, `~/.codex`, `~/.config/opencode`, atau 9router secara manual.
> - Jalankan `./docs/uji-prompt-14/periksa.sh` terlebih dahulu untuk penjaga otomatis.

---

## 0. Pengecekan Otomatis Tanpa GUI (`periksa.sh`)

```bash
cd "/home/zwart/Unduhan/Coding AI/Aplikasi & Games/omnix"
./docs/uji-prompt-14/periksa.sh
```

**Hasil yang diharapkan:**
- `free -m` `available >= 1500 MB`.
- `cargo fmt --check` lulus (exit 0).
- `cargo clippy --all-targets -- -D warnings` lulus (exit 0).
- `cargo test -- --test-threads=1` lulus (exit 0).
- `node app/selftest.cjs` lulus (exit 0, bila `node` tersedia).
- Sidik SHA-256 dan izin (`stat -c '%a'`) `~/.claude.json` **identik sebelum & sesudah**; sidik pohon `~/.omnix` **identik sebelum & sesudah**.
- Seluruh cek statis & integrasi CLI (cabang bawaan tanpa `omnix/`, `--branch`, `branch_prefix`, pesan alasan headless, uji mock PTY) berstatus `PASS`.

---

## 1. Butir 1 — Handoff Panel, `agent send`, Status Jujur, dan Tampilan Panel

### Persiapan brief panjang multibaris (±4 KB, bahasa Indonesia, ada backtick)

Buat berkas brief sementara di `/tmp/brief-uji-p14.md` (atau pakai brief `ctwa-antre-ulang` dari kasus nyata):

```bash
cat > /tmp/brief-uji-p14.md <<'EOF'
# Brief Uji Prompt 14 — Tugas Multi-Alat

Kerjakan tugas baca-saja berikut secara berurutan (minimal 5 panggilan alat terpisah, jangan digabung jadi satu perintah):

1. Cek status git dengan `git status -s` dan `git branch --show-current`.
2. Hitung jumlah baris `Cargo.toml` dan `README.md`.
3. Cari simbol `PromptDue` di `src/gui.rs`.
4. Cari simbol `branch_prefix` di `src/handoff.rs`.
5. Baca 20 baris pertama `CHANGELOG.md`.

Setelah 5 panggilan alat selesai, tulis ringkasan 5 baris singkat berisi temuan dari langkah 1-5, lalu berhenti dan tunggu instruksi lanjutan.
Jangan mengubah berkas apa pun di repo.
EOF
# Perpanjang agar >= 3,5 KB dengan daftar catatan bertanda backtick
python3 - <<'PY'
with open('/tmp/brief-uji-p14.md', 'a') as f:
    f.write('\n## Catatan Referensi Tambahan\n')
    for i in range(1, 45):
        f.write(f'- Referensi {i:02d}: pastikan berkas `src/gui.rs`, `src/pty.rs`, dan `src/handoff.rs` tidak dimodifikasi saat uji baca-saja.\n')
PY
wc -c -l /tmp/brief-uji-p14.md
```

### 1A. Handoff panel — **Codex**

1. Pastikan jendela Omnix terbuka (build hasil perbaikan OpenCode).
2. Jalankan handoff:
   ```bash
   omnix handoff uji-p14-codex \
     --repo "/home/zwart/Unduhan/Coding AI/Aplikasi & Games/omnix" \
     --base HEAD \
     --agent codex \
     --effort low \
     --prompt-file /tmp/brief-uji-p14.md \
     --run
   ```
3. **Hasil yang diharapkan:**
   - Panel `ho:uji-p14-codex` terbuka dan menampilkan TUI Codex (bukan kotak kosong `Ask Codex to do anything`, bukan tersangkut di dialog `Trust this folder?`).
   - Brief masuk utuh (terlihat user turn / `[Pasted Content ...]` sudah terkirim dan mulai `Working`).
   - Codex menjalankan **minimal 5 panggilan alat** sampai selesai tanpa disentuh, dan output kerjanya **tetap terlihat di panel** (tidak hilang saat panel dibuka atau diubah ukurannya).
   - `omnix agent status --handoff uji-p14-codex` menunjukkan `working` selama Codex benar-benar bekerja, lalu berubah menjadi `turn-done` setelah giliran selesai, dan **tidak berkedip** `agent-working`/`agent-turn-done` saat diam selama 2 menit.
4. **Uji `agent send` lanjutan (harus diterima):**
   ```bash
   omnix agent send ho:uji-p14-codex --text $'Pertanyaan lanjutan baris 1.\nBaris 2: balas tepat "LANJUT-OK".'
   ```
   - Keluaran CLI melaporkan berhasil **hanya setelah** giliran benar-benar mulai.
   - Panel Codex menerima pesan, membalas `LANJUT-OK`, lalu kembali `turn-done`.
5. **Cara membuktikan dari transkrip & event:**
   ```bash
   # Cek event Omnix (tidak ada ping-pong palsu saat diam; ada handoff-prompt-sent / agent-message-sent setelah diterima)
   omnix events --limit 40
   # Cek transkrip Codex terbaru untuk worktree uji-p14-codex (baca-saja, jangan ubah!)
   python3 - <<'PY'
   import glob, json, os
   files = sorted(glob.glob(os.path.expanduser('~/.codex/sessions/*/*/*/*.jsonl')), key=os.path.getmtime, reverse=True)
   for p in files[:15]:
       with open(p) as f:
           first = json.loads(f.readline())
       if 'uji-p14-codex' in json.dumps(first):
           print('FOUND:', p)
           user_msgs = tool_calls = 0
           with open(p) as f:
               for line in f:
                   d = json.loads(line)
                   pl = d.get('payload', {})
                   if d.get('type') == 'response_item' and pl.get('role') == 'user':
                       user_msgs += 1
                   if d.get('type') == 'response_item' and pl.get('type') in ('function_call', 'custom_tool_call'):
                       tool_calls += 1
           print(f'originator={first["payload"].get("originator")} user_msgs={user_msgs} tool_calls={tool_calls}')
           break
   PY
   ```
   - Bukti lulus: `originator` adalah TUI interaktif (bukan `codex_exec`), `user_msgs >= 2` (brief awal + `agent send`, ditambah AGENTS.md bila ada), `tool_calls >= 5`, dan `LANJUT-OK` muncul di balasan terakhir.
6. Bersihkan worktree uji:
   ```bash
   omnix handoff close uji-p14-codex --force
   ```

### 1B. Handoff panel — **Claude**

1. Jalankan handoff:
   ```bash
   omnix handoff uji-p14-claude \
     --repo "/home/zwart/Unduhan/Coding AI/Aplikasi & Games/omnix" \
     --base HEAD \
     --agent claude \
     --mode full \
     --prompt-file /tmp/brief-uji-p14.md \
     --run
   ```
2. **Hasil yang diharapkan:**
   - Panel `ho:uji-p14-claude` terbuka tanpa tersangkut di dialog trust folder.
   - Brief panjang masuk utuh dan **terkirim** (tidak menggantung di kotak input sebagai draft yang belum di-Enter).
   - Claude menyelesaikan **minimal 5 panggilan alat** tanpa disentuh; tampilan kerja tetap terlihat di panel.
   - Kirim lanjutan:
     ```bash
     omnix agent send ho:uji-p14-claude --text $'Lanjutan Claude baris 1.\nBalas tepat "CLAUDE-LANJUT-OK".'
     ```
   - Terbukti diterima di panel dan transkrip.
3. **Cara membuktikan dari transkrip & event (baca-saja):**
   ```bash
   omnix events --limit 30
   # Cari berkas sesi Claude berdasarkan sessionId dari `omnix agent status --json`
   omnix agent status --handoff uji-p14-claude --json
   # Periksa berkas ~/.claude/projects/<slug>/<sessionId>.jsonl memiliki >= 2 pesan user & >= 5 tool_use
   ```
4. Bersihkan:
   ```bash
   omnix handoff close uji-p14-claude --force
   ```

### 1C. Handoff panel — **Antigravity (`agy`)**

1. Jalankan handoff:
   ```bash
   omnix handoff uji-p14-agy \
     --repo "/home/zwart/Unduhan/Coding AI/Aplikasi & Games/omnix" \
     --base HEAD \
     --agent antigravity \
     --mode full \
     --prompt-file /tmp/brief-uji-p14.md \
     --run
   ```
2. **Hasil yang diharapkan:**
   - Panel `ho:uji-p14-agy` terbuka, brief panjang masuk utuh dan terkirim (tidak terpotong atau tertinggal belum di-Enter).
   - `agy` menyelesaikan **minimal 5 panggilan alat** tanpa disentuh, tampilan tetap terlihat di panel.
   - Kirim lanjutan:
     ```bash
     omnix agent send ho:uji-p14-agy --text $'Lanjutan AGY baris 1.\nBalas tepat "AGY-LANJUT-OK".'
     ```
   - `AGY-LANJUT-OK` muncul di panel dan event mencatat pengiriman berhasil setelah giliran mulai.
3. **Cara membuktikan:**
   - Cek `omnix events --limit 30` (`agent-started`, `handoff-prompt-sent`, `agent-message-sent`, `agent-turn-done`).
   - Cek transkrip percakapan AGY di `~/.gemini/antigravity-cli/brain/<conversation-id>/.system_generated/logs/transcript.jsonl` (baca-saja).
4. Bersihkan:
   ```bash
   omnix handoff close uji-p14-agy --force
   ```

### 1D. Uji Negatif: `not-accepted` & status `stalled`

1. **Uji `not-accepted`:**
   - Buka pane shell biasa (atau proses `sleep 60` / mock TUI yang tidak membaca input / tidak menaikkan transkrip/status) di Omnix.
   - Jalankan `omnix agent send <paneKey> --text "tes gagal"`.
   - **Hasil yang diharapkan:** Omnix mencoba ulang sekali (bila dirancang begitu), lalu mengembalikan error/status **`not-accepted` + alasan jelas** (BUKAN `sent to ...` palsu!). Event mencatat kegagalan pengiriman, bukan sukses palsu.
2. **Uji `stalled`:**
   - Set ambang stall pendek (mis. `OMNIX_STALL_SECONDS=15` bila didukung, atau gunakan unit test).
   - Mulai giliran pada proses/mock yang berhenti menulis ke transkrip & PTY tanpa sinyal selesai selama > ambang stall.
   - **Hasil yang diharapkan:** status berubah menjadi `stalled` di `omnix agent status`, tampil di panel/sidebar, dan muncul event terkait di `omnix events`.
3. **Uji Resize / Buka-tutup tab:**
   - Saat agen sudah mencetak banyak baris di panel, ubah ukuran jendela Omnix dan pindah tab lalu kembali ke tab agen.
   - **Hasil yang diharapkan:** isi kerja dan scrollback tetap terlihat, tidak mendadak kosong.

---

## 2. Butir 2 — Handoff `--headless` Terlihat di Aplikasi (Termasuk Subfolder Proyek)

1. Pastikan proyek induk di sidebar Omnix memuat folder induk repo (contoh: folder `/home/zwart/Unduhan/Claude Code` terbuka di sidebar, sementara repo handoff berada di subfolder `/home/zwart/Unduhan/Claude Code/CRM/crm-sb` — atau buat repo uji sementara di bawah folder proyek aktif).
2. Jalankan handoff headless:
   ```bash
   omnix handoff uji-p14-headless \
     --repo "/home/zwart/Unduhan/Claude Code/CRM/crm-sb" \
     --base HEAD \
     --agent codex \
     --effort low \
     --prompt "Jangan ubah berkas apa pun. Jalankan git rev-parse --short HEAD lalu lapor selesai." \
     --headless
   ```
3. **Hasil yang diharapkan (dalam ≤ 2 detik):**
   - Baris `uji-p14-headless` **muncul di sidebar** di bawah proyek induk (`Claude Code`).
   - Baris menampilkan: **nama**, **agen + model**, **status** (`working`/`stalled`/`done`/`failed`/`quota`/`queued`), **jam mulai**, dan tombol **Buka log**.
   - Klik **Buka log** membuka pane baca-saja yang mengikuti `.omnix/logs/handoff-uji-p14-headless.log` secara langsung.
4. **Cara membuktikan:**
   - `omnix agent status --handoff uji-p14-headless --json` menampilkan `log` yang menunjuk `.omnix/logs/handoff-uji-p14-headless.log` dan status sesuai tampilan sidebar.
   - Foto layar sidebar + pane log baca-saja.
5. Bersihkan:
   ```bash
   omnix handoff close uji-p14-headless --force
   ```

---

## 3. Butir 3 — `--min-free-mb`, `--after`, `--at` Tidak Memaksa Headless Diam-diam

Jalankan dengan jendela Omnix **terbuka**:

### 3A. `--min-free-mb`
```bash
# Pakai ambang sangat tinggi dulu (mis. 999999 MB) agar terlihat menunggu, lalu uji ambang wajar (mis. 100 MB)
omnix handoff uji-p14-ram \
  --repo "/home/zwart/Unduhan/Coding AI/Aplikasi & Games/omnix" \
  --base HEAD \
  --agent codex \
  --min-free-mb 999999 \
  --prompt "Balas OK" \
  --run
```
- **Hasil yang diharapkan:** Panel `ho:uji-p14-ram` **tetap dibuka** (TIDAK dipaksa headless!), menampilkan pesan seperti `menunggu RAM ≥ 999999 MB (sekarang <M> MB)`. Tutup dengan `omnix handoff close uji-p14-ram --force`, lalu ulangi dengan `--min-free-mb 100` dan pastikan langsung lanjut menjalankan agen di panel.

### 3B. `--at`
```bash
TARGET_TIME=$(date -d '+1 minute' '+%H:%M')
omnix handoff uji-p14-jadwal \
  --repo "/home/zwart/Unduhan/Coding AI/Aplikasi & Games/omnix" \
  --base HEAD \
  --agent codex \
  --at "$TARGET_TIME" \
  --prompt "Balas OK" \
  --run
```
- **Hasil yang diharapkan:** Panel `ho:uji-p14-jadwal` **tetap dibuka**, menampilkan `menunggu sampai HH:MM`, lalu otomatis mulai menjalankan agen tepat setelah jam tercapai. Bersihkan dengan `omnix handoff close uji-p14-jadwal --force`.

### 3C. `--after`
```bash
# Buat handoff pertama, lalu handoff kedua dengan --after
omnix handoff uji-p14-dep-a \
  --repo "/home/zwart/Unduhan/Coding AI/Aplikasi & Games/omnix" \
  --base HEAD --agent codex --prompt "Balas OK"
omnix handoff uji-p14-dep-b \
  --repo "/home/zwart/Unduhan/Coding AI/Aplikasi & Games/omnix" \
  --base HEAD --agent codex --after uji-p14-dep-a --prompt "Balas OK" --run
```
- **Hasil yang diharapkan:** Panel `ho:uji-p14-dep-b` **tetap dibuka** dengan status menunggu `uji-p14-dep-a`, lalu mulai berjalan setelah `omnix handoff report uji-p14-dep-a --status done --text "ok"` atau `close`.

### 3D. Saat GUI tidak jalan (wajib headless)
- Jika Omnix GUI tidak berjalan (atau saat `--headless` diminta eksplisit), CLI **mencetak alasan dengan jelas** mengapa berjalan headless (mis. `mode: headless (GUI tidak aktif)`).

---

## 4. Butir 4 — Nama Cabang Tanpa Kata `omnix`

Uji di repo sementara atau dengan dry-run (tanpa `--run`):

1. **Bawaan tanpa `--branch`, tanpa `--role`, tanpa `.omnix/config`:**
   ```bash
   omnix handoff uji-cabang-bawaan --repo /tmp/repo-uji --base HEAD --agent codex --prompt "tes"
   ```
   - **Hasil yang diharapkan:** `branch:` TIDAK mengandung `omnix/` (mis. `kerja/uji-cabang-bawaan`).
2. **Override `--branch NAME`:**
   ```bash
   omnix handoff uji-cabang-kustom --repo /tmp/repo-uji --base HEAD --branch fix/tautan-w-landing --agent codex --prompt "tes"
   ```
   - **Hasil yang diharapkan:** `branch: fix/tautan-w-landing` dan `git -C /tmp/repo-uji branch --list fix/tautan-w-landing` ada.
3. **Setelan per repo `.omnix/config` (`branch_prefix`):**
   ```bash
   mkdir -p /tmp/repo-uji/.omnix
   printf 'branch_prefix = fitur/\n' > /tmp/repo-uji/.omnix/config
   omnix handoff uji-cabang-cfg --repo /tmp/repo-uji --base HEAD --agent codex --prompt "tes"
   ```
   - **Hasil yang diharapkan:** `branch: fitur/uji-cabang-cfg`.
4. **Pindah pane ke worktree (`worktree_for_pane` / `paneMoveWorktree`):**
   - Pastikan cabang yang dibuat tidak lagi hardcode `omnix/...` melainkan mengikuti `branch_prefix` repo / bawaan tanpa kata `omnix`.

---

## Ringkasan Bukti yang Wajib Dilampirkan di `docs/laporan-0.9.4.md`

- Keluaran `docs/uji-prompt-14/periksa.sh` (termasuk `fmt`, `clippy`, `test`, sidik `~/.claude.json`).
- Bukti sebelum & sesudah per butir (1–4).
- Foto layar panel Omnix sungguhan yang menampilkan kerja **Codex**, **Claude**, dan **Agy** (masing-masing ≥ 5 panggilan alat + 1 `agent send`).
- Foto layar sidebar menampilkan handoff `--headless` dari repo subfolder + pane **Buka log**.
- Foto layar panel menunggu `--min-free-mb` / `--at` / `--after`.
