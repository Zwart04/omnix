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
        background: '#0a0a0a', foreground: '#d6d6d6', cursor: '#e8e8e8', cursorAccent: '#0a0a0a',
        selectionBackground: 'rgba(255,255,255,.16)',
        black: '#1f1f1f', red: '#f47067', green: '#7ee787', yellow: '#e3b341',
        blue: '#6cb6ff', magenta: '#dcbdfb', cyan: '#56d4dd', white: '#c9d1d9',
        brightBlack: '#6e7681', brightRed: '#ff938a', brightGreen: '#aff5b4',
        brightYellow: '#f2cc60', brightBlue: '#96d0ff', brightMagenta: '#f0c6ff',
        brightCyan: '#9be9ef', brightWhite: '#f0f6fc'
      },
      light: {
        background: '#ffffff', foreground: '#24292f', cursor: '#24292f', cursorAccent: '#ffffff',
        selectionBackground: 'rgba(31,111,235,.22)',
        black: '#24292f', red: '#cf222e', green: '#1a7f37', yellow: '#9a6700',
        blue: '#0969da', magenta: '#8250df', cyan: '#1b7c83', white: '#6e7781',
        brightBlack: '#57606a', brightRed: '#a40e26', brightGreen: '#2da44e',
        brightYellow: '#bf8700', brightBlue: '#218bff', brightMagenta: '#a475f9',
        brightCyan: '#3192aa', brightWhite: '#24292f'
      }
    }
  };

  /* Monospace stack per OS, then Orca's fallback chain: the terminal font each
     platform ships first, then fonts that cover Nerd Font glyphs. */
  var TERM_FONTS = {
    windows: '"Cascadia Mono", Consolas, "DejaVu Sans Mono", "Liberation Mono", "Symbols Nerd Font Mono", "MesloLGS Nerd Font", "JetBrainsMono Nerd Font", "Hack Nerd Font", monospace',
    macos: '"SF Mono", Menlo, Monaco, "Cascadia Mono", Consolas, "DejaVu Sans Mono", "Liberation Mono", "Symbols Nerd Font Mono", "MesloLGS Nerd Font", "JetBrainsMono Nerd Font", "Hack Nerd Font", monospace',
    linux: '"DejaVu Sans Mono", "Liberation Mono", "Noto Sans Mono", "Symbols Nerd Font Mono", "MesloLGS Nerd Font", "JetBrainsMono Nerd Font", "Hack Nerd Font", monospace'
  };

  var PLATFORM = 'linux';
  var TRACE_KEYS = false;

  function traceLine(obj) {
    if (!TRACE_KEYS) return;
    try {
      obj.ts = Date.now();
      call('uiTrace', { line: JSON.stringify(obj) }, { quiet: true });
    } catch (err) { /* tracing must never break input */ }
  }

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
    return rpc(cmd, args, opts && opts.timeout).then(function (res) {
      if (!quiet) setLast(cmd + ' ok');
      return res;
    }, function (err) {
      if (!quiet) {
        toast('error', cmd + ': ' + message(err));
        setLast(cmd + ' failed');
      }
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

  var PANEL_TITLE = { browser: 'Browser', search: 'Search', docs: 'Docs', settings: 'Settings' };
  var KIND_TAG = { terminal: 'pty', browser: 'web', search: 'srch', docs: 'docs', settings: 'set' };

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
        main.addEventListener('pointerdown', guard(function () {
          if (tab.kind === 'terminal') focusTerm(tab);
        }));

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
    var group = [tab];
    if (tab.splitWith) group.push(tab.splitWith);
    for (var i = 0; i < tabs.length; i++) {
      if (tabs[i].el) tabs[i].el.hidden = group.indexOf(tabs[i]) < 0;
      if (tabs[i].splitEl) tabs[i].splitEl.hidden = group.indexOf(tabs[i]) < 0;
    }
    renderTabs();
    markRail();
    if (tab.kind === 'terminal') {
      scheduleFit(tab);
      focusTerm(tab);
    } else if (tab.kind === 'browser') {
      if (browser.engine === 'native' && browser.url) {
        if (browser.open) syncBrowserBounds(true);
        else openBrowserNative();
      }
    } else if (tab.kind === 'docs') {
      ensureDocs();
    } else if (tab.kind === 'settings') {
      loadAgentSettings();
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
      var mate = tab.splitWith;
      if (mate) {
        var wrap = tab.splitEl;
        mate.splitWith = null;
        mate.splitEl = null;
        tab.splitWith = null;
        tab.splitEl = null;
        if (wrap && wrap.parentNode) {
          wrap.parentNode.insertBefore(mate.el, wrap);
          wrap.parentNode.removeChild(wrap);
        }
      }
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

  /* Splits the active terminal in two. Orca keeps both panes on screen and
     treats them as one group; closing either promotes the survivor. */
  function splitActive() {
    var t = activeTab;
    if (!t || t.kind !== 'terminal') {
      toast('warn', 'open a terminal before splitting');
      return null;
    }
    if (t.splitWith) {
      activate(t.splitWith);
      return t.splitWith;
    }
    return createTerminal({ split: true, cwd: t.cwd, title: 'terminal 2' });
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
    if (glyph) row.appendChild(iconNode(glyph));
    row.appendChild(el('span', 'menu-label', label));
    if (hint) row.appendChild(el('span', 'menu-hint', hint));
    if (!disabled) row.addEventListener('click', guard(run));
    return row;
  }

  function buildTabMenu() {
    var box = $('tab-new-menu');
    if (!box) return;
    box.textContent = '';
    box.appendChild(menuRow('Terminal', 'pty', 'terminal', function () {
      closeTabMenu();
      createTerminal({ title: 'terminal' });
    }));
    box.appendChild(menuRow('Split Terminal', 'Ctrl+Shift+D', 'terminal', function () {
      closeTabMenu();
      splitActive();
    }));
    box.appendChild(menuRow('Browser', 'web', 'browser', function () {
      closeTabMenu();
      openStatic('browser');
    }));
    box.appendChild(menuRow('Custom CLI\u2026', 'command', 'command', function () {
      closeTabMenu();
      openCmdModal();
    }));
    /* Orca lists only the CLIs it can actually launch: an item that would fail
       on click is noise, and most agents are simply not installed. */
    var detected = agentsInfo.filter(function (a) { return a.available !== false; });
    box.appendChild(el('div', 'menu-sep'));
    if (detected.length) {
      box.appendChild(el('div', 'menu-label-group', 'Detected agents'));
      for (var i = 0; i < detected.length; i++) {
        (function (agent) {
          box.appendChild(menuRow(
            agentLabel(agent.name),
            '',
            agent.name,
            function () {
              closeTabMenu();
              createTerminal({
                paneKey: 'agent:' + agent.name + ':' + Date.now(),
                agent: agent.name,
                title: agentLabel(agent.name)
              });
            }
          ));
        })(detected[i]);
      }
    } else {
      box.appendChild(el('div', 'menu-empty', 'No agent CLIs detected on PATH.'));
    }
    box.appendChild(el('div', 'menu-sep'));
    box.appendChild(menuRow('Agent settings\u2026', 'settings', 'gear', openAgentSettings));
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
    var anchor = opts.split ? activeTab : null;
    if (anchor && (anchor.kind !== 'terminal' || !anchor.el || anchor.splitWith)) anchor = null;
    var splitWrap = null;
    if (anchor) {
      splitWrap = el('div', 'pane-split');
      if (panes.insertBefore) panes.insertBefore(splitWrap, anchor.el);
      else panes.appendChild(splitWrap);
      splitWrap.appendChild(anchor.el);
      splitWrap.appendChild(pane);
    } else {
      panes.appendChild(pane);
    }

    var title = opts.title ||
      (opts.agent ? opts.agent + ' ' + shortId(opts.sessionId)
        : (splitWrap ? 'terminal 2' : 'terminal'));

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
      cwd: opts.cwd || (anchor && anchor.cwd) || null,
      agent: opts.agent || null,
      sessionId: opts.sessionId || null,
      lastCols: 0,
      lastRows: 0,
      spawned: false,
      closed: false,
      exited: false,
      handoffBrief: opts.handoffBrief || null,
      splitEl: splitWrap,
      splitWith: anchor
    };
    if (anchor) {
      anchor.splitEl = splitWrap;
      anchor.splitWith = tab;
    }
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
        fontSize: 14,
        lineHeight: 1.2,
        letterSpacing: 0,
        fontWeight: '500',
        fontWeightBold: '700',
        cursorStyle: 'block',
        cursorBlink: true,
        minimumContrastRatio: 4.5,
        scrollback: 10000,
        scrollSensitivity: 3,
        smoothScrollDuration: 80,
        macOptionIsMeta: true,
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

    /* WebGL draws glyphs at the pixel grid instead of laying out DOM spans, so
       text stays crisp at fractional scale factors. Falls back to xterm's DOM
       renderer when the context is unavailable (software GL, Xvfb, old driver). */
    if (window.WebglAddon && typeof window.WebglAddon.WebglAddon === 'function') {
      try {
        var gl = new window.WebglAddon.WebglAddon();
        term.loadAddon(gl);
        gl.onContextLoss(function () {
          traceLine({ kind: 'webgl', state: 'context-lost' });
          try { gl.dispose(); } catch (err) { /* already gone */ }
        });
        tab.webgl = gl;
        traceLine({ kind: 'webgl', state: 'active' });
      } catch (err) {
        tab.webgl = null;
        traceLine({ kind: 'webgl', state: 'fallback-dom', error: message(err) });
      }
    }

    /* xterm mounts its hidden textarea asynchronously; focus it a few times */
    focusTerm(tab);
    setTimeout(function () { focusTerm(tab); }, 0);
    setTimeout(function () { focusTerm(tab); }, 80);
    setTimeout(function () { focusTerm(tab); }, 300);

    term.onData(function (data) {
      traceLine({ kind: 'ptyData', len: data.length, pane: paneKey });
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
      focusTerm(tab);
      if (tab.handoffBrief && tab.term) {
        tab.term.write('\x1b[38;5;244m[omnix] handoff brief: ' +
          tab.handoffBrief + '\x1b[0m\r\n');
      }
      if (res.pid != null) setLast('ptySpawn ' + paneKey + ' pid ' + res.pid);
    });

    return tab;
  }

  function focusTerm(tab) {
    if (!tab || tab.kind !== 'terminal' || !tab.term || tab.closed) return;
    if (tab !== activeTab) return;
    try { tab.term.focus(); } catch (err) { /* terminal not mounted yet */ }
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

  /* ---------- agent settings ---------- */

  function openAgentSettings() {
    closeTabMenu();
    var tab = openStatic('settings');
    loadAgentSettings();
    return tab;
  }

  function loadAgentSettings() {
    return call('agentSettings', {}).then(function (res) {
      renderAgentSettings(res);
      return res;
    }).catch(function (err) {
      toast('error', 'agent settings: ' + message(err));
    });
  }

  function renderAgentSettings(res) {
    var box = $('agent-settings');
    if (!box) return;
    box.textContent = '';
    var list = res && Array.isArray(res.agents) ? res.agents : [];
    var installed = list.filter(function (a) { return a && a.available !== false; });
    var meta = $('agent-settings-meta');
    if (meta) {
      meta.textContent = installed.length + ' detected' +
        (res && res.path ? '  \u00b7  ' + res.path : '');
    }
    if (!installed.length) {
      box.appendChild(el('p', 'side-empty', 'No agent CLIs detected. Install one, then hit Refresh.'));
      return;
    }
    for (var i = 0; i < installed.length; i++) {
      box.appendChild(agentSettingsCard(installed[i]));
    }
  }

  function settingsField(card, label, value, placeholder, save) {
    var wrap = el('label', 'set-field');
    wrap.appendChild(el('span', 'set-field-label', label));
    var input = el('input', 'input mono set-input');
    input.type = 'text';
    input.value = value || '';
    input.placeholder = placeholder;
    input.spellcheck = false;
    input.autocomplete = 'off';
    var timer = null;
    input.addEventListener('input', function () {
      if (timer) clearTimeout(timer);
      timer = setTimeout(save, 500);
    });
    wrap.appendChild(input);
    card.appendChild(wrap);
    return input;
  }

  function agentSettingsCard(agent) {
    var card = el('div', 'set-card');
    var head = el('div', 'set-head');
    head.appendChild(iconNode(agent.name));
    head.appendChild(el('span', 'set-name', agentLabel(agent.name)));
    head.appendChild(el('span', 'tag mono', agent.bin || agent.name));
    head.appendChild(el('span', 'set-state', 'detected'));
    card.appendChild(head);

    var argsInput = null;
    var envInput = null;
    var save = function () {
      call('agentSettingsSave', {
        agent: agent.name,
        args: argsInput ? argsInput.value : '',
        env: envInput ? envInput.value : ''
      }).then(function () {
        toast('ok', 'Saved ' + agentLabel(agent.name) + ' launch settings');
      }).catch(function (err) {
        toast('error', 'Save failed: ' + message(err));
      });
    };

    argsInput = settingsField(card, 'Arguments', agent.args,
      'extra arguments, e.g. --dangerously-skip-permissions', save);
    envInput = settingsField(card, 'Environment', agent.env, 'KEY=value KEY=value', save);
    return card;
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

  /* Agent marks are inlined Lobe (@lobehub/icons-static-svg) and Simple Icons
     SVGs. Inlining keeps the shell working offline and lets every mark
     inherit currentColor, so themes and the disabled state both apply. */
  var ICONS = {
    'antigravity': `<svg viewBox="0 0 24 24" fill="currentColor" fill-rule="evenodd" aria-hidden="true"><path d="M21.751 22.607c1.34 1.005 3.35.335 1.508-1.508C17.73 15.74 18.904 1 12.037 1 5.17 1 6.342 15.74.815 21.1c-2.01 2.009.167 2.511 1.507 1.506 5.192-3.517 4.857-9.714 9.715-9.714 4.857 0 4.522 6.197 9.714 9.715z"></path></svg>`,
    'browser': `<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" aria-hidden="true"><circle cx="8" cy="8" r="5.4"/><path d="M2.6 8h10.8"/><path d="M8 2.6c1.65 1.55 2.5 3.4 2.5 5.4S9.65 12.45 8 13.4C6.35 12.45 5.5 10.6 5.5 8S6.35 4.15 8 2.6Z"/></svg>`,
    'claude': `<svg viewBox="0 0 24 24" fill="currentColor" fill-rule="evenodd" aria-hidden="true"><path d="M4.709 15.955l4.72-2.647.08-.23-.08-.128H9.2l-.79-.048-2.698-.073-2.339-.097-2.266-.122-.571-.121L0 11.784l.055-.352.48-.321.686.06 1.52.103 2.278.158 1.652.097 2.449.255h.389l.055-.157-.134-.098-.103-.097-2.358-1.596-2.552-1.688-1.336-.972-.724-.491-.364-.462-.158-1.008.656-.722.881.06.225.061.893.686 1.908 1.476 2.491 1.833.365.304.145-.103.019-.073-.164-.274-1.355-2.446-1.446-2.49-.644-1.032-.17-.619a2.97 2.97 0 01-.104-.729L6.283.134 6.696 0l.996.134.42.364.62 1.414 1.002 2.229 1.555 3.03.456.898.243.832.091.255h.158V9.01l.128-1.706.237-2.095.23-2.695.08-.76.376-.91.747-.492.584.28.48.685-.067.444-.286 1.851-.559 2.903-.364 1.942h.212l.243-.242.985-1.306 1.652-2.064.73-.82.85-.904.547-.431h1.033l.76 1.129-.34 1.166-1.064 1.347-.881 1.142-1.264 1.7-.79 1.36.073.11.188-.02 2.856-.606 1.543-.28 1.841-.315.833.388.091.395-.328.807-1.969.486-2.309.462-3.439.813-.042.03.049.061 1.549.146.662.036h1.622l3.02.225.79.522.474.638-.079.485-1.215.62-1.64-.389-3.829-.91-1.312-.329h-.182v.11l1.093 1.068 2.006 1.81 2.509 2.33.127.578-.322.455-.34-.049-2.205-1.657-.851-.747-1.926-1.62h-.128v.17l.444.649 2.345 3.521.122 1.08-.17.353-.608.213-.668-.122-1.374-1.925-1.415-2.167-1.143-1.943-.14.08-.674 7.254-.316.37-.729.28-.607-.461-.322-.747.322-1.476.389-1.924.315-1.53.286-1.9.17-.632-.012-.042-.14.018-1.434 1.967-2.18 2.945-1.726 1.845-.414.164-.717-.37.067-.662.401-.589 2.388-3.036 1.44-1.882.93-1.086-.006-.158h-.055L4.132 18.56l-1.13.146-.487-.456.061-.746.231-.243 1.908-1.312-.006.006z"></path></svg>`,
    'codex': `<svg viewBox="0 0 24 24" fill="currentColor" fill-rule="evenodd" aria-hidden="true"><path clip-rule="evenodd" d="M8.086.457a6.105 6.105 0 013.046-.415c1.333.153 2.521.72 3.564 1.7a.117.117 0 00.107.029c1.408-.346 2.762-.224 4.061.366l.063.03.154.076c1.357.703 2.33 1.77 2.918 3.198.278.679.418 1.388.421 2.126a5.655 5.655 0 01-.18 1.631.167.167 0 00.04.155 5.982 5.982 0 011.578 2.891c.385 1.901-.01 3.615-1.183 5.14l-.182.22a6.063 6.063 0 01-2.934 1.851.162.162 0 00-.108.102c-.255.736-.511 1.364-.987 1.992-1.199 1.582-2.962 2.462-4.948 2.451-1.583-.008-2.986-.587-4.21-1.736a.145.145 0 00-.14-.032c-.518.167-1.04.191-1.604.185a5.924 5.924 0 01-2.595-.622 6.058 6.058 0 01-2.146-1.781c-.203-.269-.404-.522-.551-.821a7.74 7.74 0 01-.495-1.283 6.11 6.11 0 01-.017-3.064.166.166 0 00.008-.074.115.115 0 00-.037-.064 5.958 5.958 0 01-1.38-2.202 5.196 5.196 0 01-.333-1.589 6.915 6.915 0 01.188-2.132c.45-1.484 1.309-2.648 2.577-3.493.282-.188.55-.334.802-.438.286-.12.573-.22.861-.304a.129.129 0 00.087-.087A6.016 6.016 0 015.635 2.31C6.315 1.464 7.132.846 8.086.457zm-.804 7.85a.848.848 0 00-1.473.842l1.694 2.965-1.688 2.848a.849.849 0 001.46.864l1.94-3.272a.849.849 0 00.007-.854l-1.94-3.393zm5.446 6.24a.849.849 0 000 1.695h4.848a.849.849 0 000-1.696h-4.848z"></path></svg>`,
    'command': `<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 3.6 8.6 8 4 12.4"/><path d="M9.4 12.4H12.4"/></svg>`,
    'copilot': `<svg viewBox="0 0 24 24" fill="currentColor" fill-rule="evenodd" aria-hidden="true"><path d="M16.295 5.29l-0.242 0.986h-2.32c-1.721 0-3.22 1.197-3.638 2.902l-2.101 8.593H3.798c-2.44 0-4.229-2.342-3.637-4.761l2.106-8.612c0.557-2.274 2.556-3.87 4.85-3.87h5.54c2.44 0 4.23 2.343 3.638 4.761"></path><path d="M16.295 5.29l-0.242 0.986h-2.32c-1.721 0-3.22 1.197-3.638 2.902l-2.101 8.593H3.798c-2.44 0-4.229-2.342-3.637-4.761l2.106-8.612c0.557-2.274 2.556-3.87 4.85-3.87h5.54c2.44 0 4.23 2.343 3.638 4.761M7.752 18.758l0.242-0.986h2.321c1.72 0 3.22-1.197 3.637-2.903l2.102-8.593h4.196c2.439 0 4.228 2.343 3.637 4.762l-2.107 8.611c-0.556 2.274-2.555 3.87-4.85 3.87h-5.54c-2.44 0-4.229-2.342-3.638-4.761"></path><path d="M7.752 18.758l0.242-0.986h2.321c1.72 0 3.22-1.197 3.637-2.903l2.102-8.593h4.196c2.439 0 4.228 2.343 3.637 4.762l-2.107 8.611c-0.556 2.274-2.555 3.87-4.85 3.87h-5.54c-2.44 0-4.229-2.342-3.638-4.761"></path></svg>`,
    'cursor': `<svg viewBox="0 0 24 24" fill="currentColor" fill-rule="evenodd" aria-hidden="true"><path d="M22.106 5.68L12.5.135a.998.998 0 00-.998 0L1.893 5.68a.84.84 0 00-.419.726v11.186c0 .3.16.577.42.727l9.607 5.547a.999.999 0 00.998 0l9.608-5.547a.84.84 0 00.42-.727V6.407a.84.84 0 00-.42-.726zm-.603 1.176L12.228 22.92c-.063.108-.228.064-.228-.061V12.34a.59.59 0 00-.295-.51l-9.11-5.26c-.107-.062-.063-.228.062-.228h18.55c.264 0 .428.286.296.514z"></path></svg>`,
    'gemini': `<svg viewBox="0 0 24 24" fill="currentColor" fill-rule="evenodd" aria-hidden="true"><path d="M20.616 10.835a14.147 14.147 0 01-4.45-3.001 14.111 14.111 0 01-3.678-6.452.503.503 0 00-.975 0 14.134 14.134 0 01-3.679 6.452 14.155 14.155 0 01-4.45 3.001c-.65.28-1.318.505-2.002.678a.502.502 0 000 .975c.684.172 1.35.397 2.002.677a14.147 14.147 0 014.45 3.001 14.112 14.112 0 013.679 6.453.502.502 0 00.975 0c.172-.685.397-1.351.677-2.003a14.145 14.145 0 013.001-4.45 14.113 14.113 0 016.453-3.678.503.503 0 000-.975 13.245 13.245 0 01-2.003-.678z"></path></svg>`,
    'grok': `<svg viewBox="0 0 24 24" fill="currentColor" fill-rule="evenodd" aria-hidden="true"><path d="M9.27 15.29l7.978-5.897c.391-.29.95-.177 1.137.272.98 2.369.542 5.215-1.41 7.169-1.951 1.954-4.667 2.382-7.149 1.406l-2.711 1.257c3.889 2.661 8.611 2.003 11.562-.953 2.341-2.344 3.066-5.539 2.388-8.42l.006.007c-.983-4.232.242-5.924 2.75-9.383.06-.082.12-.164.179-.248l-3.301 3.305v-.01L9.267 15.292M7.623 16.723c-2.792-2.67-2.31-6.801.071-9.184 1.761-1.763 4.647-2.483 7.166-1.425l2.705-1.25a7.808 7.808 0 00-1.829-1A8.975 8.975 0 005.984 5.83c-2.533 2.536-3.33 6.436-1.962 9.764 1.022 2.487-.653 4.246-2.34 6.022-.599.63-1.199 1.259-1.682 1.925l7.62-6.815"></path></svg>`,
    'kimi': `<svg viewBox="0 0 24 24" fill="currentColor" fill-rule="evenodd" aria-hidden="true"><path d="M21.846 0a1.923 1.923 0 110 3.846H20.15a.226.226 0 01-.227-.226V1.923C19.923.861 20.784 0 21.846 0z"></path><path d="M11.065 11.199l7.257-7.2c.137-.136.06-.41-.116-.41H14.3a.164.164 0 00-.117.051l-7.82 7.756c-.122.12-.302.013-.302-.179V3.82c0-.127-.083-.23-.185-.23H3.186c-.103 0-.186.103-.186.23V19.77c0 .128.083.23.186.23h2.69c.103 0 .186-.102.186-.23v-3.25c0-.069.025-.135.069-.178l2.424-2.406a.158.158 0 01.205-.023l6.484 4.772a7.677 7.677 0 003.453 1.283c.108.012.2-.095.2-.23v-3.06c0-.117-.07-.212-.164-.227a5.028 5.028 0 01-2.027-.807l-5.613-4.064c-.117-.078-.132-.279-.028-.381z"></path></svg>`,
    'kiro': `<svg viewBox="0 0 24 24" fill="currentColor" fill-rule="evenodd" aria-hidden="true"><path d="M4.594 6.677C6.67-2.226 18.746-2.211 21.16 6.632c.353 1.297 1.725 7.582-1.673 13.747-1.545 2.797-5.841 5.49-6.99 1.883C8.6 25.477 3.315 24.1 5.789 18.609l-.318.143c-3.57 1.305-3.863-1.208-3.173-2.513.45-.84.727-1.335.937-1.897.353-.975.458-1.568.593-2.498.27-1.837.277-3.607.765-5.167zm8.37.01a.92.92 0 00-.81.428c-.217.323-.33.825-.33 1.462 0 .705.15 1.89 1.14 1.89h.008c.757 0 1.214-.705 1.214-1.89 0-.622-.127-1.125-.367-1.455a1.014 1.014 0 00-.855-.435zm4.08 0a.92.92 0 00-.81.428c-.217.323-.33.825-.33 1.462 0 .705.15 1.89 1.14 1.89h.008c.757 0 1.215-.705 1.215-1.89 0-.622-.128-1.125-.368-1.455a1.014 1.014 0 00-.855-.435z"></path></svg>`,
    'opencode': `<svg viewBox="0 0 24 24" fill="currentColor" fill-rule="evenodd" aria-hidden="true"><path d="M16 6H8v12h8V6zm4 16H4V2h16v20z"></path></svg>`,
    'pi': `<svg viewBox="0 0 24 24" fill="currentColor" fill-rule="evenodd" aria-hidden="true"><path clip-rule="evenodd" d="M1 1h16.5v11H12v5.5H6.5V23H1V1zm5.5 5.5V12H12V6.5H6.5z"></path><path d="M17.5 12H23v11h-5.5V12z"></path></svg>`,
    'qwen': `<svg viewBox="0 0 24 24" fill="currentColor" fill-rule="evenodd" aria-hidden="true"><path d="M19.713 2.825h-5.907c0-.251-.066-.503-.196-.728L12.819.728A1.46 1.46 0 0 0 11.559 0h-1.58c-.52 0-1 .275-1.26.728l-2.42 4.19a1.45 1.45 0 0 0 0 1.453l.791 1.37c.26.448.74.727 1.26.727h11.36c.52 0 1-.275 1.26-.728l.79-1.37a1.45 1.45 0 0 0 0-1.451l-.79-1.37a1.46 1.46 0 0 0-1.26-.728zM.195 9.905l2.951 5.115a1.46 1.46 0 0 0-.531.531l-.791 1.37a1.45 1.45 0 0 0 0 1.452l.79 1.37c.26.448.74.727 1.26.727h4.84c.52 0 1-.275 1.26-.728l.79-1.37a1.45 1.45 0 0 0 0-1.451L5.086 7.083a1.46 1.46 0 0 0-1.26-.728h-1.58c-.52 0-1 .276-1.26.728l-.79 1.37a1.45 1.45 0 0 0 0 1.452m15.888 13.367 2.951-5.115c.217.125.469.196.728.196h1.582c.52 0 1-.275 1.26-.728l.79-1.37a1.45 1.45 0 0 0 0-1.451l-2.42-4.191a1.46 1.46 0 0 0-1.259-.728h-1.582c-.52 0-1 .276-1.259.728l-5.678 9.838a1.45 1.45 0 0 0 0 1.452l.79 1.37c.26.448.74.727 1.26.727h1.582c.52 0 1-.275 1.26-.728z"></path></svg>`,
    'rovo': `<svg viewBox="0 0 24 24" fill="currentColor" fill-rule="evenodd" aria-hidden="true"><path d="M7.12 11.084a.683.683 0 00-1.16.126L.075 22.974a.703.703 0 00.63 1.018h8.19a.678.678 0 00.63-.39c1.767-3.65.696-9.203-2.406-12.52zM11.434.386a15.515 15.515 0 00-.906 15.317l3.95 7.9a.703.703 0 00.628.388h8.19a.703.703 0 00.63-1.017L12.63.38a.664.664 0 00-1.196.006z"/></svg>`,
    'terminal': `<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 4.5 6.1 8 3 11.5"/><path d="M8.6 11.5H13"/></svg>`,
    'command-code': `<svg viewBox="0 0 24 24" fill="currentColor" fill-rule="evenodd" aria-hidden="true"><path clip-rule="evenodd" d="M19.334 0a4.67 4.67 0 0 0-4.666 4.668v2H9.334v-2A4.67 4.67 0 0 0 4.668 0 4.67 4.67 0 0 0 0 4.668a4.67 4.67 0 0 0 4.668 4.665h2v5.334h-2A4.67 4.67 0 0 0 0 19.334 4.67 4.67 0 0 0 4.668 24a4.67 4.67 0 0 0 4.666-4.666v-2h5.334v2A4.67 4.67 0 0 0 19.334 24 4.67 4.67 0 0 0 24 19.334a4.67 4.67 0 0 0-4.666-4.667h-2V9.333h2A4.67 4.67 0 0 0 24 4.668 4.673 4.673 0 0 0 19.334 0m-2 6.666v-2c0-1.107.893-2 2-2s2 .893 2 2-.893 2-2 2zm-12.668 0c-1.107 0-2-.893-2-2s.893-2 2-2 2 .893 2 2v2zm4.666 8V9.334h5.334v5.334zm10.002 6.667c-1.107 0-2-.893-2-2v-2h2c1.107 0 2 .892 2 2 0 1.107-.893 2-2 2m-14.668 0c-1.107 0-2-.893-2-2 0-1.108.893-2 2-2h2v2c0 1.107-.893 2-2 2"></path></svg>`,
    gear: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.6a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>`,
  };

  function iconKey(name) {
    var k = String(name == null ? '' : name).trim().toLowerCase().replace(/[\s_]+/g, '-');
    if (k === 'opencode2') return 'opencode';
    if (k === 'qwen-code') return 'qwen';
    if (k === 'claude-code') return 'claude';
    return k;
  }

  /* One icon node for menus, project chips, and session rows: a brand SVG when
     the agent is known, otherwise the agent's initial in the same 16px box. */
  function iconNode(name) {
    var mark = ICONS[iconKey(name)];
    var box = el('span', mark ? 'icon' : 'icon is-mono');
    if (mark) {
      box.innerHTML = mark;
      box.setAttribute('aria-hidden', 'true');
    } else {
      box.textContent = String(name == null ? '?' : name).slice(0, 1).toUpperCase();
    }
    return box;
  }

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
    rovo: 'Rovo',
    'command-code': 'Command Code'
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
      TRACE_KEYS = !!info.traceKeys;
      setAgents(info.agents);
      var parts = ['omnix ' + version];
      if (info.root) parts.push(info.root);
      parts.push('lightpanda ' + (engine.lightpanda ? (engine.version || 'ready') : 'unavailable'));
      $('status-left').textContent = parts.join('   ');
      return info;
    });
  }

  var lastStatus = null;

  var savedProjects = [];
  var projectStats = {};
  var trackedByPath = {};

  function refreshStatus() {
    return call('status', {}).then(function (res) {
      if (!res) return null;
      lastStatus = res;
      var list = Array.isArray(res.workspaces) ? res.workspaces : [];
      /* tracked workspace state only decorates sidebar rows now, so the saved
         project list is what decides which folders are listed */
      trackedByPath = {};
      for (var i = 0; i < list.length; i++) {
        if (list[i] && list[i].path) trackedByPath[list[i].path] = list[i];
      }
      var db = typeof res.dbBytes === 'number' ? (res.dbBytes / 1048576).toFixed(1) + ' MiB' : '?';
      $('status-right').textContent =
        'sessions ' + num(res.sessions) + '   messages ' + num(res.messages) + '   db ' + db;
      return res;
    });
  }

  /* ---------- usage and memory ---------- */

  var usageData = { ram: null, agents: [] };
  var usageTimer = null;

  function refreshUsage() {
    return call('usage', {}, { quiet: true }).then(function (res) {
      if (!res) return null;
      usageData = res;
      renderUsage();
      return res;
    });
  }

  function fmtBytes(bytes) {
    var n = Number(bytes) || 0;
    if (n <= 0) return '-';
    if (n >= 1073741824) return (n / 1073741824).toFixed(1) + ' GB';
    return Math.round(n / 1048576) + ' MB';
  }

  function fmtReset(seconds) {
    var sec = Number(seconds) || 0;
    if (sec <= 0) return 'now';
    var min = Math.round(sec / 60);
    if (min < 60) return min + 'm';
    var hours = Math.floor(min / 60);
    var rest = min % 60;
    if (hours < 24) return hours + 'h' + (rest ? ' ' + rest + 'm' : '');
    return Math.floor(hours / 24) + 'd ' + (hours % 24) + 'h';
  }

  function windowLabel(win) {
    var minutes = Number(win.windowMinutes) || 0;
    if (minutes >= 1440) return Math.round(minutes / 1440) + 'd window';
    if (minutes >= 60) return Math.round(minutes / 60) + 'h window';
    if (minutes > 0) return minutes + 'm window';
    return String(win.kind || 'window');
  }

  function renderUsage() {
    var ram = usageData.ram || {};
    var total = Number(ram.totalBytes) || 0;
    var used = Number(ram.usedBytes) || 0;
    var pct = total > 0 ? Math.round(used / total * 100) : null;
    var agents = Array.isArray(usageData.agents) ? usageData.agents : [];

    /* the compact chip warns with the tightest window it can see */
    var tightest = null;
    for (var a = 0; a < agents.length; a++) {
      var wins = Array.isArray(agents[a].windows) ? agents[a].windows : [];
      for (var w = 0; w < wins.length; w++) {
        var usedPct = Number(wins[w].usedPercent) || 0;
        if (!tightest || usedPct > tightest.percent) {
          tightest = { agent: agents[a].agent, percent: usedPct };
        }
      }
    }
    var chip = $('usage-chip');
    if (chip) {
      var text = 'RAM ' + (pct == null ? '-' : pct + '%');
      if (tightest) text += ' \u00b7 ' + tightest.agent + ' ' + Math.round(tightest.percent) + '%';
      chip.textContent = text;
      chip.title = total > 0
        ? fmtBytes(used) + ' of ' + fmtBytes(total) + ' in use'
        : 'memory data unavailable';
    }

    var ramNode = $('usage-ram');
    if (ramNode) {
      ramNode.textContent = total > 0
        ? 'Memory ' + fmtBytes(used) + ' of ' + fmtBytes(total) + ' (' + pct + '%)'
        : 'Memory data unavailable';
    }

    var list = $('usage-list');
    if (!list) return;
    list.textContent = '';
    if (!agents.length) {
      list.appendChild(el('div', 'usage-empty', 'No provider keeps local quota data yet.'));
      return;
    }
    for (var i = 0; i < agents.length; i++) {
      var agent = agents[i] || {};
      var card = el('div', 'usage-agent');
      var head = el('div', 'usage-agent-head');
      head.appendChild(el('span', 'usage-agent-name', String(agent.agent || 'agent')));
      if (agent.plan) head.appendChild(el('span', 'usage-agent-plan', String(agent.plan) + ' plan'));
      card.appendChild(head);
      var rows = Array.isArray(agent.windows) ? agent.windows : [];
      for (var r = 0; r < rows.length; r++) {
        var win = rows[r] || {};
        var value = Math.max(0, Math.min(100, Number(win.usedPercent) || 0));
        var row = el('div', 'usage-window');
        var label = el('span', 'usage-window-label');
        label.appendChild(el('span', '', windowLabel(win)));
        label.appendChild(el('small', '', 'resets in ' + fmtReset(win.resetsInSeconds)));
        row.appendChild(label);
        var bar = el('div', 'usage-bar');
        var fill = el('span', value >= 90 ? 'high' : '');
        fill.style.width = value + '%';
        bar.appendChild(fill);
        row.appendChild(bar);
        row.appendChild(el('span', 'usage-window-pct', Math.round(value) + '%'));
        card.appendChild(row);
      }
      list.appendChild(card);
    }
  }

  function openUsage() {
    renderUsage();
    showModal($('usage-modal'));
    return refreshUsage();
  }

  function startUsagePoll() {
    if (usageTimer) return;
    setTimeout(function () { refreshUsage(); }, 4000);
    usageTimer = setInterval(function () { refreshUsage(); }, 30000);
  }

  /* The sidebar lists the projects the user added, like Orca. Indexed history
     only fills in the session count and agent marks for those paths. */
  function refreshProjects() {
    return call('projects', {}).then(function (res) {
      savedProjects = res && Array.isArray(res.projects) ? res.projects.slice() : [];
      renderProjects(savedProjects);
      return call('historyProjects', { limit: 200 }).then(function (hist) {
        var list = hist && Array.isArray(hist.projects) ? hist.projects : [];
        projectStats = {};
        for (var i = 0; i < list.length; i++) {
          if (list[i] && list[i].path) projectStats[list[i].path] = list[i];
        }
        renderProjects(savedProjects);
        return hist;
      });
    }).catch(function (err) {
      toast('error', 'projects: ' + message(err));
      return null;
    });
  }

  /* ---------- add / remove project ---------- */

  function openProjectModal() {
    var input = $('project-path');
    var err = $('project-error');
    if (err) err.textContent = '';
    if (input) input.value = appRoot || '';
    showModal($('project-modal'));
    if (input && input.focus) input.focus();
  }

  function addProjectPath(path) {
    return call('projectAdd', { path: path }).then(function (res) {
      savedProjects = res && Array.isArray(res.projects) ? res.projects : savedProjects;
      toast('ok', 'Project added');
      return refreshProjects().then(function () {
        openProject({ path: path, name: baseName(path) });
      });
    });
  }

  function submitProject() {
    var input = $('project-path');
    var err = $('project-error');
    var path = input ? input.value.trim() : '';
    if (!path) {
      if (err) err.textContent = 'folder path is required';
      return;
    }
    if (err) err.textContent = '';
    addProjectPath(path).then(function () {
      hideModal($('project-modal'));
    }).catch(function (e) {
      if (err) err.textContent = message(e);
    });
  }

  /* + Project asks the OS for a folder. Cancelling leaves everything alone;
     only a failed dialog falls back to the typed-path modal. */
  function pickProject() {
    return call('pickFolder', {}, { quiet: true, timeout: 600000 }).then(function (res) {
      if (!res) {
        openProjectModal();
        return null;
      }
      var path = res.path ? String(res.path).trim() : '';
      if (!path) return null;
      return addProjectPath(path).catch(function (e) {
        openProjectModal();
        var input = $('project-path');
        if (input) input.value = path;
        var err = $('project-error');
        if (err) err.textContent = message(e);
        return null;
      });
    });
  }

  function removeProject(p) {
    if (!p || !p.path) return;
    call('projectRemove', { path: p.path }).then(function (res) {
      savedProjects = res && Array.isArray(res.projects) ? res.projects : [];
      renderProjects(savedProjects);
      toast('ok', 'Removed ' + (p.name || baseName(p.path)) + ' from the sidebar');
    }).catch(function (err) {
      toast('error', 'remove project: ' + message(err));
    });
  }

  /* A project row opens a terminal rooted at that project, the same gesture
     Orca uses when a workspace row is clicked. */
  function openProject(p) {
    if (!p || !p.path) return;
    var name = p.name || baseName(p.path);
    var existing = null;
    for (var i = 0; i < tabs.length; i++) {
      if (tabs[i].kind === 'terminal' && tabs[i].cwd === p.path) { existing = tabs[i]; break; }
    }
    if (existing) { activate(existing); return; }
    createTerminal({ title: name, cwd: p.path });
  }

  function renderProjects(list) {
    var box = $('workspace-list');
    if (!box) return;
    if (!list.length) {
      box.textContent = '';
      box.appendChild(el('div', 'side-empty', 'No projects yet. Add one with + Project.'));
      return;
    }
    box.textContent = '';
    /* Most recently used first, using the indexed timestamp when the path has
       history and the stored value otherwise. */
    var rows = list.slice();
    rows.sort(function (a, b) {
      var sa = projectStats[(a || {}).path] || {};
      var sb = projectStats[(b || {}).path] || {};
      return Number(sb.updatedAt || (b || {}).updatedAt || 0) -
        Number(sa.updatedAt || (a || {}).updatedAt || 0);
    });
    for (let i = 0; i < rows.length; i++) {
      let p = rows[i] || {};
      var ws = trackedByPath[p.path];
      if (ws) {
        box.appendChild(renderWorkspace(ws));
        continue;
      }
      var row = el('div', 'proj-row');

      var stat = projectStats[p.path] || {};
      var name = p.name || stat.name || baseName(p.path);

      var top = el('div', 'proj-top');
      top.appendChild(el('span', 'ws-name', name));
      top.appendChild(el('span', 'proj-time', relTime(stat.updatedAt || p.updatedAt)));
      row.appendChild(top);

      var count = Number(stat.sessions || p.sessions || 0);
      var meta = el('div', 'proj-meta');
      meta.appendChild(el('span', null, fmtNum(count) + (count === 1 ? ' session' : ' sessions')));
      var agents = Array.isArray(stat.agents) ? stat.agents : (Array.isArray(p.agents) ? p.agents : []);
      var chips = el('span', 'proj-agents');
      var shown = 0;
      for (var a = 0; a < agents.length && shown < 3; a++) {
        chips.appendChild(iconNode(agents[a]));
        shown++;
      }
      if (agents.length > shown) {
        chips.appendChild(el('span', 'glyph is-more', '+' + (agents.length - shown)));
      }
      meta.appendChild(chips);
      row.appendChild(meta);

      var remove = el('button', 'icon-btn proj-remove');
      remove.type = 'button';
      remove.title = 'Remove from sidebar';
      remove.setAttribute('aria-label', 'Remove ' + name + ' from the sidebar');
      remove.textContent = '\u00d7';
      row.appendChild(remove);

      row.tabIndex = 0;
      row.setAttribute('role', 'button');
      remove.addEventListener('click', guard(function (ev) {
        ev.stopPropagation();
        removeProject(p);
      }));
      row.addEventListener('click', guard(function () { openProject(p); }));
      row.addEventListener('keydown', guard(function (ev) {
        if (ev.key === 'Enter' || ev.key === ' ') {
          ev.preventDefault();
          openProject(p);
        }
      }));

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
        line.appendChild(iconNode(s.agent));
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
    { label: 'Agent settings', hint: 'settings', run: openAgentSettings },
    { label: 'Split Terminal', hint: 'Ctrl+Shift+D', run: splitActive },
    { label: 'Usage and memory', hint: 'quota', run: openUsage },
    { label: 'Add Project Folder', hint: 'picker', run: pickProject },
    { label: 'Add Project by Path', hint: 'workspace', run: openProjectModal },
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
    if (mod && ev.shiftKey && (key === 'd' || key === 'D')) {
      ev.preventDefault();
      splitActive();
      return;
    }
    if (key === 'Escape') {
      if (paletteVisible()) paletteClose();
      else if (!$('handoff-modal').hidden) hideModal($('handoff-modal'));
      else if (!$('project-modal').hidden) hideModal($('project-modal'));
      else if (!$('cmd-modal').hidden) hideModal($('cmd-modal'));
      else if (!$('usage-modal').hidden) hideModal($('usage-modal'));
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

    var newProject = $('new-project');
    if (newProject) newProject.addEventListener('click', guard(pickProject));

    var projectAdd = $('project-add');
    if (projectAdd) projectAdd.addEventListener('click', guard(submitProject));
    var projectCancel = $('project-cancel');
    if (projectCancel) projectCancel.addEventListener('click', guard(function () {
      hideModal($('project-modal'));
    }));
    var projectClose = $('project-close');
    if (projectClose) projectClose.addEventListener('click', guard(function () {
      hideModal($('project-modal'));
    }));
    var projectPath = $('project-path');
    if (projectPath) projectPath.addEventListener('keydown', guard(function (ev) {
      if (ev.key === 'Enter') {
        ev.preventDefault();
        submitProject();
      }
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

    var projectAddIcon = $('project-add-icon');
    if (projectAddIcon) projectAddIcon.addEventListener('click', guard(pickProject));

    var sideSettings = $('side-settings');
    if (sideSettings) sideSettings.addEventListener('click', guard(openAgentSettings));

    var usageChip = $('usage-chip');
    if (usageChip) usageChip.addEventListener('click', guard(openUsage));
    var usageRefresh = $('usage-refresh');
    if (usageRefresh) usageRefresh.addEventListener('click', guard(refreshUsage));
    var usageClose = $('usage-close');
    if (usageClose) usageClose.addEventListener('click', guard(function () {
      hideModal($('usage-modal'));
    }));
    var usageModal = $('usage-modal');
    if (usageModal) usageModal.addEventListener('click', guard(function (ev) {
      if (ev && ev.target === usageModal) hideModal($('usage-modal'));
    }));

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

    var settingsRefresh = $('agent-settings-refresh');
    if (settingsRefresh) settingsRefresh.addEventListener('click', guard(loadAgentSettings));

    var stripCmd = $('strip-command');
    if (stripCmd) stripCmd.addEventListener('click', guard(paletteOpen));

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

    window.addEventListener('focus', function () {
      if (activeTab && activeTab.kind === 'terminal') focusTerm(activeTab);
    });

    /* WebKitGTK sometimes keeps DOM focus on the body after a tab switch.
       Any plain typing then goes nowhere, so pull focus back to the pty. */
    document.addEventListener('keydown', function (ev) {
      traceLine({
        kind: 'keydown',
        key: ev.key,
        code: ev.code,
        target: (ev.target && (ev.target.tagName + '#' + (ev.target.id || ev.target.className || ''))) || 'none',
        active: activeTab ? activeTab.kind : 'none',
        defaultPrevented: ev.defaultPrevented
      });
      if (!activeTab || activeTab.kind !== 'terminal') return;
      var tag = (ev.target && ev.target.tagName || '').toLowerCase();
      if (tag === 'input' || tag === 'textarea' || tag === 'select') return;
      if (ev.metaKey || ev.ctrlKey || ev.altKey) return;
      focusTerm(activeTab);
    }, true);

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
    startUsagePoll();
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', boot);
  } else {
    boot();
  }
})();
