/* Omnix app UI. Vanilla JS, no frameworks, no CDN. */
(function () {
  'use strict';

  var AGENTS = [
    'claude', 'codex', 'opencode', 'opencode2', 'cursor', 'gemini', 'grok',
    'copilot', 'qwen-code', 'pi', 'kimi', 'kiro', 'antigravity', 'rovo'
  ];

  var RPC_TIMEOUT_MS = 20000;
  var SCAN_TIMEOUT_MS = 180000;
  var HISTORY_LIMIT = 80;
  var SEARCH_DEBOUNCE_MS = 200;

  /* Terminal palettes copied from each OS's shipped default profile so the
     workbench and the shell feel like the same terminal. */
  var PALETTES = {
    windows: {
      dark: {
        background: '#0c0c0c', foreground: '#cccccc', cursor: '#ffffff', cursorAccent: '#0c0c0c',
        selectionBackground: 'rgba(255,255,255,.25)',
        black: '#0c0c0c', red: '#c50f1f', green: '#13a10e', yellow: '#c19c00',
        blue: '#0037da', magenta: '#881798', cyan: '#3a96dd', white: '#cccccc',
        brightBlack: '#767676', brightRed: '#e74856', brightGreen: '#16c60c',
        brightYellow: '#f9f1a5', brightBlue: '#3b78ff', brightMagenta: '#b4009e',
        brightCyan: '#61d6d6', brightWhite: '#f2f2f2'
      },
      light: {
        background: '#ffffff', foreground: '#0c0c0c', cursor: '#0c0c0c', cursorAccent: '#ffffff',
        selectionBackground: 'rgba(0,55,218,.22)',
        black: '#0c0c0c', red: '#c50f1f', green: '#107c10', yellow: '#8a6d00',
        blue: '#0037da', magenta: '#881798', cyan: '#0f7b8a', white: '#767676',
        brightBlack: '#767676', brightRed: '#e74856', brightGreen: '#16c60c',
        brightYellow: '#c19c00', brightBlue: '#3b78ff', brightMagenta: '#b4009e',
        brightCyan: '#3a96dd', brightWhite: '#0c0c0c'
      }
    },
    macos: {
      dark: {
        background: '#000000', foreground: '#f2f2f2', cursor: '#52f2ff', cursorAccent: '#000000',
        selectionBackground: 'rgba(255,255,255,.28)',
        black: '#000000', red: '#c23621', green: '#25bc24', yellow: '#adad27',
        blue: '#492ee1', magenta: '#d338d3', cyan: '#33bbc8', white: '#cbcccd',
        brightBlack: '#818383', brightRed: '#fc391f', brightGreen: '#31e722',
        brightYellow: '#eaec23', brightBlue: '#5833ff', brightMagenta: '#f935f8',
        brightCyan: '#14f0f0', brightWhite: '#e9ebeb'
      },
      light: {
        background: '#ffffff', foreground: '#000000', cursor: '#000000', cursorAccent: '#ffffff',
        selectionBackground: 'rgba(0,0,0,.18)',
        black: '#000000', red: '#c23621', green: '#1c8f1b', yellow: '#8a8a1f',
        blue: '#3b25b3', magenta: '#a82aa8', cyan: '#1f8a94', white: '#818383',
        brightBlack: '#818383', brightRed: '#c23621', brightGreen: '#25bc24',
        brightYellow: '#adad27', brightBlue: '#492ee1', brightMagenta: '#d338d3',
        brightCyan: '#33bbc8', brightWhite: '#000000'
      }
    },
    linux: {
      dark: {
        background: '#000000', foreground: '#d3d7cf', cursor: '#d3d7cf', cursorAccent: '#000000',
        selectionBackground: 'rgba(211,215,207,.28)',
        black: '#2e3436', red: '#cc0000', green: '#4e9a06', yellow: '#c4a000',
        blue: '#3465a4', magenta: '#75507b', cyan: '#06989a', white: '#d3d7cf',
        brightBlack: '#555753', brightRed: '#ef2929', brightGreen: '#8ae234',
        brightYellow: '#fce94f', brightBlue: '#729fcf', brightMagenta: '#ad7fa8',
        brightCyan: '#34e2e2', brightWhite: '#eeeeec'
      },
      light: {
        background: '#ffffff', foreground: '#2e3436', cursor: '#2e3436', cursorAccent: '#ffffff',
        selectionBackground: 'rgba(52,101,164,.24)',
        black: '#2e3436', red: '#a40000', green: '#3d7a05', yellow: '#8f7600',
        blue: '#204a87', magenta: '#5c385f', cyan: '#067276', white: '#555753',
        brightBlack: '#555753', brightRed: '#cc0000', brightGreen: '#4e9a06',
        brightYellow: '#c4a000', brightBlue: '#3465a4', brightMagenta: '#75507b',
        brightCyan: '#06989a', brightWhite: '#2e3436'
      }
    }
  };

  /* Monospace stack per OS: the terminal font each platform ships. */
  var TERM_FONTS = {
    windows: '"Cascadia Mono", "Cascadia Code", Consolas, "Courier New", monospace',
    macos: '"SF Mono", SFMono-Regular, Menlo, Monaco, monospace',
    linux: '"DejaVu Sans Mono", "Ubuntu Mono", "Liberation Mono", "Noto Sans Mono", monospace'
  };

  var PLATFORM = 'linux';

  function termFont() {
    return TERM_FONTS[PLATFORM] || TERM_FONTS.linux;
  }

  function darkScheme() {
    if (!window.matchMedia) return true;
    return !window.matchMedia('(prefers-color-scheme: light)').matches;
  }

  function termTheme() {
    var set = PALETTES[PLATFORM] || PALETTES.linux;
    return darkScheme() ? set.dark : set.light;
  }

  function refreshTermThemes() {
    var theme = termTheme();
    var fontFamily = termFont();
    for (var i = 0; i < tabs.length; i++) {
      var term = tabs[i].term;
      if (!term || !term.options) continue;
      term.options.theme = theme;
      term.options.fontFamily = fontFamily;
    }
  }

  /* Adopt the host OS: platform fonts, terminal palette, chrome accent. */
  function applyPlatform(platform) {
    var name = String(platform || '').toLowerCase();
    if (name !== 'windows' && name !== 'macos' && name !== 'linux') name = 'linux';
    var root = document.documentElement;
    if (root && root.dataset) root.dataset.platform = name;
    if (name === PLATFORM) return;
    PLATFORM = name;
    refreshTermThemes();
  }

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

  function fmtNum(v) {
    var n = Number(v);
    if (v == null || !isFinite(n)) return v == null ? '?' : String(v);
    return n.toLocaleString('en-US');
  }

  /* local relative time, no dependency; accepts seconds or milliseconds */
  function relTime(ts) {
    var t = Number(ts);
    if (!isFinite(t) || t <= 0) return '';
    if (t > 1e12) t = t / 1000;
    var secs = Math.floor(Date.now() / 1000 - t);
    if (secs < 0) secs = 0;
    if (secs < 45) return 'just now';
    var mins = Math.floor(secs / 60);
    if (mins < 60) return mins + 'm ago';
    var hours = Math.floor(mins / 60);
    if (hours < 24) return hours + 'h ago';
    var days = Math.floor(hours / 24);
    if (days < 30) return days + 'd ago';
    var months = Math.floor(days / 30);
    if (months < 12) return months + 'mo ago';
    return Math.floor(months / 12) + 'y ago';
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

  function rpc(cmd, args, timeoutMs) {
    return new Promise(function (resolve, reject) {
      if (!window.ipc || typeof window.ipc.postMessage !== 'function') {
        reject(new Error('IPC bridge not available (window.ipc.postMessage)'));
        return;
      }
      var id = ++nextId;
      var timer = setTimeout(function () {
        pending.delete(id);
        reject(new Error(cmd + ': no reply from host (timeout)'));
      }, timeoutMs || RPC_TIMEOUT_MS);
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

  window.__omnix = {
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

  var PANEL_TITLE = { browser: 'Browser', search: 'Search', docs: 'Docs' };
  var KIND_TAG = { terminal: 'pty', browser: 'web', search: 'srch', docs: 'docs' };

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

  function renderTabs() {
    renderTabStrip();
    var empty = $('pane-empty');
    if (empty) empty.hidden = tabs.length > 0;
  }

  function renderTabStrip() {
    var box = $('tabstrip-tabs');
    if (!box) return;
    box.textContent = '';
    for (var i = 0; i < tabs.length; i++) {
      (function (tab) {
        var row = el('div', 'strip-tab' + (tab === activeTab ? ' active' : ''));
        var main = el('button', 'strip-main');
        main.type = 'button';
        if (tab === activeTab) main.setAttribute('aria-current', 'page');
        main.appendChild(el('span', 'kind-tag', KIND_TAG[tab.kind] || tab.kind));
        var title = el('span', 'strip-title', tab.title);
        title.title = tab.paneKey || tab.title;
        main.appendChild(title);
        main.addEventListener('click', guard(function () { activate(tab); }));

        var close = el('button', 'strip-close', '\u00d7');
        close.type = 'button';
        close.setAttribute('aria-label', 'Close ' + tab.title);
        close.addEventListener('click', function (ev) {
          if (ev && typeof ev.stopPropagation === 'function') ev.stopPropagation();
          guard(function () { closeTab(tab); })(ev);
        });

        row.appendChild(main);
        row.appendChild(close);
        box.appendChild(row);
      })(tabs[i]);
    }
  }

  function activate(tab) {
    if (!tab || !tab.el) return;
    var prev = activeTab;
    if (prev && prev !== tab && prev.kind === 'browser') {
      /* keep the page alive: hide the native view instead of destroying it */
      hideBrowser();
    }
    activeTab = tab;
    for (var i = 0; i < tabs.length; i++) {
      if (tabs[i].el) tabs[i].el.hidden = tabs[i] !== tab;
    }
    renderTabs();
    markRail();
    if (tab.kind === 'terminal') {
      scheduleFit(tab);
    } else if (tab.kind === 'browser') {
      if (browser.engine === 'native' && browser.url) {
        if (browser.open) syncBrowserBounds(true);
        else openBrowserNative();
      }
    } else if (tab.kind === 'docs') {
      ensureDocs();
    }
  }

  function markRail() {
    var buttons = document.querySelectorAll('.panel-btn');
    for (var i = 0; i < buttons.length; i++) {
      var panel = buttons[i].dataset.panel;
      var on = !!(activeTab && panel && activeTab.kind === panel);
      buttons[i].setAttribute('aria-current', on ? 'true' : 'false');
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
      /* a pty that is still spawning is killed once its reply lands */
      if (tab.spawned) {
        tab.spawned = false;
        call('ptyKill', { paneKey: tab.paneKey }, { quiet: true });
      }
    }
    if (tab.kind === 'terminal' && tab.el && tab.el.parentNode) {
      tab.el.parentNode.removeChild(tab.el);
    } else if (tab.el) {
      tab.el.hidden = true;
    }
    var index = tabs.indexOf(tab);
    if (index >= 0) tabs.splice(index, 1);
    if (activeTab === tab) {
      activeTab = null;
      if (tabs.length) activate(tabs[tabs.length - 1]);
      else renderTabs();
    } else {
      renderTabs();
    }
  }

  /* tab strip "+" menu */

  function closeTabMenu() {
    var menu = $('tab-new-menu');
    if (menu) menu.hidden = true;
    var btn = $('tab-new');
    if (btn) btn.setAttribute('aria-expanded', 'false');
  }

  function toggleTabMenu() {
    var menu = $('tab-new-menu');
    if (!menu) return;
    var open = menu.hidden;
    if (open) buildTabMenu();
    menu.hidden = !open;
    var btn = $('tab-new');
    if (btn) btn.setAttribute('aria-expanded', open ? 'true' : 'false');
  }

  function menuRow(label, hint, glyph, run, disabled) {
    var row = el('button', 'menu-item' + (disabled ? ' is-disabled' : ''));
    row.type = 'button';
    row.setAttribute('role', 'menuitem');
    if (disabled) {
      row.disabled = true;
      row.setAttribute('aria-disabled', 'true');
      row.title = 'not found on PATH';
    }
    if (glyph) row.appendChild(el('span', 'menu-glyph', glyph));
    row.appendChild(el('span', 'menu-label', label));
    if (hint) row.appendChild(el('span', 'menu-hint', hint));
    if (!disabled) row.addEventListener('click', guard(run));
    return row;
  }

  function buildTabMenu() {
    var box = $('tab-new-menu');
    if (!box) return;
    box.textContent = '';
    box.appendChild(menuRow('Terminal', 'pty', '>_', function () {
      closeTabMenu();
      createTerminal({ title: 'terminal' });
    }));
    box.appendChild(menuRow('Browser', 'web', 'web', function () {
      closeTabMenu();
      openStatic('browser');
    }));
    box.appendChild(menuRow('Custom CLI\u2026', 'command', '!', function () {
      closeTabMenu();
      openCmdModal();
    }));
    box.appendChild(el('div', 'menu-sep'));
    box.appendChild(el('div', 'menu-label-group', 'Agents'));
    for (var i = 0; i < agentsInfo.length; i++) {
      (function (agent) {
        box.appendChild(menuRow(
          agentLabel(agent.name),
          agent.available === false ? 'not found' : (agent.bin === agent.name ? '' : agent.bin),
          agent.name.slice(0, 1),
          function () {
            closeTabMenu();
            createTerminal({
              paneKey: 'agent:' + agent.name + ':' + Date.now(),
              command: agent.bin,
              title: agent.name
            });
          },
          agent.available === false
        ));
      })(agentsInfo[i]);
    }
  }

  /* ---------- terminal ---------- */

  function spawnArgs(opts, paneKey) {
    var args = { paneKey: paneKey };
    if (opts.cwd) args.cwd = opts.cwd;
    if (opts.shell) args.shell = opts.shell;
    if (opts.agent) args.agent = opts.agent;
    if (opts.sessionId) args.sessionId = opts.sessionId;
    if (opts.command) args.command = opts.command;
    return args;
  }

  function createTerminal(opts) {
    opts = opts || {};
    closeTabMenu();
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
      agent: opts.agent || null,
      sessionId: opts.sessionId || null,
      lastCols: 0,
      lastRows: 0,
      spawned: false,
      closed: false,
      exited: false,
      handoffBrief: opts.handoffBrief || null
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
        fontFamily: termFont(),
        fontSize: 12,
        lineHeight: 1.2,
        cursorBlink: true,
        scrollback: 5000,
        theme: termTheme()
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
      /* typing can beat the spawn reply; hold keystrokes until the pty exists */
      if (tab.exited) return;
      if (!tab.spawned) {
        var queued = tab.pendingWrites || '';
        if (queued.length < 4096) tab.pendingWrites = (queued + data).slice(0, 4096);
        return;
      }
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
      if (!res || tab.closed) {
        if (tab.closed) call('ptyKill', { paneKey: paneKey }, { quiet: true });
        return;
      }
      tab.spawned = true;
      pushResize(tab);
      if (tab.pendingWrites) {
        var queued = tab.pendingWrites;
        tab.pendingWrites = '';
        call('ptyWrite', { paneKey: paneKey, data: queued }, { quiet: true });
      }
      if (tab.handoffBrief && tab.term) {
        tab.term.write('\x1b[38;5;244m[omnix] handoff brief: ' +
          tab.handoffBrief + '\x1b[0m\r\n');
      }
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
    visible: false,
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
      browser.visible = true;
      browser.lastRect = rect;
      setLast('browserOpen ' + (res.url || browser.url));
      return res;
    });
  }

  function closeBrowser(opts) {
    if (!browser.open) return;
    browser.open = false;
    browser.visible = false;
    browser.lastRect = null;
    call('browserClose', {}, opts);
  }

  function hideBrowser() {
    if (!browser.open || !browser.visible) return;
    syncBrowserBounds(false);
  }

  function syncBrowserBounds(visible) {
    if (!browser.open) return;
    var show = visible !== false;
    /* never paint the native view over a different tab */
    if (show && (!activeTab || activeTab.kind !== 'browser' || activeTab.el.hidden)) show = false;
    var rect = browserRect();
    /* a hidden pane measures 0x0; keep the last good rect */
    if (rect.width < 8 || rect.height < 8) rect = browser.lastRect || rect;
    var prev = browser.lastRect;
    if (browser.visible === show && prev && prev.x === rect.x && prev.y === rect.y &&
        prev.width === rect.width && prev.height === rect.height) {
      return;
    }
    browser.visible = show;
    browser.lastRect = rect;
    call('browserBounds', { rect: rect, visible: show }, { quiet: true });
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

  function resumeSession(s) {
    if (!s || !s.agent || !s.sessionId) {
      toast('warn', 'session has no agent or id');
      return null;
    }
    return createTerminal({
      paneKey: s.paneKey || ('h:' + s.agent + ':' + s.sessionId),
      agent: s.agent,
      sessionId: s.sessionId,
      cwd: s.cwd || null,
      title: s.agent + ' ' + shortId(s.sessionId)
    });
  }

  function renderHit(hit) {
    var item = el('li', 'hit');

    var top = el('div', 'hit-top');
    top.appendChild(el('span', 'hit-agent', hit.agent || '?'));
    top.appendChild(el('span', 'hit-title', hit.title || '(untitled)'));
    var resume = button('resume', 'btn sm', function () {
      resumeSession(hit);
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

  /* ---------- handoff to another agent ---------- */

  var AGENT_LABEL = {
    claude: 'Claude Code',
    codex: 'Codex',
    opencode: 'OpenCode',
    opencode2: 'OpenCode 2',
    cursor: 'Cursor Agent',
    gemini: 'Gemini CLI',
    grok: 'Grok',
    copilot: 'GitHub Copilot',
    'qwen-code': 'Qwen Code',
    pi: 'Pi',
    kimi: 'Kimi',
    kiro: 'Kiro',
    antigravity: 'Antigravity',
    rovo: 'Rovo'
  };

  function agentLabel(name) {
    return AGENT_LABEL[name] || name;
  }

  var agentsInfo = AGENTS.map(function (name) {
    return { name: name, bin: name, available: true };
  });
  var appRoot = '';
  var handoffSource = null;
  var handoffContext = 'focused';

  function agentBin(name) {
    for (var i = 0; i < agentsInfo.length; i++) {
      if (agentsInfo[i].name === name) return agentsInfo[i].bin || name;
    }
    return name;
  }

  function setAgents(list) {
    if (!Array.isArray(list) || !list.length) return;
    agentsInfo = list
      .filter(function (a) { return a && a.name; })
      .map(function (a) {
        return {
          name: String(a.name),
          bin: String(a.bin || a.name),
          available: a.available !== false
        };
      });
    fillAgentSelect($('handoff-target'), 'opencode2');
  }

  function fillAgentSelect(select, preferred) {
    if (!select) return;
    var current = select.value;
    select.textContent = '';
    var fallback = '';
    for (var i = 0; i < agentsInfo.length; i++) {
      var agent = agentsInfo[i];
      var opt = el('option', null,
        agentLabel(agent.name) + ' (CLI)' + (agent.available ? '' : ' \u00b7 not found'));
      opt.value = agent.name;
      if (!agent.available) opt.disabled = true;
      else if (!fallback) fallback = agent.name;
      select.appendChild(opt);
    }
    var pick = current || preferred || fallback;
    var usable = false;
    for (var j = 0; j < agentsInfo.length; j++) {
      if (agentsInfo[j].name === pick && agentsInfo[j].available) usable = true;
    }
    select.value = usable ? pick : fallback;
  }

  function showModal(node) {
    if (!node) return;
    node.hidden = false;
    var focus = node.querySelector('select, input');
    if (focus && focus.focus) focus.focus();
  }

  function hideModal(node) {
    if (node) node.hidden = true;
  }

  function setHandoffContext(value) {
    handoffContext = value === 'full' ? 'full' : 'focused';
    var rows = document.querySelectorAll('#handoff-modal .radio-row');
    for (var i = 0; i < rows.length; i++) {
      rows[i].setAttribute('aria-checked',
        rows[i].dataset.context === handoffContext ? 'true' : 'false');
    }
  }

  function openHandoff(session) {
    var pick = session;
    if (!pick) {
      pick = hist.sessions[0] || hist.lastSession ||
        (activeTab && activeTab.kind === 'terminal' && activeTab.agent
          ? { agent: activeTab.agent, sessionId: activeTab.sessionId, cwd: activeTab.cwd, title: activeTab.title }
          : null);
    }
    handoffSource = pick && pick.agent && pick.sessionId ? pick : null;

    var title = $('handoff-source-title');
    var sub = $('handoff-source-sub');
    var cwd = $('handoff-cwd');
    if (handoffSource) {
      if (title) title.textContent = handoffSource.title || handoffSource.agent + ' session';
      if (sub) {
        sub.textContent = handoffSource.agent + ' \u00b7 ' +
          shortId(handoffSource.sessionId) + ' \u00b7 ' + (handoffSource.cwd || 'unknown cwd');
      }
    } else {
      if (title) title.textContent = 'No session selected';
      if (sub) sub.textContent = 'Open a session from Agents history, then hand off';
    }
    if (cwd) {
      cwd.textContent = (handoffSource && handoffSource.cwd) || appRoot || '-';
    }
    fillAgentSelect($('handoff-target'), 'opencode2');
    var start = $('handoff-start');
    if (start) start.disabled = !handoffSource;
    setHandoffContext('focused');
    showModal($('handoff-modal'));
  }

  function startHandoff() {
    if (!handoffSource) {
      toast('warn', 'choose a session from Agents history first');
      return Promise.resolve(null);
    }
    var select = $('handoff-target');
    var agent = select ? select.value : '';
    if (!agent) {
      toast('warn', 'pick a target agent');
      return Promise.resolve(null);
    }
    var start = $('handoff-start');
    if (start) start.disabled = true;
    setLast('handoffAgent ' + agent);
    return call('handoffAgent', {
      agent: agent,
      sourceAgent: handoffSource.agent,
      sessionId: handoffSource.sessionId,
      context: handoffContext,
      cwd: handoffSource.cwd || appRoot
    }).then(function (res) {
      if (start) start.disabled = false;
      if (!res) return null;
      hideModal($('handoff-modal'));
      createTerminal({
        paneKey: 'hf:' + agent + ':' + Date.now(),
        command: agentBin(agent),
        cwd: res.cwd,
        title: agent + ' handoff',
        handoffBrief: res.brief
      });
      toast('ok', 'handoff to ' + agent + ' \u00b7 ' + res.messages + ' messages');
      setLast('brief ' + res.brief);
      return res;
    }, function (err) {
      if (start) start.disabled = false;
      throw err;
    });
  }

  /* ---------- custom CLI ---------- */

  function openCmdModal() {
    var line = $('cmd-line');
    var cwd = $('cmd-cwd');
    var err = $('cmd-error');
    if (err) err.textContent = '';
    if (line) line.value = '';
    if (cwd) cwd.value = appRoot || '';
    showModal($('cmd-modal'));
    if (line && line.focus) line.focus();
  }

  function startCmd() {
    var line = $('cmd-line');
    var cwd = $('cmd-cwd');
    var err = $('cmd-error');
    var command = line ? line.value.trim() : '';
    if (!command) {
      if (err) err.textContent = 'command is required';
      return;
    }
    if (err) err.textContent = '';
    hideModal($('cmd-modal'));
    createTerminal({
      paneKey: 'cli:' + Date.now(),
      command: command,
      cwd: cwd ? cwd.value.trim() : '',
      title: command.split(/\s+/)[0]
    });
  }

  /* ---------- status and project tree ---------- */  /* ---------- status and project tree ---------- */

  function refreshAppInfo() {
    return call('appInfo', {}).then(function (info) {
      if (!info) return null;
      var engine = info.engine || {};
      var version = info.version || '?';
      $('app-version').textContent = 'v' + version;
      applyPlatform(info.platform);
      appRoot = info.root || '';
      setAgents(info.agents);
      var parts = ['omnix ' + version];
      if (info.root) parts.push(info.root);
      parts.push('lightpanda ' + (engine.lightpanda ? (engine.version || 'ready') : 'unavailable'));
      $('status-left').textContent = parts.join('   ');
      return info;
    });
  }

  var lastStatus = null;

  var projects = [];

  function refreshStatus() {
    return call('status', {}).then(function (res) {
      if (!res) return null;
      lastStatus = res;
      var list = Array.isArray(res.workspaces) ? res.workspaces : [];
      renderWorkspaces(list);
      var db = typeof res.dbBytes === 'number' ? (res.dbBytes / 1048576).toFixed(1) + ' MiB' : '?';
      $('status-right').textContent =
        'sessions ' + num(res.sessions) + '   messages ' + num(res.messages) + '   db ' + db;
      return res;
    });
  }

  /* no tracked workspaces yet: list indexed projects so the tree is not empty */
  function refreshProjects() {
    return call('historyProjects', { limit: 60 }).then(function (res) {
      if (!res) return null;
      projects = Array.isArray(res.projects) ? res.projects.slice() : [];
      /* contract says updatedAt desc; sorting keeps the tree stable if the host order drifts */
      projects.sort(function (a, b) { return Number(b.updatedAt || 0) - Number(a.updatedAt || 0); });
      var tracked = lastStatus && Array.isArray(lastStatus.workspaces) ? lastStatus.workspaces : [];
      if (!tracked.length) renderProjects(projects);
      return res;
    });
  }

  function renderProjects(list) {
    var box = $('workspace-list');
    if (!box) return;
    if (!list.length) {
      box.textContent = '';
      box.appendChild(el('div', 'side-empty', 'No projects indexed yet. Press Rescan.'));
      return;
    }
    box.textContent = '';
    for (var i = 0; i < list.length; i++) {
      var p = list[i] || {};
      var row = el('div', 'proj-row');
      row.title = p.path || '';

      var top = el('div', 'proj-top');
      top.appendChild(el('span', 'ws-name', p.name || baseName(p.path)));
      top.appendChild(el('span', 'proj-time', relTime(p.updatedAt)));
      row.appendChild(top);

      var count = Number(p.sessions || 0);
      var meta = el('div', 'proj-meta');
      meta.appendChild(el('span', null, fmtNum(count) + (count === 1 ? ' session' : ' sessions')));
      var agents = Array.isArray(p.agents) ? p.agents : [];
      var shown = 0;
      for (var a = 0; a < agents.length && shown < 3; a++) {
        meta.appendChild(el('span', 'glyph', String(agents[a] || '?').slice(0, 1)));
        shown++;
      }
      if (agents.length > shown) {
        meta.appendChild(el('span', 'glyph', '+' + (agents.length - shown)));
      }
      row.appendChild(meta);

      box.appendChild(row);
    }
  }

  function chevron() {
    var node = el('span', 'ws-chev', '\u25b8');
    node.setAttribute('aria-hidden', 'true');
    return node;
  }

  function renderWorkspace(ws) {
    var wrap = el('div', 'ws');

    var sessions = Array.isArray(ws.sessions) ? ws.sessions : [];
    var row = el('button', 'ws-row');
    row.type = 'button';
    row.setAttribute('aria-expanded', 'false');
    row.title = ws.path || '';
    row.appendChild(chevron());
    row.appendChild(el('span', 'ws-name', baseName(ws.path)));
    row.appendChild(el('span', 'tag branch', ws.branch || 'no branch'));
    row.appendChild(el('span', 'ws-state' + (ws.sleeping ? ' sleeping' : ''),
      ws.sleeping ? 'sleeping' : 'awake'));

    var kids = el('div', 'ws-kids');
    kids.hidden = true;

    row.addEventListener('click', guard(function () {
      var open = row.getAttribute('aria-expanded') === 'true';
      row.setAttribute('aria-expanded', open ? 'false' : 'true');
      kids.hidden = open;
    }));

    for (var i = 0; i < sessions.length; i++) {
      (function (s) {
        var line = el('button', 'sess-row');
        line.type = 'button';
        line.title = s.resumeCommand || s.sessionId || '';
        line.appendChild(el('span', 'glyph', String(s.agent || '?').slice(0, 1)));
        line.appendChild(el('span', 'inv-title',
          (s.agent || 'agent') + ' ' + shortId(s.sessionId)));
        if (s.paneKey) line.appendChild(el('span', 'pane-tag', s.paneKey));
        line.addEventListener('click', guard(function () { resumeSession(s); }));
        kids.appendChild(line);
      })(sessions[i]);
    }

    if (!sessions.length) {
      kids.appendChild(el('div', 'side-empty', 'No tracked sessions.'));
    }

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
        for (var k = 0; k < list.length; k++) {
          var s = list[k] || {};
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
    kids.appendChild(actions);

    wrap.appendChild(row);
    wrap.appendChild(kids);
    return wrap;
  }

  function renderWorkspaces(list) {
    var box = $('workspace-list');
    if (!box) return;
    box.textContent = '';
    if (!list.length) {
      box.appendChild(el('div', 'side-empty', 'No workspaces tracked yet.'));
      return;
    }
    for (var i = 0; i < list.length; i++) {
      box.appendChild(renderWorkspace(list[i]));
    }
  }

  /* ---------- indexed history panel ---------- */

  var hist = {
    scope: 'all',
    agent: '',
    query: '',
    scan: null,
    scannedAt: 0,
    sessions: [],
    lastSession: null,
    loading: false,
    scanError: null,
    listError: null
  };

  var historyTimer = null;

  function historyCount() {
    if (hist.scan && Array.isArray(hist.scan.agents)) {
      var total = 0;
      for (var i = 0; i < hist.scan.agents.length; i++) {
        total += Number(hist.scan.agents[i].sessions) || 0;
      }
      if (total > 0) return total;
    }
    return hist.sessions.length;
  }

  function renderHistory() {
    var count = $('history-count');
    if (count) {
      var n = historyCount();
      count.textContent = fmtNum(n) + ' session' + (n === 1 ? '' : 's');
    }

    renderScopeChips();
    renderAgentChips();
    renderHistoryMeta();
    renderHistoryList();
  }

  function renderScopeChips() {
    var box = $('history-scope');
    if (!box) return;
    var chips = box.children || [];
    for (var i = 0; i < chips.length; i++) {
      var scope = chips[i].dataset ? chips[i].dataset.scope : null;
      if (!scope) continue;
      chips[i].setAttribute('aria-pressed', scope === hist.scope ? 'true' : 'false');
    }
  }

  var agentListSig = '';

  function agentSignature() {
    var agents = hist.scan && Array.isArray(hist.scan.agents) ? hist.scan.agents : [];
    var parts = [];
    for (var i = 0; i < agents.length; i++) {
      parts.push(String(agents[i].agent || '') + ':' + Number(agents[i].sessions || 0));
    }
    return parts.join(',');
  }

  function renderAgentChips() {
    var box = $('history-agents');
    if (!box) return;

    var sig = agentSignature();
    if (sig !== agentListSig) {
      agentListSig = sig;
      box.textContent = '';

      var all = el('button', 'chip', 'All agents');
      all.type = 'button';
      all.dataset.agent = '';
      all.addEventListener('click', guard(function () {
        if (hist.agent === '') return;
        hist.agent = '';
        renderHistory();
        return loadSessions().then(renderHistory);
      }));
      box.appendChild(all);

      var agents = hist.scan && Array.isArray(hist.scan.agents) ? hist.scan.agents : [];
      for (var i = 0; i < agents.length; i++) {
        (function (entry) {
          var name = entry && entry.agent ? String(entry.agent) : '';
          if (!name) return;
          var chip = el('button', 'chip', name + ' ' + fmtNum(entry.sessions || 0));
          chip.type = 'button';
          chip.dataset.agent = name;
          chip.addEventListener('click', guard(function () {
            hist.agent = hist.agent === name ? '' : name;
            renderHistory();
            return loadSessions().then(renderHistory);
          }));
          box.appendChild(chip);
        })(agents[i]);
      }
    }

    /* pressed state updates in place so the focused chip is not rebuilt */
    var chips = box.children || [];
    for (var c = 0; c < chips.length; c++) {
      var value = chips[c].dataset ? String(chips[c].dataset.agent || '') : '';
      chips[c].setAttribute('aria-pressed', value === hist.agent ? 'true' : 'false');
    }
  }

  function renderHistoryMeta() {
    var node = $('history-meta');
    if (!node) return;
    if (!hist.scan) {
      node.textContent = '';
      return;
    }
    node.textContent = 'Scanned ' + fmtNum(hist.scan.files || 0) + ' files \u00b7 ' +
      fmtNum(hist.scan.messages || 0) + ' messages \u00b7 ' +
      (relTime(hist.scannedAt) || 'just now');
  }

  function renderHistoryList() {
    var list = $('history-list');
    if (!list) return;
    list.textContent = '';

    if (hist.loading) {
      var loading = el('div', 'loading', 'Scanning agent history...');
      loading.setAttribute('role', 'status');
      list.appendChild(loading);
      return;
    }

    var failure = hist.scanError || hist.listError;
    if (failure) {
      var err = el('div', 'error', failure);
      err.setAttribute('role', 'alert');
      list.appendChild(err);
      list.appendChild(el('div', 'empty', 'Press Rescan to try again.'));
      return;
    }

    if (!hist.sessions.length) {
      var empty = el('div', 'empty', 'No sessions indexed yet. Press Rescan.');
      list.appendChild(empty);
      return;
    }

    for (var i = 0; i < hist.sessions.length; i++) {
      list.appendChild(sessionCard(hist.sessions[i]));
    }
  }

  /* history titles often start with <system-reminder> scaffolding */
  function cleanText(text) {
    return String(text == null ? '' : text)
      .replace(/<system-reminder>[\s\S]*?<\/system-reminder>/gi, ' ')
      .replace(/<[^>]{1,80}>/g, ' ')
      .replace(/\s+/g, ' ')
      .trim();
  }

  function sessionCard(s) {
    var card = el('div', 'sess');
    card.setAttribute('role', 'listitem');

    var main = el('button', 'sess-main');
    main.type = 'button';
    main.setAttribute('aria-label',
      'Resume ' + (s.title || 'session') + ' from ' + (s.agent || 'agent'));

    var top = el('span', 'sess-top');
    top.appendChild(el('span', 'tag agent', s.agent || '?'));
    var titleText = cleanText(s.title) || '(untitled)';
    var title = el('span', 'sess-title', titleText);
    title.title = titleText;
    top.appendChild(title);
    main.appendChild(top);

    var preview = cleanText(s.preview);
    if (preview) main.appendChild(el('span', 'sess-preview', preview));

    var meta = el('span', 'sess-meta');
    meta.appendChild(el('span', null, fmtNum(s.messageCount || 0) + ' msgs'));
    meta.appendChild(el('span', null, relTime(s.updatedAt) || 'unknown'));
    meta.appendChild(el('span', null, s.agent || '?'));
    main.appendChild(meta);

    main.addEventListener('click', guard(function () {
      resumeSession({
        paneKey: 'h:' + s.agent + ':' + s.sessionId,
        agent: s.agent,
        sessionId: s.sessionId,
        cwd: s.cwd,
        title: s.title
      });
    }));

    var hop = el('button', 'sess-hop', '\u2192');
    hop.type = 'button';
    hop.title = 'Hand off to another agent';
    hop.setAttribute('aria-label', 'Hand off ' + (s.title || 'session'));
    hop.addEventListener('click', guard(function () {
      openHandoff({ agent: s.agent, sessionId: s.sessionId, cwd: s.cwd, title: s.title });
    }));

    card.appendChild(main);
    card.appendChild(hop);
    return card;
  }

  function loadSessions() {
    var args = { scope: hist.scope, limit: HISTORY_LIMIT };
    if (hist.agent) args.agent = hist.agent;
    if (hist.query) args.query = hist.query;

    return rpc('historySessions', args).then(function (res) {
      hist.sessions = res && Array.isArray(res.sessions) ? res.sessions : [];
      if (hist.sessions.length) hist.lastSession = hist.sessions[0];
      hist.listError = null;
      setLast('historySessions ok');
      return res;
    }, function (err) {
      hist.sessions = [];
      hist.listError = 'History listing failed: ' + message(err);
      toast('error', 'historySessions: ' + message(err));
      setLast('historySessions failed');
      return null;
    });
  }

  function loadHistory() {
    hist.loading = true;
    hist.scanError = null;
    hist.listError = null;
    renderHistory();

    var rescan = $('history-rescan');
    if (rescan) rescan.setAttribute('aria-busy', 'true');

    // Full index can walk hundreds of transcripts on first run.
    return rpc('historyScan', {}, SCAN_TIMEOUT_MS).then(function (scan) {
      hist.scan = scan || null;
      hist.scannedAt = Date.now();
      setLast('historyScan ok');
      renderHistory();
    }, function (err) {
      hist.scanError = 'History scan failed: ' + message(err);
      toast('error', 'historyScan: ' + message(err));
      setLast('historyScan failed');
      renderHistory();
    }).then(function () {
      return loadSessions();
    }).then(function () {
      hist.loading = false;
      if (rescan) rescan.removeAttribute('aria-busy');
      renderHistory();
      var tracked = lastStatus && Array.isArray(lastStatus.workspaces) ? lastStatus.workspaces : [];
      if (!tracked.length) refreshProjects();
    });
  }

  function onHistorySearch() {
    var input = $('history-search');
    var value = input ? String(input.value || '') : '';
    if (historyTimer) clearTimeout(historyTimer);
    historyTimer = setTimeout(guard(function () {
      historyTimer = null;
      var trimmed = value.trim();
      if (trimmed === hist.query) return;
      hist.query = trimmed;
      return loadSessions().then(renderHistory);
    }), SEARCH_DEBOUNCE_MS);
  }

  /* ---------- command palette ---------- */

  var paletteItems = [
    { label: 'New Terminal', hint: 'pty', run: function () { createTerminal({ title: 'terminal' }); } },
    { label: 'New Browser', hint: 'web', run: function () { openStatic('browser'); } },
    { label: 'New Agent Session', hint: 'agent', run: function () { toggleTabMenu(); } },
    { label: 'Custom CLI', hint: 'agent', run: openCmdModal },
    { label: 'Search History', hint: 'panel', run: function () {
      openStatic('search');
      var input = $('search-query');
      if (input) input.focus();
    } },
    { label: 'Hand off to another agent', hint: 'agent', run: function () { openHandoff(null); } },
    { label: 'Docs', hint: 'panel', run: function () { openStatic('docs'); } },
    { label: 'Rescan History', hint: 'history', run: function () { return loadHistory(); } },
    { label: 'Resume last session', hint: 'history', run: resumeLastSession }
  ];

  var paletteReturn = null;
  var paletteMatches = [];
  var paletteIndex = 0;

  function resumeLastSession() {
    var s = hist.sessions[0] || hist.lastSession;
    if (!s) {
      toast('warn', 'No indexed session to resume yet.');
      return;
    }
    resumeSession({
      paneKey: 'h:' + s.agent + ':' + s.sessionId,
      agent: s.agent,
      sessionId: s.sessionId,
      cwd: s.cwd,
      title: s.title
    });
  }

  function paletteVisible() {
    var box = $('palette');
    return !!box && !box.hidden;
  }

  function paletteOpen() {
    var box = $('palette');
    if (!box) return;
    paletteReturn = document.activeElement || null;
    box.hidden = false;
    var input = $('palette-input');
    if (input) {
      input.value = '';
      input.focus();
    }
    paletteFilter('');
  }

  function paletteClose() {
    var box = $('palette');
    if (!box || box.hidden) return;
    box.hidden = true;
    var back = paletteReturn;
    paletteReturn = null;
    if (back && typeof back.focus === 'function') {
      try { back.focus(); } catch (err) { /* element gone */ }
    }
  }

  function paletteFilter(query) {
    var q = String(query == null ? '' : query).trim().toLowerCase();
    paletteMatches = [];
    for (var i = 0; i < paletteItems.length; i++) {
      if (!q || paletteItems[i].label.toLowerCase().indexOf(q) >= 0) {
        paletteMatches.push(paletteItems[i]);
      }
    }
    paletteIndex = 0;
    renderPalette();
  }

  function renderPalette() {
    var list = $('palette-list');
    if (!list) return;
    list.textContent = '';

    if (!paletteMatches.length) {
      list.appendChild(el('div', 'palette-empty', 'No command matches.'));
      syncPaletteSelection();
      return;
    }

    for (var i = 0; i < paletteMatches.length; i++) {
      (function (item, index) {
        var row = el('div', 'palette-item', item.label);
        row.id = 'palette-item-' + index;
        row.dataset.index = String(index);
        row.setAttribute('role', 'option');
        row.setAttribute('aria-selected', index === paletteIndex ? 'true' : 'false');
        row.appendChild(el('span', 'palette-hint', item.hint));
        row.addEventListener('click', guard(function () { paletteRun(index); }));
        list.appendChild(row);
      })(paletteMatches[i], i);
    }
    syncPaletteSelection();
  }

  function syncPaletteSelection() {
    var input = $('palette-input');
    if (!input) return;
    if (paletteMatches.length) {
      input.setAttribute('aria-activedescendant', 'palette-item-' + paletteIndex);
    } else {
      input.removeAttribute('aria-activedescendant');
    }
  }

  function paletteMove(delta) {
    if (!paletteMatches.length) return;
    paletteIndex = (paletteIndex + delta + paletteMatches.length) % paletteMatches.length;
    var list = $('palette-list');
    if (list && list.children) {
      for (var i = 0; i < list.children.length; i++) {
        var row = list.children[i];
        if (!row.id) continue;
        row.setAttribute('aria-selected', i === paletteIndex ? 'true' : 'false');
      }
    }
    syncPaletteSelection();
  }

  function paletteRun(index) {
    var item = paletteMatches[index];
    if (!item) return;
    paletteClose();
    guard(item.run)();
  }

  function onPaletteKey(ev) {
    if (!paletteVisible()) return;
    if (ev.key === 'Escape') {
      ev.preventDefault();
      paletteClose();
      return;
    }
    if (ev.key === 'ArrowDown') {
      ev.preventDefault();
      paletteMove(1);
      return;
    }
    if (ev.key === 'ArrowUp') {
      ev.preventDefault();
      paletteMove(-1);
      return;
    }
    if (ev.key === 'Enter') {
      ev.preventDefault();
      paletteRun(paletteIndex);
      return;
    }
    if (ev.key === 'Tab') {
      ev.preventDefault();
      var input = $('palette-input');
      if (input) input.focus();
    }
  }

  function onPaletteInput() {
    var input = $('palette-input');
    paletteFilter(input ? input.value : '');
  }

  function onGlobalKey(ev) {
    var key = ev && ev.key ? String(ev.key) : '';
    var mod = ev && (ev.ctrlKey || ev.metaKey);
    if (mod && (key === 'k' || key === 'K')) {
      ev.preventDefault();
      if (paletteVisible()) paletteClose();
      else paletteOpen();
      return;
    }
    if (mod && ev.shiftKey && (key === 'p' || key === 'P')) {
      ev.preventDefault();
      paletteOpen();
      return;
    }
    if (key === 'Escape') {
      if (paletteVisible()) paletteClose();
      else if (!$('handoff-modal').hidden) hideModal($('handoff-modal'));
      else if (!$('cmd-modal').hidden) hideModal($('cmd-modal'));
      else if (!($('tab-new-menu') || {}).hidden) closeTabMenu();
    }
  }

  /* ---------- panel collapse ---------- */

  function setHistoryCollapsed(collapsed) {
    var app = $('app');
    var panel = $('agents-panel');
    var btn = $('history-collapse');
    var rail = $('rail-agents');
    if (app) {
      if (collapsed) app.classList.add('hist-collapsed');
      else app.classList.remove('hist-collapsed');
    }
    if (panel) {
      if (collapsed) panel.classList.add('collapsed');
      else panel.classList.remove('collapsed');
    }
    if (btn) btn.setAttribute('aria-expanded', collapsed ? 'false' : 'true');
    if (rail) {
      rail.setAttribute('aria-expanded', collapsed ? 'false' : 'true');
      rail.setAttribute('aria-current', collapsed ? 'false' : 'true');
    }
    if (!collapsed) ensureHistory();
  }

  /* the history panel is closed by default: index only when it is opened */
  function ensureHistory() {
    if (hist.loading) return null;
    if (!hist.scan) return loadHistory();
    if (!hist.sessions.length) return loadSessions().then(renderHistory);
    return null;
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
    if (!tab || tab.exited) return;
    tab.exited = true;
    tab.spawned = false;
    var code = payload.code == null ? '?' : payload.code;
    if (tab.term) {
      tab.term.write('\r\n\x1b[38;5;244m[pty exit ' + code + ']\x1b[0m\r\n');
    }
    tab.title = tab.title + ' (exited)';
    renderTabs();
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

  function wire() {
    fillAgents($('search-agent'), 'any agent', '');
    fillAgentSelect($('handoff-target'), 'opencode2');

    $('new-terminal').addEventListener('click', guard(function () {
      createTerminal({ title: 'terminal' });
    }));
    $('new-browser').addEventListener('click', guard(function () {
      openStatic('browser');
    }));

    var emptyBtn = $('pane-empty-terminal');
    if (emptyBtn) {
      emptyBtn.addEventListener('click', guard(function () {
        createTerminal({ title: 'terminal' });
      }));
    }

    var projectRefresh = $('project-refresh');
    if (projectRefresh) {
      projectRefresh.addEventListener('click', guard(function () {
        refreshAppInfo();
        refreshStatus();
        return refreshProjects();
      }));
    }

    var panelButtons = document.querySelectorAll('.panel-btn[data-panel]');
    for (var i = 0; i < panelButtons.length; i++) {
      (function (node) {
        var panel = node.dataset ? node.dataset.panel : null;
        if (!panel) return;
        node.addEventListener('click', guard(function () {
          if (panel === 'handoff') {
            openHandoff(null);
            return;
          }
          openStatic(panel);
        }));
      })(panelButtons[i]);
    }

    var cmdBtn = $('cmd-btn');
    if (cmdBtn) cmdBtn.addEventListener('click', guard(paletteOpen));

    $('browser-open').addEventListener('click', guard(onBrowserOpen));
    $('browser-engine').addEventListener('change', guard(onEngineChange));
    $('browser-url').addEventListener('keydown', guard(function (ev) {
      if (ev.key === 'Enter') onBrowserOpen();
    }));

    $('search-form').addEventListener('submit', guard(runSearch));

    $('tab-new').addEventListener('click', guard(toggleTabMenu));

    /* handoff modal */
    $('handoff-close').addEventListener('click', guard(function () {
      hideModal($('handoff-modal'));
    }));
    $('handoff-cancel').addEventListener('click', guard(function () {
      hideModal($('handoff-modal'));
    }));
    $('handoff-start').addEventListener('click', guard(startHandoff));
    $('handoff-modal').addEventListener('click', guard(function (ev) {
      if (ev && ev.target === $('handoff-modal')) hideModal($('handoff-modal'));
    }));
    var radios = document.querySelectorAll('#handoff-modal .radio-row');
    for (var r = 0; r < radios.length; r++) {
      (function (row) {
        row.addEventListener('click', guard(function () {
          setHandoffContext(row.dataset ? row.dataset.context : 'focused');
        }));
      })(radios[r]);
    }

    /* custom CLI modal */
    $('cmd-close').addEventListener('click', guard(function () { hideModal($('cmd-modal')); }));
    $('cmd-cancel').addEventListener('click', guard(function () { hideModal($('cmd-modal')); }));
    $('cmd-start').addEventListener('click', guard(startCmd));
    $('cmd-modal').addEventListener('click', guard(function (ev) {
      if (ev && ev.target === $('cmd-modal')) hideModal($('cmd-modal'));
    }));
    $('cmd-line').addEventListener('keydown', guard(function (ev) {
      if (ev.key === 'Enter') startCmd();
    }));

    var railAgents = $('rail-agents');
    if (railAgents) {
      railAgents.addEventListener('click', guard(function () {
        setHistoryCollapsed(!(document.getElementById('app') || {}).classList.contains('hist-collapsed'));
      }));
    }

    $('history-rescan').addEventListener('click', guard(loadHistory));
    $('history-search').addEventListener('input', guard(onHistorySearch));

    var scopeBox = $('history-scope');
    if (scopeBox && scopeBox.children) {
      for (var s = 0; s < scopeBox.children.length; s++) {
        (function (chip) {
          var scope = chip.dataset ? chip.dataset.scope : null;
          if (!scope) return;
          chip.addEventListener('click', guard(function () {
            if (hist.scope === scope) return;
            hist.scope = scope;
            renderHistory();
            return loadSessions().then(renderHistory);
          }));
        })(scopeBox.children[s]);
      }
    }

    $('history-collapse').addEventListener('click', guard(function () {
      setHistoryCollapsed(true);
    }));

    $('palette-input').addEventListener('keydown', guard(onPaletteKey));
    $('palette-input').addEventListener('input', guard(onPaletteInput));
    $('palette').addEventListener('mousedown', guard(function (ev) {
      if (ev && ev.target === $('palette')) paletteClose();
    }));

    document.addEventListener('keydown', guard(onGlobalKey));

    window.addEventListener('resize', guard(function () {
      for (var j = 0; j < tabs.length; j++) {
        if (tabs[j].kind === 'terminal') scheduleFit(tabs[j]);
      }
      syncBrowserBounds(true);
    }));

    /* panel collapse/resize changes layout without a window resize event */
    if (typeof ResizeObserver === 'function') {
      try {
        var mainRo = new ResizeObserver(guard(function () { syncBrowserBounds(true); }));
        var main = $('main');
        if (main) mainRo.observe(main);
      } catch (err) { /* layout observer is optional */ }
    }

    if (window.matchMedia) {
      try {
        var scheme = window.matchMedia('(prefers-color-scheme: light)');
        var onScheme = function () { refreshTermThemes(); };
        if (scheme.addEventListener) scheme.addEventListener('change', onScheme);
        else if (scheme.addListener) scheme.addListener(onScheme);
      } catch (err) { /* older webkit */ }
    }
  }

  function boot() {
    wire();
    renderTabs();
    /* the right column starts closed: no index walk, no chat cards */
    setHistoryCollapsed(true);
    renderHistory();

    refreshAppInfo();
    refreshStatus().then(refreshProjects);
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', boot);
  } else {
    boot();
  }
})();
