# Orca Parity — rencana rewrite UI/UX Omnix di Rust

Status: rencana kerja (belum implementasi). Target: Omnix meniru Orca 1.4.259
secara struktur, perilaku, dan kepadatan informasi, bukan menyalin kode.

---

## 1. Tujuan dan ruang lingkup

Omnix sekarang: satu jendela `wry` + `tao`, HTML/CSS/JS tertanam, empat zona
(rail, sidebar, pane, panel kanan), terminal `portable-pty` + xterm, riwayat
agent di SQLite FTS5, handoff, browser native/lightpanda.

Rencana ini menutup jarak ke Orca pada:

- bahasa visual (token warna, font, radius, kerapatan),
- anatomi jendela (titlebar, sidebar, tab strip, pane, panel kanan, status bar),
- fitur per zona (project/worktree tree, session rows, panel Agents, source
  control, review, remote/SSH, usage, plugin, mobile),
- perilaku interaksi (fokus keyboard, split pane, drag tab, palet, pintasan).

Non-goals fase awal: implementasi penuh plugin marketplace, Orca Cloud/akun,
dan mobile pairing; ketiganya masuk fase akhir dengan cakupan minimum.

---

## 2. Identitas target dan metode bukti

| Item | Nilai |
| --- | --- |
| Aplikasi | Orca (stablyai) |
| Versi | `1.4.259` (`package.json` di dalam ASAR) |
| Artefak | `/opt/Orca/resources/app.asar` |
| SHA-256 | `df35b52e9d4fdf278f73b0bac34758817b89f88ce58fe5f2e8d108bf0d128f47` |
| Ukuran | 9.726.207 byte |
| Runtime | Electron (Chromium + Node), `main`, `preload`, `renderer`, `cli` |
| Renderer | 1072 chunk JS, 9 berkas CSS, tanpa source map |

Metode: ASAR diekstrak ke `/tmp/opencode/orca-asar` (header pickle + offset
entri), lalu dibaca `out/renderer/index.html`, `assets/*.css`, dan nama chunk
`assets/*.js`. Semua temuan di bawah berasal dari artefak terkirim (statis),
bukan dari eksekusi aplikasi. Batasan: kode sudah diminifikasi, tidak ada
source map, jadi perilaku runtime hanya disimpulkan dari CSS, nama chunk, string
UI, dan tangkapan layar Orca milik pengguna (3072×1664 piksel perangkat, ~1.6×
skala logis 1920×1040).

Dependensi runtime yang terbaca di `package.json` (menentukan fitur yang ada):
`node-pty`, `@xterm/headless`, `@xterm/addon-serialize`, `ssh2`, `ws`,
`agent-browser`, `@parcel/watcher`, `@linear/sdk`, `serve-sim`, `electron-updater`,
`i18next`/`react-i18next`, `posthog-node` (telemetri), `qrcode` (pairing mobile).

---

## 3. Design system (hasil ekstraksi CSS)

### 3.1 Font

```
--font-sans: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto,
             "Helvetica Neue", "Noto Sans", Arial, sans-serif,
             "Apple Color Emoji", "Segoe UI Emoji", "Segoe UI Symbol";
--font-mono: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
             "Liberation Mono", "Courier New", monospace;
```

Catatan Omnix: `--sans: system-ui` di WebKitGTK Linux jatuh ke fallback jelek.
Ganti ke stack di atas, sisipkan `Cantarell` sebelum `"Noto Sans"` untuk GNOME.

### 3.2 Skala tipe

| Token | Nilai | Pakai |
| --- | --- | --- |
| `--text-xs` | .75rem (12px) | meta, badge |
| `--text-sm` | .875rem (14px) | **default UI** |
| `--text-base` | 1rem (16px) | judul kecil |
| `--text-lg` | 1.125rem (18px) | judul panel |
| `--text-xl` | 1.25rem (20px) | judul modal |

### 3.3 Radius, spasi, bobot

| Token | Nilai |
| --- | --- |
| `--radius` | .625rem (10px) |
| `--radius-sm` | `calc(var(--radius) * .6)` = 6px |
| `--radius-md` | `calc(var(--radius) * .8)` = 8px |
| `--radius-xl` | `calc(var(--radius) * 1.4)` = 14px |
| `--radius-xs` | .125rem (2px) |
| `--spacing` | .25rem (4px), kelipatan 4 |
| bobot | 300 / 400 / 500 / 600 / 700 |

### 3.4 Warna tema

| Token | Terang | Gelap |
| --- | --- | --- |
| `--background` | `#ffffff` | `#0a0a0a` |
| `--foreground` | `#0a0a0a` | `#f0f0f0` |
| `--card` | `#ffffff` | `#171717` |
| `--popover` | `#ffffff` | `#171717` |
| `--primary` | `#171717` | `#e5e5e5` |
| `--secondary` | `#f5f5f5` | `#262626` |
| `--muted` | `#f5f5f5` | `#262626` |
| `--muted-foreground` | `#737373` | `#acacac` |
| `--accent` | `#f5f5f5` | `#404040` |
| `--destructive` | `#e40014` | `#e40014` |
| `--border` | `#e5e5e5` | `rgba(255,255,255,.07)` |
| `--input` | `#e5e5e5` | `rgba(255,255,255,.15)` |
| `--ring` | `#a1a1a1` | `#737373` |
| `--sidebar` | `#fafafa` | turunan `--background` |
| `--editor-surface` | — | `#1e1e1e` |
| `--floating-surface` | — | `rgba(0,0,0,.72)` |
| `--dialog-surface` | — | `rgba(23,23,23,.96)` |
| `--tab-active` | campuran `foreground` 6% | idem |

Palet aksen semantik tersedia penuh (red/orange/amber/yellow/green/emerald/
sky/blue/indigo/violet/purple/fuchsia/rose/slate/gray/zinc/neutral) untuk
status agent, diff, review.

Implikasi untuk Omnix: buang hitam pekat `#000` sebagai `--bg`; pakai
`#0a0a0a` (kanvas) dan `#171717` (panel/kartu), border hairline 7% putih,
radius kontrol 6–8px, kartu 10px.

---

## 4. Anatomi jendela (terukur dari tangkapan layar)

Skala logis 1920×1040:

```
┌──────────────────────────────────────────────────────────────────────────┐
│ titlebar   (h≈29)  [traffic lights] [⟨] …                                │
├───────────────┬──────────────────────────────────────┬───────────────────┤
│ sidebar 275px │ tab strip (h≈30)                     │ panel kanan 274px │
│               ├──────────────────────────────────────┤                   │
│ Search        │                                      │ [icons] Agents    │
│ Tasks         │                                      │ Indexed history   │
│ Automations   │              pane                    │ Worksp|Project|All│
│ Orca Mobile   │           (terminal)                 │ Search sessions   │
│               │                                      │ Scanned 70 files  │
│ Projects  +   │                                      │ 1 session         │
│  ▸ repo       │                                      │ ▸ united     (1)  │
│    ⑂ branch   │                                      │  Sesi Baru …      │
│      session  │                                      │                   │
│               │                                      │                   │
├───────────────┴──────────────────────────────────────┴───────────────────┤
│ status bar (h≈29) Used · 100% · 1d 1h · 8% Gemini wk · On · 2 · 0 hosts  │
└──────────────────────────────────────────────────────────────────────────┘
```

Ukuran kunci: sidebar 275, panel kanan 274, tab strip ~30–34, status bar ~29,
rail tidak ada di Orca (ikon pindah ke dalam sidebar dan panel kanan).

### 4.1 Titlebar

- Kiri: kontrol jendela (macOS traffic lights; Windows/Linux tombol native),
  tombol collapse sidebar, logo, judul workspace.
- Kanan: indikator foreground/pane, tombol Command.
- Kelas bukti: `titlebar`, `titlebar-left`, `titlebar-logo`, `titlebar-app-name`,
  `titlebar-icon-button`, `titlebar-traffic-light-pad`, `titlebar-spacer`.

### 4.2 Sidebar

- Kotak `Search` (bukan ikon saja), lalu nav: **Tasks**, **Automations**,
  **Orca Mobile** (dengan toggle/koneksi), masing-masing dengan ikon 16px.
- Header **Projects** + empat ikon aksi (agent dashboard, layout, terminal, `+`).
- Baris repo: ikon folder + nama; bisa emoji (`RepositoryIconEmojiPicker`).
- Baris worktree: ikon branch + nama branch + pill `primary`, path, ikon manifest.
- Baris sesi: glyph agent (`OC`, `C`, `G`), judul sesi, waktu relatif, tombol
  buka/resume; bisa dikelompokkan per worktree.
- Footer: settings, help, indikator daftar status, tombol collapse panel kanan.
- Kelas bukti: `sidebar`, `sidebar-header`, `sidebar-toggle`, `sidebar-accent`,
  `sidebar-active`, `sidebar-scrollbar`, `sidebar-notice-card`, `sidebar-live-width`.

### 4.3 Tab strip

- Tab pill: ikon agent + judul + `×`, aktif = permukaan terang 6% + border.
- `+` di kanan tab terakhir; `Command` pill di kanan strip.
- Tab bisa di-drag, di-close tengah, dan punya menu konteks (split, popout).
- Kelas bukti: `tab-bar`, `--tab-active`, chunk `activate-tab-and-focus-pane`,
  `tab-change-through-layout-owner`, `terminal-surface-id`.

### 4.4 Pane

- Grid split: 1 pane penuh, 2 kolom, 2 baris, 4 kuadran (`WorkspaceSpacePage`,
  `WorkspacePaneOverlayLayers`, `FloatingTerminalPanel`).
- Tiap pane punya header tipis saat lebih dari satu pane: judul, status agent,
  tombol maximuze/close.
- Terminal memakai xterm dengan palet per OS, kanvas gelap `#0a0a0a`/`#1e1e1e`.

### 4.5 Panel kanan

- Kolom ikon vertikal: clipboard, agent dashboard, source control (branch),
  daftar tugas/notes.
- Panel `Agents`: judul + `Indexed history`, aksi (riwayat, filter, refresh),
  segmented `Worksp… | Project | All`, input `Search sessions`,
  baris `Scanned 70 files · Just now`, header `1 session · Last updated`,
  grup per project (`united  1`), kartu sesi: judul, preview 2 baris,
  `Agent: …`, statistik `2116 msgs · 29m ago · oc/d…`, menu `…`.
- Fase lanjutan: clipboard, source control, tasks memakai panel yang sama.

### 4.6 Status bar

Segmen kiri: `Used` + ikon jam + `100% 1d 1h` + `8% Gemini wk` + tombol refresh.
Segmen kanan: ikon layout, `On` (koneksi), toggle panel, ikon commit, jumlah
`2`, `0 hosts`. Kelas bukti: `StatusBar`, `StatusIndicator`,
`ResourceUsageStatusSegment`, `PortsStatusSegment`, `PetStatusSegment`,
`SshStatusSegment`, `status-bar-usage-cluster`.

---

## 5. Inventaris fitur (dari 1000 nama chunk unik)

Dikelompokkan menurut domain. Nama di kurung adalah chunk/komponen bukti.

### 5.1 Workspace, project, worktree
- Project dari folder, repo git, dan folder non-git (`AddRepoDialog`,
  `AddProjectFromFolderDialog`, `NonGitFolderDialog`, `CreateProjectLocationField`,
  `SetProjectLocationDialog`, `ProjectAddedDialog`, `RemoveFolderDialog`).
- Worktree: buat, hapus, jump, metadata, visibilitas, setup steps
  (`WorktreeCreationPanel`, `DeleteWorktreeDialog`, `WorktreeJumpPalette`,
  `WorktreeMetaDialog`, `WorktreeSetupSteps`, `WorktreeVisibilityDialog`,
  `WorktreeVisibilitySourceList`, `WorktreeOpenInMenu`, `WorktreeHostContextBadge`).
- Branch: pemilih base ref, badge repo, salinan branch
  (`BaseRefPicker`, `PreservedBranchBatchReviewModal`, `RepoBadgeLabel`).
- Workspace cleanup, emoji, space page, attachment origins
  (`WorkspaceCleanupDialog`, `WorkspaceEmojiPickerPanel`, `WorkspaceSpacePage`,
  `WorkspaceAttachmentOrigins`).

### 5.2 Tab, pane, layout
- Split pane, overlay layer, floating terminal, focus pane
  (`FloatingTerminalPanel`, `FloatingTerminalIconContextMenu`,
  `WorkspacePaneOverlayLayers`, `activate-tab-and-focus-pane`,
  `tab-change-through-layout-owner`, `terminal-surface-id`).
- Quick open dan palet (`QuickOpen`, worktree jump, `ClaudeSwitcherMenu`).

### 5.3 Terminal
- `Terminal` chunk, `@xterm/headless` + `addon-serialize` untuk restore,
  right-click paste gesture, IME overlay escape, paste draft agent.
- Preferensi metrik pane (`pane-metric-options-deferral`), restorasi sesi
  (`session-restored-banner`).

### 5.4 Agent
- Katalog agent, launch picker, setup skill, status primitives, lineage
  (`agent-catalog`, `AgentLaunchPicker`, `AgentSkillSetupPanel`,
  `agent-status-primitives`, `agent-row-lineage`, `AgentDefaultSurfacePreviews`,
  `AgentsOrchestrationVisual`, `OrchestrationCrewPanel`).
- Dashboard agent dan daftar sidebar (`AgentDashboardSidebarEntry`,
  `AgentDashboardSidebarHost`, `SidebarAgentsList`).
- Chat native, composer, usage dialog, history
  (`NativeChatComposer`, `NativeChatAccountUsageDialog`, `NativeChatHighlightedCode`,
  `PendingNativeChatCreation`, `agent-session-chat-label`).
- Kegagalan/penolakan sesi, label alasan kirim, interrupt key
  (`agent-session-failure`, `agent-session-refusal-notice`, `agent-send-disabled-reason-label`,
  `agent-interrupt-key-event`, `agent-awake-copy`).
- Fork/clone percakapan (`claude-conversation-fork-worker-entry`,
  `claude-profile-setup-worker-entry`).

### 5.5 Riwayat dan pencarian
- Panel Agents (indexed history) dengan scope Workspace/Project/All, pencarian,
  scan counter, kartu sesi dengan statistik
  (`AiVaultSessionHistoryDialog`, `AiVaultSearchEvidence`, `AiVaultPanel`,
  `AiVaultSidebarPanel`, `ai-vault-session-drag`, `ai-vault-session-deletion`,
  `ai-vault-session-time`).

### 5.6 Source control, diff, review
- `SourceControl`, `FolderWorkspaceSourceControlPanel`,
  `FolderWorkspaceWorktreesPanel`, `FolderWorkspacePrChecksPanel`,
  `CombinedDiffViewer`, `DiffViewer`, `ImageDiffViewer`,
  `DiffCommentCard`, `DiffCommentPopover`, `DiffNotesSendMenu`,
  `CommentMarkdown`, `CommentReactions`, `ChecksPanel`,
  `SourceControlAgentActionDialog`, `ai-commit-pr-picker`.

### 5.7 Browser dan viewer
- Pane browser + devtools, screenshot, konsol; viewer media dan dokumen:
  `ImageViewer`, `CsvViewer`, `CsvPagedViewer`, `IpynbViewer`, `MarkdownPreview`,
  `MarkdownPreviewBody`, `MermaidViewer`, `DiagramViewerModal`, `MediaViewer`,
  `PanZoomImageDialog`, `MonacoEditor`, `MonacoCodeExcerpt`, `EditorPanel`,
  `FileExplorer`, `RichMarkdownEditor`.

### 5.8 Remote, SSH, host
- `HostCombobox`, `SshTargetCard`, `SshPassphraseDialog`, `SshStatusSegment`,
  `ForgetSshWorkspaceDialog`, `OpenPlainSshTerminalButton`,
  `RemoteServerUpdateDialog`, `RemoteServerUpdateStatus`, `WslHostMark`,
  `WslHostStoppedNotice`, `NodeDistMirrorDialog`.

### 5.9 Mobile, integrasi, plugin
- `MobilePage`, `qrcode` pairing, `DashboardPopoutBridge`, `Orca Mobile` toggle.
- Integrasi Linear/Jira/GitHub (`LinearIcon`, `JiraIcon`, `GitHubIcon`,
  `LinearAgentSkillSetupDialog`, `[linear](https://linear.app)` SDK).
- Plugin: `PluginPanel`, `PluginDeclarativeView`, `PluginShotReplaySurface`,
  `plugin-language-pack-i18n-resources`, `orca-plugin-image` protocol.
- Automations dan Tasks (`AutomationsPage`, `TaskPage`, `ArtifactsPage`,
  `ActivityPrototypePage`, `activity-thread-list-pane`).

### 5.10 Status, usage, biaya
- `ResourceUsageStatusSegment` (kuota 100% + sisa waktu), `8% Gemini wk`,
  `PortsStatusSegment`, `PetStatusSegment`, `Usage` dialog
  (`NativeChatAccountUsageDialog`, `CodexResetCredits`, `ResourceUsageStatusSegment`).

### 5.11 Settings, onboarding, tema
- `Settings` + form controls, keymap, tema, font, notifikasi, telemetri
  (`SettingsFormControls`, `orcad-template`, `OrcaYamlTrustDialog`).
- Onboarding: `OnboardingFlow`, `Landing`, `SetupGuideModal`,
  `FeatureWallModal`, `FeatureTipsModal`, `ContextualTourOverlay`,
  `ChatUiHereCornerCard`, `OnboardingInlineCommandTerminal`.
- Pet overlay (`PetOverlay`, `PetStatusSegment`), dictation (`DictationController`).

---

## 6. Matriks gap Omnix → Orca

| Zona | Omnix sekarang | Orca | Prioritas |
| --- | --- | --- | --- |
| Tema | hitam `#000`, radius 6–8 | `#0a0a0a`/`#171717`, radius 6–14 | P0 |
| Font | `system-ui` fallback buruk | stack eksplisit + mono | P0 |
| Titlebar | titlebar native OS | titlebar kustom + collapse | P1 |
| Sidebar | rail 44 + Projects ringkas | nav Tasks/Automations/Mobile + tree repo→branch→sesi | P1 |
| Tab strip | pill + `+` + Command | sama + drag, split, popout | P2 |
| Pane | 1 terminal/tab | split grid, floating, overlay | P2 |
| Terminal | xterm, fokus diperbaiki | + restore serialisasi, right-click paste, IME | P3 |
| Panel kanan | Agents saja | 4 modul (clipboard, dashboard, SCM, tasks) | P4 |
| Riwayat | FTS5, scope all/project | scope Worksp/Project/All, kartu kaya | P4 |
| Source control | belum ada | status, diff, komentar, PR checks | P5 |
| Remote/SSH | belum ada | host, passphrase, WSL | P6 |
| Usage/status | teks kecil | segmen usage, ports, pet, koneksi | P6 |
| Plugin/integrasi | belum ada | plugin panel, Linear/Jira/GitHub | P7 |
| Onboarding | kosong | tour, tips, setup guide | P7 |

---

## 7. Arsitektur target (Rust, tanpa Electron)

Tetap `tao` + `wry` + `portable-pty` + `rusqlite`. UI tetap HTML/CSS/JS
tertanam (bukan React) supaya build tetap satu binary.

Modul baru/ubah:

```
src/
  gui.rs         # jendela, IPC registry, bounds konten
  layout.rs      # state pane tree: split, tab order, floating, persist
  projects.rs    # project, worktree, branch, status git
  agents.rs      # (dari agent.rs) katalog, launch, availability, lineage
  sessions.rs    # index transkrip, kartu sesi, scope, resume command
  git.rs         # status, diff, commit, PR checks (via git CLI)
  usage.rs       # sampel token/kuota provider + agregasi harian
  hosts.rs       # SSH/WSL target, passphrase via keyring
  notes.rs       # notes/comments per diff line + kirim ke agent
  statusbar.rs   # segmen: usage, ports, host, koneksi
app/
  index.html     # shell: titlebar, sidebar, strip, panes, panel, statusbar
  style.css      # token Orca (bagian 3) + layout grid
  app.js         # render zona, event, palet, keybinds
```

Persistensi SQLite baru (satu DB di `OMNIX_HOME/.omnix/omnix.sqlite`):

```sql
projects(id, path, name, emoji, added_at, last_opened_at)
worktrees(id, project_id, path, branch, is_primary, created_at)
panes(id, workspace_id, kind, title, split_tree_json, tab_order, floating, rect)
sessions(id, agent, session_id, cwd, worktree_id, title, updated_at, resume_cmd)
notes(id, session_id, file, line, body, sent_at)
hosts(id, kind, name, target, created_at)
usage(id, provider, model, day, tokens_in, tokens_out, quota_pct)
```

IPC: satu `call(method, args)` JSON yang sudah ada; tambah method
`projects.list`, `worktrees.list`, `git.status`, `git.diff`, `usage.summary`,
`hosts.list`, `panes.save`, `panes.load`, `notes.add`, `notes.send`.

---

## 8. Rencana implementasi step by step

### P0 — Fondasi visual (1 PR, ±1 hari)

1. Ganti token warna di `app/style.css` ke tabel 3.4 (gelap + terang).
2. Ganti `--sans`/`--mono` ke stack Orca + Cantarell.
3. Set `--radius` 10px, kontrol 6–8px, skala tipe 12/14/16.
4. Hapus rail 44px; pindahkan ikon ke sidebar dan panel kanan.
5. Grid shell: `grid-template-columns: 275px 1fr 274px;`
   baris `29px 1fr 29px`.
6. Acceptance: `node app/selftest.cjs`; screenshot 1240×820 tanpa overflow;
   teks UI ≥14px; tidak ada fallback font DejaVu.

### P1 — Titlebar + sidebar (1 PR)

1. Titlebar kustom: logo, judul, collapse sidebar, tombol Command.
2. Sidebar: Search, Tasks, Automations, Orca Mobile (toggle), header Projects
   + 4 aksi, baris repo→branch→sesi, footer.
3. `git.rs`: baca branch aktif, ahead/behind, dirty count per worktree.
4. `projects.rs`: CRUD project/folder/worktree, simpan ke SQLite.
5. Acceptance: repo nyata tampil dengan branch + pill `primary` + baris sesi;
   klik sesi membuka tab agent; collapse sidebar menggeser grid.

### P2 — Tab dan pane (1 PR)

1. `layout.rs`: pane tree (leaf/split), tab order, floating flag.
2. Tab strip: drag reorder, close tengah, menu konteks (split kanan/bawah,
   popout, rename).
3. Split: 1 / 2 kolom / 2 baris / 4 kuadran, masing-masing pane punya pty.
4. Persist layout ke tabel `panes`, restore saat start.
5. Acceptance: split 4 pane, tiap pane terima ketikan, restart memulihkan
   susunan dan tab.

### P3 — Terminal parity (1 PR)

1. Serialisasi buffer (`@xterm/addon-serialize` setara: simpan byte log) untuk
   restore setelah restart.
2. Right-click paste + link detection + search-in-terminal.
3. IME: overlay composition, Escape menutup.
4. Palet per OS sudah ada; tambah opsi tema di Settings.
5. Acceptance: `top`/Codex/OpenCode TUI jalan di 4 OS; paste via klik kanan;
   ketik unicode lewat IME masuk benar.

### P4 — Panel kanan modular + riwayat (1 PR)

1. Panel dengan 4 modul: Clipboard, Agents, Source control, Tasks.
2. Agents: segmented `Workspace | Project | All`, cari, scan counter,
   grup per project, kartu sesi (judul, preview, agent, `n msgs · waktu · model`),
   aksi resume/hapus/drag.
3. `sessions.rs`: agregasi statistik dan model per sesi.
4. Acceptance: 300+ sesi tersaring per scope <150ms; kartu identik struktur
   dengan Orca (bagian 4.5).

### P5 — Source control, diff, review (2 PR)

1. `git.rs`: status, stage/unstage, commit, branch switch, worktree add/remove.
2. Diff viewer dua kolom + numbering, komentar per baris (`notes.rs`).
3. Kirim komentar ke agent aktif sebagai prompt.
4. PR checks via `gh`/GitHub API; panel `ChecksPanel`.
5. Acceptance: ubah file → status muncul; komentar baris tersimpan; commit
   dari UI sukses; PR checks tampil.

### P6 — Remote, SSH, usage, status bar (2 PR)

1. `hosts.rs`: SSH target, passphrase dialog, terminal remote.
2. WSL host mark (Linux/Windows), node dist mirror (opsional).
3. `usage.rs`: baca kuota provider yang tersedia lokal; segmen status bar
   `Used`, persen, sisa waktu, provider mingguan, ports, host count.
4. Acceptance: SSH ke host uji membuka pty remote; status bar menampilkan
   angka nyata; klik segmen membuka dialog.

### P7 — Integrasi, plugin, mobile, onboarding (2 PR)

1. Plugin: manifest + panel deklaratif + izin (`OrcaYamlTrustDialog` setara).
2. Integrasi Linear/Jira/GitHub: minimal buka issue/PR dari sidebar.
3. Mobile: QR pairing via `qrcode` (read-only dashboard).
4. Onboarding: tour 4 langkah, tips, setup guide.
5. Acceptance: plugin contoh tampil; QR ter-scan; tour selesai tersimpan.

### P8 — Poles dan performa

1. i18n (`i18next` setara: tabel pesan JSON, fallback en/id).
2. Aksesibilitas: fokus ring, aria, kontras.
3. Telemetri opt-in (default mati).
4. Benchmark: start <400ms, memori <120MB, 20 tab tetap <60fps.

---

## 9. Definition of done

- `cargo test --offline` dan `node app/selftest.cjs` hijau tiap PR.
- Screenshot regresi 1240×820 dan 1920×1040 untuk tema gelap/terang.
- Tidak ada `system-ui` tunggal; font terverifikasi lewat `fc-match`.
- Semua state UI (tab, split, project, panel) bertahan setelah restart.
- Setiap fitur yang diklaim ada punya jalur uji di `selftest` atau tes Rust.

## 10. Risiko

| Risiko | Mitigasi |
| --- | --- |
| WebKitGTK fokus keyboard | `WebView::focus()` + fallback keydown (sudah ada) |
| xterm restore besar | simpan byte terbatas (mis. 2 MB/pane) |
| SSH keyring | pakai `ssh-agent`; passphrase tidak disimpan |
| Scope melar | satu PR satu fase, non-goals ditahan |
| Data pengguna | migrasi DB berversi, backup sebelum ALTER |

---

## 11. Lampiran — komponen UI terdeteksi

Nama diambil dari nama chunk `out/renderer/assets/*.js` (hash dibuang).
Total 141 komponen/screen, 1072 chunk JS, 9 berkas CSS.

| # | Komponen |
| --- | --- |
| 1 | `ActivityPrototypePage` |
| 2 | `AddProjectFromFolderDialog` |
| 3 | `AddRepoDialog` |
| 4 | `AddToChatMenu` |
| 5 | `AgentDashboardSidebarEntry` |
| 6 | `AgentDashboardSidebarHost` |
| 7 | `AgentDefaultSurfacePreviews` |
| 8 | `AgentLaunchPicker` |
| 9 | `AgentSkillSetupPanel` |
| 10 | `AgentsOrchestrationVisual` |
| 11 | `AiVaultPanel` |
| 12 | `AiVaultSearchEvidence` |
| 13 | `AiVaultSessionHistoryDialog` |
| 14 | `AiVaultSidebarPanel` |
| 15 | `App` |
| 16 | `ArtifactsPage` |
| 17 | `AutomationsPage` |
| 18 | `BaseRefPicker` |
| 19 | `ChatUiHereCornerCard` |
| 20 | `ChatUiHereFeatureTipVisual` |
| 21 | `ChecksPanel` |
| 22 | `ClaudeSwitcherMenu` |
| 23 | `CloneFolderNameField` |
| 24 | `CodexResetCredits` |
| 25 | `CombinedDiffViewer` |
| 26 | `CommentMarkdown` |
| 27 | `CommentReactions` |
| 28 | `ContextualTourOverlay` |
| 29 | `CrashReportDialogSurface` |
| 30 | `CreateProjectLocationField` |
| 31 | `CsvPagedViewer` |
| 32 | `CsvViewer` |
| 33 | `DashboardPopoutBridge` |
| 34 | `DeleteWorktreeDialog` |
| 35 | `DiagramViewerModal` |
| 36 | `DictationController` |
| 37 | `DiffCommentCard` |
| 38 | `DiffCommentPopover` |
| 39 | `DiffNotesSendMenu` |
| 40 | `DiffViewer` |
| 41 | `EditorPanel` |
| 42 | `FeatureTipsModal` |
| 43 | `FeatureWallModal` |
| 44 | `FeatureWallSetupChecklist` |
| 45 | `FileExplorer` |
| 46 | `FloatingTerminalIconContextMenu` |
| 47 | `FloatingTerminalPanel` |
| 48 | `FolderWorkspacePrChecksPanel` |
| 49 | `FolderWorkspaceSourceControlPanel` |
| 50 | `FolderWorkspaceWorktreesPanel` |
| 51 | `ForgetSshWorkspaceDialog` |
| 52 | `GitHubIcon` |
| 53 | `HostCombobox` |
| 54 | `HostedReviewUnlinkMenuItem` |
| 55 | `ImageDiffViewer` |
| 56 | `ImageViewer` |
| 57 | `IntegrationsStep` |
| 58 | `IpynbViewer` |
| 59 | `JiraIcon` |
| 60 | `Landing` |
| 61 | `LinearAgentSkillSetupDialog` |
| 62 | `LinearIcon` |
| 63 | `LinuxPenguinIcon` |
| 64 | `LoadingSpinner` |
| 65 | `MacosUpdateRequirementNotice` |
| 66 | `MarkdownPreview` |
| 67 | `MarkdownPreviewBody` |
| 68 | `MediaViewer` |
| 69 | `MermaidViewer` |
| 70 | `MobilePage` |
| 71 | `MonacoCodeExcerpt` |
| 72 | `MonacoEditor` |
| 73 | `NativeChatAccountUsageDialog` |
| 74 | `NativeChatComposer` |
| 75 | `NativeChatHighlightedCode` |
| 76 | `NodeDistMirrorDialog` |
| 77 | `NonGitFolderDialog` |
| 78 | `NotesSendMenu` |
| 79 | `OnboardingFlow` |
| 80 | `OnboardingInlineCommandTerminal` |
| 81 | `OpenInContextSubmenu` |
| 82 | `OpenPlainSshTerminalButton` |
| 83 | `OrcaSignInProgress` |
| 84 | `OrcaYamlTrustDialog` |
| 85 | `OrchestrationCrewPanel` |
| 86 | `PanZoomImageDialog` |
| 87 | `PendingNativeChatCreation` |
| 88 | `PetOverlay` |
| 89 | `PetStatusSegment` |
| 90 | `PluginDeclarativeView` |
| 91 | `PluginPanel` |
| 92 | `PluginShotReplaySurface` |
| 93 | `PortsPanel` |
| 94 | `PortsStatusSegment` |
| 95 | `PreservedBranchBatchReviewModal` |
| 96 | `ProjectAddedDialog` |
| 97 | `QuickOpen` |
| 98 | `RemoteServerUpdateDialog` |
| 99 | `RemoteServerUpdateStatus` |
| 100 | `RemoveFolderDialog` |
| 101 | `RepoBadgeLabel` |
| 102 | `RepositoryIconEmojiPicker` |
| 103 | `ResourceUsageStatusSegment` |
| 104 | `ReviewNotesSendMenuContent` |
| 105 | `RichMarkdownEditor` |
| 106 | `SelectedTextCopyMenu` |
| 107 | `SetProjectLocationDialog` |
| 108 | `Settings` |
| 109 | `SettingsFormControls` |
| 110 | `SetupGuideModal` |
| 111 | `SetupGuideTelemetryObserver` |
| 112 | `SidebarAgentsList` |
| 113 | `SidebarFeedbackDialog` |
| 114 | `SkillsPage` |
| 115 | `SleepTwinWorkspaceDialog` |
| 116 | `SourceControl` |
| 117 | `SourceControlAgentActionDialog` |
| 118 | `SshPassphraseDialog` |
| 119 | `SshStatusSegment` |
| 120 | `SshTargetCard` |
| 121 | `StatusBar` |
| 122 | `StatusIndicator` |
| 123 | `TaskPage` |
| 124 | `Terminal` |
| 125 | `UnexpectedSignoutCard` |
| 126 | `UpdateCard` |
| 127 | `WorkspaceAttachmentOrigins` |
| 128 | `WorkspaceCleanupDialog` |
| 129 | `WorkspaceEmojiPickerPanel` |
| 130 | `WorkspacePaneOverlayLayers` |
| 131 | `WorkspaceSpacePage` |
| 132 | `WorktreeCreationPanel` |
| 133 | `WorktreeHostContextBadge` |
| 134 | `WorktreeJumpPalette` |
| 135 | `WorktreeMetaDialog` |
| 136 | `WorktreeOpenInMenu` |
| 137 | `WorktreeSetupSteps` |
| 138 | `WorktreeVisibilityDialog` |
| 139 | `WorktreeVisibilitySourceList` |
| 140 | `WslHostMark` |
| 141 | `WslHostStoppedNotice` |
