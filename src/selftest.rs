//! Headless end-to-end check for the app shell.
//!
//! Enabled with `OMNIX_SELFTEST=1`. The embedded UI is loaded for real, then driven
//! through `window.ipc`, so the WebView, the IPC bridge, the assets and every Rust
//! command run together without needing a screenshot or a human.

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
      }, 20000);
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

    const status = await ask('status', {});
    record('status', status.ok && typeof status.payload.sessions === 'number',
      status.ok ? 'sessions=' + status.payload.sessions : status.payload);

    const index = await ask('historyIndex', {});
    record('historyIndex', index.ok && index.payload.files >= 1,
      index.ok ? 'files=' + index.payload.files + ' messages=' + index.payload.messages : index.payload);

    const search = await ask('historySearch', { query: 'selftest needle', limit: 5 });
    const hits = (search.ok && search.payload.hits) || [];
    record('historySearch', hits.length === 1 && hits[0].resumeCommand.indexOf('resume') >= 0,
      hits.length ? hits[0].resumeCommand : 'no hits');

    const blocked = await ask('browserOpen', { url: 'file:///etc/passwd', rect: { x: 0, y: 0, width: 10, height: 10 } });
    record('browserBlockedScheme', blocked.ok && blocked.payload.ok === false && blocked.payload.code === 'blocked_scheme',
      blocked.payload && blocked.payload.code);

    const spawn = await ask('ptySpawn', { paneKey: ptyKey });
    record('ptySpawn', spawn.ok && spawn.payload.pid > 0, spawn.ok ? 'pid=' + spawn.payload.pid : spawn.payload);

    if (spawn.ok) {
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

    const handoff = await ask('handoff', {
      name: 'selftest-' + Date.now(),
      prompt: 'selftest prompt',
      agent: 'codex',
    });
    record('handoff', handoff.ok && handoff.payload.outcome === 'not-delivered',
      handoff.ok ? handoff.payload.worktree : handoff.payload);

    const tracked = await ask('agentTrack', { agent: 'codex', session: '2026-01-01T00-00-00-selftest', paneKey: 'selftest-pane-2' });
    record('agentTrack', tracked.ok, JSON.stringify(tracked.payload));

    const sleep = await ask('sleep', {});
    record('sleep', sleep.ok && sleep.payload.sessions >= 1, sleep.ok ? 'sessions=' + sleep.payload.sessions : sleep.payload);

    const resume = await ask('resume', {});
    record('resume', resume.ok && resume.payload.sessions.length >= 1,
      resume.ok ? 'sessions=' + resume.payload.sessions.length : resume.payload);

    const failed = steps.filter((s) => !s.ok).length;
    record('done', failed === 0, failed + ' failed of ' + steps.length);
  } catch (err) {
    record('done', false, String(err));
  }
})();
"#;
