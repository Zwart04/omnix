# Perch App (GUI) — kontrak internal

Satu binary `perch`: tanpa argumen membuka jendela app; subcommand tetap jalan di terminal.
Shell GUI = satu WebView UI (aset embedded, protokol `perch://`) + N WebView konten untuk browser
+ terminal asli lewat PTY.

## Aturan keamanan (tidak boleh dilanggar)

1. WebView konten (halaman web apa pun) **tidak** diberi IPC handler. Hanya WebView UI yang punya.
2. Hanya `http:` dan `https:` boleh dimuat di WebView konten (`with_navigation_handler`).
3. Nama file yang dilayani protokol `perch://` berasal dari daftar tetap, bukan path dari request.
4. Tidak ada `eval` teks halaman, tidak ada shell string: argumen selalu array.
5. Session id divalidasi `agent::resume_args` sebelum dipakai.

## Aset UI

Disajikan `perch://localhost/<path>` dari `include_str!`. Daftar tetap:

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
window.__perch.reply(1, true, "{\"hits\":[]}")     // ok / error
window.__perch.emit("ptyData", "{\"paneKey\":\"p1\"}")
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
| `historySearch` | `{query,agent?,conversation?,newest?,limit?}` | `{hits:[hit]}` |
| `agentTrack` | `{agent,session,paneKey?,cwd?}` | `{paneKey,agent,sessionId,cwd,resumeCommand}` |
| `sleep` | `{workspace?}` | `{path,sessions}` |
| `resume` | `{workspace?}` | `{path,sessions:[{paneKey,agent,sessionId,cwd,resumeCommand}]}` |
| `handoff` | `{name,prompt,agent?,base?}` | `{outcome,worktree,branch,brief,record,launch?}` |
| `ptySpawn` | `{paneKey,cwd?,shell?,agent?,sessionId?}` | `{paneKey,pid}` |
| `ptyWrite` | `{paneKey,data}` | `{ok}` |
| `ptyResize` | `{paneKey,cols,rows}` | `{ok}` |
| `ptyKill` | `{paneKey}` | `{ok}` |
| `browserOpen` | `{url,rect:{x,y,width,height}}` | `{url}` |
| `browserBounds` | `{rect,visible}` | `{ok}` |
| `browserClose` | `{}` | `{ok}` |
| `browserFetch` | `{url,dump?}` | `{text}` |

`hit` = `{agent,sessionId,title,cwd,updatedAt,score,snippet,resumeCommand}`.
`session` = `{paneKey,agent,sessionId,cwd,resumeCommand,savedAt}`.
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
