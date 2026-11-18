//! Headless end-to-end check for the app shell.
//!
//! Enabled with `OMNIX_SELFTEST=1`. The embedded UI is loaded for real, then driven
//! through `window.ipc`, so the WebView, the IPC bridge, the assets and every Rust
//! command run together without needing a screenshot or a human.

use std::fs;
use std::path::Path;

/// The GUI check needs one transcript it can index and search no matter what
/// the developer's real history looks like. Seeded only under OMNIX_SELFTEST.
pub fn seed(root: &Path) {
    let dir = root.join(".omnix").join("transcripts").join("codex");
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let body = "{\"role\":\"user\",\"text\":\"selftest needle\"}\n\
                {\"role\":\"assistant\",\"text\":\"blank restore path\"}\n";
    let _ = fs::write(dir.join("selftest.jsonl"), body);
}

pub const JS: &str = r#"
(async () => {
  const outer = window.__omnix || {};
  const originalReply = outer.reply || function () {};
  const originalEmit = outer.emit || function () {};
  const waiting = new Map();
  let next = 9000;
  const ptyChunks = [];
  let ptyKey = 'selftest-pane';

  window.__omnix = Object.assign({}, outer, {
    reply(id, ok, payload) {
      const entry = waiting.get(id);
      if (entry) {
        waiting.delete(id);
        entry.resolve({ ok, payload: safeParse(payload) });
        return;
      }
      originalReply(id, ok, payload);
    },
    emit(event, payload) {
      if (event === 'ptyData') {
        const data = safeParse(payload);
        if (data && data.paneKey === ptyKey) ptyChunks.push(data.dataBase64 || '');
        return;
      }
      originalEmit(event, payload);
    },
  });

  function safeParse(text) {
    try { return JSON.parse(text); } catch (e) { return null; }
  }

  function ask(cmd, args) {
    return new Promise(function (resolve) {
      const id = next++;
      waiting.set(id, { resolve: resolve });
      setTimeout(function () {
        if (waiting.has(id)) {
          waiting.delete(id);
          resolve({ ok: false, payload: { code: 'timeout', message: cmd } });
        }
      }, 180000);
      window.ipc.postMessage(JSON.stringify({ id: id, cmd: cmd, args: args || {} }));
    });
  }

  const steps = [];
  function record(step, ok, detail) {
    steps.push({ step: step, ok: ok, detail: detail });
    window.ipc.postMessage(JSON.stringify({
      id: 1,
      cmd: 'selftest',
      args: { step: step, ok: ok, detail: detail },
    }));
  }

  try {
    const info = await ask('appInfo', {});
    record('appInfo', info.ok && !!(info.payload && info.payload.version),
      JSON.stringify(info.payload));

    /* Linux WebKitGTK smears and unlinks backdrop-filter layers. Keep the
       blur off there, and prove the tab menu still sits above the terminal. */
    const layers = (function () {
      const strip = document.getElementById('tabstrip');
      const menu = document.getElementById('tab-new-menu');
      const btn = document.getElementById('tab-new');
      const cs = getComputedStyle(strip);
      const blur = cs.backdropFilter || cs.webkitBackdropFilter || '';
      btn.click();
      const item = menu.querySelector('.menu-item');
      const rect = item ? item.getBoundingClientRect() : null;
      const hit = rect ? document.elementFromPoint(rect.left + 10, rect.top + 10) : null;
      const onTop = !!(item && (hit === item || item.contains(hit)));
      menu.hidden = true;
      return { blur: blur, onTop: onTop };
    })();
    const noBlur = !info.payload || info.payload.platform !== 'linux' || layers.blur === 'none';
    record('layers', noBlur && layers.onTop,
      'tabstrip=' + layers.blur + ' menuHit=' + layers.onTop);

    const status = await ask('status', {});
    record('status', status.ok && typeof status.payload.sessions === 'number',
      status.ok ? 'sessions=' + status.payload.sessions : status.payload);

    const index = await ask('historyIndex', {});
    record('historyIndex', index.ok && index.payload.files >= 1,
      index.ok ? 'files=' + index.payload.files + ' messages=' + index.payload.messages : index.payload);

    const scan = await ask('historyScan', {});
    record('historyScan', scan.ok && Array.isArray(scan.payload.agents),
      scan.ok ? 'agents=' + scan.payload.agents.length + ' files=' + scan.payload.files : JSON.stringify(scan.payload));

    const listing = await ask('historySessions', { scope: 'all', limit: 5 });
    const listed = (listing.ok && listing.payload.sessions) || [];
    record('historySessions', listing.ok && Array.isArray(listed),
      listing.ok ? 'sessions=' + listed.length + ' first=' + (listed[0] ? listed[0].agent : '-') : JSON.stringify(listing.payload));

    const proj = await ask('historyProjects', { limit: 10 });
    const projects = (proj.ok && proj.payload.projects) || [];
    record('historyProjects', proj.ok && Array.isArray(projects) && projects.every(p => typeof p.name === 'string'),
      proj.ok ? 'projects=' + projects.length + ' first=' + (projects[0] ? projects[0].name : '-') : JSON.stringify(proj.payload));

    const search = await ask('historySearch', { query: 'selftest needle', limit: 5 });
    const hits = (search.ok && search.payload.hits) || [];
    record('historySearch', search.ok && hits.some((h) => h.resumeCommand.indexOf('resume') >= 0),
      hits.length ? hits.map((h) => h.resumeCommand).join(' | ') : 'no hits');

    const blocked = await ask('browserOpen', { url: 'file:///etc/passwd', rect: { x: 0, y: 0, width: 10, height: 10 } });
    record('browserBlockedScheme', blocked.ok && blocked.payload.ok === false && blocked.payload.code === 'blocked_scheme',
      blocked.payload && blocked.payload.code);

    const spawn = await ask('ptySpawn', { paneKey: ptyKey });
    record('ptySpawn', spawn.ok && spawn.payload.pid > 0, spawn.ok ? 'pid=' + spawn.payload.pid : spawn.payload);

    if (spawn.ok) {
      const mem = await ask('memory', {});
      const panes = (mem.ok && mem.payload.panes) || [];
      const own = panes.filter((p) => p.pid === spawn.payload.pid);
      record('memory', mem.ok && mem.payload.app.rss > 0 && own.length === 1,
        mem.ok ? 'app=' + mem.payload.app.rss + ' panes=' + panes.length : mem.payload);
      await ask('ptyWrite', { paneKey: ptyKey, data: "printf 'selftest-pty-marker\\n'\n" });
      await new Promise((r) => setTimeout(r, 1200));
      const text = ptyChunks.join('');
      record('ptyData', text.length > 0, 'chunks=' + ptyChunks.length);
      const killed = await ask('ptyKill', { paneKey: ptyKey });
      record('ptyKill', killed.ok, killed.ok ? 'killed' : killed.payload);
    }

    const fetched = await ask('browserFetch', { url: 'https://example.com', dump: 'markdown' });
    const body = (fetched.ok && fetched.payload.text) || '';
    record('browserFetch', body.toLowerCase().indexOf('example') >= 0,
      fetched.ok ? 'chars=' + body.length : fetched.payload);

    const opened = await ask('browserOpen', {
      url: 'https://example.com',
      rect: { x: 0, y: 0, width: 320, height: 200 },
    });
    record('browserOpen', opened.ok && opened.payload.url === 'https://example.com',
      JSON.stringify(opened.payload));
    await new Promise((r) => setTimeout(r, 800));
    const closed = await ask('browserClose', {});
    record('browserClose', closed.ok === true, JSON.stringify(closed.payload));

    const failed = steps.filter((s) => !s.ok).length;
    record('done', failed === 0, failed + ' failed of ' + steps.length);
  } catch (err) {
    record('done', false, String(err));
  }
})();
"#;
