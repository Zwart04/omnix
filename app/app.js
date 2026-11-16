/* Perch app UI. Vanilla JS, no frameworks, no CDN. */
(function () {
  'use strict';

  var AGENTS = [
    'claude', 'codex', 'opencode', 'opencode2', 'cursor', 'gemini', 'grok',
    'copilot', 'qwen-code', 'pi', 'kimi', 'kiro', 'antigravity', 'rovo'
  ];

  var RPC_TIMEOUT_MS = 20000;

  var TERM_THEME = {
    background: '#0b0f14',
    foreground: '#e6edf3',
    cursor: '#7ee0b8',
    cursorAccent: '#0b0f14',
    selectionBackground: '#27303b',
    black: '#0b0f14',
    red: '#ff7b72',
    green: '#7ee0b8',
    yellow: '#ffd479',
    blue: '#79b8ff',
    magenta: '#d2a8ff',
    cyan: '#76e3ea',
    white: '#e6edf3',
    brightBlack: '#6b7785',
    brightRed: '#ff9b93',
    brightGreen: '#a8f0d2',
    brightYellow: '#ffe3a3',
    brightBlue: '#a3d0ff',
    brightMagenta: '#e2c7ff',
    brightCyan: '#a9f1f6',
    brightWhite: '#ffffff'
  };

  /* ---------- helpers ---------- */

  function $(id) {
    return document.getElementById(id);
  }

  function el(tag, cls, text) {
    var node = document.createElement(tag);
    if (cls) node.className = cls;
    if (text != null) node.textContent = text;
    return node;
  }

  function message(err) {
    return err && err.message ? String(err.message) : String(err);
  }

  function guard(fn) {
    return function (ev) {
      try {
        var out = fn(ev);
        if (out && typeof out.catch === 'function') {
          out.catch(function (err) { toast('error', message(err)); });
        }
      } catch (err) {
        toast('error', message(err));
      }
    };
  }

  function button(label, cls, onClick) {
    var node = el('button', cls || 'btn', label);
    node.type = 'button';
    node.addEventListener('click', guard(onClick));
    return node;
  }

  function safeParse(text) {
    if (text == null || text === '') return {};
    if (typeof text !== 'string') return text;
    try {
      return JSON.parse(text);
    } catch (err) {
      return { raw: text };
    }
  }

  function baseName(path) {
    if (!path) return 'workspace';
    var parts = String(path).replace(/\/+$/, '').split('/');
    return parts[parts.length - 1] || String(path);
  }

  function shortId(id) {
    if (!id) return 'new';
    var s = String(id);
    return s.length > 12 ? s.slice(0, 12) : s;
  }

  function num(v) {
    return typeof v === 'number' ? String(v) : (v == null ? '?' : String(v));
  }

  /* ---------- toasts ---------- */

  var lastToast = { text: '', at: 0 };

  function toast(level, text) {
    var body = text == null || text === '' ? 'unknown error' : String(text);
    var now = Date.now();
    if (body === lastToast.text && now - lastToast.at < 2000) return;
    lastToast = { text: body, at: now };
    var box = $('toasts');
    if (!box) return;
    var kind = level === 'error' || level === 'warn' || level === 'ok' ? level : 'info';
    var node = el('div', 'toast toast-' + kind, body);
    box.appendChild(node);
    setTimeout(function () {
      if (node.parentNode) node.parentNode.removeChild(node);
    }, 5000);
  }

  /* ---------- rpc ---------- */

  var nextId = 0;
  var pending = new Map();
  var EVENTS = Object.create(null);

  function rpc(cmd, args) {
    return new Promise(function (resolve, reject) {
      if (!window.ipc || typeof window.ipc.postMessage !== 'function') {
        reject(new Error('IPC bridge not available (window.ipc.postMessage)'));
        return;
      }
      var id = ++nextId;
      var timer = setTimeout(function () {
        pending.delete(id);
        reject(new Error(cmd + ': no reply from host (timeout)'));
      }, RPC_TIMEOUT_MS);
      pending.set(id, { resolve: resolve, reject: reject, timer: timer });
      try {
        window.ipc.postMessage(JSON.stringify({ id: id, cmd: cmd, args: args || {} }));
      } catch (err) {
        clearTimeout(timer);
        pending.delete(id);
        reject(err instanceof Error ? err : new Error(String(err)));
      }
    });
  }

  function rpcError(payload) {
    var code = payload && payload.code ? String(payload.code) : 'internal';
    var text = payload && payload.message ? String(payload.message) : 'request failed';
    var err = new Error(code + ': ' + text);
    err.code = code;
    return err;
  }

  window.__perch = {
    reply: function (id, ok, payloadJson) {
      var entry = pending.get(id);
      if (!entry) return;
      pending.delete(id);
      clearTimeout(entry.timer);
      var payload = safeParse(payloadJson);
      if (ok) entry.resolve(payload);
      else entry.reject(rpcError(payload));
    },
    emit: function (event, payloadJson) {
      var fn = EVENTS[event];
      if (typeof fn !== 'function') return;
      var payload = safeParse(payloadJson);
      try {
        fn(payload && typeof payload === 'object' ? payload : {});
      } catch (err) {
        toast('error', event + ': ' + message(err));
      }
    }
  };

  function call(cmd, args, opts) {
    var quiet = !!(opts && opts.quiet);
    return rpc(cmd, args).then(function (res) {
      if (!quiet) setLast(cmd + ' ok');
      return res;
    }, function (err) {
      toast('error', cmd + ': ' + message(err));
      if (!quiet) setLast(cmd + ' failed');
      return null;
    });
  }

  function setLast(text) {
    var node = $('status-last');
    if (node) node.textContent = text == null ? '' : String(text);
  }

  /* ---------- tabs ---------- */

  var tabs = [];
  var tabSeq = 0;
  var paneSeq = 0;
  var activeTab = null;

  var PANEL_TITLE = { browser: 'Browser', search: 'Search', handoff: 'Handoff', docs: 'Docs' };

  function byPaneKey(key) {
    for (var i = 0; i < tabs.length; i++) {
      if (tabs[i].paneKey === key) return tabs[i];
    }
    return null;
  }

  function tabByKind(kind) {
    for (var i = 0; i < tabs.length; i++) {
      if (tabs[i].kind === kind) return tabs[i];
    }
    return null;
  }

  function renderTabList() {
    var list = $('tab-list');
    if (!list) return;
    list.textContent = '';
    if (!tabs.length) {
      list.appendChild(el('div', 'side-empty', 'no tabs'));
      return;
    }
    for (var i = 0; i < tabs.length; i++) {
      (function (tab) {
        var row = el('div', 'tab-row' + (tab === activeTab ? ' active' : ''));
        var kind = el('span', 'tab-kind', tab.kind === 'terminal' ? 'pty' : tab.kind);
        var title = el('span', 'tab-title', tab.title);
        title.title = tab.paneKey || tab.title;
        var close = el('button', 'tab-close', 'x');
        close.type = 'button';
        close.title = 'close tab';
        close.addEventListener('click', function (ev) {
          ev.stopPropagation();
          guard(function () { closeTab(tab); })(ev);
        });
        row.appendChild(kind);
        row.appendChild(title);
        row.appendChild(close);
        row.addEventListener('click', function () { activate(tab); });
        list.appendChild(row);
      })(tabs[i]);
    }
  }

  function activate(tab) {
    if (!tab || !tab.el) return;
    var prev = activeTab;
    if (prev && prev !== tab && prev.kind === 'browser' && browser.open) {
      closeBrowser({ quiet: true });
    }
    activeTab = tab;
    for (var i = 0; i < tabs.length; i++) {
      if (tabs[i].el) tabs[i].el.hidden = tabs[i] !== tab;
    }
    renderTabList();
    if (tab.kind === 'terminal') {
      scheduleFit(tab);
    } else if (tab.kind === 'browser') {
      if (browser.engine === 'native' && browser.url && !browser.open) {
        openBrowserNative();
      } else {
        syncBrowserBounds();
      }
    } else if (tab.kind === 'docs') {
      ensureDocs();
    }
  }

  function openStatic(kind) {
    var tab = tabByKind(kind);
    if (!tab) {
      var pane = document.querySelector('.pane[data-kind="' + kind + '"]');
      if (!pane) {
        toast('error', 'pane not found: ' + kind);
        return null;
      }
      tab = {
        id: 'tab' + (++tabSeq),
        kind: kind,
        el: pane,
        title: PANEL_TITLE[kind] || kind,
        paneKey: null
      };
      tabs.push(tab);
      renderTabList();
    }
    activate(tab);
    return tab;
  }

  function closeTab(tab) {
    if (!tab) return;
    if (tab.kind === 'browser' && activeTab === tab) {
      closeBrowser({ quiet: true });
    }
    if (tab.kind === 'terminal') {
      tab.closed = true;
      if (tab.ro) {
        try { tab.ro.disconnect(); } catch (err) { /* ignore */ }
      }
      if (tab.term) {
        try { tab.term.dispose(); } catch (err) { /* ignore */ }
      }
      call('ptyKill', { paneKey: tab.paneKey }, { quiet: true });
    }
    if (tab.el && tab.el.parentNode) tab.el.parentNode.removeChild(tab.el);
    var index = tabs.indexOf(tab);
    if (index >= 0) tabs.splice(index, 1);
    if (activeTab === tab) {
      activeTab = null;
      if (tabs.length) {
        activate(tabs[tabs.length - 1]);
      } else {
        createTerminal({ title: 'terminal' });
      }
    } else {
      renderTabList();
    }
  }

  /* ---------- terminal ---------- */

  function spawnArgs(opts, paneKey) {
    var args = { paneKey: paneKey };
    if (opts.cwd) args.cwd = opts.cwd;
    if (opts.shell) args.shell = opts.shell;
    if (opts.agent) args.agent = opts.agent;
    if (opts.sessionId) args.sessionId = opts.sessionId;
    return args;
  }

  function createTerminal(opts) {
    opts = opts || {};
    var paneKey = opts.paneKey || ('tab:' + (++paneSeq));
    var existing = byPaneKey(paneKey);
    if (existing) {
      activate(existing);
      toast('warn', 'pane already open: ' + paneKey);
      return existing;
    }

    var pane = el('section', 'pane pane-term');
    pane.dataset.kind = 'terminal';
    var host = el('div', 'term-host');
    pane.appendChild(host);
    var panes = $('panes');
    if (!panes) {
      toast('error', 'panes container missing');
      return null;
    }
    panes.appendChild(pane);

    var title = opts.title ||
      (opts.agent ? opts.agent + ' ' + shortId(opts.sessionId) : 'terminal');

    var tab = {
      id: 'tab' + (++tabSeq),
      kind: 'terminal',
      el: pane,
      host: host,
      paneKey: paneKey,
      term: null,
      fit: null,
      ro: null,
      title: title,
      cwd: opts.cwd || null,
      lastCols: 0,
      lastRows: 0,
      spawned: false,
      closed: false
    };
    tabs.push(tab);
    activate(tab);

    if (typeof window.Terminal !== 'function') {
      host.appendChild(el('div', 'side-empty', 'xterm.js global Terminal is missing'));
      toast('error', 'xterm.js failed to load (window.Terminal missing)');
      return tab;
    }

    var term;
    try {
      term = new window.Terminal({
        fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
        fontSize: 12,
        lineHeight: 1.2,
        cursorBlink: true,
        scrollback: 5000,
        theme: TERM_THEME
      });
    } catch (err) {
      toast('error', 'terminal init failed: ' + message(err));
      return tab;
    }
    tab.term = term;

    if (window.FitAddon && typeof window.FitAddon.FitAddon === 'function') {
      try {
        tab.fit = new window.FitAddon.FitAddon();
        term.loadAddon(tab.fit);
      } catch (err) {
        tab.fit = null;
        toast('warn', 'fit addon unavailable: ' + message(err));
      }
    } else {
      toast('warn', 'window.FitAddon.FitAddon missing; terminal will not auto-fit');
    }

    try {
      term.open(host);
    } catch (err) {
      toast('error', 'terminal mount failed: ' + message(err));
      return tab;
    }

    term.onData(function (data) {
      call('ptyWrite', { paneKey: paneKey, data: data }, { quiet: true });
    });

    term.onResize(function () {
      pushResize(tab);
    });

    if (typeof ResizeObserver === 'function') {
      try {
        tab.ro = new ResizeObserver(function () { scheduleFit(tab); });
        tab.ro.observe(host);
      } catch (err) {
        tab.ro = null;
      }
    }

    scheduleFit(tab);

    call('ptySpawn', spawnArgs(opts, paneKey)).then(function (res) {
      if (!res || tab.closed) return;
      tab.spawned = true;
      pushResize(tab);
      if (res.pid != null) setLast('ptySpawn ' + paneKey + ' pid ' + res.pid);
    });

    return tab;
  }

  function scheduleFit(tab) {
    if (!tab || tab.kind !== 'terminal' || tab.closed) return;
    if (tab.fitQueued) return;
    tab.fitQueued = true;
    var run = function () {
      tab.fitQueued = false;
      if (tab.closed || tab.kind !== 'terminal' || !tab.term) return;
      if (tab.fit) {
        try { tab.fit.fit(); } catch (err) { /* container not measurable yet */ }
      }
      pushResize(tab);
    };
    if (typeof requestAnimationFrame === 'function') requestAnimationFrame(run);
    else setTimeout(run, 16);
  }

  function pushResize(tab) {
    if (!tab || !tab.term || tab.closed) return;
    if (!tab.spawned) return;
    var cols = tab.term.cols;
    var rows = tab.term.rows;
    if (!cols || !rows) return;
    if (tab.lastCols === cols && tab.lastRows === rows) return;
    tab.lastCols = cols;
    tab.lastRows = rows;
    call('ptyResize', { paneKey: tab.paneKey, cols: cols, rows: rows }, { quiet: true });
  }

  function b64ToBytes(value) {
    if (typeof value !== 'string' || value === '') return null;
    try {
      var bin = window.atob(value);
      var out = new Uint8Array(bin.length);
      for (var i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
      return out;
    } catch (err) {
      toast('error', 'ptyData: invalid base64');
      return null;
    }
  }

  /* ---------- browser ---------- */

  var browser = {
    open: false,
    url: '',
    engine: 'native',
    lastRect: null
  };

  function browserRect() {
    var node = $('browser-native');
    if (!node) return { x: 0, y: 0, width: 0, height: 0 };
    var r = node.getBoundingClientRect();
    return {
      x: Math.round(r.x),
      y: Math.round(r.y),
      width: Math.round(r.width),
      height: Math.round(r.height)
    };
  }

  function normalizeUrl(raw) {
    var value = String(raw == null ? '' : raw).trim();
    if (!value) return null;
    if (!/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(value)) value = 'https://' + value;
    var colon = value.indexOf(':');
    var scheme = value.slice(0, colon).toLowerCase();
    if (scheme !== 'http' && scheme !== 'https') {
      toast('error', 'only http and https URLs are allowed');
      return null;
    }
    return value;
  }

  function openBrowserNative() {
    if (!browser.url) {
      toast('warn', 'no URL to open');
      return Promise.resolve(null);
    }
    var rect = browserRect();
    if (rect.width < 8 || rect.height < 8) {
      toast('warn', 'browser area is not laid out yet');
      return Promise.resolve(null);
    }
    return call('browserOpen', { url: browser.url, rect: rect }).then(function (res) {
      if (!res) return null;
      browser.open = true;
      browser.lastRect = rect;
      setLast('browserOpen ' + (res.url || browser.url));
      return res;
    });
  }

  function closeBrowser(opts) {
    if (!browser.open) return;
    browser.open = false;
    browser.lastRect = null;
    call('browserClose', {}, opts);
  }

  function syncBrowserBounds() {
    if (!browser.open) return;
    if (!activeTab || activeTab.kind !== 'browser') return;
    var rect = browserRect();
    var prev = browser.lastRect;
    if (prev && prev.x === rect.x && prev.y === rect.y &&
        prev.width === rect.width && prev.height === rect.height) {
      return;
    }
    browser.lastRect = rect;
    call('browserBounds', { rect: rect, visible: true }, { quiet: true });
  }

  function onBrowserOpen() {
    var url = normalizeUrl($('browser-url').value);
    if (!url) {
      toast('warn', 'enter an http or https URL');
      return Promise.resolve(null);
    }
    $('browser-url').value = url;
    browser.url = url;

    if (browser.engine === 'lightpanda') {
      var pre = $('browser-text');
      if (!pre) return Promise.resolve(null);
      pre.hidden = false;
      pre.textContent = 'fetching ' + url + ' ...';
      setLast('browserFetch ' + url);
      return call('browserFetch', { url: url, dump: 'markdown' }).then(function (res) {
        if (!res) {
          pre.textContent = 'fetch failed: ' + url;
          return null;
        }
        var text = typeof res.text === 'string' ? res.text : JSON.stringify(res, null, 2);
        pre.textContent = text || '(empty document)';
        setLast('browserFetch ok ' + url);
        return res;
      });
    }

    return openBrowserNative();
  }

  function onEngineChange() {
    var engine = $('browser-engine').value === 'lightpanda' ? 'lightpanda' : 'native';
    if (engine === browser.engine) return;
    browser.engine = engine;
    if (browser.open) closeBrowser({ quiet: true });
    var pre = $('browser-text');
    if (pre) {
      pre.hidden = true;
      pre.textContent = '';
    }
    setLast('engine ' + engine);
    if (engine === 'native' && activeTab && activeTab.kind === 'browser' && browser.url) {
      openBrowserNative();
    }
  }

  /* ---------- search ---------- */

  function renderHit(hit) {
    var item = el('li', 'hit');

    var top = el('div', 'hit-top');
    top.appendChild(el('span', 'hit-agent', hit.agent || '?'));
    top.appendChild(el('span', 'hit-title', hit.title || '(untitled)'));
    var resume = button('resume', 'btn sm', function () {
      if (!hit.agent || !hit.sessionId) {
        toast('warn', 'hit has no agent/sessionId');
        return;
      }
      createTerminal({
        agent: hit.agent,
        sessionId: hit.sessionId,
        cwd: hit.cwd || null,
        title: hit.agent + ' ' + shortId(hit.sessionId)
      });
    });
    top.appendChild(resume);
    item.appendChild(top);

    if (hit.sessionId || hit.cwd) {
      var meta = el('div', 'hit-meta mono');
      var bits = [];
      if (hit.sessionId) bits.push(hit.sessionId);
      if (hit.cwd) bits.push(hit.cwd);
      meta.textContent = bits.join('  ');
      meta.title = meta.textContent;
      item.appendChild(meta);
    }

    if (hit.snippet) item.appendChild(el('div', 'hit-snippet', hit.snippet));
    if (hit.resumeCommand) item.appendChild(el('div', 'hit-cmd mono', hit.resumeCommand));

    return item;
  }

  function runSearch(ev) {
    if (ev && typeof ev.preventDefault === 'function') ev.preventDefault();
    var query = $('search-query').value.trim();
    if (!query) {
      toast('warn', 'search query is empty');
      return Promise.resolve(null);
    }
    var args = { query: query, limit: 20 };
    var agent = $('search-agent').value;
    if (agent) args.agent = agent;
    if ($('search-conversation').checked) args.conversation = true;

    setLast('historySearch ' + query);
    return call('historySearch', args).then(function (res) {
      var list = $('search-results');
      var meta = $('search-meta');
      list.textContent = '';
      if (!res) {
        meta.textContent = 'search failed';
        return null;
      }
      var hits = Array.isArray(res.hits) ? res.hits : [];
      meta.textContent = hits.length + ' hit' + (hits.length === 1 ? '' : 's') + ' for "' + query + '"';
      if (!hits.length) {
        list.appendChild(el('li', 'side-empty', 'no matches'));
        return res;
      }
      for (var i = 0; i < hits.length; i++) list.appendChild(renderHit(hits[i]));
      return res;
    });
  }

  /* ---------- handoff ---------- */

  function renderResult(node, rows) {
    if (!node) return;
    node.textContent = '';
    node.hidden = false;
    var shown = 0;
    for (var i = 0; i < rows.length; i++) {
      var value = rows[i][1];
      if (value == null || value === '') continue;
      var row = el('div', 'row');
      row.appendChild(el('div', 'key', rows[i][0]));
      row.appendChild(el('div', 'val' + (typeof value === 'string' ? ' mono' : ''), String(value)));
      node.appendChild(row);
      shown++;
    }
    if (!shown) node.appendChild(el('div', 'hint', 'no fields returned'));
  }

  function runHandoff(ev) {
    if (ev && typeof ev.preventDefault === 'function') ev.preventDefault();
    var name = $('handoff-name').value.trim();
    var prompt = $('handoff-prompt').value.trim();
    var agent = $('handoff-agent').value;
    var base = $('handoff-base').value.trim();

    if (!name) {
      toast('warn', 'handoff name is required');
      $('handoff-name').focus();
      return Promise.resolve(null);
    }
    if (!prompt) {
      toast('warn', 'handoff prompt is required');
      $('handoff-prompt').focus();
      return Promise.resolve(null);
    }

    var args = { name: name, prompt: prompt };
    if (agent) args.agent = agent;
    if (base) args.base = base;

    setLast('handoff ' + name);
    return call('handoff', args).then(function (res) {
      if (!res) return null;
      renderResult($('handoff-result'), [
        ['outcome', res.outcome],
        ['worktree', res.worktree],
        ['branch', res.branch],
        ['brief', res.brief],
        ['record', res.record],
        ['launch', res.launch]
      ]);
      setLast('handoff ' + (res.outcome || 'done'));
      if (res.outcome) toast('ok', 'handoff: ' + res.outcome);
      return res;
    });
  }

  /* ---------- status and workspaces ---------- */

  function refreshAppInfo() {
    return call('appInfo', {}).then(function (info) {
      if (!info) return null;
      var engine = info.engine || {};
      var version = info.version || '?';
      $('app-version').textContent = 'v' + version;
      var parts = ['perch ' + version];
      if (info.root) parts.push(info.root);
      parts.push('lightpanda ' + (engine.lightpanda ? (engine.version || 'ready') : 'unavailable'));
      $('status-left').textContent = parts.join('   ');
      return info;
    });
  }

  function refreshStatus() {
    return call('status', {}).then(function (res) {
      if (!res) return null;
      renderWorkspaces(Array.isArray(res.workspaces) ? res.workspaces : []);
      var db = typeof res.dbBytes === 'number' ? (res.dbBytes / 1048576).toFixed(1) + ' MiB' : '?';
      $('status-right').textContent =
        'sessions ' + num(res.sessions) + '   messages ' + num(res.messages) + '   db ' + db;
      return res;
    });
  }

  function renderWorkspace(ws) {
    var node = el('div', 'ws');

    var top = el('div', 'ws-top');
    var name = el('span', 'ws-name', baseName(ws.path));
    name.title = ws.path || '';
    var state = el('span', 'ws-state', ws.sleeping ? 'sleeping' : 'awake');
    if (ws.sleeping) state.className = 'ws-state sleeping';
    top.appendChild(name);
    top.appendChild(state);

    var sessions = Array.isArray(ws.sessions) ? ws.sessions : [];
    var meta = el('div', 'ws-branch',
      (ws.branch || 'no branch') + '   ' + sessions.length + ' session' + (sessions.length === 1 ? '' : 's'));

    var actions = el('div', 'ws-actions');

    var sleepBtn = button('sleep', 'btn sm', function () {
      sleepBtn.disabled = true;
      return call('sleep', { workspace: ws.path }).then(function (res) {
        sleepBtn.disabled = false;
        if (!res) return null;
        toast('ok', 'sleeping ' + baseName(res.path) + ' (' +
          (Array.isArray(res.sessions) ? res.sessions.length : 0) + ' session(s))');
        return refreshStatus();
      });
    });
    sleepBtn.disabled = !!ws.sleeping;

    var resumeBtn = button('resume', 'btn sm', function () {
      resumeBtn.disabled = true;
      return call('resume', { workspace: ws.path }).then(function (res) {
        resumeBtn.disabled = false;
        if (!res) return null;
        var list = Array.isArray(res.sessions) ? res.sessions : [];
        toast('ok', 'resumed ' + baseName(res.path) + ' (' + list.length + ' session(s))');
        for (var i = 0; i < list.length; i++) {
          var s = list[i] || {};
          if (!s.paneKey) continue;
          if (byPaneKey(s.paneKey)) continue;
          createTerminal({
            paneKey: s.paneKey,
            agent: s.agent,
            sessionId: s.sessionId,
            cwd: s.cwd,
            title: (s.agent || 'agent') + ' ' + shortId(s.sessionId)
          });
        }
        return refreshStatus();
      });
    });
    resumeBtn.disabled = !ws.sleeping;

    actions.appendChild(sleepBtn);
    actions.appendChild(resumeBtn);

    node.appendChild(top);
    node.appendChild(meta);
    node.appendChild(actions);
    return node;
  }

  function renderWorkspaces(list) {
    var box = $('workspace-list');
    if (!box) return;
    box.textContent = '';
    if (!list.length) {
      box.appendChild(el('div', 'side-empty', 'no workspaces yet'));
      return;
    }
    for (var i = 0; i < list.length; i++) {
      box.appendChild(renderWorkspace(list[i]));
    }
  }

  /* ---------- events from host ---------- */

  EVENTS.ptyData = function (payload) {
    var tab = byPaneKey(payload.paneKey);
    if (!tab || !tab.term) return;
    var bytes = b64ToBytes(payload.dataBase64);
    if (!bytes) return;
    tab.term.write(bytes);
  };

  EVENTS.ptyExit = function (payload) {
    var tab = byPaneKey(payload.paneKey);
    if (!tab) return;
    var code = payload.code == null ? '?' : payload.code;
    if (tab.term) {
      tab.term.write('\r\n\x1b[38;5;244m[pty exit ' + code + ']\x1b[0m\r\n');
    }
    tab.title = tab.title + ' (exited)';
    renderTabList();
    setLast('ptyExit ' + payload.paneKey + ' code ' + code);
  };

  EVENTS.browserNav = function (payload) {
    if (payload.url) {
      browser.url = String(payload.url);
      var input = $('browser-url');
      if (input) input.value = browser.url;
    }
    if (payload.title) setLast('browser: ' + payload.title);
  };

  EVENTS.toast = function (payload) {
    toast(payload.level || 'info', payload.message || 'host message');
  };

  /* ---------- boot ---------- */

  function fillAgents(select, placeholder, fallback) {
    if (!select) return;
    select.textContent = '';
    if (placeholder) {
      var any = el('option', null, placeholder);
      any.value = '';
      select.appendChild(any);
    }
    for (var i = 0; i < AGENTS.length; i++) {
      var opt = el('option', null, AGENTS[i]);
      opt.value = AGENTS[i];
      select.appendChild(opt);
    }
    select.value = fallback || '';
  }

  function ensureDocs() {
    var frame = $('docs-frame');
    if (frame && !frame.getAttribute('src')) frame.setAttribute('src', '/tutorial.html');
  }

  function boot() {
    fillAgents($('search-agent'), 'any agent', '');
    fillAgents($('handoff-agent'), null, 'codex');

    $('new-terminal').addEventListener('click', guard(function () {
      createTerminal({ title: 'terminal' });
    }));
    $('new-browser').addEventListener('click', guard(function () {
      openStatic('browser');
    }));

    var panelButtons = document.querySelectorAll('.panel-btn');
    for (var i = 0; i < panelButtons.length; i++) {
      (function (node) {
        node.addEventListener('click', guard(function () {
          openStatic(node.dataset.panel);
        }));
      })(panelButtons[i]);
    }

    $('browser-open').addEventListener('click', guard(onBrowserOpen));
    $('browser-engine').addEventListener('change', guard(onEngineChange));
    $('browser-url').addEventListener('keydown', guard(function (ev) {
      if (ev.key === 'Enter') onBrowserOpen();
    }));

    $('search-form').addEventListener('submit', guard(runSearch));
    $('handoff-form').addEventListener('submit', guard(runHandoff));

    window.addEventListener('resize', guard(function () {
      for (var j = 0; j < tabs.length; j++) {
        if (tabs[j].kind === 'terminal') scheduleFit(tabs[j]);
      }
      syncBrowserBounds();
    }));

    refreshAppInfo();
    refreshStatus();
    createTerminal({ title: 'terminal' });
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', boot);
  } else {
    boot();
  }
})();
