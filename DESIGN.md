# Omnix Design Rules

Arah: minimalis, padat, datar. Omnix adalah workbench, bukan landing page. Chrome tenang; isi pane yang boleh ramai. Tidak meniru macOS dan tidak memakai dekorasi AI-slop.

Aturan ini mengikat semua perubahan UI di `app/`. Bila sebuah keputusan tidak diatur di sini, pilih opsi yang paling sedikit: warna paling sedikit, bayangan paling sedikit, radius paling sedikit, elemen paling sedikit.

## 1. Prinsip

1. **Konten dulu.** Sidebar, bar jendela, dan status bar adalah bingkai. Terminal dan browser adalah isi.
2. **Satu aksen.** Hanya `--accent` untuk aksi, fokus, dan state aktif. `--folder` hanya untuk ikon project. `--good`, `--warn`, `--bad` hanya untuk status.
3. **Satu pola per peran.** Semua tombol dengan peran sama punya tinggi, padding, radius, dan font yang sama.
4. **Bila belum ada, jangan tampilkan.** Menu, tab, tombol, dan badge yang tidak punya aksi harus dihapus, bukan diberi label `Soon` atau `disabled`.
5. **Konsisten lebih penting daripada pintar.** Tidak ada variasi khusus per layar tanpa alasan fungsional.

## 2. Skala

### Jarak

Skala: `2, 4, 6, 8, 12, 16, 20, 24`. Tidak ada 3, 5, 7, 9, 11, 13, 15 untuk padding, margin, atau gap.

| Konteks | Nilai |
| --- | --- |
| Jarak antar ikon dan label | 6px |
| Jarak antar tombol sejajar | 6px |
| Jarak antar item daftar | 2px |
| Padding baris daftar | 0 8px |
| Padding section | 8px 12px |
| Jarak antar grup | 12px |
| Jarak dari tepi bar jendela | 8px |
| Padding vertikal pane | 12px |

### Tinggi kontrol

| Peran | Tinggi |
| --- | --- |
| Chip filter | 24px |
| Tombol kecil (`.btn.sm`), ikon kompak | 24px |
| Tombol, input, select, tombol ikon di bar | 28px |
| Tab, baris daftar, item menu, nav | 32px |
| Bar jendela | 44px |
| Baris status | 28px |

Tidak boleh ada dua kontrol sejajar dengan tinggi berbeda.

### Radius

| Radius | Token | Pakai untuk |
| --- | --- | --- |
| 2px | `--r-sm` | Tag kecil, badge, sudut dalam |
| 4px | `--r-ctl` | Tombol, input, tab, menu item, chip |
| 6px | `--r-panel` | Menu, palette, modal, panel |
| penuh | `--r-pill` | Hanya switch track dan progress |

Tidak ada pill pada tombol, chip, atau badge. Tidak ada radius berbeda untuk kontrol yang sejajar. Sudut kontrol tidak boleh lebih bulat dari 4px.

### Ikon

| Ukuran | Pakai untuk |
| --- | --- |
| 12px | Ikon di dalam teks meta atau twisty |
| 14px | Tombol ikon kompak, ikon sidebar |
| 16px | Ikon menu, ikon folder, ikon baris |
| 20px | Ikon empty state |

- Satu baris hanya memakai satu ukuran ikon.
- Semua ikon garis: `fill="none"`, `stroke="currentColor"`, `stroke-width="1.7"`, ujung `round`.
- Ikon tanpa label wajib punya `aria-label` dan `title`.
- Tidak ada ikon dekoratif yang tidak punya fungsi.

## 3. Tipografi

- UI: `var(--sans)` per platform. Terminal: `var(--mono)`.
- Ukuran: 11px meta mono, 12px caption, 13px UI dan tombol, 14px menu, 15px heading kecil, 20px heading halaman.
- Berat: 400 body, 500 kontrol dan label, 600 heading dan nama. Tidak ada 700 di UI.
- `letter-spacing: 0`, kecuali heading besar maksimal `-.02em`.
- Angka memakai `font-variant-numeric: tabular-nums` di count, durasi, dan memori.
- Satu layar maksimal memakai tiga ukuran teks.

Larangan:

- Tidak ada `text-transform: uppercase`.
- Tidak ada `text-transform: lowercase` paksa pada data.
- Tidak ada ALL CAPS untuk judul, tombol, menu, tab, atau meta.

## 4. Casing dan Copy

| Elemen | Casing | Contoh |
| --- | --- | --- |
| Tombol | Title Case | `New Terminal`, `Add Project` |
| Menu item | Title Case | `Split Terminal`, `Agent Settings` |
| Tab | Title Case | `Terminal`, `Browser`, `Settings` |
| Heading | Title Case | `Process Memory`, `History Search` |
| Label field | Title Case | `Folder Path`, `Working Directory` |
| Body, hint, error | Sentence case | `Start a terminal in this workspace.` |
| Nama file, command, path | Verbatim | `ptySpawn`, `/home/user/app` |

Aturan copy:

1. Pakai nama yang dipakai user: `Terminal`, `Browser`, `Project`, `Agent`, bukan `pty`, `web`, `pane`.
2. Jangan tampilkan nama RPC internal di UI.
3. Toast dan error: satu kalimat pendek, sentence case.
4. Hint menu hanya berisi shortcut nyata.
5. Jangan pakai em dash, emoji, atau kata pemasaran (`seamless`, `powerful`, `supercharge`).
6. Placeholder memberi contoh nyata, bukan instruksi kosong.

## 5. Tombol

Tiga varian saja. Tidak ada varian keempat.

| Varian | Bentuk | Pakai untuk |
| --- | --- | --- |
| Primer | `--accent` solid, teks putih, radius 6px | Satu aksi utama per layar |
| Sekunder | `--surface`, border `--line-soft`, teks utama | Aksi biasa |
| Ghost | tanpa border dan background, hover `--hover` | Aksi baris, ikon, menu |

Aturan:

- Tinggi 28px, padding `0 12px`, gap 6px, font 13px/500.
- `.btn.sm` tinggi 24px, padding `0 9px`.
- Maksimal satu tombol primer terlihat dalam satu pane.
- Tombol ikon 28px, ikon 14px, tanpa teks, wajib `title` dan `aria-label`.
- Hover ghost hanya mengubah background dan warna teks. Tidak menambah border.
- Disabled hanya untuk aksi sementara yang jelas (mis. sedang menyimpan). Menu yang belum ada dihapus.
- Tidak ada gradient, shadow, uppercase, atau ikon berbeda ukuran dalam satu baris tombol.

## 6. Warna, Elevasi, dan Gerak

| Token | Pakai untuk |
| --- | --- |
| `--bg` | Kanvas pane dan terminal |
| `--bg-side`, `--bg-rail` | Sidebar, bar jendela, status bar |
| `--surface`, `--surface-2` | Kontrol dan hover |
| `--line`, `--line-soft` | Garis struktural |
| `--text`, `--text-2`, `--meta` | Teks utama, sekunder, meta |
| `--accent` | Aksi primer, fokus, state aktif |
| `--folder` | Hanya ikon folder project |
| `--warn`, `--bad`, `--good` | Status, hanya teks, dot, dan border tipis |

- Chrome opaque. Overlay memakai `--float` dan satu `--shadow`.
- Tidak ada `backdrop-filter`. WebKitGTK pernah menelan klik karena blur.
- Tidak ada gradient, inset highlight, atau bayangan pada kontrol.
- Fokus memakai satu pola: outline 2px `--accent`, offset 1px.
- Transisi hanya warna, border, opacity, dan transform; 120ms `ease-out`.
- Tidak ada animasi masuk, parallax, bounce, atau spinner ambient.
- Spinner hanya untuk pekerjaan yang punya durasi dan berhenti saat selesai. Status "sedang berjalan" memakai dot statis.

## 7. Komponen

- **Bar jendela:** brand di atas kolom sidebar berisi tombol collapse dan Settings 28px. Tab 32px menempel pada batas kolom, `+` 6px setelah tab, `Command` dan tombol Agents di kanan. Tab memakai nama project atau label pane, bukan judul window shell, dan dipotong dengan ellipsis.
- **Sidebar:** search 28px di paling atas. Baris judul workspace memuat refresh dan add. Tidak ada label section tambahan bila isinya hanya satu grup.
- **Sidebar collapsed:** lebar 56px, hanya ikon. Logo, nama, search, judul workspace, nama project, branch, dan sesi disembunyikan; folder tetap bisa diklik dan tooltip membawa nama. Tombol collapse tetap di bar jendela.
- **Panel Agents:** header tanpa ikon dekoratif: judul `Agents`, sub `Indexed history`, lalu tiga tombol ikon 28px (collapse semua grup, filter agent, rescan). Di bawahnya segmented control 28px `Workspace | Project | All`, search 28px, baris status 11px (`History refreshed` + waktu scan), baris ringkasan `N sessions` + sort `Last updated`, lalu grup per project dengan count 18px. Kartu sesi memakai `--surface`, border `--line-soft`, radius 4px. Tombol hide 32px mengambang di kanan bawah panel; setelah tertutup, tombol Agents di bar jendela membukanya lagi.
- **State yang disimpan:** sidebar collapse, panel Agents terbuka/tertutup, scope history, sort, grup yang dilipat, filter agent, judul workspace, dan project yang dibuka. Data sesi ada di SQLite `.omnix/history.sqlite`, jadi tetap ada setelah aplikasi ditutup.
- **Baris project:** 32px, twisty, ikon folder 16px, nama 13px/500. Branch dan sesi hanya saat dibuka. Status berjalan memakai dot 6px statis.
- **Menu:** lebar 268px, padding 6px, item 32px, ikon 16px, font 13px. Item disabled tidak ditampilkan; deteksi agent yang tidak terpasang tidak masuk menu.
- **Settings:** nav item 32px, satu section aktif. Tidak ada nav ke halaman yang belum ada.
- **Tab aktif:** `--surface` dan border tipis. Tidak ada shadow atau garis aksen tambahan.
- **Empty state:** satu ikon 20px, satu judul, satu hint, maksimal dua aksi.
- **Status bar:** teks 11px, angka tabular, tanpa pill.

## 8. Gate Anti-Slop

Tolak perubahan bila salah satu benar:

- Ada gradient, `backdrop-filter`, atau shadow pada kontrol.
- Ada `border-radius` di atas 6px selain switch dan progress.
- Ada lebih dari satu aksen warna bersaing. Ikon folder bukan pelanggaran.
- Ada label ALL CAPS, emoji, em dash, atau kata pemasaran.
- Ada dua kontrol sejajar dengan tinggi, radius, atau ukuran ikon berbeda.
- Ada jarak di luar skala `2, 4, 6, 8, 12, 16, 20, 24` untuk chrome.
- Ada tombol atau menu `Soon`, `disabled`, atau placeholder yang tidak bisa dipakai.
- Ada spinner yang berputar tanpa pekerjaan yang selesai.
- Ada istilah internal (`pty`, `web`, `pane`, nama RPC) di teks UI.
- Ada label lowercase yang seharusnya Title Case.
- Ada CSS duplikat yang menimpa aturan dasar dengan nilai berbeda.
