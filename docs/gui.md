# Omnix App (GUI) — kontrak internal

Satu binary `omnix`: tanpa argumen membuka jendela app; subcommand tetap jalan di terminal.
Shell GUI = satu WebView UI (aset embedded, protokol `omnix://`) + N WebView konten untuk browser
+ terminal asli lewat PTY.

## Aturan keamanan (tidak boleh dilanggar)

1. WebView konten (halaman web apa pun) **tidak** diberi IPC handler. Hanya WebView UI yang punya.
2. Hanya `http:` dan `https:` boleh dimuat di WebView konten (`with_navigation_handler`).
3. Nama file yang dilayani protokol `omnix://` berasal dari daftar tetap, bukan path dari request.
4. Tidak ada `eval` teks halaman, tidak ada shell string: argumen selalu array.
5. Session id divalidasi `agent::resume_args` sebelum dipakai.

## Aset UI

Disajikan `omnix://localhost/<path>` dari `include_str!`. Daftar tetap:

| Path | Isi |
| --- | --- |
| `/` | `app/index.html` |
| `/style.css` | `app/style.css` |
| `/app.js` | `app/app.js` |
| `/vendor/xterm.js` | `@xterm/xterm` 5.5.0 UMD |
| `/vendor/xterm.css` | `@xterm/xterm` 5.5.0 CSS |
| `/vendor/addon-fit.js` | `@xterm/addon-fit` 0.10.0 UMD |
| `/tutorial.html` | `tutorial/index.html` |
| `/icon.svg` | `app/icon.svg` |

Vendor diambil dari `https://cdn.jsdelivr.net/npm/@xterm/xterm@5.5.0/lib/xterm.js`,
`.../css/xterm.css`, `https://cdn.jsdelivr.net/npm/@xterm/addon-fit@0.10.0/lib/addon-fit.js`.
Global UMD: `Terminal` dan `FitAddon.FitAddon`.

## Transport

UI → Rust: `window.ipc.postMessage(JSON.stringify(req))`.

```json
{"id":1,"cmd":"historySearch","args":{"query":"blank restore"}}
```

Rust → UI, dua bentuk:

```js
window.__omnix.reply(1, true, "{\"hits\":[]}")     // ok / error
window.__omnix.emit("ptyData", "{\"paneKey\":\"p1\"}")
```

`reply` id cocok dengan request. `emit` untuk event tanpa id. Payload selalu string JSON
(UI melakukan `JSON.parse`).

Error: `{"code":"...","message":"..."}` dengan code salah satu
`bad_request`, `invalid_arg`, `not_found`, `blocked_scheme`, `engine_unavailable`, `internal`.

## Command

| cmd | args | result |
| --- | --- | --- |
| `appInfo` | `{}` | `{version,root,engine:{lightpanda:bool,version:string\|null}}` |
| `status` | `{}` | `{sessions,messages,dbBytes,workspaces:[{path,branch,sleeping,sessions:[session]}]}` |
| `historyIndex` | `{dir?,agent?}` | `{files,messages,skippedLines,db}` |
| `historyScan` | `{}` | `{files,messages,skipped,unchanged,agents:[{agent,sessions,messages}],roots:[{agent,path}],db}` |
| `historyProjects` | `{limit?}` | `{root,projects:[{path,name,sessions,messages,updatedAt,agents}]}` |
| `historySessions` | `{scope:"workspace"\|"project"\|"all",agent?,query?,limit?}` | `{scope,root,sessions:[sessionRow]}` |
| `historySearch` | `{query,agent?,conversation?,newest?,limit?}` | `{hits:[hit]}` |
| `ptySpawn` | `{paneKey,cwd?,shell?,agent?,sessionId?,command?,label?}` | `{paneKey,pid}` |
| `ptyWrite` | `{paneKey,data}` | `{ok}` |
| `ptyResize` | `{paneKey,cols,rows}` | `{ok}` |
| `ptyKill` | `{paneKey}` | `{ok}` |
| `memory` | `{}` | `{available,pid,app:{rss,processes},ai:{rss,processes},other:{rss,processes},budget:{limitBytes,over},panes:[{key,label,cwd,pid,rss,processes,top:[name]}]}` |
| `browserOpen` | `{url,rect:{x,y,width,height}}` | `{url}` |
| `browserBounds` | `{rect,visible}` | `{ok}` |
| `browserClose` | `{}` | `{ok}` |
| `browserStop` | `{}` | `{ok}` |
| `browserFetch` | `{url,dump?}` | `{text}` |
| `windowMinimize` | `{}` | `{ok}` |
| `windowMaximize` | `{}` | `{maximized}` |
| `windowClose` | `{}` | `{ok}` |
| `updateCheck` | `{}` | `{available,current,version?}` |
| `updateInstall` | `{}` | `{ok,quit,message}` |

`hit` = `{agent,sessionId,title,cwd,updatedAt,score,snippet,resumeCommand}`.
`sessionRow` = `{agent,sessionId,title,cwd,updatedAt,messageCount,resumeCommand,preview}`.
`historyScan` indexes the transcripts already on disk: `~/.codex/sessions/**/*.jsonl`
(`CODEX_HOME`), `~/.claude/projects/**/*.jsonl` without `subagents/`
(`CLAUDE_CONFIG_DIR`), and `~/.local/share/opencode/opencode.db` opened
read-only (`XDG_DATA_HOME`). Files whose mtime already matches the stored row
are skipped, so a rescan is cheap.
`session` = `{paneKey,agent,sessionId,cwd,resumeCommand,savedAt}`.

`memory.app` adalah seluruh process tree Omnix; `memory.ai` menjumlahkan tree
setiap pane PTY (agent + child process), `memory.other` adalah sisa app shell
dan WebKit. Ketiganya memakai satu snapshot `/proc` (Linux), `ps` (macOS), atau
CIM (Windows) supaya tidak ada proses yang dihitung dua kali.
`budget.limitBytes` berasal dari `OMNIX_AI_RAM_MB`; `budget.over` true saat
`ai.rss` melewati batas. UI memakai flag itu untuk mewarnai chip, bukan untuk
menghentikan proses.
`browserStop` menghentikan daemon Lightpanda bersama dari `.omnix/browser.json`
dan menghapus state-nya. `browserClose` hanya menutup WebView konten.
`windowMinimize`/`windowMaximize` bekerja pada jendela native; `windowClose`
menjalankan shutdown yang sama dengan tombol close OS (PTY dimatikan).
`updateCheck` dan `updateInstall` berjalan di thread latar supaya jaringan
tidak membekukan event loop; balasan tetap lewat `Msg::Reply`. `quit: true`
berarti UI harus menutup jendela setelah balasan (Windows dan Linux).
`rect` dalam piksel logis relatif jendela, `x/y` sudut kiri atas.

## Event

| event | payload |
| --- | --- |
| `ptyData` | `{paneKey,dataBase64}` |
| `ptyExit` | `{paneKey,code}` |
| `browserNav` | `{url,title}` |
| `toast` | `{level,message}` |

`ptyData` base64 agar byte non-UTF8 selamat. UI: `atob` → `Uint8Array` → `term.write`.

## Tata letak

UI melaporkan rect area konten setiap `resize`/tab berubah:
`{"cmd":"browserBounds","args":{"visible":true,"rect":{...}}}`.
WebView konten hanya dibuat saat tab browser aktif; ditutup saat tab lain aktif.
