# Omnix Design Rules

Arah: minimalis. Satu bar jendela, daftar proyek padat, dan satu pane konten. Omnix adalah workbench, bukan landing page, dan tidak meniru macOS: chrome tenang, isi pane yang boleh ramai.

Aturan ini mengikat semua perubahan UI di `app/`. Bila sebuah keputusan tidak diatur di sini, pilih opsi yang paling sedikit: lebih sedikit warna, lebih sedikit bayangan, lebih sedikit radius.

## 1. Prinsip

1. **Konten dulu.** Sidebar, tab strip, dan status bar adalah bingkai. Terminal dan browser adalah isi. Bingkai tidak boleh menarik perhatian lebih dari isi.
2. **Satu aksen.** Hanya `--accent` yang menandai aksi, fokus, dan state aktif. Pengecualian: `--folder` hangat khusus ikon project, karena itu penanda jenis, bukan aksen kedua.
3. **Padat dan datar.** Baris 28 sampai 40px, kontrol 24 sampai 30px, jarak kelipatan 4px. Tidak ada dekorasi di belakang konten.
4. **Restraint.** Tidak ada gradient dekoratif, glass berlebih, pill di mana-mana, atau bayangan yang tidak menjelaskan elevasi.
5. **Konsisten lebih penting daripada pintar.** Satu pola untuk semua tombol, chip, menu, dan heading.

## 2. Warna

| Token | Pakai untuk |
| --- | --- |
| `--bg` | Kanvas pane dan terminal |
| `--bg-side`, `--bg-rail` | Sidebar, tab strip, status bar |
| `--surface`, `--surface-2` | Kontrol dan baris saat hover |
| `--line`, `--line-soft` | Garis struktural, bukan dekorasi |
| `--text`, `--text-2`, `--meta` | Teks utama, sekunder, metadata |
| `--accent` | Aksi primer, fokus, state aktif |
| `--warn`, `--bad`, `--good` | Status, hanya pada teks dan border tipis |
| `--folder` | Hanya ikon folder project |

Larangan:

- Tidak ada gradient pada tombol, badge, ikon, atau panel.
- Tidak ada warna aksen tambahan di luar token status dan `--folder` pada ikon folder.
- Tidak ada teks berwarna aksen kecuali state aktif, link, atau status.
- Background translusen hanya untuk overlay yang melayang di atas konten.

## 3. Tipografi

- UI: `var(--sans)` per platform. Terminal: `var(--mono)` dengan rantai DejaVu lebih dulu di Linux.
- Body 13 sampai 14px. Metadata 10.5 sampai 12px. Heading 15 sampai 21px.
- Berat 400 untuk body, 500 untuk kontrol dan label, 600 untuk heading dan nama.
- `letter-spacing`: 0 untuk teks normal, negatif kecil hanya untuk heading besar.
- Angka pakai `font-variant-numeric: tabular-nums` di tabel, count, durasi, dan memori.

Larangan:

- Tidak ada `text-transform: uppercase` untuk label UI.
- Tidak ada tracking lebar (`letter-spacing` di atas `.02em`) pada label.
- Tidak ada `text-transform: lowercase` paksa pada nama plan, agent, atau data.
- Tidak ada ALL CAPS untuk judul, tombol, menu, tab, atau metadata.
- Satu-satunya pengecualian: monogram ikon agent (mis. `CC`) boleh kapital karena itu mark, bukan label.

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

1. Sebut benda dengan nama yang dipakai user: `Terminal`, `Browser`, `Project`, `Agent`, bukan `pty`, `web`, `pane`, `workspace` bila maksudnya project.
2. Jangan tampilkan nama RPC internal di UI. `ptySpawn`, `historyScan`, `browserFetch` hanya boleh muncul di log teknis.
3. Toast dan error: kalimat pendek, sentence case, akhiri titik bila lebih dari satu frasa.
4. Hint menu hanya berisi shortcut nyata. Jangan isi dengan kategori internal seperti `pty`, `web`, `panel`.
5. Jangan pakai em dash. Pakai koma, titik, titik dua, atau kurung.
6. Jangan pakai emoji di UI.
7. Jangan pakai kata pemasaran: `seamless`, `powerful`, `supercharge`, `unleash`, `elevate`.
8. Placeholder memberi contoh nyata, bukan instruksi kosong: `https://example.com`, `query, e.g. blank restore`.

## 5. Spacing dan Radius

Skala jarak: `4, 8, 12, 16, 20, 24`. Tidak ada nilai 5, 7, 9, 11, 13, 15 untuk padding, margin, atau gap.

| Radius | Token | Pakai untuk |
| --- | --- | --- |
| 4px | `--r-sm` | Badge count, tag kecil, penanda |
| 6px | `--r-ctl` | Tombol, input, select, tab, baris menu, chip |
| 10px | `--r-panel` | Menu, palette, modal, kartu besar |
| penuh | `--r-pill` | Hanya switch track dan progress bar |

Larangan:

- Tidak ada `border-radius` 12px atau lebih pada kontrol.
- Tidak ada pill pada tombol, chip filter, status chip, atau badge angka.
- Tidak ada radius berbeda untuk dua kontrol yang sejajar.

## 6. Elevasi dan Glass

- Chrome (sidebar, tab strip, status bar) **opaque**. Tidak ada `backdrop-filter`.
- Overlay (menu, palette, modal, toast) memakai `--float` solid dan satu bayangan `--shadow`.
- Bayangan hanya untuk elemen yang benar-benar melayang. Baris, tombol, tab, dan input tidak punya bayangan.
- Pengecualian: focus ring tanpa blur (`0 0 0 3px`) dihitung penanda fokus, bukan elevasi.
- Tidak ada inset highlight putih untuk membuat kontrol terlihat "hidup".
- `backdrop-filter` dimatikan di semua platform. WebKitGTK pernah menaruh menu di sidebar dan menelan klik terminal.

## 7. Gerak

- Transisi hanya `background-color`, `border-color`, `color`, `opacity`, dan `transform`.
- Durasi 120ms, easing `ease-out`.
- Tidak ada animasi masuk, parallax, atau bounce.
- Spinner hanya untuk pekerjaan yang benar-benar berjalan.

## 8. Komponen

- **Bar jendela:** satu baris penuh, tanpa pemisah ganda. Brand di atas kolom sidebar, diikuti tombol Settings dan Agents, lalu tab yang menempel pada batas kolom, `+`, dan tombol Command di kanan. Tab memakai judul dari shell (`user@host:path`) dan dipotong dengan ellipsis.
- **Sidebar:** search field di paling atas. Mengetik memfilter daftar project; Enter membuka History Search dengan query yang sama. Aksi refresh dan add ada di baris judul workspace; Settings dan Agents tinggal di bar jendela, bukan rail ikon terpisah.
- **Baris project:** satu baris 32px, twisty, ikon folder, nama. Sesi tidak tampil sebelum project dibuka. Saat dibuka, branch tampil sebagai satu baris redup, lalu sesi yang hidup.

- **Tombol primer:** accent solid, teks putih, radius 6px, tanpa gradient dan tanpa shadow. Hover menurunkan opacity sedikit.
- **Tombol sekunder:** surface, border `--line-soft`, teks utama.
- **Tab aktif:** `--surface` + border tipis. Tidak ada shadow atau garis aksen tambahan.
- **Tab title:** hanya judul. Tidak ada tag tipe, badge, atau ikon status.
- **Empty state:** satu ikon garis, satu judul Title Case, satu hint sentence case, maksimal dua aksi.
- **Status bar:** teks 11px, angka tabular, tanpa pill kecuali memang satu kesatuan kontrol.

## 9. Gate Anti-Slop

Sebelum commit, tolak perubahan bila salah satu benar:

- Ada label ALL CAPS atau `text-transform: uppercase`.
- Ada gradient pada kontrol.
- Ada `border-radius` di atas 10px selain switch dan progress bar.
- Ada `box-shadow` pada elemen yang tidak melayang.
- Ada `backdrop-filter`.
- Ada teks UI yang memakai istilah internal (`pty`, `web`, `pane`, nama RPC).
- Ada label lowercase yang seharusnya Title Case.
- Ada emoji, em dash, atau kata pemasaran.
- Ada dua aksen warna yang bersaing. Ikon folder `--folder` bukan pelanggaran.
