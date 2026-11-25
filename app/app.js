/* Omnix app UI. Vanilla JS, no frameworks, no CDN. */
(function () {
  'use strict';

  var RPC_TIMEOUT_MS = 20000;
  var SCAN_TIMEOUT_MS = 180000;
  var HISTORY_LIMIT = 500;
  var SEARCH_DEBOUNCE_MS = 200;
  var SUPPORT_URL = 'https://tako.id/Zwart04/gift';
  var REPO_URL = 'https://github.com/Zwart04/omnix';

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

  function chromeDrag(target) {
    return !(target && target.closest &&
      target.closest('button, a, input, textarea, select, [role="button"]'));
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
    var parts = String(path).replace(/[\\/]+$/, '').split(/[\\/]/);
    return parts[parts.length - 1] || String(path);
  }

  function parentName(path) {
    var text = String(path || '').replace(/[\\/]+$/, '');
    var cut = Math.max(text.lastIndexOf('/'), text.lastIndexOf('\\'));
    return cut > 0 ? baseName(text.slice(0, cut)) : '';
  }

  /* Two saved folders can share a basename. Extend the parent trail until the
     label is unique, so clicking never depends on guessing which row is which. */
  function disambiguatedName(name, path, list) {
    var key = normPath(path);
    var others = [];
    for (var i = 0; i < (list || []).length; i++) {
      var other = list[i] || {};
      if (!other.path || normPath(other.path) === key) continue;
      if ((other.name || baseName(other.path)) === name) others.push(normPath(other.path));
    }
    if (!others.length) return name;

    var parts = key.split(/[\\/]/).filter(function (p) { return !!p; });
    if (parts.length < 2) return name;
    for (var take = 1; take < parts.length; take++) {
      var trail = parts.slice(parts.length - 1 - take, parts.length - 1).join('/');
      var collision = false;
      for (var o = 0; o < others.length; o++) {
        var op = others[o].split(/[\\/]/).filter(function (p) { return !!p; });
        if (op.slice(op.length - 1 - take, op.length - 1).join('/') === trail) {
          collision = true;
          break;
        }
      }
      if (trail && !collision) return name + ' · ' + trail;
    }
    return name + ' · ' + key;
  }

  /* localStorage is optional in the test stub and in locked-down webviews. */
  function storeGet(key) {
    try {
      return window.localStorage ? window.localStorage.getItem(key) : null;
    } catch (err) {
      return null;
    }
  }

  function storeSet(key, value) {
    try {
      if (window.localStorage) window.localStorage.setItem(key, value);
    } catch (err) { /* private mode: keep the in-memory title */ }
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

  /* Status bar copy stays human: RPC names belong in logs, not in the UI. */
  var STATUS_DONE = {
    appInfo: 'Ready',
    status: 'Workspace loaded',
    projects: 'Projects loaded',
    historyProjects: 'History stats loaded',
    historyScan: 'Scan complete',
    historySessions: 'History loaded',
    historySearch: 'Search complete',
    ptySpawn: 'Terminal started',
    ptyKill: 'Terminal closed',
    ptyResize: 'Terminal resized',
    ptyWrite: 'Input sent',
    browserOpen: 'Browser opened',
    browserClose: 'Browser closed',
    browserFetch: 'Page fetched',
    browserBounds: 'Layout updated',
    agentSettings: 'Agent settings loaded',
    projectAdd: 'Project added',
    projectRemove: 'Project removed',
    pickFolder: 'Folder selected',
    handoffAgent: 'Handoff started',
    handoffList: 'Handoffs loaded',
    handoffClose: 'Handoff closed',
    agentSend: 'Message sent',
    paneState: 'Pane state',
    gc: 'Cleanup',
    memory: 'Memory loaded',
    usage: 'Usage loaded'
  };

  var STATUS_FAIL = {
    appInfo: 'App info',
    status: 'Workspace',
    projects: 'Projects',
    historyProjects: 'History stats',
    historyScan: 'History scan',
    historySessions: 'History',
    historySearch: 'Search',
    ptySpawn: 'Terminal',
    ptyKill: 'Terminal',
    ptyResize: 'Terminal',
    ptyWrite: 'Input',
    browserOpen: 'Browser',
    browserClose: 'Browser',
    browserFetch: 'Page',
    browserBounds: 'Browser layout',
    agentSettings: 'Agent settings',
    projectAdd: 'Project',
    projectRemove: 'Project',
    pickFolder: 'Folder picker',
    handoffAgent: 'Handoff',
    handoffList: 'Handoffs',
    handoffClose: 'Handoff',
    agentSend: 'Message',
    paneState: 'Pane state',
    gc: 'Cleanup',
    memory: 'Memory',
    usage: 'Usage'
  };

  function call(cmd, args, opts) {
    var quiet = !!(opts && opts.quiet);
    return rpc(cmd, args, opts && opts.timeout).then(function (res) {
      if (!quiet && STATUS_DONE[cmd]) setLast(STATUS_DONE[cmd]);
      return res;
    }, function (err) {
      if (!quiet) {
        var name = STATUS_FAIL[cmd] || 'Request';
        toast('error', name + ' failed: ' + message(err));
        setLast(name + ' failed');
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
  var lastLaunchCwd = '';
  var paneSeq = 0;
  var activeTab = null;
  /* Tabs belong to the project that opened them, like Orca workspaces. The
     strip only renders the active scope. */
  var activeScope = '';

  function normScope(path) {
    return normPath(path || appRoot || '');
  }

  function tabInScope(tab) {
    return !tab || !tab.scope || tab.scope === activeScope;
  }

  function scopedTabs() {
    var out = [];
    for (var i = 0; i < tabs.length; i++) {
      if (tabInScope(tabs[i])) out.push(tabs[i]);
    }
    return out;
  }

  var PANEL_TITLE = { browser: 'Browser', search: 'Search', docs: 'Docs', settings: 'Settings' };

  function byPaneKey(key) {
    for (var i = 0; i < tabs.length; i++) {
      if (tabs[i].paneKey === key) return tabs[i];
    }
    return null;
  }

  function tabByKind(kind) {
    for (var i = 0; i < tabs.length; i++) {
      if (tabs[i].kind === kind && tabInScope(tabs[i])) return tabs[i];
    }
    return null;
  }

  function renderTabs() {
    renderTabStrip();
    var empty = $('pane-empty');
    if (empty) empty.hidden = scopedTabs().length > 0;
  }

  function renderTabStrip() {
    var box = $('tabstrip-tabs');
    if (!box) return;
    box.textContent = '';
    var list = scopedTabs();
    for (var i = 0; i < list.length; i++) {
      (function (tab) {
        var row = el('div', 'strip-tab' + (tab === activeTab ? ' active' : ''));
        var main = el('button', 'strip-main');
        main.type = 'button';
        if (tab === activeTab) main.setAttribute('aria-current', 'page');
        if (tab.kind === 'terminal') {
          var mark = el('span', null);
          tab.statusNode = mark;
          paintTabStatus(tab);
          main.appendChild(mark);
        } else {
          tab.statusNode = null;
        }
        var title = el('span', 'strip-title', tab.title);
        title.title = tab.cwd ? tab.title + ' · ' + tab.cwd : tab.title;
        var badge = profileBadge(tab.agent, 'strip-profile');
        if (badge) main.appendChild(badge);
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
      })(list[i]);
    }
  }

  function maxPanes() {
    return 4;
  }

  /* All panes in a split share one group wrapper; the wrapper is what the
     workbench shows or hides, so a group can grow past two panes. */
  function updateSplitControls(group) {
    if (!group) return;
    var full = group.panes.length >= maxPanes();
    for (var i = 0; i < group.panes.length; i++) {
      var pane = group.panes[i];
      if (!pane) continue;
      if (pane.splitCtl) pane.splitCtl.disabled = full;
      if (pane.headSplit) pane.headSplit.disabled = full;
    }
    if (group.el && group.el.dataset) group.el.dataset.panes = String(group.panes.length);
  }

  function unwrapGroup(group) {
    if (!group || !group.el || !group.el.parentNode) return;
    var wrap = group.el;
    var survivor = group.panes.length === 1 ? group.panes[0] : null;
    if (survivor && survivor.el) wrap.parentNode.insertBefore(survivor.el, wrap);
    wrap.parentNode.removeChild(wrap);
    if (survivor) {
      survivor.group = null;
      if (survivor.splitCtl) survivor.splitCtl.disabled = false;
      if (survivor.headSplit) survivor.headSplit.disabled = false;
    }
    group.panes = [];
  }

  function activate(tab) {
    if (!tab || !tab.el) return;
    var prev = activeTab;
    if (prev && prev !== tab && prev.kind === 'browser') {
      /* keep the page alive: hide the native view instead of destroying it */
      hideBrowser();
    }
    var scopeChanged = !!(tab.scope && tab.scope !== activeScope);
    if (scopeChanged) activeScope = tab.scope;
    activeTab = tab;
    var activeGroup = tab.kind === 'terminal' ? tab.group : null;
    for (var i = 0; i < tabs.length; i++) {
      var t = tabs[i];
      if (!t.el) continue;
      if (!tabInScope(t)) {
        t.el.hidden = true;
        if (t.kind === 'terminal' && t.group) t.group.el.hidden = true;
        continue;
      }
      if (t.kind === 'terminal') {
        if (t.group) {
          t.el.hidden = false;
          t.group.el.hidden = t.group !== activeGroup;
        } else {
          t.el.hidden = !(tab.kind === 'terminal' && !activeGroup && t === tab);
        }
      } else {
        t.el.hidden = t !== tab;
      }
    }
    renderTabs();
    markRail();
    if (scopeChanged && savedProjects.length) renderProjects(savedProjects);
    if (tab.kind === 'terminal') {
      var panes = tab.group ? tab.group.panes : [tab];
      for (var p = 0; p < panes.length; p++) scheduleFit(panes[p]);
      focusTerm(tab);
    } else if (tab.kind === 'browser') {
      if (browser.engine === 'native' && browser.url) {
        if (browser.open) syncBrowserBounds(true);
        else openBrowserNative();
      }
    } else if (tab.kind === 'docs') {
      ensureDocs();
    } else if (tab.kind === 'settings') {
      loadSettingsSection();
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
      /* A panel is one DOM node, so it can belong to one project at a time.
         Take it from a tab left open in another project instead of letting
         two tabs fight over the same element's hidden flag. */
      for (var i = tabs.length - 1; i >= 0; i--) {
        if (tabs[i].kind === kind && tabs[i].el === pane) {
          if (activeTab === tabs[i]) activeTab = null;
          tabs.splice(i, 1);
        }
      }
      tab = {
        id: 'tab' + (++tabSeq),
        kind: kind,
        el: pane,
        title: PANEL_TITLE[kind] || kind,
        paneKey: null,
        scope: normScope(activeScope)
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
      if (tab.settleTimer) {
        clearTimeout(tab.settleTimer);
        tab.settleTimer = 0;
      }
      tab.handoffSend = null;
      var group = tab.group;
      if (group) {
        group.panes = group.panes.filter(function (p) { return p !== tab; });
        if (tab.el && tab.el.parentNode) tab.el.parentNode.removeChild(tab.el);
        tab.group = null;
        if (!group.panes.length) {
          if (group.el && group.el.parentNode) group.el.parentNode.removeChild(group.el);
        } else if (group.panes.length === 1) {
          unwrapGroup(group);
        } else {
          updateSplitControls(group);
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
      renderWorkers();
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
      var list = scopedTabs();
      if (list.length) activate(list[list.length - 1]);
      else renderTabs();
    } else {
      renderTabs();
    }
    updateWorkingMarks();
  }

  /* Splits the active terminal into the same group. Orca keeps every pane on
     screen and treats them as one tab; four panes is the practical ceiling. */
  function splitActive() {
    var t = activeTab;
    if (!t || t.kind !== 'terminal') {
      toast('warn', 'Open a terminal before splitting.');
      return null;
    }
    if (t.group && t.group.panes.length >= maxPanes()) {
      toast('warn', 'Split limit reached: ' + maxPanes() + ' panes.');
      return null;
    }
    var count = t.group ? t.group.panes.length + 1 : 2;
    return createTerminal({ split: true, cwd: t.cwd, title: 'Terminal ' + count });
  }

  /* tab strip "+" menu */

  function closeTabMenu() {
    var menu = $('tab-new-menu');
    if (menu) menu.hidden = true;
    var btn = $('tab-new');
    if (btn) {
      btn.setAttribute('aria-expanded', 'false');
      btn.title = 'New Tab';
    }
  }

  function toggleTabMenu() {
    var menu = $('tab-new-menu');
    if (!menu) return;
    var open = menu.hidden;
    var btn = $('tab-new');
    if (open) {
      buildTabMenu();
      menu.hidden = false;
      /* Anchor the menu under the + button, not to the far right of the bar. */
      var bar = $('tabstrip');
      var width = menu.offsetWidth || 268;
      var left = btn && isFinite(btn.offsetLeft) ? btn.offsetLeft - 6 : 8;
      var max = ((bar && bar.clientWidth) || 0) - width - 8;
      if (!(max > 8)) max = 8;
      menu.style.left = Math.max(8, Math.min(left, max)) + 'px';
      menu.style.right = 'auto';
    }
    menu.hidden = !open;
    if (btn) {
      btn.setAttribute('aria-expanded', open ? 'true' : 'false');
      /* Drop the native tooltip so it cannot float over the open menu. */
      btn.title = open ? '' : 'New Tab';
    }
  }

  function menuRow(label, hint, glyph, run) {
    var row = el('button', 'menu-item');
    row.type = 'button';
    row.setAttribute('role', 'menuitem');
    if (glyph) row.appendChild(iconNode(glyph));
    row.appendChild(el('span', 'menu-label', label));
    if (hint) row.appendChild(el('span', 'menu-hint', hint));
    row.addEventListener('click', guard(run));
    return row;
  }

  function buildTabMenu() {
    var box = $('tab-new-menu');
    if (!box) return;
    box.textContent = '';
    box.appendChild(menuRow('Terminal', '', 'terminal', function () {
      closeTabMenu();
      createTerminal({ title: 'Terminal', cwd: launchCwd() });
    }));
    box.appendChild(menuRow('Split Terminal', 'Ctrl+Shift+D', 'terminal', function () {
      closeTabMenu();
      splitActive();
    }));
    box.appendChild(menuRow('Browser', '', 'browser', function () {
      closeTabMenu();
      openStatic('browser');
    }));
    box.appendChild(menuRow('Custom CLI\u2026', '', 'command', function () {
      closeTabMenu();
      openCmdModal();
    }));
    box.appendChild(menuRow('Hand Off to Agent\u2026', '', 'handoff', function () {
      closeTabMenu();
      openHandoff(null);
    }));
    /* Orca lists only the CLIs it can actually launch: an item that would fail
       on click is noise, and most agents are simply not installed. */
    var detected = agentsInfo.filter(function (a) {
      return a.available !== false && a.enabled !== false;
    });
    box.appendChild(el('div', 'menu-sep'));
    if (detected.length) {
      box.appendChild(el('div', 'menu-label-group', 'Detected Agents'));
      for (var i = 0; i < detected.length; i++) {
        (function (agent) {
          box.appendChild(menuRow(
            agentDisplay(agent),
            '',
            agent.name,
            function () {
              closeTabMenu();
              createTerminal({
                paneKey: 'agent:' + agent.name + ':' + Date.now(),
                agent: agent.name,
                cwd: launchCwd(),
                title: agentDisplay(agent)
              });
            }
          ));
        })(detected[i]);
      }
    } else {
      box.appendChild(el('div', 'menu-empty', 'No agent CLIs detected on PATH.'));
    }
    box.appendChild(el('div', 'menu-sep'));
    box.appendChild(menuRow('Agent Settings\u2026', '', 'gear', openAgentSettings));
  }

  /* ---------- terminal ---------- */

  function spawnArgs(opts, paneKey, label) {
    var args = { paneKey: paneKey };
    if (label) args.label = label;
    if (opts.cwd) args.cwd = opts.cwd;
    if (opts.shell) args.shell = opts.shell;
    if (opts.agent) args.agent = opts.agent;
    if (opts.sessionId) args.sessionId = opts.sessionId;
    if (opts.command) args.command = opts.command;
    if (opts.handoff) args.handoff = opts.handoff;
    if (opts.parent) args.parent = opts.parent;
    if (opts.orchestrator) args.orchestrator = true;
    if (opts.model) args.model = opts.model;
    if (opts.effort) args.effort = opts.effort;
    if (opts.mode) args.mode = opts.mode;
    if (opts.login) args.login = true;
    return args;
  }

  var SPLIT_ICON = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="2"/><path d="M12 4v16"/></svg>';
  var CLOSE_ICON = '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" aria-hidden="true"><path d="m5 5 8 8M13 5l-8 8"/></svg>';
  var HANDOFF_ICON = '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M2.5 9h8"/><path d="m7.5 5.5 3.5 3.5-3.5 3.5"/><path d="M13.5 3v12"/></svg>';
  var ROLE_ICON = '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="9" cy="5" r="2.4"/><path d="M4 15c.6-3 2.3-4.4 5-4.4s4.4 1.4 5 4.4"/></svg>';

  /* A small floating split control that stays out of the terminal text and can
     be dragged anywhere inside its pane. It is translucent while idle and
     becomes a solid pill on hover or focus. */
  function makePaneFloat(tab) {
    var pane = tab.el;
    var box = el('div', 'pane-float');
    var grip = el('span', 'pane-grip');
    grip.title = 'Drag to move \u00b7 double-click to reset';
    grip.setAttribute('aria-hidden', 'true');
    var btn = el('button', 'pane-float-split');
    btn.type = 'button';
    btn.title = 'Split terminal (Ctrl+Shift+D)';
    btn.setAttribute('aria-label', 'Split ' + tab.title);
    btn.innerHTML = SPLIT_ICON;
    btn.addEventListener('click', guard(function (ev) {
      if (ev && typeof ev.stopPropagation === 'function') ev.stopPropagation();
      activate(tab);
      return splitActive();
    }));
    box.appendChild(grip);
    box.appendChild(btn);

    grip.addEventListener('pointerdown', function (ev) {
      if (ev && ev.button) return;
      if (ev && ev.preventDefault) ev.preventDefault();
      var paneRect = pane.getBoundingClientRect();
      var boxRect = box.getBoundingClientRect();
      var dx = ev.clientX - boxRect.left;
      var dy = ev.clientY - boxRect.top;
      box.classList.add('dragging');
      var startLeft = boxRect.left - paneRect.left;
      var startTop = boxRect.top - paneRect.top;
      function place(left, top) {
        var maxLeft = Math.max(4, pane.clientWidth - box.offsetWidth - 4);
        var maxTop = Math.max(4, pane.clientHeight - box.offsetHeight - 4);
        box.style.left = Math.max(4, Math.min(maxLeft, left)) + 'px';
        box.style.top = Math.max(4, Math.min(maxTop, top)) + 'px';
        box.style.right = 'auto';
      }
      function move(e) {
        place(e.clientX - paneRect.left - dx, e.clientY - paneRect.top - dy);
      }
      function stop() {
        box.classList.remove('dragging');
        window.removeEventListener('pointermove', move);
        window.removeEventListener('pointerup', stop);
        window.removeEventListener('pointercancel', stop);
      }
      place(startLeft, startTop);
      window.addEventListener('pointermove', move);
      window.addEventListener('pointerup', stop);
      window.addEventListener('pointercancel', stop);
    });

    box.addEventListener('dblclick', function () {
      box.style.left = '';
      box.style.top = '';
      box.style.right = '';
    });

    pane.appendChild(box);
    return btn;
  }

  function paneHeadBtn(cls, label, svg, run) {
    var btn = el('button', 'pane-head-btn ' + cls);
    btn.type = 'button';
    btn.title = label;
    btn.setAttribute('aria-label', label);
    btn.innerHTML = svg;
    btn.addEventListener('click', guard(function (ev) {
      if (ev && typeof ev.stopPropagation === 'function') ev.stopPropagation();
      return run();
    }));
    return btn;
  }

  /* Split panes get a thin bar with split, handoff, and close. A lone
     terminal keeps only the floating split pill, so it stays clean. */
  function makePaneHead(tab) {
    var head = el('div', 'pane-head');
    var title = el('span', 'pane-head-title', tab.title);
    title.title = tab.title;
    head.appendChild(title);
    var actions = el('div', 'pane-head-actions');
    var role = paneHeadBtn('pane-head-role', 'Mark as orchestrator', ROLE_ICON, function () {
      tab.orchestrator = !tab.orchestrator;
      role.setAttribute('aria-pressed', tab.orchestrator ? 'true' : 'false');
      role.classList.toggle('is-on', !!tab.orchestrator);
      role.title = tab.orchestrator ? 'Orchestrator pane (click to unmark)' : 'Mark as orchestrator';
      publishPane(tab);
      renderWorkers();
    });
    role.setAttribute('aria-pressed', tab.orchestrator ? 'true' : 'false');
    role.classList.toggle('is-on', !!tab.orchestrator);
    var split = paneHeadBtn('pane-head-split', 'Split terminal', SPLIT_ICON, function () {
      activate(tab);
      return splitActive();
    });
    split.disabled = !!(tab.group && tab.group.panes.length >= maxPanes());
    var handoff = paneHeadBtn('pane-head-handoff', 'Hand off to agent', HANDOFF_ICON, function () {
      activate(tab);
      openHandoff(tab.agent && tab.sessionId
        ? { agent: tab.agent, sessionId: tab.sessionId, cwd: tab.cwd, title: tab.title }
        : null);
    });
    var close = paneHeadBtn('pane-head-close', 'Close terminal', CLOSE_ICON, function () {
      closeTab(tab);
    });
    actions.appendChild(role);
    actions.appendChild(split);
    actions.appendChild(handoff);
    actions.appendChild(close);
    head.appendChild(actions);
    tab.el.insertBefore(head, tab.host);
    tab.headSplit = split;
    tab.headRole = role;
  }

  /* New terminals inherit the folder the user is already working in: the
     active terminal first, then the project last opened from the sidebar. */
  function launchCwd() {
    if (activeTab && activeTab.kind === 'terminal' && activeTab.cwd) return activeTab.cwd;
    if (activeScope) return activeScope;
    if (lastLaunchCwd) return lastLaunchCwd;
    for (var i = tabs.length - 1; i >= 0; i--) {
      if (tabs[i].kind === 'terminal' && tabs[i].cwd) return tabs[i].cwd;
    }
    return null;
  }

  function createTerminal(opts) {
    opts = opts || {};
    closeTabMenu();
    var paneKey = opts.paneKey || ('tab:' + (++paneSeq));
    var existing = byPaneKey(paneKey);
    if (existing) {
      activate(existing);
      toast('warn', 'This pane is already open: ' + paneKey);
      return existing;
    }

    var pane = el('section', 'pane pane-term');
    pane.dataset.kind = 'terminal';
    var host = el('div', 'term-host');
    pane.appendChild(host);
    var panes = $('panes');
    if (!panes) {
      toast('error', 'Pane container is missing.');
      return null;
    }
    var anchor = opts.split ? activeTab : null;
    if (anchor && (anchor.kind !== 'terminal' || !anchor.el)) anchor = null;
    if (anchor && anchor.group && anchor.group.panes.length >= maxPanes()) {
      toast('warn', 'Split limit reached: ' + maxPanes() + ' panes.');
      return null;
    }
    var splitWrap = null;
    if (anchor) {
      var group = anchor.group;
      if (!group) {
        splitWrap = el('div', 'pane-split');
        splitWrap.dataset.panes = '2';
        group = { el: splitWrap, panes: [anchor] };
        anchor.group = group;
        if (panes.insertBefore) panes.insertBefore(splitWrap, anchor.el);
        else panes.appendChild(splitWrap);
        splitWrap.appendChild(anchor.el);
      }
      splitWrap = group.el;
      splitWrap.appendChild(pane);
      group.panes.push(null); /* replaced by tab below */
    } else {
      panes.appendChild(pane);
    }

    var title = opts.title ||
      (opts.agent ? opts.agent + ' ' + shortId(opts.sessionId)
        : (splitWrap ? 'Terminal ' + splitWrap.children.length : 'Terminal'));

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
      scope: normScope(opts.scope || opts.cwd || (anchor && anchor.cwd) || activeScope),
      lastOutput: 0,
      status: 'working',
      statusNode: null,
      tailText: '',
      settleTimer: 0,
      agent: opts.agent || null,
      sessionId: opts.sessionId || null,
      lastCols: 0,
      lastRows: 0,
      spawned: false,
      closed: false,
      exited: false,
      handoffBrief: opts.handoffBrief || null,
      handoffPrompt: opts.handoffPrompt || null,
      handoffPending: false,
      handoff: opts.handoff || null,
      parent: opts.parent || null,
      orchestrator: !!opts.orchestrator,
      exitedCode: null,
      group: anchor ? anchor.group : null,
      splitCtl: null
    };
    if (anchor) {
      var g = anchor.group;
      g.panes[g.panes.length - 1] = tab;
      updateSplitControls(g);
    }
    makePaneHead(tab);
    tabs.push(tab);
    if (tab.cwd) lastLaunchCwd = tab.cwd;
    activate(tab);
    tab.splitCtl = makePaneFloat(tab);
    if (tab.group) updateSplitControls(tab.group);

    if (typeof window.Terminal !== 'function') {
      host.appendChild(el('div', 'side-empty', 'xterm.js global Terminal is missing'));
      toast('error', 'Terminal engine failed to load.');
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
        scrollback: 2000,
        scrollSensitivity: 3,
        /* Instant wheel scrolling: the animated variant kept re-scheduling
           frames and felt laggy on software-rendered WebKitGTK. */
        smoothScrollDuration: 0,
        macOptionIsMeta: true,
        theme: termTheme()
      });
    } catch (err) {
      toast('error', 'Terminal failed to start: ' + message(err));
      return tab;
    }
    tab.term = term;

    if (window.FitAddon && typeof window.FitAddon.FitAddon === 'function') {
      try {
        tab.fit = new window.FitAddon.FitAddon();
        term.loadAddon(tab.fit);
      } catch (err) {
        tab.fit = null;
        toast('warn', 'Terminal fit is unavailable: ' + message(err));
      }
    } else {
      toast('warn', 'Terminal auto-fit is unavailable.');
    }

    try {
      term.open(host);
    } catch (err) {
      toast('error', 'Terminal failed to mount: ' + message(err));
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
      noteInput(tab);
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

    /* A control-spawned handoff already owns its pty; attach without a second
       spawn and let the host deliver the brief. */
    if (opts.attach) {
      tab.spawned = true;
      pushResize(tab);
      publishPane(tab);
      renderWorkers();
      focusTerm(tab);
      return tab;
    }

    call('ptySpawn', spawnArgs(opts, paneKey, title)).then(function (res) {
      if (!res || tab.closed) {
        if (tab.closed) call('ptyKill', { paneKey: paneKey }, { quiet: true });
        return;
      }
      tab.spawned = true;
      pushResize(tab);
      updateWorkingMarks();
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
      if (tab.handoffPrompt) queueHandoffPrompt(tab);
      if (res.pid != null) setLast('Terminal started (PID ' + res.pid + ')');
      publishPane(tab);
      renderWorkers();
    });

    return tab;
  }

  /* The new agent TUI must be in raw mode before the brief can be typed in.
     Its first output is the signal; a short fallback covers silent starts.
     Bracketed paste keeps the transcript lines from submitting one by one. */
  function queueHandoffPrompt(tab) {
    if (!tab || !tab.handoffPrompt) return;
    var done = false;
    var fallback = setTimeout(send, 3500);

    function send() {
      if (done || tab.closed || tab.exited || !tab.handoffPrompt) return;
      done = true;
      clearTimeout(fallback);
      tab.handoffSend = null;
      var text = tab.handoffPrompt;
      tab.handoffPrompt = null;
      var paste = '\x1b[200~' + text + '\x1b[201~';
      call('ptyWrite', { paneKey: tab.paneKey, data: paste }, { quiet: true })
        .then(function () {
          noteInput(tab);
          toast('ok', 'Handoff prompt sent to ' + tab.title);
          setLast('Handoff prompt sent \u00b7 ' + tab.title);
          /* The TUI needs a moment to ingest the bracketed paste before it
             treats Enter as "submit" instead of part of the paste. */
          setTimeout(function () {
            if (tab.closed || tab.exited) return;
            call('ptyWrite', { paneKey: tab.paneKey, data: '\r' }, { quiet: true });
          }, 1200);
        });
    }

    tab.handoffSend = send;
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
      toast('error', 'Terminal data is not valid base64.');
      return null;
    }
  }

  /* Approval prompts are plain ASCII, so a streaming decoder is enough. The
     browser test stub has no TextDecoder and gets an empty string. */
  var ptyDecoder = null;

  function bytesText(bytes) {
    if (!bytes || typeof window.TextDecoder !== 'function') return '';
    if (!ptyDecoder) ptyDecoder = new window.TextDecoder();
    try {
      return ptyDecoder.decode(bytes, { stream: true });
    } catch (err) {
      return '';
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
      toast('error', 'Only http and https URLs are allowed.');
      return null;
    }
    return value;
  }

  function openBrowserNative() {
    if (!browser.url) {
      toast('warn', 'Open a URL first.');
      return Promise.resolve(null);
    }
    var rect = browserRect();
    if (rect.width < 8 || rect.height < 8) {
      toast('warn', 'Browser area is not ready yet.');
      return Promise.resolve(null);
    }
    return call('browserOpen', { url: browser.url, rect: rect }).then(function (res) {
      if (!res) return null;
      browser.open = true;
      browser.visible = true;
      browser.lastRect = rect;
      setLast('Browser: ' + (res.url || browser.url));
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
      toast('warn', 'Enter an http or https URL.');
      return Promise.resolve(null);
    }
    $('browser-url').value = url;
    browser.url = url;

    if (browser.engine === 'lightpanda') {
      var pre = $('browser-text');
      if (!pre) return Promise.resolve(null);
      pre.hidden = false;
      pre.textContent = 'Fetching ' + url + '…';
      setLast('Fetching ' + url);
      return call('browserFetch', { url: url, dump: 'markdown' }).then(function (res) {
        if (!res) {
          pre.textContent = 'Fetch failed: ' + url;
          return null;
        }
        var text = typeof res.text === 'string' ? res.text : JSON.stringify(res, null, 2);
        pre.textContent = text || '(empty document)';
        setLast('Fetched ' + url);
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
    setLast('Engine: ' + engine);
    if (engine === 'native' && activeTab && activeTab.kind === 'browser' && browser.url) {
      openBrowserNative();
    }
  }

  /* ---------- first-run setup and support ---------- */

  var setupInfo = null;
  var setupStep = 0;
  var setupAgents = [];
  var setupSaving = false;
  var setupSeen = false;

  function setupRenderAgents() {
    var box = $('setup-agents');
    if (!box) return;
    box.textContent = '';
    setupAgents = [];
    var ready = 0;
    for (var i = 0; i < agentsInfo.length; i++) {
      var agent = agentsInfo[i];
      if (agent.profile || agent.plugin) continue;
      var row = el('label', agent.available === false
        ? 'setup-agent is-disabled'
        : 'setup-agent');
      var input = el('input');
      input.type = 'checkbox';
      input.id = 'setup-agent-' + agent.name;
      input.checked = agent.enabled !== false;
      input.disabled = agent.available === false;
      var text = el('span', 'setup-agent-text');
      text.appendChild(el('span', 'setup-agent-name', agentLabel(agent.name)));
      text.appendChild(el('span', 'setup-agent-bin mono',
        agent.available === false ? 'not installed' : (agent.bin || agent.name)));
      row.appendChild(input);
      row.appendChild(text);
      box.appendChild(row);
      setupAgents.push({
        name: agent.name,
        enabled: agent.enabled !== false,
        box: input
      });
      if (agent.available !== false) ready += 1;
    }
    var note = $('setup-agents-note');
    if (note) {
      note.textContent = ready
        ? ready + (ready === 1 ? ' agent is' : ' agents are') +
          ' ready on this machine. You can change this later in Settings.'
        : 'No agent CLI was found yet. Omnix still works as a terminal and browser; install a CLI later.';
    }
  }

  function setupRender() {
    for (var i = 0; i < 4; i++) {
      var step = $('setup-step-' + i);
      if (step) step.hidden = i !== setupStep;
      var dot = $('setup-dot-' + i);
      if (dot) {
        dot.classList.toggle('is-active', i === setupStep);
        dot.classList.toggle('is-done', i < setupStep);
      }
    }
    var back = $('setup-back');
    if (back) back.hidden = setupStep === 0;
    var skip = $('setup-skip');
    if (skip) skip.hidden = setupStep === 3;
    var next = $('setup-next');
    if (next) next.textContent = setupStep === 3 ? 'Start Omnix' : 'Continue';
    var note = $('setup-step-note');
    if (note) note.textContent = 'Step ' + (setupStep + 1) + ' of 4';
  }

  function setupOpen(state, force) {
    if (!force && setupSeen) return;
    setupSeen = true;
    setupInfo = state || setupInfo;
    setupStep = 0;
    setupSaving = false;
    var project = $('setup-project');
    if (project) project.value = '';
    var error = $('setup-project-error');
    if (error) error.textContent = '';
    setupRenderAgents();
    setupRender();
    var modal = $('setup-modal');
    if (modal) modal.hidden = false;
    setLast('First-run setup');
    var next = $('setup-next');
    if (next && next.focus) next.focus();
  }

  function setupBack() {
    if (setupSaving || setupStep === 0) return;
    setupStep -= 1;
    setupRender();
  }

  function setupNext() {
    if (setupSaving) return;
    if (setupStep < 3) {
      setupStep += 1;
      setupRender();
      if (setupStep === 1) {
        var project = $('setup-project');
        if (project && project.focus) project.focus();
      }
      return;
    }
    setupFinish();
  }

  function setupBrowse() {
    var input = $('setup-project');
    var start = input && input.value ? input.value.trim() : appRoot;
    return call('pickFolder', { path: start || '' }).then(function (res) {
      if (res && res.path && input) {
        input.value = res.path;
        if (input.focus) input.focus();
      }
      return res;
    }, function (failure) {
      toast('error', 'Folder picker: ' + message(failure));
    });
  }

  function setupProjectPending(path) {
    if (!path) return Promise.resolve(null);
    for (var i = 0; i < savedProjects.length; i++) {
      if (normPath(savedProjects[i].path) === normPath(path)) {
        return Promise.resolve(savedProjects[i]);
      }
    }
    return call('projectAdd', { path: path }).then(function (res) {
      savedProjects = res && Array.isArray(res.projects) ? res.projects : savedProjects;
      for (var j = 0; j < savedProjects.length; j++) {
        if (normPath(savedProjects[j].path) === normPath(path)) return savedProjects[j];
      }
      return { path: path, name: baseName(path) };
    });
  }

  function setupAgentsSave() {
    var jobs = [];
    for (var i = 0; i < setupAgents.length; i++) {
      var entry = setupAgents[i];
      if (!entry.box || entry.box.disabled) continue;
      if (!!entry.box.checked === !!entry.enabled) continue;
      jobs.push(call('agentEnable', {
        agent: entry.name,
        enabled: !!entry.box.checked
      }).then(function () {}, function () {}));
    }
    return Promise.all(jobs);
  }

  function setupFinish() {
    if (setupSaving) return;
    setupSaving = true;
    var project = $('setup-project');
    var error = $('setup-project-error');
    var path = project ? project.value.trim() : '';
    if (error) error.textContent = '';
    setupProjectPending(path).then(function (picked) {
      setupAgentsSave().then(function () {
        return call('setupComplete', {});
      }).then(function (res) {
        setupSaving = false;
        setupInfo = (res && res.setup) || setupInfo;
        hideModal($('setup-modal'));
        setLast('Setup complete');
        toast('ok', 'Omnix is ready');
        return refreshProjects().then(function () {
          if (picked) openProject(picked);
        });
      }).catch(function (failure) {
        setupSaving = false;
        toast('error', 'Setup: ' + message(failure));
      });
    }, function (failure) {
      setupSaving = false;
      if (error) error.textContent = message(failure);
      setupStep = 1;
      setupRender();
      toast('error', 'Setup: ' + message(failure));
    });
  }

  function setupSkip() {
    if (setupSaving) return;
    setupSaving = true;
    call('setupComplete', {}).then(function (res) {
      setupSaving = false;
      setupInfo = (res && res.setup) || setupInfo;
      hideModal($('setup-modal'));
      setLast('Setup skipped');
    }, function (failure) {
      setupSaving = false;
      toast('error', 'Setup: ' + message(failure));
    });
  }

  function openSupport() {
    return call('supportOpen', {}).then(function (res) {
      setLast('Support page opened');
      return res;
    }, function (failure) {
      toast('error', 'Support: ' + message(failure));
    });
  }

  function openRepo() {
    var tab = openStatic('browser');
    var input = $('browser-url');
    if (input) input.value = REPO_URL;
    var engine = $('browser-engine');
    if (engine) engine.value = 'native';
    browser.url = REPO_URL;
    browser.engine = 'native';
    setLast('GitHub: Zwart04/omnix');
    return tab && openBrowserNative();
  }

  function openSetupAgain() {
    return call('setupReset', {}).then(function (res) {
      setupInfo = res && res.setup ? res.setup : setupInfo;
      setupSeen = false;
      setupOpen(setupInfo, true);
      return res;
    }, function (failure) {
      toast('error', 'Setup: ' + message(failure));
    });
  }

  /* ---------- agent settings ---------- */

  var agentSettingsData = null;
  var settingsSection = 'agents';
  var pluginsData = null;
  var pluginCatalogData = null;
  var devData = null;

  function openAgentSettings() {
    return openSettings('agents');
  }

  function openPluginsSettings() {
    return openSettings('plugins');
  }

  function openDevSettings() {
    return openSettings('dev');
  }

  function openSettings(section) {
    closeTabMenu();
    var tab = openStatic('settings');
    setSettingsSection(section || 'agents');
    return tab;
  }

  function setSettingsSection(section) {
    var meta = {
      agents: {
        title: 'Agents',
        crumb: 'Agents',
        hint: 'Choose which CLIs Omnix may launch and how they start.',
        refresh: 'Refresh',
        search: 'Search settings'
      },
      plugins: {
        title: 'Plugins',
        crumb: 'Plugins',
        hint: 'Install only what you need. A plugin process starts when you call it, then exits.',
        refresh: 'Refresh',
        search: 'Search installed plugins'
      },
      dev: {
        title: 'Kembangkan Omnix',
        crumb: 'Kembangkan Omnix',
        hint: 'Let an agent work in its own worktree. You decide when a build is installed.',
        refresh: 'Refresh',
        search: 'Search builds and tasks'
      },
      about: {
        title: 'About & Support',
        crumb: 'About & Support',
        hint: 'Version, license, and a way to keep Omnix moving.',
        refresh: 'Refresh',
        search: 'Search settings'
      }
    };
    if (!meta[section]) section = 'agents';
    settingsSection = section;
    var navs = document.querySelectorAll('.settings-nav-item[data-settings]');
    for (var i = 0; i < navs.length; i++) {
      var on = navs[i].dataset && navs[i].dataset.settings === section;
      if (on) navs[i].classList.add('is-active');
      else navs[i].classList.remove('is-active');
      navs[i].setAttribute('aria-current', on ? 'page' : 'false');
    }
    var paneIds = {
      agents: 'settings-agents',
      plugins: 'settings-plugins',
      dev: 'settings-dev',
      about: 'settings-about'
    };
    for (var key in paneIds) {
      if (!Object.prototype.hasOwnProperty.call(paneIds, key)) continue;
      var pane = $(paneIds[key]);
      if (pane) pane.hidden = key !== section;
    }
    var crumb = $('settings-crumb');
    if (crumb) crumb.textContent = 'Settings / ' + meta[section].crumb;
    var title = $('settings-title');
    if (title) title.textContent = meta[section].title;
    var hint = $('agent-settings-meta');
    if (hint) hint.textContent = meta[section].hint;
    var refresh = $('agent-settings-refresh');
    if (refresh) {
      refresh.textContent = meta[section].refresh;
      refresh.title = 'Reload ' + meta[section].title;
    }
    var search = $('settings-search');
    if (search) {
      search.value = '';
      search.placeholder = meta[section].search;
    }
    loadSettingsSection();
  }

  function loadSettingsSection() {
    if (settingsSection === 'plugins') return loadPlugins();
    if (settingsSection === 'dev') return loadDev();
    if (settingsSection === 'about') return renderAboutSettings();
    return loadAgentSettings();
  }

  function settingsSearchChanged() {
    if (settingsSection === 'plugins') renderPlugins(pluginsData);
    else if (settingsSection === 'dev') renderDev(devData);
    else if (settingsSection === 'about') return;
    else renderAgentSettings(agentSettingsData);
  }

  function renderAboutSettings() {
    var version = $('app-version');
    var node = $('about-version');
    if (node) {
      node.textContent = version && version.textContent ? version.textContent : '?';
    }
    var state = $('about-setup-state');
    if (state) {
      state.textContent = setupInfo && setupInfo.completedAt
        ? 'Last completed ' + new Date(setupInfo.completedAt * 1000).toLocaleDateString()
        : 'Not completed on this workspace yet.';
    }
    return Promise.resolve(null);
  }

  function loadAgentSettings() {
    return call('agentSettings', {}).then(function (res) {
      if (!res) return null;
      agentSettingsData = res;
      var perms = $('agent-default-perms');
      if (perms) {
        perms.value = res.defaults && res.defaults.fullAccess ? 'full' : 'ask';
      }
      var path = $('agent-settings-path');
      if (path) {
        path.textContent = res.path || '';
        path.title = res.path || '';
      }
      renderAgentSettings(res);
      return res;
    }).catch(function (err) {
      toast('error', 'Agent settings: ' + message(err));
    });
  }

  function agentCmdLine(agent, defaults) {
    var parts = [agent.bin || agent.name];
    if (agent.args) parts.push(String(agent.args).trim());
    if (defaults && defaults.fullAccess && agent.fullAccessFlag) parts.push(agent.fullAccessFlag);
    return '$ ' + parts.join(' ');
  }

  function agentMatches(agent, query) {
    if (!query) return true;
    var hay = (agentLabel(agent.name) + ' ' + agent.name + ' ' + (agent.bin || '')).toLowerCase();
    return hay.indexOf(query.toLowerCase()) >= 0;
  }

  function settingsGroup(title, hint) {
    var head = el('div', 'set-group');
    head.appendChild(el('span', 'set-group-title', title));
    if (hint) head.appendChild(el('span', 'set-group-hint', hint));
    return head;
  }

  function renderAgentSettings(res) {
    var box = $('agent-settings');
    if (!box) return;
    box.textContent = '';
    var data = res || agentSettingsData || {};
    var list = Array.isArray(data.agents) ? data.agents : [];
    var defaults = data.defaults || {};
    var search = $('settings-search');
    var query = search ? String(search.value || '').trim() : '';

    var installed = list.filter(function (a) {
      return a && a.available !== false && agentMatches(a, query);
    });
    var missing = list.filter(function (a) {
      return a && a.available === false && agentMatches(a, query);
    });
    var installedAll = list.filter(function (a) { return a && a.available !== false; });
    var missingAll = list.filter(function (a) { return a && a.available === false; });

    var meta = $('agent-settings-meta');
    if (meta) {
      meta.textContent = installedAll.length + ' installed \u00b7 ' +
        missingAll.length + ' available to install' +
        (query ? ' \u00b7 filtered by "' + query + '"' : '') +
        '. Changes save automatically.';
    }

    box.appendChild(settingsGroup('Installed',
      installedAll.length + ' CLI' + (installedAll.length === 1 ? '' : 's') + ' on this machine'));
    if (!installed.length) {
      box.appendChild(el('p', 'side-empty',
        query ? 'No installed agent matches "' + query + '".'
          : 'No agent CLIs detected. Install one, then press Refresh.'));
    }
    for (var i = 0; i < installed.length; i++) {
      box.appendChild(agentSettingsRow(installed[i], defaults, true));
      var profiles = Array.isArray(data.profiles) ? data.profiles : [];
      for (var k = 0; k < profiles.length; k++) {
        if (profiles[k].base === installed[i].name && agentMatches(profiles[k], query)) {
          box.appendChild(agentProfileRow(profiles[k], defaults, installed[i]));
        }
      }
    }

    box.appendChild(settingsGroup('Available to install',
      'Install the CLI, then press Refresh'));
    if (!missing.length) {
      box.appendChild(el('p', 'side-empty',
        missingAll.length ? 'No available agent matches "' + query + '".'
          : 'Every known agent CLI is already installed.'));
    }
    for (var j = 0; j < missing.length; j++) {
      box.appendChild(agentSettingsRow(missing[j], defaults, false));
    }
  }

  function agentSettingsRow(agent, defaults, installed) {
    var row = el('div', 'agent-row' +
      (agent.enabled === false ? ' is-disabled' : '') +
      (installed ? '' : ' is-missing'));
    row.dataset.name = agent.name;

    var main = el('div', 'agent-main');
    var title = el('div', 'agent-title');
    title.appendChild(iconNode(agent.name, 'agent-mark'));
    title.appendChild(el('span', 'agent-name', agentLabel(agent.name)));
    title.appendChild(el('span', 'tag agent-kind', 'CLI'));
    if (!installed) title.appendChild(el('span', 'agent-missing-tag', 'Not Detected'));
    main.appendChild(title);
    var cmd = el('code', 'agent-cmd mono', agentCmdLine(agent, defaults));
    main.appendChild(cmd);
    row.appendChild(main);

    if (!installed) {
      row.appendChild(el('span', 'agent-install-hint', 'Install to enable'));
      return row;
    }

    var more = el('button', 'icon-btn agent-more');
    more.type = 'button';
    more.title = 'Launch options';
    more.setAttribute('aria-label', 'Launch options for ' + agentLabel(agent.name));
    more.setAttribute('aria-expanded', 'false');
    more.innerHTML = '<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">' +
      '<circle cx="12" cy="5" r="1.7"/><circle cx="12" cy="12" r="1.7"/>' +
      '<circle cx="12" cy="19" r="1.7"/></svg>';
    row.appendChild(more);

    var sw = el('label', 'switch');
    sw.title = agent.enabled === false ? 'Enable ' + agentLabel(agent.name)
      : 'Disable ' + agentLabel(agent.name);
    var cb = el('input');
    cb.type = 'checkbox';
    cb.checked = agent.enabled !== false;
    cb.setAttribute('aria-label', sw.title);
    sw.appendChild(cb);
    sw.appendChild(el('span', 'switch-track'));
    cb.addEventListener('change', guard(function () {
      return setAgentEnabled(agent, !!cb.checked, row, cb);
    }));
    row.appendChild(sw);

    var details = el('div', 'agent-details');
    details.hidden = true;
    more.addEventListener('click', guard(function () {
      details.hidden = !details.hidden;
      more.setAttribute('aria-expanded', details.hidden ? 'false' : 'true');
    }));

    var state = el('span', 'set-state is-saved', agent.enabled === false ? 'Disabled' : 'Enabled');
    var argsInput = settingsField(details, 'Arguments', agent.args,
      '--dangerously-skip-permissions',
      function () { saveLaunch(agent, argsInput, envInput, state, cmd); },
      'Whitespace separates arguments. Stored per workspace.');
    var envInput = settingsField(details, 'Environment', agent.env,
      'KEY=value KEY=value',
      function () { saveLaunch(agent, argsInput, envInput, state, cmd); },
      'Merged into the agent process environment.');
    var foot = el('div', 'agent-detail-foot');
    foot.appendChild(el('span', 'set-path mono', 'Command overrides apply on the next launch.'));
    foot.appendChild(state);
    details.appendChild(foot);
    details.appendChild(agentProfileAdder(agent.name));
    row.appendChild(details);
    return row;
  }

  function agentProfileAdder(base) {
    var wrap = el('div', 'agent-profile-add');
    wrap.appendChild(el('span', 'set-field-label', 'Add account profile'));
    var input = el('input', 'input');
    input.type = 'text';
    input.placeholder = 'profile id, e.g. akun2';
    input.spellcheck = false;
    input.autocomplete = 'off';
    input.setAttribute('aria-label', 'New profile id for ' + agentLabel(base));
    var add = button('Add profile', 'btn sm', function () {
      var id = String(input.value || '').trim().toLowerCase();
      if (!/^[a-z0-9][a-z0-9-]{0,47}$/.test(id)) {
        toast('warn', 'Profile id: 1-48 chars, lowercase letters, digits, hyphen.');
        return;
      }
      add.disabled = true;
      rpc('agentSettingsSave', {
        agent: base + ':' + id,
        label: id,
        args: '',
        env: '',
        configDir: '',
        shareProjects: false
      }).then(function (res) {
        add.disabled = false;
        if (!res) return;
        input.value = '';
        toast('ok', 'Profile added: ' + base + ':' + id);
        return loadAgentSettings();
      }, function (err) {
        add.disabled = false;
        toast('error', 'Add profile: ' + message(err));
      });
    });
    add.type = 'button';
    wrap.appendChild(input);
    wrap.appendChild(add);
    wrap.appendChild(el('span', 'set-field-help',
      'Each profile gets its own login folder; Omnix never reads the token.'));
    return wrap;
  }

  function agentProfileRow(profile, defaults, base) {
    var row = el('div', 'agent-row agent-profile-row' +
      (profile.enabled === false ? ' is-disabled' : '') +
      (profile.available === false ? ' is-missing' : ''));
    row.dataset.name = profile.name;

    var main = el('div', 'agent-main');
    var title = el('div', 'agent-title');
    var badge = profileBadge(profile.name, 'agent-profile-badge');
    if (badge) title.appendChild(badge);
    var nameNode = el('span', 'agent-name', profile.label || profile.profile);
    title.appendChild(nameNode);
    title.appendChild(el('span', 'tag agent-kind', 'PROFILE'));
    if (profile.available === false) {
      title.appendChild(el('span', 'agent-missing-tag', 'Not Detected'));
    }
    main.appendChild(title);
    var cmd = el('code', 'agent-cmd mono', profileCommand(profile, base, defaults));
    main.appendChild(cmd);

    var login = button('Login profile', 'btn sm agent-login', function () {
      openProfileLogin(profile.name, profile.supportsConfig === false);
    });
    login.type = 'button';
    login.title = 'Open a terminal signed in as ' + profile.name;
    main.appendChild(login);
    row.appendChild(main);

    var more = el('button', 'icon-btn agent-more');
    more.type = 'button';
    more.title = 'Profile options';
    more.setAttribute('aria-label', 'Profile options for ' + profile.name);
    more.setAttribute('aria-expanded', 'false');
    more.innerHTML = '<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">' +
      '<circle cx="12" cy="5" r="1.7"/><circle cx="12" cy="12" r="1.7"/>' +
      '<circle cx="12" cy="19" r="1.7"/></svg>';
    row.appendChild(more);

    var sw = el('label', 'switch');
    sw.title = profile.enabled === false ? 'Enable ' + profile.name : 'Disable ' + profile.name;
    var cb = el('input');
    cb.type = 'checkbox';
    cb.checked = profile.enabled !== false;
    cb.setAttribute('aria-label', sw.title);
    sw.appendChild(cb);
    sw.appendChild(el('span', 'switch-track'));
    cb.addEventListener('change', guard(function () {
      return setAgentEnabled(profile, !!cb.checked, row, cb);
    }));
    row.appendChild(sw);

    var details = el('div', 'agent-details');
    details.hidden = true;
    more.addEventListener('click', guard(function () {
      details.hidden = !details.hidden;
      more.setAttribute('aria-expanded', details.hidden ? 'false' : 'true');
    }));

    var state = el('span', 'set-state is-saved', 'Saved');
    var argsInput;
    var envInput;
    var labelInput;
    var dirInput;
    var shareCb;
    function save() {
      state.textContent = 'Saving';
      state.className = 'set-state is-saving';
      rpc('agentSettingsSave', {
        agent: profile.name,
        label: labelInput ? labelInput.value : profile.label,
        configDir: dirInput ? dirInput.value : profile.configDir,
        shareProjects: shareCb ? !!shareCb.checked : !!profile.shareProjects,
        args: argsInput ? argsInput.value : profile.args,
        env: envInput ? envInput.value : profile.env
      }).then(function (res) {
        if (!res) return null;
        profile.label = labelInput ? labelInput.value.trim() : profile.label;
        profile.configDir = dirInput ? dirInput.value.trim() : profile.configDir;
        profile.shareProjects = shareCb ? !!shareCb.checked : profile.shareProjects;
        profile.args = argsInput ? argsInput.value : profile.args;
        profile.env = envInput ? envInput.value : profile.env;
        nameNode.textContent = profile.label || profile.profile;
        cmd.textContent = profileCommand(profile, base, defaults);
        state.textContent = 'Saved';
        state.className = 'set-state is-saved';
        return res;
      }, function (err) {
        state.textContent = 'Not saved';
        state.className = 'set-state is-error';
        toast('error', 'Save failed: ' + message(err));
        return null;
      });
    }

    labelInput = settingsField(details, 'Display name', profile.label || '',
      profile.profile, save, 'Shown in the tab and the Agents panel.');
    if (profile.supportsConfig !== false) {
      dirInput = settingsField(details, 'Login folder', profile.configDir || '',
        profile.configDir || '', save,
        'Empty uses ~/.omnix/profiles/' + profile.base + '/' + profile.profile + '.');
    } else {
      details.appendChild(el('p', 'set-field-help',
        'Antigravity has no official config-folder flag; this profile shares the main login.'));
    }
    argsInput = settingsField(details, 'Arguments', profile.args || '',
      '--dangerously-skip-permissions', save,
      'Profile arguments run after the base arguments.');
    envInput = settingsField(details, 'Environment', profile.env || '',
      'KEY=value KEY=value', save,
      'Merged into the agent process environment.');
    if (profile.base === 'claude') {
      var share = el('label', 'set-field agent-share');
      share.appendChild(el('span', 'set-field-label', 'Share project memory'));
      var shareRow = el('span', 'agent-share-row');
      shareCb = el('input');
      shareCb.type = 'checkbox';
      shareCb.checked = !!profile.shareProjects;
      shareCb.addEventListener('change', guard(save));
      shareRow.appendChild(shareCb);
      shareRow.appendChild(el('span', 'hint', 'Symlink the base account\u2019s projects/ into this profile.'));
      share.appendChild(shareRow);
      details.appendChild(share);
    }
    var foot = el('div', 'agent-detail-foot');
    var remove = button('Delete profile', 'btn sm', function () {
      rpc('agentProfileDelete', { agent: profile.name }).then(function (res) {
        if (!res) return null;
        toast('ok', 'Profile removed: ' + profile.name);
        return loadAgentSettings();
      }, function (err) {
        toast('error', 'Delete failed: ' + message(err));
      });
    });
    remove.type = 'button';
    remove.title = 'Remove this profile from Settings; its login folder is left on disk';
    foot.appendChild(remove);
    foot.appendChild(el('span', 'set-path mono', 'Login folder is never deleted.'));
    foot.appendChild(state);
    details.appendChild(foot);
    row.appendChild(details);
    return row;
  }

  function profileCommand(profile, base, defaults) {
    var parts = [(base && base.bin) || profile.base];
    if (profile.args) parts.push(String(profile.args).trim());
    if (defaults && defaults.fullAccess && base && base.fullAccessFlag) {
      parts.push(base.fullAccessFlag);
    }
    return '$ ' + parts.join(' ');
  }

  function openProfileLogin(name, shared) {
    closeTabMenu();
    createTerminal({
      paneKey: 'login:' + name,
      agent: name,
      cwd: launchCwd(),
      title: 'Login ' + agentDisplay(agentsInfo.filter(function (a) {
        return a.name === name;
      })[0] || name),
      login: true
    });
    if (shared) {
      toast('warn', 'This profile shares the main Antigravity login (no config-folder flag).');
    }
    setLast('Login: ' + name);
  }

  function saveLaunch(agent, argsInput, envInput, state, cmd) {
    state.textContent = 'Saving';
    state.className = 'set-state is-saving';
    rpc('agentSettingsSave', {
      agent: agent.name,
      args: argsInput ? argsInput.value : '',
      env: envInput ? envInput.value : ''
    }).then(function (res) {
      if (!res) return;
      agent.args = argsInput ? argsInput.value : '';
      agent.env = envInput ? envInput.value : '';
      if (cmd) cmd.textContent = agentCmdLine(agent,
        (agentSettingsData && agentSettingsData.defaults) || {});
      state.textContent = 'Saved';
      state.className = 'set-state is-saved';
    }, function (err) {
      state.textContent = 'Not saved';
      state.className = 'set-state is-error';
      toast('error', 'Save failed: ' + message(err));
    });
  }

  function setAgentEnabled(agent, enabled, row, cb) {
    return rpc('agentEnable', { agent: agent.name, enabled: enabled }).then(function (res) {
      if (!res) {
        if (cb) cb.checked = !enabled;
        return null;
      }
      agent.enabled = enabled;
      if (row && row.classList) {
        if (enabled) row.classList.remove('is-disabled');
        else row.classList.add('is-disabled');
      }
      for (var i = 0; i < agentsInfo.length; i++) {
        if (agentsInfo[i].name === agent.name) agentsInfo[i].enabled = enabled;
      }
      fillAgentPicker('opencode2');
      toast('ok', agentLabel(agent.name) + (enabled ? ' enabled' : ' disabled'));
      renderAgentSettings(agentSettingsData);
      return res;
    }, function (err) {
      if (cb) cb.checked = !enabled;
      toast('error', 'Enable failed: ' + message(err));
      return null;
    });
  }

  function settingsField(card, label, value, placeholder, save, help) {
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
    if (help) wrap.appendChild(el('span', 'set-field-help', help));
    card.appendChild(wrap);
    return input;
  }

  /* ---------- plugins ---------- */

  function settingsSwitch(checked, title, onChange) {
    var wrap = el('label', 'switch');
    wrap.title = title;
    var cb = el('input');
    cb.type = 'checkbox';
    cb.checked = !!checked;
    cb.setAttribute('aria-label', title);
    cb.addEventListener('change', guard(function () {
      return onChange(!!cb.checked, cb);
    }));
    wrap.appendChild(cb);
    wrap.appendChild(el('span', 'switch-track'));
    return wrap;
  }

  function pluginRequested(plugin) {
    return Array.isArray(plugin.requested) ? plugin.requested : [];
  }

  function pluginGranted(plugin) {
    return Array.isArray(plugin.granted) ? plugin.granted : [];
  }

  function pluginMatches(plugin, query) {
    if (!query) return true;
    var hay = [
      plugin.name,
      plugin.description,
      plugin.author,
      (plugin.kinds || []).join(' ')
    ].join(' ').toLowerCase();
    return hay.indexOf(query) >= 0;
  }

  function loadPlugins() {
    return call('plugins', {}).then(function (res) {
      if (!res) return null;
      pluginsData = res;
      renderPlugins(res);
      return res;
    });
  }

  function renderPlugins(res) {
    var box = $('plugin-list');
    if (!box) return;
    box.textContent = '';
    var data = res || pluginsData || {};
    var list = Array.isArray(data.plugins) ? data.plugins : [];
    var search = $('settings-search');
    var query = search ? String(search.value || '').trim().toLowerCase() : '';
    var shown = list.filter(function (plugin) { return pluginMatches(plugin, query); });

    if (data.catalogNote) {
      var note = $('plugin-catalog-note');
      if (note) note.textContent = data.catalogNote;
    }
    var note = $('plugin-catalog-note');
    if (note) {
      note.textContent = 'Katalog: ' + (data.catalog || 'public GitHub index') +
        ' \u00b7 data: ' + (data.path || '-') +
        ' \u00b7 plugin hanya berjalan saat dipanggil.';
    }
    var hint = $('agent-settings-meta');
    if (hint) {
      hint.textContent = list.length + ' plugin terpasang' +
        (query ? ' \u00b7 filter "' + query + '"' : '') +
        '. Proses plugin tidak hidup di latar belakang.';
    }

    box.appendChild(settingsGroup('Terpasang',
      list.length ? list.length + ' folder plugin' : 'belum ada plugin'));
    if (!shown.length) {
      box.appendChild(el('p', 'side-empty',
        list.length ? 'Tidak ada plugin yang cocok dengan "' + query + '".'
          : 'Belum ada plugin. Pasang dari folder, arsip .tar.gz, atau katalog.'));
      return;
    }
    for (var i = 0; i < shown.length; i++) {
      box.appendChild(pluginRow(shown[i]));
    }
  }

  function pluginRow(plugin) {
    var row = el('div', 'plugin-row' +
      (plugin.enabled === false ? ' is-disabled' : '') +
      (plugin.error ? ' is-error' : ''));
    var head = el('div', 'plugin-head');
    var title = el('div', 'plugin-title');
    title.appendChild(el('span', 'plugin-name', plugin.name));
    if (plugin.version) title.appendChild(el('span', 'tag plugin-kind', 'v' + plugin.version));
    var kinds = Array.isArray(plugin.kinds) ? plugin.kinds : [];
    for (var k = 0; k < kinds.length; k++) {
      title.appendChild(el('span', 'tag plugin-kind', kinds[k]));
    }
    if (!plugin.installed) title.appendChild(el('span', 'agent-missing-tag', 'Tidak dikenal'));
    if (plugin.error) title.appendChild(el('span', 'agent-missing-tag', 'Manifest rusak'));
    head.appendChild(title);

    var controls = el('div', 'plugin-controls');
    if (plugin.installed) {
      controls.appendChild(settingsSwitch(plugin.enabled !== false,
        plugin.enabled === false ? 'Aktifkan ' + plugin.name : 'Nonaktifkan ' + plugin.name,
        function (on, cb) {
          return pluginAction(on ? 'enable' : 'disable', plugin.name, []).then(function (res) {
            if (!res && cb) cb.checked = !on;
            return res;
          });
        }));
    }
    head.appendChild(controls);
    row.appendChild(head);

    if (plugin.description) row.appendChild(el('p', 'plugin-desc', plugin.description));
    var meta = [];
    if (plugin.author) meta.push(plugin.author);
    if (plugin.license) meta.push(plugin.license);
    if (plugin.api) meta.push('API ' + plugin.api);
    if (plugin.source) meta.push(plugin.source);
    if (plugin.sha256) meta.push('sha256 ' + String(plugin.sha256).slice(0, 12));
    if (meta.length) row.appendChild(el('div', 'plugin-meta mono', meta.join(' \u00b7 ')));

    var requested = pluginRequested(plugin);
    var perms = el('div', 'plugin-perms');
    perms.appendChild(el('span', 'set-field-label', 'Izin'));
    if (!requested.length) {
      perms.appendChild(el('span', 'plugin-none', 'Tidak meminta izin'));
    } else {
      for (var p = 0; p < requested.length; p++) {
        perms.appendChild(pluginPermChip(plugin, requested[p]));
      }
    }
    row.appendChild(perms);
    if (plugin.permissions && requested.length) {
      row.appendChild(el('div', 'plugin-note', plugin.permissions));
    }

    var actions = el('div', 'plugin-actions');
    var panels = pluginPanels.filter(function (panel) {
      return panel && panel.plugin === plugin.name;
    });
    for (var pi = 0; pi < panels.length; pi++) {
      actions.appendChild(pluginActionButton('Buka panel: ' + (panels[pi].title || panels[pi].name),
        function (panel) {
          return function () { return openPluginPanel(panel.plugin, panel.name); };
        }(panels[pi])));
    }
    var commands = pluginCommands.filter(function (command) {
      return String(command[0] || '').split(':')[0] === plugin.name;
    });
    for (var ci = 0; ci < commands.length; ci++) {
      actions.appendChild(pluginActionButton(commands[ci][1] || commands[ci][0],
        function (command) {
          return function () { return runPluginCommand(command[0], command[1]); };
        }(commands[ci])));
    }
    if (plugin.installed) {
      actions.appendChild(pluginRemoveButton(plugin));
    }
    if (actions.children.length) row.appendChild(actions);
    return row;
  }

  function pluginActionButton(label, run) {
    var btn = button(label, 'btn sm', run);
    btn.type = 'button';
    return btn;
  }

  function pluginPermChip(plugin, key) {
    var granted = pluginGranted(plugin).indexOf(key) >= 0;
    var label = key + (granted ? ' \u2713' : ' \u2717');
    var chip = button(label, 'perm-chip' + (granted ? ' is-granted' : ''), function () {
      return pluginAction(granted ? 'revoke' : 'grant', plugin.name, [key]);
    });
    chip.title = granted ? 'Cabut izin ' + key : 'Beri izin ' + key;
    return chip;
  }

  function pluginRemoveButton(plugin) {
    var armed = false;
    var timer = 0;
    var btn = el('button', 'btn sm danger', 'Hapus');
    btn.type = 'button';
    btn.addEventListener('click', guard(function () {
      if (!armed) {
        armed = true;
        btn.textContent = 'Yakin hapus?';
        timer = setTimeout(function () {
          armed = false;
          btn.textContent = 'Hapus';
        }, 4000);
        return;
      }
      clearTimeout(timer);
      return pluginAction('remove', plugin.name, []);
    }));
    return btn;
  }

  function pluginAction(action, name, names, timeout) {
    var label = action === 'remove' ? 'Plugin dihapus' :
      action === 'grant' ? 'Izin diberikan' :
        action === 'revoke' ? 'Izin dicabut' :
          action === 'enable' ? 'Plugin diaktifkan' :
            action === 'disable' ? 'Plugin dinonaktifkan' : 'Plugin diperbarui';
    return rpc('pluginAction', {
      action: action,
      name: name || '',
      names: names || []
    }, timeout || 60000).then(function (res) {
      if (action === 'search' || action === 'doctor') return res;
      toast('ok', label + ': ' + (name || ''));
      return refreshAppInfo().then(loadPlugins);
    }, function (err) {
      toast('error', 'Plugin: ' + message(err));
      return null;
    });
  }

  function installPlugin(source) {
    var value = String(source || '').trim();
    if (!value) {
      toast('warn', 'Isi nama katalog, folder plugin, atau file .tar.gz.');
      return Promise.resolve(null);
    }
    var btn = $('plugin-install-btn');
    if (btn) btn.disabled = true;
    return pluginAction('install', value, [], 180000).then(function (res) {
      if (btn) btn.disabled = false;
      if (res) toast('ok', 'Terpasang: ' + value + '. Jalankan hanya saat dibutuhkan.');
      return res;
    });
  }

  function searchPluginCatalog(query) {
    var input = $('plugin-search');
    var value = String(query == null ? (input ? input.value : '') : query).trim();
    var box = $('plugin-catalog');
    if (box) {
      box.textContent = '';
      box.appendChild(el('p', 'side-empty', 'Mencari katalog...'));
    }
    return pluginAction('search', value, [], 30000).then(function (res) {
      if (!res) return null;
      pluginCatalogData = res;
      renderPluginCatalog(res);
      return res;
    });
  }

  function renderPluginCatalog(res) {
    var box = $('plugin-catalog');
    if (!box) return;
    box.textContent = '';
    var rows = Array.isArray(res && res.plugins) ? res.plugins : [];
    box.appendChild(settingsGroup('Katalog',
      res && res.catalog ? res.catalog : (rows.length + ' hasil')));
    if (!rows.length) {
      box.appendChild(el('p', 'side-empty', 'Katalog tidak mengembalikan plugin.'));
      return;
    }
    for (var i = 0; i < rows.length; i++) {
      box.appendChild(catalogRow(rows[i]));
    }
  }

  function catalogRow(entry) {
    var row = el('div', 'plugin-row catalog-row');
    var head = el('div', 'plugin-head');
    var title = el('div', 'plugin-title');
    title.appendChild(el('span', 'plugin-name', entry.name));
    if (entry.version) title.appendChild(el('span', 'tag plugin-kind', 'v' + entry.version));
    var kinds = Array.isArray(entry.kinds) ? entry.kinds : [];
    for (var k = 0; k < kinds.length; k++) {
      title.appendChild(el('span', 'tag plugin-kind', kinds[k]));
    }
    head.appendChild(title);
    var controls = el('div', 'plugin-controls');
    var installed = entry.installed || null;
    var label = installed
      ? (installed === entry.version ? 'Terpasang' : 'Perbarui')
      : 'Pasang';
    var btn = pluginActionButton(label, function () {
      return pluginAction(installed ? 'update' : 'install', entry.name, [], 180000);
    });
    if (installed === entry.version) btn.disabled = true;
    controls.appendChild(btn);
    head.appendChild(controls);
    row.appendChild(head);
    if (entry.description) row.appendChild(el('p', 'plugin-desc', entry.description));
    var meta = [];
    if (entry.author) meta.push(entry.author);
    if (entry.license) meta.push(entry.license);
    if (entry.permissions) meta.push(entry.permissions);
    if (entry.sha256) meta.push('sha256 ' + String(entry.sha256).slice(0, 12));
    if (meta.length) row.appendChild(el('div', 'plugin-meta mono', meta.join(' \u00b7 ')));
    return row;
  }

  var panelState = null;

  function openPluginPanel(plugin, panel) {
    return rpc('pluginPanel', { name: plugin, panel: panel || '' }, 15000).then(function (info) {
      if (!info) return null;
      var frame = $('plugin-panel-frame');
      var title = $('plugin-panel-title');
      var source = $('plugin-panel-source');
      if (title) title.textContent = info.title || info.name || 'Plugin panel';
      if (source) source.textContent = info.url || '';
      if (frame) frame.setAttribute('src', info.url || 'about:blank');
      panelState = { plugin: info.plugin, name: info.name, frame: frame };
      showModal($('plugin-panel-modal'));
      return info;
    }, function (err) {
      toast('error', 'Panel: ' + message(err));
      return null;
    });
  }

  function closePluginPanel() {
    var frame = $('plugin-panel-frame');
    if (frame) frame.setAttribute('src', 'about:blank');
    panelState = null;
    hideModal($('plugin-panel-modal'));
  }

  function showPluginOutput(title, result, note) {
    var head = $('plugin-output-title');
    if (head) head.textContent = title || 'Plugin output';
    var pre = $('plugin-output-pre');
    if (pre) {
      var text = '';
      if (typeof result === 'string') text = result;
      else if (result == null) text = 'Selesai tanpa output.';
      else {
        try { text = JSON.stringify(result, null, 2); }
        catch (err) { text = String(result); }
      }
      pre.textContent = text;
    }
    var foot = $('plugin-output-note');
    if (foot) foot.textContent = note || '';
    showModal($('plugin-output-modal'));
  }

  function showPluginError(title, err) {
    showPluginOutput(title, { error: message(err) }, 'Plugin tidak menghasilkan jawaban.');
  }

  function runPluginCommand(fullName, title) {
    var text = String(fullName || '');
    var at = text.indexOf(':');
    if (at <= 0) {
      toast('warn', 'Nama perintah plugin tidak lengkap.');
      return Promise.resolve(null);
    }
    var plugin = text.slice(0, at);
    var command = text.slice(at + 1);
    setLast('Plugin: ' + text);
    return rpc('pluginRun', {
      name: plugin,
      command: command,
      args: []
    }, 180000).then(function (res) {
      showPluginOutput(title || text, res, text);
      return res;
    }, function (err) {
      showPluginError(title || text, err);
      return null;
    });
  }

  /* The sandboxed panel may only answer through this narrow bridge. Nothing
     from the frame is trusted beyond a method name and JSON params. */
  window.addEventListener('message', function (ev) {
    var data = ev && ev.data;
    if (!data || data.omnixPanel !== true) return;
    if (!panelState || !panelState.frame) return;
    if (ev.source !== panelState.frame.contentWindow) return;
    if (data.ready || !data.method) return;
    var frame = panelState.frame;
    var plugin = panelState.plugin;
    var id = data.id;
    var reply = function (ok, payload) {
      try {
        frame.contentWindow.postMessage({
          omnixPanelReply: true,
          id: id,
          ok: ok,
          result: ok ? payload : null,
          error: ok ? null : payload
        }, '*');
      } catch (err) { /* frame went away */ }
    };
    rpc('pluginPanelCall', {
      name: plugin,
      method: String(data.method),
      params: data.params || {}
    }, 60000).then(function (res) {
      reply(true, res);
    }, function (err) {
      reply(false, { message: message(err) });
    });
  });

  /* ---------- self-development ---------- */

  function fillDevAgents() {
    var select = $('dev-agent');
    if (!select) return;
    var current = select.value;
    select.textContent = '';
    var order = ['codex', 'claude', 'antigravity'];
    var rows = agentsInfo.slice();
    for (var o = 0; o < order.length; o++) {
      var found = false;
      for (var r = 0; r < rows.length; r++) {
        if (rows[r].name === order[o]) found = true;
      }
      if (!found) rows.push({ name: order[o], bin: order[o], available: false, enabled: true });
    }
    rows.sort(function (a, b) {
      var ai = order.indexOf(a.name);
      var bi = order.indexOf(b.name);
      if (ai < 0) ai = 99;
      if (bi < 0) bi = 99;
      return ai - bi || String(a.name).localeCompare(String(b.name));
    });
    var first = '';
    for (var i = 0; i < rows.length; i++) {
      var agent = rows[i];
      var usable = agent.available !== false && agent.enabled !== false;
      var opt = el('option', null,
        (agentDisplay(agent) || agent.name) +
        (usable ? '' : ' \u00b7 tidak siap'));
      opt.value = agent.name;
      if (!usable) opt.disabled = true;
      else if (!first) first = agent.name;
      select.appendChild(opt);
    }
    var pick = current || first || 'codex';
    select.value = pick;
  }

  function loadDev() {
    return call('devStatus', {}).then(function (res) {
      if (!res) return null;
      devData = res;
      fillDevAgents();
      renderDev(res);
      return res;
    });
  }

  function renderDev(res) {
    var data = res || devData || {};
    var builds = Array.isArray(data.builds) ? data.builds : [];
    var tasks = Array.isArray(data.tasks) ? data.tasks : [];
    var search = $('settings-search');
    var query = search ? String(search.value || '').trim().toLowerCase() : '';
    var hint = $('agent-settings-meta');
    if (hint) {
      var installed = data.installed && data.installed.build ? data.installed.build : null;
      hint.textContent = builds.length + ' build \u00b7 ' + tasks.length + ' tugas' +
        (installed ? ' \u00b7 terpasang ' + installed : '') +
        (query ? ' \u00b7 filter "' + query + '"' : '');
    }
    var restore = $('dev-restore-btn');
    if (restore) restore.disabled = !data.previous;

    var tasksBox = $('dev-tasks');
    if (tasksBox) {
      tasksBox.textContent = '';
      var shownTasks = tasks.filter(function (task) {
        if (!query) return true;
        return [task.name, task.agent, task.branch, task.worktree, task.status, task.outcome]
          .join(' ').toLowerCase().indexOf(query) >= 0;
      });
      tasksBox.appendChild(settingsGroup('Tugas',
        data.devHome ? 'home terpisah: ' + data.devHome : 'worktree agent'));
      if (!shownTasks.length) {
        tasksBox.appendChild(el('p', 'side-empty', 'Belum ada tugas. Tulis brief lalu buat tugas.'));
      }
      for (var t = 0; t < shownTasks.length; t++) {
        tasksBox.appendChild(devTaskRow(shownTasks[t]));
      }
    }

    var buildsBox = $('dev-builds');
    if (buildsBox) {
      buildsBox.textContent = '';
      var shownBuilds = builds.filter(function (build) {
        if (!query) return true;
        return [build.id, build.branch, build.commit, build.worktree]
          .join(' ').toLowerCase().indexOf(query) >= 0;
      });
      buildsBox.appendChild(settingsGroup('Build',
        data.installed && data.installed.build
          ? 'terpasang: ' + data.installed.build
          : 'belum ada build yang dipasang'));
      if (!shownBuilds.length) {
        buildsBox.appendChild(el('p', 'side-empty', 'Belum ada build. Tekan Build release.'));
      }
      for (var b = 0; b < shownBuilds.length; b++) {
        buildsBox.appendChild(devBuildRow(shownBuilds[b], data));
      }
    }
  }

  function devTaskRow(task) {
    var row = el('div', 'plugin-row dev-row');
    var head = el('div', 'plugin-head');
    var title = el('div', 'plugin-title');
    title.appendChild(el('span', 'plugin-name', task.name || 'dev task'));
    title.appendChild(el('span', 'tag plugin-kind', task.agent || 'agent'));
    var state = task.closed ? 'closed' : (task.status || task.outcome || 'journaled');
    title.appendChild(el('span', 'tag plugin-kind', state));
    head.appendChild(title);
    var controls = el('div', 'plugin-controls');
    controls.appendChild(pluginActionButton('Buka terminal', function () {
      return openDevTask(task);
    }));
    controls.appendChild(pluginActionButton('Build', function () {
      return devJob('Build ' + task.name, 'devBuild', { workspace: task.worktree });
    }));
    head.appendChild(controls);
    row.appendChild(head);
    var meta = [task.branch, task.worktree].filter(Boolean).join(' \u00b7 ');
    if (meta) row.appendChild(el('div', 'plugin-meta mono', meta));
    return row;
  }

  function devBuildRow(build, data) {
    var row = el('div', 'plugin-row dev-row');
    var head = el('div', 'plugin-head');
    var title = el('div', 'plugin-title');
    title.appendChild(el('span', 'plugin-name', build.id));
    title.appendChild(el('span', 'tag plugin-kind', build.profile || 'release'));
    if (data && data.installed && data.installed.build === build.id) {
      title.appendChild(el('span', 'tag plugin-kind', 'terpasang'));
    }
    head.appendChild(title);
    var controls = el('div', 'plugin-controls');
    controls.appendChild(pluginActionButton('Jalankan', function () {
      return devJob('Jalankan ' + build.id, 'devRun', { build: build.id });
    }));
    controls.appendChild(pluginActionButton('Pasang build ini', function () {
      return rpc('devInstall', { build: build.id }, 60000).then(function (res) {
        if (!res) return null;
        toast('ok', res.message || 'Build terpasang. Restart Omnix untuk memakainya.');
        return loadDev();
      }, function (err) {
        toast('error', 'Pasang build: ' + message(err));
        return null;
      });
    }));
    head.appendChild(controls);
    row.appendChild(head);
    var meta = [
      build.branch,
      build.commit,
      build.size ? (build.size / 1048576).toFixed(1) + ' MiB' : '',
      build.built_at ? relTime(build.built_at) : '',
      build.sha256 ? 'sha256 ' + String(build.sha256).slice(0, 12) : ''
    ].filter(Boolean).join(' \u00b7 ');
    if (meta) row.appendChild(el('div', 'plugin-meta mono', meta));
    var path = el('div', 'plugin-note mono', build.binary || '');
    path.title = build.binary || '';
    row.appendChild(path);
    return row;
  }

  function openDevTask(task) {
    if (!task || !task.name) return Promise.resolve(null);
    return rpc('devTaskInfo', { name: task.name }, 15000).then(function (res) {
      if (!res || !res.record) return null;
      return openDevTaskRecord(res.record, res.brief);
    }, function (err) {
      toast('error', 'Buka tugas: ' + message(err));
      return null;
    });
  }

  function openDevTaskRecord(record, brief) {
    if (!record) return null;
    var tab = createTerminal({
      paneKey: 'dev:' + record.name,
      agent: record.agent,
      cwd: record.worktree,
      title: record.name,
      handoff: record.name,
      handoffBrief: record.name,
      handoffPrompt: brief || ''
    });
    if (tab && tab.paneKey) {
      rpc('devTaskDelivered', { name: record.name, paneKey: tab.paneKey }, 15000)
        .then(function () { return loadDev(); }, function () { return null; });
    }
    return tab;
  }

  function createDevTask() {
    var brief = $('dev-brief');
    var agent = $('dev-agent');
    var value = brief ? String(brief.value || '').trim() : '';
    if (!value) {
      toast('warn', 'Tulis brief terlebih dahulu.');
      return Promise.resolve(null);
    }
    var btn = $('dev-task-btn');
    if (btn) btn.disabled = true;
    var meta = $('dev-task-meta');
    if (meta) meta.textContent = 'Membuat worktree dan brief...';
    return rpc('devTask', {
      brief: value,
      agent: agent ? agent.value : 'codex'
    }, 120000).then(function (res) {
      if (btn) btn.disabled = false;
      if (!res || !res.record) {
        if (meta) meta.textContent = 'Tugas tidak dibuat.';
        return null;
      }
      if (brief) brief.value = '';
      if (meta) meta.textContent = 'Tugas dibuat di ' + res.record.worktree + '.';
      toast('ok', 'Tugas dibuat: ' + res.record.name);
      openDevTaskRecord(res.record, res.brief);
      return loadDev();
    }, function (err) {
      if (btn) btn.disabled = false;
      if (meta) meta.textContent = 'Gagal: ' + message(err);
      toast('error', 'Tugas: ' + message(err));
      return null;
    });
  }

  function devJob(label, cmd, args) {
    var output = $('dev-output');
    if (output) {
      output.hidden = false;
      output.textContent = label + '...';
    }
    setLast(label);
    return rpc(cmd, args || {}, 1800000).then(function (res) {
      if (output) {
        output.hidden = false;
        try { output.textContent = JSON.stringify(res, null, 2); }
        catch (err) { output.textContent = String(res); }
      }
      toast('ok', label + ' selesai');
      return loadDev();
    }, function (err) {
      if (output) {
        output.hidden = false;
        output.textContent = label + ' gagal:\n' + message(err);
      }
      toast('error', label + ': ' + message(err));
      return null;
    });
  }

  /* ---------- search ---------- */

  function resumeSession(s) {
    if (!s || !s.agent || !s.sessionId) {
      toast('warn', 'This session has no agent or ID.');
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
    var resume = button('Resume', 'btn sm', function () {
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
      toast('warn', 'Enter a search query.');
      return Promise.resolve(null);
    }
    var args = { query: query, limit: 20 };
    var agent = $('search-agent').value;
    if (agent) args.agent = agent;
    if ($('search-conversation').checked) args.conversation = true;

    setLast('Search: ' + query);
    return call('historySearch', args).then(function (res) {
      var list = $('search-results');
      var meta = $('search-meta');
      list.textContent = '';
      if (!res) {
        meta.textContent = 'Search failed';
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
    'handoff': `<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M2.5 8h8"/><path d="m7.5 4.5 3.5 3.5-3.5 3.5"/><path d="M13 3v10"/></svg>`,
    branch: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 3v12"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M18 9a9 9 0 0 1-9 9"/></svg>`,
    chat: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21 12a8 8 0 0 1-8 8H7l-4 3v-6a8 8 0 0 1 8-8h2a8 8 0 0 1 8 3z"/><path d="M8 10h8"/><path d="M8 14h5"/></svg>`,
    more: `<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><circle cx="5" cy="12" r="1.6"/><circle cx="12" cy="12" r="1.6"/><circle cx="19" cy="12" r="1.6"/></svg>`,
    copy: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="9" y="9" width="11" height="11" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>`,
    trash: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 7h16"/><path d="M10 11v6"/><path d="M14 11v6"/><path d="M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12"/><path d="M9 7V4h6v3"/></svg>`,
    folder: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 6a2 2 0 0 1 2-2h3.6a2 2 0 0 1 1.4.6L12.5 6H18a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2z"/></svg>`,
    file: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M14 3H6a1 1 0 0 0-1 1v16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V8z"/><path d="M14 3v5h5"/></svg>`,
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
    gear: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.6a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>`,
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
  function iconNode(name, extra) {
    var mark = ICONS[iconKey(name)];
    var box = el('span', (mark ? 'icon' : 'icon is-mono') + (extra ? ' ' + extra : ''));
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
    var parts = agentParts(name);
    if (parts) return (AGENT_LABEL[parts.base] || parts.base) + ' \u00b7 ' + parts.profile;
    return AGENT_LABEL[name] || name;
  }

  function agentParts(name) {
    var text = String(name || '');
    var at = text.indexOf(':');
    if (at <= 0 || at === text.length - 1) return null;
    return { base: text.slice(0, at), profile: text.slice(at + 1) };
  }

  function agentDisplay(agent) {
    if (!agent) return '';
    var name = typeof agent === 'string' ? agent : agent.name;
    var parts = agentParts(name);
    if (parts) {
      var label = (agent && agent.label) || parts.profile;
      return (AGENT_LABEL[parts.base] || parts.base) + ' \u00b7 ' + label;
    }
    return AGENT_LABEL[name] || name;
  }

  function profileHue(text) {
    var hue = 0;
    for (var i = 0; i < text.length; i++) hue = (hue * 31 + text.charCodeAt(i)) % 360;
    return hue;
  }

  function profileBadge(name, cls) {
    var parts = agentParts(name);
    if (!parts) return null;
    var badge = el('span', cls || 'profile-badge', parts.profile.charAt(0).toUpperCase());
    badge.style.backgroundColor = 'hsl(' + profileHue(parts.profile) + ' 68% 42%)';
    badge.title = 'Profile ' + parts.profile;
    return badge;
  }

  var agentsInfo = [];
  var pluginCommands = [];
  var pluginPanels = [];
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
          available: a.available !== false,
          enabled: a.enabled !== false,
          label: String(a.label || ''),
          base: String(a.base || ''),
          profile: String(a.profile || ''),
          plugin: String(a.plugin || ''),
          supportsConfig: a.supportsConfig !== false
        };
      });
    fillAgentPicker('opencode2');
    fillAgents($('search-agent'), 'any agent', '');
    fillDevAgents();
  }

  /* The handoff target is a DOM listbox, not a native <select>. GTK gives a
     native select its own popup window, and that window survived minimizing
     the app. A DOM menu is clipped to this window and closes with it. */
  var handoffTarget = '';

  function findAgent(name) {
    for (var i = 0; i < agentsInfo.length; i++) {
      if (agentsInfo[i].name === name) return agentsInfo[i];
    }
    return null;
  }

  function fillAgentPicker(preferred) {
    var fallback = '';
    var pick = '';
    for (var i = 0; i < agentsInfo.length; i++) {
      var agent = agentsInfo[i];
      if (!agent.available || !agent.enabled) continue;
      if (!fallback) fallback = agent.name;
      if (!pick && (agent.name === handoffTarget || agent.name === preferred)) pick = agent.name;
    }
    setHandoffTarget(pick || fallback);
    buildHandoffOptions();
  }

  function buildHandoffOptions() {
    var menu = $('handoff-target-menu');
    if (!menu) return;
    menu.textContent = '';
    for (var i = 0; i < agentsInfo.length; i++) {
      var agent = agentsInfo[i];
      var option = el('button', 'picker-option');
      option.type = 'button';
      option.setAttribute('role', 'option');
      option.dataset.agent = agent.name;
      option.setAttribute('aria-selected', agent.name === handoffTarget ? 'true' : 'false');
      option.appendChild(iconNode(agent.name));
      option.appendChild(el('span', 'picker-option-label', agentDisplay(agent) + ' (CLI)'));
      if (!agent.available) option.appendChild(el('span', 'picker-option-note', 'not found'));
      else if (!agent.enabled) option.appendChild(el('span', 'picker-option-note', 'disabled'));
      if (!agent.available || !agent.enabled) option.disabled = true;
      option.addEventListener('click', guard((function (name) {
        return function (ev) {
          if (ev && ev.stopPropagation) ev.stopPropagation();
          chooseHandoffTarget(name);
        };
      })(agent.name)));
      menu.appendChild(option);
    }
  }

  function setHandoffTarget(name) {
    handoffTarget = String(name || '');
    var agent = findAgent(handoffTarget);
    var value = $('handoff-target-value');
    if (value) {
      value.textContent = agent ? agentDisplay(agent) : (handoffTarget || 'Choose an agent');
    }
    var button = $('handoff-target');
    if (button) button.removeAttribute('title');
    var options = document.querySelectorAll('#handoff-target-menu .picker-option');
    for (var i = 0; i < options.length; i++) {
      options[i].setAttribute('aria-selected',
        options[i].dataset.agent === handoffTarget ? 'true' : 'false');
    }
  }

  function closeHandoffPicker() {
    var menu = $('handoff-target-menu');
    var button = $('handoff-target');
    if (menu) menu.hidden = true;
    if (button) button.setAttribute('aria-expanded', 'false');
  }

  function openHandoffPicker() {
    var menu = $('handoff-target-menu');
    var button = $('handoff-target');
    if (!menu || !button || menu.hidden === false) return;
    buildHandoffOptions();
    menu.hidden = false;
    button.setAttribute('aria-expanded', 'true');
    var selected = menu.querySelector('[aria-selected="true"]:not(:disabled)') ||
      menu.querySelector('.picker-option:not(:disabled)');
    if (selected && selected.focus) selected.focus();
  }

  function toggleHandoffPicker() {
    var menu = $('handoff-target-menu');
    if (menu && menu.hidden) openHandoffPicker();
    else closeHandoffPicker();
  }

  function chooseHandoffTarget(name) {
    setHandoffTarget(name);
    closeHandoffPicker();
    var button = $('handoff-target');
    if (button && button.focus) button.focus();
  }

  function handoffPickerKeydown(ev) {
    var menu = $('handoff-target-menu');
    if (!menu || menu.hidden) return;
    var options = Array.prototype.slice.call(
      menu.querySelectorAll('.picker-option:not(:disabled)'));
    if (!options.length) return;
    var at = options.indexOf(document.activeElement);
    if (ev.key === 'ArrowDown') {
      ev.preventDefault();
      options[Math.min(options.length - 1, at + 1)].focus();
    } else if (ev.key === 'ArrowUp') {
      ev.preventDefault();
      options[Math.max(0, at - 1)].focus();
    } else if (ev.key === 'Home') {
      ev.preventDefault();
      options[0].focus();
    } else if (ev.key === 'End') {
      ev.preventDefault();
      options[options.length - 1].focus();
    }
  }

  function showModal(node) {
    if (!node) return;
    node.hidden = false;
    var focus = node.querySelector('[data-autofocus], select, input');
    if (focus && focus.focus) focus.focus();
  }

  function hideModal(node) {
    if (!node) return;
    if (node.id === 'handoff-modal') closeHandoffPicker();
    node.hidden = true;
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
    closeHandoffPicker();
    fillAgentPicker('opencode2');
    var start = $('handoff-start');
    if (start) start.disabled = !handoffSource;
    setHandoffContext('focused');
    showModal($('handoff-modal'));
  }

  function startHandoff() {
    if (!handoffSource) {
      toast('warn', 'Choose a session from Agents history first.');
      return Promise.resolve(null);
    }
    var agent = handoffTarget;
    if (!agent) {
      toast('warn', 'Pick a target agent.');
      return Promise.resolve(null);
    }
    var start = $('handoff-start');
    if (start) start.disabled = true;
    setLast('Handoff to ' + agent);
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
        agent: agent,
        cwd: res.cwd,
        title: agent + ' handoff',
        handoffBrief: res.brief,
        handoffPrompt: res.prompt || ''
      });
      toast('ok', 'Handoff to ' + agent + ' \u00b7 ' + res.messages + ' messages');
      setLast('Handoff brief ready');
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
    if (cwd) cwd.value = launchCwd() || appRoot || '';
    showModal($('cmd-modal'));
    if (line && line.focus) line.focus();
  }

  function startCmd() {
    var line = $('cmd-line');
    var cwd = $('cmd-cwd');
    var err = $('cmd-error');
    var command = line ? line.value.trim() : '';
    if (!command) {
      if (err) err.textContent = 'Command is required.';
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
      if (!activeScope) {
        activeScope = normScope(appRoot);
        /* A tab opened before appInfo landed must not stay global. */
        for (var i = 0; i < tabs.length; i++) {
          if (!tabs[i].scope) tabs[i].scope = activeScope;
        }
      }
      applyWorkspaceTitle();
      TRACE_KEYS = !!info.traceKeys;
      pluginCommands = Array.isArray(info.pluginCommands) ? info.pluginCommands : [];
      pluginPanels = Array.isArray(info.pluginPanels) ? info.pluginPanels : [];
      setAgents(info.agents);
      setupInfo = info.setup || setupInfo;
      if (setupInfo && setupInfo.needed) {
        setTimeout(function () { setupOpen(setupInfo); }, 160);
      }
      var parts = ['omnix ' + version];
      if (info.root) parts.push(info.root);
      parts.push('lightpanda ' + (engine.lightpanda ? (engine.version || 'ready') : 'unavailable'));
      $('status-left').textContent = parts.join(' \u00b7 ');
      return info;
    });
  }

  /* ---------- updates ---------- */

  var updateInfo = null;

  function checkUpdates(manual) {
    return rpc('updateCheck', {}, 25000).then(function (res) {
      updateInfo = res && res.available ? res : null;
      var chip = $('update-chip');
      if (chip) {
        chip.hidden = !updateInfo;
        chip.disabled = false;
        if (updateInfo) {
          chip.textContent = 'Update ' + updateInfo.version;
          chip.title = 'Download Omnix ' + updateInfo.version;
        }
      }
      if (manual) {
        setLast('Update check complete');
        toast('info', updateInfo
          ? 'Omnix ' + updateInfo.version + ' is available'
          : 'Omnix ' + ((res && res.current) || '') + ' is up to date');
      }
      return res;
    }, function (err) {
      if (manual) {
        setLast('Update check failed');
        toast('error', 'Update check failed: ' + message(err));
      }
      return null;
    });
  }

  function installUpdate() {
    if (!updateInfo) return null;
    var version = updateInfo.version;
    var chip = $('update-chip');
    if (chip) {
      chip.disabled = true;
      chip.textContent = 'Updating...';
    }
    setLast('Downloading Omnix ' + version + '...');
    return call('updateInstall', {}, { quiet: true, timeout: 180000 }).then(function (res) {
      if (!res || res.ok === false) {
        if (chip) {
          chip.disabled = false;
          chip.textContent = 'Update ' + version;
        }
        setLast('Update failed');
        toast('error', (res && res.message) || 'Update failed. Download it from GitHub instead.');
        return null;
      }
      toast('info', res.message || 'Update started');
      if (res.quit) {
        setLast('Restarting Omnix...');
        setTimeout(function () { call('windowClose', {}, { quiet: true }); }, 1200);
      } else if (chip) {
        chip.hidden = true;
      }
      return res;
    });
  }

  var lastStatus = null;

  var savedProjects = [];
  var projectStats = {};
  var projFilter = '';

  function refreshStatus() {
    return call('status', {}).then(function (res) {
      if (!res) return null;
      lastStatus = res;
      var list = Array.isArray(res.workspaces) ? res.workspaces : [];
      applyWorkspaceTitle();
      renderProjects(savedProjects);
      var db = typeof res.dbBytes === 'number' ? (res.dbBytes / 1048576).toFixed(1) + ' MiB' : '?';
      $('status-right').textContent =
        'Sessions ' + num(res.sessions) + ' \u00b7 Messages ' + num(res.messages) + ' \u00b7 Index ' + db;
      renderConflicts(res.conflicts);
      return res;
    });
  }

  /* ---------- shared work warnings ---------- */

  var conflictDismissed = '';

  function conflictSignature(list) {
    return list.map(function (row) {
      return String(row.kind || '') + '|' + String(row.value || '');
    }).join('\n');
  }

  function renderConflicts(list) {
    var banner = $('conflict-banner');
    if (!banner) return;
    list = Array.isArray(list) ? list : [];
    var signature = conflictSignature(list);
    banner.textContent = '';
    if (!list.length || signature === conflictDismissed) {
      banner.hidden = true;
      return;
    }
    banner.hidden = false;

    var head = el('div', 'conflict-head');
    head.appendChild(el('span', 'conflict-title', 'Kerja bersama'));
    head.appendChild(el('span', 'conflict-sub', list.length + ' peringatan'));
    var close = button('\u00d7', 'icon-btn conflict-close', function () {
      conflictDismissed = signature;
      banner.hidden = true;
    });
    close.setAttribute('aria-label', 'Dismiss shared-work warnings');
    head.appendChild(close);
    banner.appendChild(head);

    list.forEach(function (row) {
      var line = el('div', 'conflict-row');
      line.appendChild(el('span', 'conflict-kind',
        row.kind === 'folder' ? 'Folder' : 'Akun'));
      line.appendChild(el('span', 'conflict-msg', String(row.message || '')));
      if (row.kind === 'folder') {
        var panes = Array.isArray(row.panes) ? row.panes : [];
        var target = panes.length ? panes[panes.length - 1] : null;
        if (target && target.paneKey) {
          line.appendChild(button('Pindahkan ke worktree baru', 'btn sm', function () {
            return movePaneToWorktree(target.paneKey);
          }));
        }
      }
      banner.appendChild(line);
    });
  }

  function movePaneToWorktree(paneKey) {
    var old = byPaneKey(paneKey);
    return call('paneMoveWorktree', { paneKey: paneKey }, 60000).then(function (res) {
      if (!res || !res.worktree) return null;
      var tab = createTerminal({
        agent: res.agent || null,
        sessionId: res.sessionId || null,
        cwd: res.worktree,
        title: res.title || res.name,
        label: res.label,
        handoff: res.handoff || null,
        parent: res.parent || null,
        orchestrator: !!res.orchestrator
      });
      if (tab && old) closeTab(old);
      toast('ok', 'Pane dipindahkan ke worktree ' + res.name);
      refreshMemory();
      return tab;
    });
  }

  /* ---------- usage and memory ---------- */

  var usageData = { ram: null, agents: [] };

  function refreshUsage() {
    return call('usage', {}, { quiet: true }).then(function (res) {
      if (!res) return null;
      usageData = res;
      renderUsage();
      return res;
    });
  }

  var memData = null;
  var memoryTimer = null;
  var memoryBusy = false;
  var ramOverWarned = false;

  function refreshMemory() {
    if (memoryBusy) return Promise.resolve(memData);
    memoryBusy = true;
    return call('memory', {}, { quiet: true }).then(function (res) {
      memoryBusy = false;
      if (!res) return null;
      memData = res;
      renderMemory();
      renderConflicts(res.conflicts);
      return res;
    }, function () {
      memoryBusy = false;
      return null;
    });
  }

  /* Stepped spinner: four transforms per second instead of a 60fps CSS
     animation. The CSS version repainted the whole window on this WebKitGTK
     build, so an agent TUI that redraws its status line made typing lag. */
  function startWorkingSpin() {
    var angle = 0;
    setInterval(function () {
      if (document.hidden) return;
      var nodes = document.querySelectorAll('.tab-status.is-working, .work-dot.is-working');
      if (!nodes.length) return;
      angle = (angle + 90) % 360;
      for (var i = 0; i < nodes.length; i++) {
        nodes[i].style.transform = 'rotate(' + angle + 'deg)';
      }
    }, 250);
  }

  function startMemoryPoll() {
    if (memoryTimer) return;
    setTimeout(refreshMemory, 1200);
    var interval = PLATFORM === 'linux' ? 3500 : 8000;
    memoryTimer = setInterval(function () {
      if (!document.hidden) {
        refreshMemory();
        refreshBusyDots();
      }
    }, interval);
    document.addEventListener('visibilitychange', function () {
      if (!document.hidden) {
        refreshMemory();
        refreshBusyDots();
      }
    });
  }

  function memoryRow(label, sub, rss, processes, total, tone, names) {
    var row = el('div', 'mem-row' + (tone ? ' ' + tone : ''));
    var head = el('div', 'mem-row-head');
    head.appendChild(el('span', 'mem-name', label));
    head.appendChild(el('span', 'mem-value mono', fmtBytes(rss)));
    row.appendChild(head);
    var detail = [];
    if (sub) detail.push(sub);
    if (Array.isArray(names) && names.length) detail.push(names.join(', '));
    var meta = el('div', 'mem-meta mono',
      processes + (processes === 1 ? ' process' : ' processes') +
      (detail.length ? ' \u00b7 ' + detail.join(' \u00b7 ') : ''));
    row.appendChild(meta);
    var bar = el('div', 'mem-bar');
    var fill = el('span');
    fill.style.width = (total > 0 ? Math.max(2, Math.min(100, rss / total * 100)) : 0) + '%';
    bar.appendChild(fill);
    row.appendChild(bar);
    return row;
  }

  function renderMemory() {
    var res = memData || {};
    var app = res.app || {};
    var ai = res.ai || {};
    var other = res.other || {};
    var appRss = Number(app.rss) || 0;
    var aiRss = Number(ai.rss) || 0;

    var chip = $('usage-chip');
    if (chip) {
      var budget = res.budget || {};
      var over = budget.over === true;
      if (over && !ramOverWarned) {
        ramOverWarned = true;
        toast('warn', 'AI terminals use ' + fmtBytes(aiRss) + ', over the ' +
          fmtBytes(Number(budget.limitBytes) || 0) + ' budget. Close idle terminals.');
      } else if (!over) {
        ramOverWarned = false;
      }
      if (over) chip.classList.add('over');
      else chip.classList.remove('over');
      chip.title = 'AI usage and memory' +
        (over ? ' \u2014 AI terminals over the ' + fmtBytes(Number(budget.limitBytes) || 0) +
          ' budget' : '');
    }

    var totalNode = $('memory-total');
    if (totalNode) {
      totalNode.textContent = appRss > 0
        ? fmtBytes(appRss) + ' in ' + (app.processes || 0) + ' processes'
        : 'Memory data unavailable';
    }
    var aiNode = $('memory-ai');
    if (aiNode) {
      aiNode.textContent = 'AI ' + fmtBytes(aiRss) + ' \u00b7 ' + (ai.processes || 0) +
        (ai.processes === 1 ? ' process' : ' processes');
    }

    var list = $('memory-list');
    if (!list) return;
    list.textContent = '';
    if (!(appRss > 0)) {
      list.appendChild(el('div', 'usage-empty', 'Process memory is unavailable on this platform.'));
      return;
    }
    list.appendChild(memoryRow('App shell + WebKit',
      'Omnix process tree minus terminals', Number(other.rss) || 0,
      Number(other.processes) || 0, appRss, 'is-app'));

    var panes = Array.isArray(res.panes) ? res.panes : [];
    for (var i = 0; i < panes.length; i++) {
      var pane = panes[i] || {};
      list.appendChild(memoryRow(
        String(pane.label || 'Terminal'),
        'PID ' + pane.pid + (pane.cwd ? ' \u00b7 ' + baseName(pane.cwd) : ''),
        Number(pane.rss) || 0,
        Number(pane.processes) || 0,
        appRss,
        'is-ai',
        pane.top));
    }
    if (!panes.length) {
      list.appendChild(el('div', 'usage-empty', 'No terminal process is running right now.'));
    }
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

  /* One line per agent, like Orca: show the window that is closest to its
     limit; on a tie the longer window explains the reset better. */
  function peakWindow(rows) {
    var best = null;
    for (var i = 0; i < rows.length; i++) {
      var win = rows[i] || {};
      var used = Math.max(0, Math.min(100, Number(win.usedPercent) || 0));
      var minutes = Number(win.windowMinutes) || 0;
      if (!best || used > best.used || (used === best.used && minutes > best.minutes)) {
        best = { win: win, used: used, minutes: minutes };
      }
    }
    return best;
  }

  /* "Gemini Models \u00b7 Weekly" becomes "Gemini wk" in the status bar. */
  function compactWindow(win) {
    var parts = String((win && win.label) || '').split('\u00b7');
    if (parts.length < 2) return '';
    var group = parts[0].trim().split(/\s+/)[0];
    var span = parts[1].trim().toLowerCase();
    if (!group) return '';
    if (span.indexOf('week') === 0) span = 'wk';
    else if (span.indexOf('five hour') === 0 || span === '5h') span = '5h';
    else span = span.replace(/[^a-z0-9]+/g, '').slice(0, 4);
    return span ? group + ' ' + span : group;
  }

  /* Bottom bar, like Orca: the brand mark, the window closest to its limit,
     and the percentage used. The modal keeps the full detail. */
  function renderUsageStrip(agents) {
    var strip = $('usage-strip');
    if (!strip) return;
    strip.textContent = '';
    var shown = 0;
    for (var i = 0; i < agents.length; i++) {
      var agent = agents[i] || {};
      var best = peakWindow(Array.isArray(agent.windows) ? agent.windows : []);
      if (!best) continue;
      var name = String(agent.agent || 'agent');
      var key = iconKey(name);
      var item = el('span', 'usage-item is-' + key);
      item.appendChild(iconNode(name, 'usage-mark'));
      var tag = key === 'antigravity' ? compactWindow(best.win) : '';
      if (tag) item.appendChild(el('span', 'usage-tag', tag));
      item.appendChild(el('span', 'usage-pct', Math.round(best.used) + '%'));
      if (key !== 'antigravity') {
        item.appendChild(el('span', 'usage-reset', fmtReset(best.win.resetsInSeconds)));
      }
      item.title = agentLabel(name) +
        (best.win.label ? ' \u00b7 ' + String(best.win.label) : '') +
        ' \u00b7 ' + Math.round(best.used) + '% used \u00b7 resets in ' +
        fmtReset(best.win.resetsInSeconds);
      strip.appendChild(item);
      shown++;
    }
    if (!shown) strip.appendChild(el('span', 'usage-strip-empty', '\u2014'));
  }

  function renderUsage() {
    var ram = usageData.ram || {};
    var total = Number(ram.totalBytes) || 0;
    var used = Number(ram.usedBytes) || 0;
    var pct = total > 0 ? Math.round(used / total * 100) : null;
    var agents = Array.isArray(usageData.agents) ? usageData.agents : [];

    renderUsageStrip(agents);

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
      var rows = Array.isArray(agent.windows) ? agent.windows : [];
      var best = peakWindow(rows);
      var row = el('div', 'usage-limit');
      var id = el('div', 'usage-limit-id');
      id.appendChild(el('span', 'usage-agent-name', agentLabel(String(agent.agent || 'agent'))));
      var bits = [];
      if (agent.plan) {
        var plan = String(agent.plan);
        bits.push(plan.charAt(0).toUpperCase() + plan.slice(1) + ' plan');
      }
      if (best) {
        bits.push(String(best.win.label || windowLabel(best.win)));
        bits.push('resets in ' + fmtReset(best.win.resetsInSeconds));
      }
      id.appendChild(el('span', 'usage-limit-sub', best ? bits.join(' \u00b7 ') : 'No quota window reported'));
      row.appendChild(id);

      var bar = el('div', 'usage-bar');
      var fill = el('span', best && best.used >= 90 ? 'high' : '');
      fill.style.width = (best ? best.used : 0) + '%';
      bar.appendChild(fill);
      row.appendChild(bar);
      row.appendChild(el('span', 'usage-window-pct', best ? Math.round(best.used) + '%' : '-'));
      list.appendChild(row);
    }
  }

  function openUsage() {
    renderUsage();
    renderMemory();
    showModal($('usage-modal'));
    refreshMemory();
    /* Antigravity is fetched on a worker; a second read picks it up without
       freezing the modal while `agy` starts. */
    setTimeout(function () {
      if (!$('usage-modal').hidden) refreshUsage();
    }, 2500);
    return refreshUsage();
  }

  /* Usage is fetched only when the owner opens the modal or presses refresh:
     no timer, because a logged-out `agy` can raise a login window. */

  /* The sidebar lists the projects the user added, like Orca. Indexed history
     only fills in the session count and agent marks for those paths. */
  function refreshProjects() {
    return Promise.all([
      call('projects', {}),
      call('historyProjects', { limit: 200 }).catch(function () { return null; })
    ]).then(function (both) {
      var res = both[0];
      var hist = both[1];
      savedProjects = res && Array.isArray(res.projects) ? res.projects.slice() : [];
      var list = hist && Array.isArray(hist.projects) ? hist.projects : [];
      projectStats = {};
      for (var i = 0; i < list.length; i++) {
        if (list[i] && list[i].path) projectStats[list[i].path] = list[i];
      }
      renderProjects(savedProjects);
      return hist;
    }).catch(function (err) {
      toast('error', 'Projects: ' + message(err));
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
    return rpc('projectAdd', { path: path }).then(function (res) {
      savedProjects = res && Array.isArray(res.projects) ? res.projects : savedProjects;
      toast('ok', 'Project added');
      return refreshProjects().then(function () {
        openProject({ path: path, name: baseName(path) });
      });
    }, function (err) {
      toast('error', 'Add project: ' + message(err));
      throw err;
    });
  }

  function submitProject() {
    var input = $('project-path');
    var err = $('project-error');
    var path = input ? input.value.trim() : '';
    if (!path) {
      if (err) err.textContent = 'Folder path is required.';
      return;
    }
    if (err) err.textContent = '';
    addProjectPath(path).then(function () {
      hideModal($('project-modal'));
    }).catch(function (e) {
      if (err) err.textContent = message(e);
    });
  }

  /* + Project asks the host for the platform's own folder chooser. */
  function pickProject() {
    return call('pickFolder', { path: appRoot || '' }).then(function (res) {
      var path = res && typeof res.path === 'string' ? res.path : '';
      if (!path) return null;
      return addProjectPath(path);
    }).catch(function (err) {
      toast('error', 'Add project: ' + message(err));
      return null;
    });
  }

  function removeProject(p) {
    if (!p || !p.path) return;
    call('projectRemove', { path: p.path }).then(function (res) {
      savedProjects = res && Array.isArray(res.projects) ? res.projects : [];
      renderProjects(savedProjects);
      toast('ok', 'Removed ' + (p.name || baseName(p.path)) + ' from the sidebar');
    }).catch(function (err) {
      toast('error', 'Remove project: ' + message(err));
    });
  }

  /* A project row opens a terminal rooted at that project, the same gesture
     Orca uses when a workspace row is clicked. */
  function openProject(p) {
    if (!p || !p.path) return;
    var name = disambiguatedName(p.name || baseName(p.path), p.path, savedProjects);
    var scope = normScope(p.path);
    var scopeChanged = scope !== activeScope;
    activeScope = scope;
    var existing = null;
    for (var i = 0; i < tabs.length; i++) {
      if (tabs[i].kind === 'terminal' && normPath(tabs[i].cwd || appRoot) === normPath(p.path)) {
        existing = tabs[i];
        break;
      }
    }
    if (existing) {
      activate(existing);
    } else {
      lastLaunchCwd = p.path;
      createTerminal({ title: name, cwd: p.path, scope: scope });
    }
    if (scopeChanged && savedProjects.length) renderProjects(savedProjects);
    return scopeHistoryToProject(p.path);
  }

  function normPath(path) {
    return String(path || '').replace(/[\\/]+$/, '');
  }

  function pathMatches(cwd, path) {
    var a = normPath(cwd);
    var b = normPath(path);
    if (!a || !b) return false;
    return a === b || a.indexOf(b + '/') === 0 || a.indexOf(b + '\\') === 0;
  }

  /* A live terminal belongs to the most specific project that contains its
     cwd. Without this, a terminal in a nested project also shows up under
     the parent folder. */
  function projectForPath(path) {
    var cwd = normPath(path);
    var best = null;
    for (var i = 0; i < savedProjects.length; i++) {
      var p = savedProjects[i] && savedProjects[i].path;
      if (!p || !pathMatches(cwd, p)) continue;
      var key = normPath(p);
      if (!best || key.length > best.length) best = key;
    }
    return best || cwd;
  }

  function liveTabsAt(path) {
    var owner = normPath(path);
    var out = [];
    for (var i = 0; i < tabs.length; i++) {
      var t = tabs[i];
      if (t.kind !== 'terminal' || !t.spawned || t.exited || t.closed) continue;
      if (projectForPath(t.cwd || appRoot) !== owner) continue;
      out.push(t);
    }
    return out;
  }

  /* A pane is working while output flows, needs approval when the CLI asks a
     yes/no question, failed on a non-zero exit, and done once a turn goes
     quiet. A project row shows the most urgent state of its panes. */
  var STATUS_RANK = { idle: 0, done: 1, working: 2, failed: 3, approval: 4 };

  function statusRank(status) {
    return STATUS_RANK[status] || 0;
  }

  function statusLabel(status, count) {
    if (status === 'approval') return 'needs approval';
    if (status === 'failed') return 'failed';
    if (status === 'working') return count > 1 ? count + ' working' : 'working';
    if (status === 'done') return 'done';
    return count > 1 ? count + ' running' : 'running';
  }

  function aggregateStatus(list) {
    var best = 'idle';
    for (var i = 0; i < (list ? list.length : 0); i++) {
      if (statusRank(list[i].status) > statusRank(best)) best = list[i].status;
    }
    return best;
  }

  function paintStatus(node, status, label) {
    if (!node) return;
    node.className = 'work-dot is-' + (status || 'idle');
    node.setAttribute('role', 'status');
    node.setAttribute('aria-label', label || 'status');
    node.title = label || '';
  }

  function workingGlyph(label, status) {
    var node = el('span', null);
    paintStatus(node, status || 'idle', label);
    return node;
  }

  function aggregateLabel(list) {
    return statusLabel(aggregateStatus(list), (list || []).length);
  }

  /* A pane still counts for the project mark after it exits, so a crash stays
     visible until the tab is closed. */
  function statusTabsAt(path) {
    var owner = normPath(path);
    var out = [];
    for (var i = 0; i < tabs.length; i++) {
      var tab = tabs[i];
      if (tab.kind !== 'terminal' || tab.closed) continue;
      if (!tab.spawned && !tab.exited) continue;
      if (projectForPath(tab.cwd || appRoot) !== owner) continue;
      out.push(tab);
    }
    return out;
  }

  var STATUS_APPROVAL = /(waiting for approval|requires approval|approval needed|do you want to (proceed|run|allow|continue|trust)|would you like to (proceed|run|allow)|run (this|the) command|allow (this|the) (command|tool|action|edit)|permission to (run|use|edit|write)|press enter to confirm|\(y\/n\)|\[y\/n\]|\[y\/n\/a\]|(?:❯|›|>)\s*1\.\s*yes)/i;

  function looksLikeApproval(text) {
    if (STATUS_APPROVAL.test(text)) return true;
    return /\b1[.)]\s*(yes|allow|approve|proceed)/i.test(text) &&
      /\b2[.)]\s*(no|deny|cancel)/i.test(text);
  }

  function stripAnsi(text) {
    return String(text || '')
      .replace(/\x1b\[[0-9;?]*[ -/]*[@-~]/g, ' ')
      .replace(/\r/g, '\n');
  }

  function setTabStatus(tab, status) {
    if (!tab || tab.kind !== 'terminal' || tab.closed) return;
    if (!status) status = 'idle';
    if (tab.status === status) return;
    tab.status = status;
    paintTabStatus(tab);
    refreshBusyDots();
    publishPane(tab);
    renderWorkers();
  }

  /* The CLI's source of truth is the host registry, not the DOM. One call per
     transition; no timers and no polling. */
  function publishPane(tab) {
    if (!tab || tab.kind !== 'terminal' || tab.closed || !tab.paneKey) return;
    call('paneState', {
      paneKey: tab.paneKey,
      status: tab.status || 'idle',
      handoff: tab.handoff || '',
      parent: tab.parent || '',
      title: tab.title || '',
      cwd: tab.cwd || '',
      agent: tab.agent || '',
      sessionId: tab.sessionId || '',
      orchestrator: !!tab.orchestrator,
      exitCode: tab.exitedCode
    }, { quiet: true });
  }

  function paintTabStatus(tab) {
    if (!tab || !tab.statusNode) return;
    var label = statusLabel(tab.status || 'idle', 1);
    tab.statusNode.className = 'tab-status is-' + (tab.status || 'idle');
    tab.statusNode.setAttribute('role', 'status');
    tab.statusNode.setAttribute('aria-label', label);
    tab.statusNode.title = label;
  }

  function trackBusy(node, tabsAt) {
    busyMarks.push({ node: node, tabsAt: tabsAt });
    paintStatus(node, aggregateStatus(tabsAt()), aggregateLabel(tabsAt()));
    return node;
  }

  var busyMarks = [];

  function refreshBusyDots() {
    for (var i = 0; i < busyMarks.length; i++) {
      var mark = busyMarks[i];
      var list = mark.tabsAt() || [];
      paintStatus(mark.node, aggregateStatus(list), aggregateLabel(list));
    }
  }

  function noteInput(tab) {
    if (!tab || tab.kind !== 'terminal') return;
    tab.tailText = '';
    if (tab.settleTimer) {
      clearTimeout(tab.settleTimer);
      tab.settleTimer = 0;
    }
    setTabStatus(tab, 'working');
  }

  function noteOutput(tab, text) {
    if (!tab) return;
    tab.lastOutput = Date.now();
    if (text) {
      tab.tailText = ((tab.tailText || '') + stripAnsi(text)).slice(-4000);
      if (looksLikeApproval(tab.tailText)) {
        if (tab.settleTimer) {
          clearTimeout(tab.settleTimer);
          tab.settleTimer = 0;
        }
        setTabStatus(tab, 'approval');
        return;
      }
      if (tab.status !== 'approval') setTabStatus(tab, 'working');
    }
    if (tab.status === 'approval') return;
    if (tab.settleTimer) clearTimeout(tab.settleTimer);
    tab.settleTimer = setTimeout(function () {
      tab.settleTimer = 0;
      if (tab.closed || tab.exited) return;
      if (tab.status === 'working') setTabStatus(tab, 'done');
    }, 5000);
  }

  var projExpanded = storedMap('omnix.projExpanded');

  function saveProjExpanded() {
    storeSet('omnix.projExpanded', JSON.stringify(projExpanded));
  }
  var wsCollapsed = storeGet('omnix.workspaceCollapsed') === '1';

  function trackedByPath() {
    var map = {};
    var list = lastStatus && Array.isArray(lastStatus.workspaces) ? lastStatus.workspaces : [];
    for (var i = 0; i < list.length; i++) {
      if (list[i] && list[i].path) map[normPath(list[i].path)] = list[i];
    }
    return map;
  }

  function applyWorkspaceTitle() {
    var title = $('side-workspace');
    var root = (lastStatus && lastStatus.root) || appRoot || '';
    var saved = root ? storeGet('omnix.workspaceTitle:' + normPath(root)) : '';
    if (title && !title.isEditing) title.textContent = saved || baseName(root) || 'Workspace';
    if (title && root) title.title = root;

    var body = $('workspace-group-body');
    if (body) body.hidden = !!wsCollapsed;
    var btn = $('workspace-collapse');
    if (btn) {
      btn.setAttribute('aria-expanded', wsCollapsed ? 'false' : 'true');
      btn.title = wsCollapsed ? 'Expand workspace' : 'Collapse workspace';
      btn.setAttribute('aria-label', btn.title);
    }
  }

  function toggleWorkspace() {
    wsCollapsed = !wsCollapsed;
    storeSet('omnix.workspaceCollapsed', wsCollapsed ? '1' : '0');
    applyWorkspaceTitle();
  }

  function saveWorkspaceTitle() {
    var title = $('side-workspace');
    var root = (lastStatus && lastStatus.root) || appRoot || '';
    if (!title || !root) return;
    title.isEditing = false;
    var name = String(title.textContent || '').replace(/\s+/g, ' ').trim();
    if (!name) name = baseName(root) || 'Workspace';
    title.textContent = name;
    storeSet('omnix.workspaceTitle:' + normPath(root), name);
  }

  function renderProjects(list) {
    savedProjects = Array.isArray(list) ? list : [];
    var box = $('workspace-list');
    if (!box) return;
    var rows = savedProjects.slice();
    if (!rows.length) {
      box.textContent = '';
      box.appendChild(el('div', 'side-empty', 'No projects yet. Add the folders you work in.'));
      return;
    }
    box.textContent = '';
    busyMarks = [];
    var tracked = trackedByPath();

    /* Most recently used first, using the indexed timestamp when the path has
       history and the stored value otherwise. */
    rows.sort(function (a, b) {
      var sa = projectStats[(a || {}).path] || {};
      var sb = projectStats[(b || {}).path] || {};
      return Number(sb.updatedAt || (b || {}).updatedAt || 0) -
        Number(sa.updatedAt || (a || {}).updatedAt || 0);
    });

    if (projFilter) {
      rows = rows.filter(function (p) {
        var name = String((p && p.name) || '').toLowerCase();
        var path = String((p && p.path) || '').toLowerCase();
        return name.indexOf(projFilter) >= 0 || path.indexOf(projFilter) >= 0;
      });
    }

    var shownRows = 0;
    for (var i = 0; i < rows.length; i++) {
      let p = rows[i] || {};
      if (!p.path) continue;
      shownRows++;
      var stat = projectStats[p.path] || {};
      var ws = tracked[normPath(p.path)] || null;
      var sessions = ws && Array.isArray(ws.sessions) ? ws.sessions : [];
      var name = disambiguatedName(p.name || stat.name || baseName(p.path), p.path, savedProjects);
      let key = normPath(p.path);
      var expanded = !!projExpanded[key];
      var live = liveTabsAt(p.path);
      var marks = statusTabsAt(p.path);
      var running = sessions.slice();
      for (var l = 0; l < live.length; l++) {
        var lt = live[l];
        if (sessions.some(function (x) { return x.paneKey === lt.paneKey; })) continue;
        running.push({
          paneKey: lt.paneKey,
          agent: lt.agent || 'terminal',
          sessionId: lt.sessionId || lt.paneKey,
          cwd: lt.cwd || p.path,
          saved_at: Math.floor(Date.now() / 1000),
          live: true,
          title: lt.title
        });
      }

      var wrap = el('div', 'proj' + (live.length ? ' is-working' : '') +
        (key === activeScope ? ' is-active' : ''));
      var row = el('div', 'proj-row');
      row.tabIndex = 0;
      row.setAttribute('role', 'button');
      row.setAttribute('aria-expanded', expanded ? 'true' : 'false');
      row.title = p.path;

      var chev = el('span', 'proj-chev', '\u25b8');
      chev.setAttribute('aria-hidden', 'true');
      chev.classList.add('is-empty');
      if (running.length) {
        chev.classList.remove('is-empty');
        chev.setAttribute('role', 'button');
        chev.title = expanded ? 'Hide sessions' : 'Show sessions';
        chev.addEventListener('click', guard(function (ev) {
          if (ev && ev.stopPropagation) ev.stopPropagation();
          projExpanded[key] = !projExpanded[key];
          saveProjExpanded();
          renderProjects(savedProjects);
        }));
      }
      row.appendChild(chev);
      row.appendChild(iconNode('folder', 'proj-icon'));

      var body = el('div', 'proj-body');
      var top = el('div', 'proj-top');
      top.appendChild(el('span', 'ws-name', name));
      if (marks.length) {
        top.appendChild(trackBusy(
          workingGlyph(aggregateLabel(marks), aggregateStatus(marks)),
          function () { return statusTabsAt(p.path); }
        ));
      }
      if (ws && ws.sleeping) top.appendChild(el('span', 'ws-state sleeping', 'Sleeping'));
      body.appendChild(top);
      row.appendChild(body);

      var remove = el('button', 'icon-btn proj-remove');
      remove.type = 'button';
      remove.title = 'Remove from Sidebar';
      remove.setAttribute('aria-label', 'Remove ' + name + ' from the sidebar');
      remove.textContent = '\u00d7';
      row.appendChild(remove);

      remove.addEventListener('click', guard(function (ev) {
        if (ev && ev.stopPropagation) ev.stopPropagation();
        removeProject(p);
      }));
      row.addEventListener('click', guard(function () { openProject(p); }));
      row.addEventListener('keydown', guard(function (ev) {
        if (ev.key === 'Enter' || ev.key === ' ') {
          ev.preventDefault();
          openProject(p);
        } else if (ev.key === 'ArrowRight' && sessions.length && !projExpanded[key]) {
          ev.preventDefault();
          projExpanded[key] = true;
          saveProjExpanded();
          renderProjects(savedProjects);
        } else if (ev.key === 'ArrowLeft' && projExpanded[key]) {
          ev.preventDefault();
          projExpanded[key] = false;
          saveProjExpanded();
          renderProjects(savedProjects);
        }
      }));
      wrap.appendChild(row);

      if (running.length) {
        var kids = el('div', 'proj-kids');
        kids.hidden = !expanded;
        if (ws && ws.branch) kids.appendChild(el('div', 'proj-branch', ws.branch));
        for (var s = 0; s < running.length; s++) {
          (function (session) {
            var liveTab = byPaneKey(session.paneKey);
            var line = el('button', 'sess-row');
            line.type = 'button';
            line.title = session.live ? 'Running in this window'
              : (session.resumeCommand || session.sessionId || '');
            line.appendChild(iconNode(session.agent));
            var label = session.live
              ? (session.title || agentLabel(session.agent))
              : agentLabel(session.agent) + ' ' + shortId(session.sessionId);
            line.appendChild(el('span', 'inv-title', label));
            if (liveTab && liveTab.spawned && !liveTab.exited) {
              line.appendChild(trackBusy(
                workingGlyph(statusLabel(liveTab.status, 1), liveTab.status),
                function () { return liveTab ? [liveTab] : []; }
              ));
            }
            line.appendChild(el('span', 'proj-time', relTime(session.saved_at)));
            line.addEventListener('click', guard(function () {
              if (session.live && liveTab) activate(liveTab);
              else resumeSession(session);
            }));
            kids.appendChild(line);
          })(running[s]);
        }
        wrap.appendChild(kids);
      }

      box.appendChild(wrap);
    }
    if (projFilter && !shownRows) {
      box.appendChild(el('p', 'side-empty', 'No projects match.'));
    }
  }

  /* Live pty state drives the animated marks; a rebuild is cheap and preserves
     the expanded-project map. */
  function updateWorkingMarks() {
    if (savedProjects.length) renderProjects(savedProjects);
  }

  /* ---------- indexed history panel ---------- */

  /* Default to the whole index so the header count matches the status bar.
     The old key stored `workspace` by default, which hid ~46 sessions. */
  function storedScope() {
    var scope = storeGet('omnix.histScope2');
    return scope === 'workspace' || scope === 'project' ? scope : 'all';
  }

  function storedMap(key) {
    var raw = storeGet(key);
    if (!raw) return {};
    try {
      var value = JSON.parse(raw);
      return value && typeof value === 'object' ? value : {};
    } catch (err) {
      return {};
    }
  }

  var hist = {
    scope: storedScope(),
    project: storeGet('omnix.histProject') || '',
    agent: '',
    query: '',
    sort: storeGet('omnix.histSort') === 'asc' ? 'asc' : 'desc',
    collapsed: storedMap('omnix.histGroups'),
    groupKeys: [],
    scan: null,
    scannedAt: 0,
    sessions: [],
    total: null,
    lastSession: null,
    loading: false,
    scanError: null,
    listError: null
  };

  var historyTimer = null;

  function historyCount() {
    if (hist.total !== null && hist.total !== undefined) return Number(hist.total) || 0;
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
    renderSortButton();
    renderHistoryList();
  }

  function renderSortButton() {
    var btn = $('history-sort');
    if (!btn) return;
    btn.dataset.order = hist.sort;
    btn.title = hist.sort === 'asc'
      ? 'Sorted oldest first. Click for newest first.'
      : 'Sorted newest first. Click for oldest first.';
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

      var all = el('button', 'chip', 'All Agents');
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
    var time = $('history-time');
    if (!node) return;
    if (!hist.scan) {
      node.textContent = hist.loading ? 'Scanning history\u2026' : 'History not scanned';
      node.title = 'The index is saved locally. A rescan only reads transcripts that are new or changed.';
      if (time) time.textContent = '';
      return;
    }
    var updated = Number(hist.scan.files) || 0;
    var unchanged = Number(hist.scan.unchanged) || 0;
    var messages = fmtNum(hist.scan.messages || 0) + (unchanged ? ' new messages' : ' messages');
    node.textContent = 'History refreshed';
    node.title = 'Updated ' + fmtNum(updated) + ' files \u00b7 ' + messages +
      '. The index is saved locally; a rescan only reads transcripts that are new or changed.';
    if (time) time.textContent = relTime(hist.scannedAt) || 'just now';
  }

  function sortedSessions() {
    var out = hist.sessions.slice();
    out.sort(function (a, b) {
      var av = Number(a && a.updatedAt) || 0;
      var bv = Number(b && b.updatedAt) || 0;
      return hist.sort === 'asc' ? av - bv : bv - av;
    });
    return out;
  }

  function groupSessions(sessions) {
    var groups = [];
    var index = {};
    for (var i = 0; i < sessions.length; i++) {
      var s = sessions[i];
      var key = s && s.cwd ? normPath(s.cwd) : '';
      if (!Object.prototype.hasOwnProperty.call(index, key)) {
        index[key] = { key: key, name: key ? baseName(key) : 'No project', sessions: [] };
        groups.push(index[key]);
      }
      index[key].sessions.push(s);
    }

    var seen = {};
    for (var g = 0; g < groups.length; g++) {
      seen[groups[g].name] = (seen[groups[g].name] || 0) + 1;
    }
    if (groups.length > 1) {
      var items = groups.map(function (group) {
        return { path: group.key, name: group.name };
      });
      for (var h = 0; h < groups.length; h++) {
        if (seen[groups[h].name] < 2 || !groups[h].key) continue;
        groups[h].name = disambiguatedName(groups[h].name, groups[h].key, items);
      }
    }
    return groups;
  }

  function saveHistGroups() {
    storeSet('omnix.histGroups', JSON.stringify(hist.collapsed));
  }

  function toggleHistoryGroup(key) {
    if (key == null) return;
    if (hist.collapsed[key]) delete hist.collapsed[key];
    else hist.collapsed[key] = true;
    saveHistGroups();
    renderHistoryList();
  }

  function toggleAllHistoryGroups() {
    var keys = hist.groupKeys || [];
    if (!keys.length) return;
    var allClosed = true;
    for (var i = 0; i < keys.length; i++) {
      if (!hist.collapsed[keys[i]]) { allClosed = false; break; }
    }
    hist.collapsed = {};
    if (!allClosed) {
      for (var c = 0; c < keys.length; c++) hist.collapsed[keys[c]] = true;
    }
    saveHistGroups();
    renderHistoryList();
  }

  function syncHistoryCollapseAll(allClosed, hasGroups) {
    var btn = $('history-collapse-all');
    if (!btn) return;
    if (hasGroups && allClosed) btn.classList.add('all-closed');
    else btn.classList.remove('all-closed');
    btn.title = hasGroups && allClosed ? 'Expand All Groups' : 'Collapse All Groups';
    btn.setAttribute('aria-label', btn.title);
    btn.disabled = !hasGroups;
  }

  function renderHistoryList() {
    var list = $('history-list');
    if (!list) return;
    list.textContent = '';
    hist.groupKeys = [];

    if (hist.loading && !hist.sessions.length) {
      var loading = el('div', 'loading', 'Checking for new or changed sessions\u2026');
      loading.setAttribute('role', 'status');
      list.appendChild(loading);
      syncHistoryCollapseAll(false, false);
      return;
    }

    var failure = hist.scanError || hist.listError;
    if (failure) {
      var err = el('div', 'error', failure);
      err.setAttribute('role', 'alert');
      list.appendChild(err);
      list.appendChild(el('div', 'empty', 'Press Rescan to try again.'));
      syncHistoryCollapseAll(false, false);
      return;
    }

    if (!hist.sessions.length) {
      var empty = el('div', 'empty', 'No sessions indexed yet. Press Rescan.');
      list.appendChild(empty);
      syncHistoryCollapseAll(false, false);
      return;
    }

    var groups = groupSessions(sortedSessions());
    for (var g = 0; g < groups.length; g++) {
      (function (group) {
        hist.groupKeys.push(group.key);
        var closed = !!hist.collapsed[group.key];
        var section = el('section', 'hist-group');
        section.setAttribute('role', 'group');

        var head = el('button', 'hist-group-head');
        head.type = 'button';
        head.title = group.key || 'Sessions without a project folder';
        head.setAttribute('aria-expanded', closed ? 'false' : 'true');
        head.setAttribute('aria-label',
          (closed ? 'Expand ' : 'Collapse ') + group.name + ' sessions');
        var chev = el('span', 'hist-group-chev');
        chev.setAttribute('aria-hidden', 'true');
        chev.innerHTML = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="m9 6 6 6-6 6"/></svg>';
        head.appendChild(chev);
        head.appendChild(el('span', 'hist-group-name', group.name));
        head.appendChild(el('span', 'hist-group-count', fmtNum(group.sessions.length)));
        head.addEventListener('click', guard(function () { toggleHistoryGroup(group.key); }));
        section.appendChild(head);

        var rows = el('div', 'hist-sessions');
        rows.setAttribute('role', 'list');
        rows.hidden = closed;
        for (var i = 0; i < group.sessions.length; i++) {
          rows.appendChild(sessionCard(group.sessions[i]));
        }
        section.appendChild(rows);
        list.appendChild(section);
      })(groups[g]);
    }

    var total = historyCount();
    if (total > hist.sessions.length) {
      list.appendChild(el('div', 'hist-more mono',
        'Showing ' + fmtNum(hist.sessions.length) + ' of ' + fmtNum(total) +
        ' \u00b7 narrow the search to see the rest'));
    }

    var allClosed = true;
    for (var k = 0; k < hist.groupKeys.length; k++) {
      if (!hist.collapsed[hist.groupKeys[k]]) { allClosed = false; break; }
    }
    syncHistoryCollapseAll(allClosed, hist.groupKeys.length > 0);
  }

  /* history titles often start with <system-reminder> scaffolding, quotes, or
     markdown heading markers; the panel wants the sentence, not the markup */
  function cleanText(text) {
    return String(text == null ? '' : text)
      .replace(/<system-reminder>[\s\S]*?<\/system-reminder>/gi, ' ')
      .replace(/<[^>]{1,80}>/g, ' ')
      .replace(/^\s*["'\u201c\u201d`]+/, '')
      .replace(/^\s*(?:#{1,6}|>|[-*+]|\d+\.)\s+/, '')
      .replace(/\s+/g, ' ')
      .trim();
  }

  /* Model ids are often provider/vendor/name; the card shows the readable
     name and keeps the full id in the tooltip. */
  function shortModel(model) {
    var text = String(model || '');
    var cut = text.lastIndexOf('/');
    return cut >= 0 && cut < text.length - 1 ? text.slice(cut + 1) : text;
  }

  var sessionMenuNode = null;
  var sessionMenuTrigger = null;

  function closeSessionMenu() {
    if (sessionMenuNode && sessionMenuNode.parentNode) {
      sessionMenuNode.parentNode.removeChild(sessionMenuNode);
    }
    if (sessionMenuTrigger && sessionMenuTrigger.setAttribute) {
      sessionMenuTrigger.setAttribute('aria-expanded', 'false');
    }
    sessionMenuNode = null;
    sessionMenuTrigger = null;
  }

  function copyText(text, label) {
    var value = String(text == null ? '' : text);
    if (!value) {
      toast('warn', 'Nothing to copy for ' + label + '.');
      return;
    }
    function fallback() {
      var area = document.createElement('textarea');
      area.value = value;
      area.setAttribute('readonly', '');
      area.style.position = 'fixed';
      area.style.top = '-1000px';
      area.style.opacity = '0';
      document.body.appendChild(area);
      area.select();
      var ok = false;
      try { ok = document.execCommand('copy'); } catch (err) { ok = false; }
      document.body.removeChild(area);
      toast(ok ? 'ok' : 'error',
        ok ? label + ' copied' : 'Could not copy ' + label.toLowerCase());
    }
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(value).then(function () {
        toast('ok', label + ' copied');
      }, fallback);
    } else {
      fallback();
    }
  }

  var confirmResolve = null;

  function settleConfirm(value) {
    var modal = $('confirm-modal');
    if (modal) modal.hidden = true;
    var resolve = confirmResolve;
    confirmResolve = null;
    if (resolve) resolve(value);
  }

  function askConfirm(title, text, okLabel) {
    var modal = $('confirm-modal');
    if (!modal) return Promise.resolve(false);
    if (confirmResolve) settleConfirm(false);
    var titleNode = $('confirm-title');
    var textNode = $('confirm-text');
    var ok = $('confirm-ok');
    if (titleNode) titleNode.textContent = title || 'Confirm';
    if (textNode) textNode.textContent = text || '';
    if (ok) {
      ok.textContent = okLabel || 'Confirm';
      ok.classList.add('is-danger');
    }
    modal.hidden = false;
    if (ok && ok.focus) ok.focus();
    return new Promise(function (resolve) { confirmResolve = resolve; });
  }

  function openSession(s) {
    return resumeSession({
      paneKey: 'h:' + s.agent + ':' + s.sessionId,
      agent: s.agent,
      sessionId: s.sessionId,
      cwd: s.cwd,
      title: s.title
    });
  }

  function resumeInWorktree(s) {
    if (!s || !s.agent || !s.sessionId) {
      toast('warn', 'This session has no agent ID to resume.');
      return Promise.resolve(null);
    }
    if (!s.cwd) {
      toast('warn', 'This session has no working folder to branch from.');
      return Promise.resolve(null);
    }
    setLast('Creating worktree\u2026');
    return call('resumeWorktree', {
      agent: s.agent,
      sessionId: s.sessionId,
      cwd: s.cwd,
      title: s.title || ''
    }, { timeout: 120000 }).then(function (res) {
      if (!res || !res.worktree) return null;
      createTerminal({
        paneKey: 'hw:' + s.agent + ':' + s.sessionId + ':' + Date.now(),
        agent: s.agent,
        sessionId: s.sessionId,
        cwd: res.worktree,
        title: agentDisplay(s.agent) + ' \u00b7 ' + res.branch
      });
      toast('ok', 'Worktree ' + res.branch + ' ready');
      setLast('Worktree ready');
      return res;
    });
  }

  function revealSessionLog(s) {
    if (!s || !s.logPath) {
      toast('warn', 'No log path is indexed for this session.');
      return Promise.resolve(null);
    }
    var database = /\.(db|sqlite|sqlite3)$/i.test(s.logPath);
    return call('openPath', { path: s.logPath, folder: database }, { quiet: true })
      .then(function (res) {
        if (res) return res;
        if (s.cwd) return call('openPath', { path: s.cwd });
        toast('warn', 'The log is no longer on disk.');
        return null;
      });
  }

  function openWorkingDirectory(s) {
    if (!s || !s.cwd) {
      toast('warn', 'This session has no working folder.');
      return Promise.resolve(null);
    }
    return call('openPath', { path: s.cwd });
  }

  function deleteHistorySession(s) {
    var title = cleanText(s.title) || 'this session';
    return askConfirm('Remove session?',
      'Remove "' + title + '" from Omnix history? The agent transcript stays on ' +
      'disk, so a later rescan can index it again.', 'Remove').then(function (ok) {
      if (!ok) return null;
      return call('historyDeleteSession', {
        agent: s.agent,
        sessionId: s.sessionId
      }, { quiet: true }).then(function (res) {
        if (!res) {
          toast('error', 'Could not remove the session.');
          return null;
        }
        toast('ok', 'Session removed from history');
        return loadSessions().then(renderHistory);
      });
    });
  }

  function sessionMenuRow(label, hint, glyph, run, danger) {
    var row = menuRow(label, hint, glyph, function () {
      closeSessionMenu();
      return run();
    });
    if (danger) row.classList.add('is-danger');
    return row;
  }

  function openSessionMenu(trigger, s) {
    closeSessionMenu();
    var menu = el('div', 'menu sess-menu');
    menu.setAttribute('role', 'menu');
    menu.setAttribute('aria-label', 'Session actions');

    menu.appendChild(sessionMenuRow('Resume in Worktree', '', 'branch', function () {
      return resumeInWorktree(s);
    }));
    menu.appendChild(sessionMenuRow('Resume in New Chat', '', 'chat', function () {
      return openSession(s);
    }));
    menu.appendChild(sessionMenuRow('Copy Resume Command', '', 'copy', function () {
      copyText(s.resumeCommand, 'Resume command');
    }));
    menu.appendChild(sessionMenuRow('Hand off to agent', '', 'handoff', function () {
      return openHandoff(s);
    }));
    menu.appendChild(el('div', 'menu-sep'));
    menu.appendChild(sessionMenuRow('Reveal Log', '', 'file', function () {
      return revealSessionLog(s);
    }));
    menu.appendChild(sessionMenuRow('Open Working Directory', '', 'folder', function () {
      return openWorkingDirectory(s);
    }));
    menu.appendChild(sessionMenuRow('Copy Session ID', '', 'copy', function () {
      copyText(s.sessionId, 'Session ID');
    }));
    menu.appendChild(sessionMenuRow('Copy Log Path', '', 'copy', function () {
      copyText(s.logPath, 'Log path');
    }));
    menu.appendChild(el('div', 'menu-sep'));
    menu.appendChild(sessionMenuRow('Delete', '', 'trash', function () {
      return deleteHistorySession(s);
    }, true));

    document.body.appendChild(menu);
    var rect = trigger.getBoundingClientRect();
    var width = menu.offsetWidth || 232;
    var height = menu.offsetHeight || 300;
    var left = Math.min(rect.right - width, window.innerWidth - width - 8);
    left = Math.max(8, left);
    var top = rect.bottom + 4;
    if (top + height > window.innerHeight - 8) {
      top = Math.max(8, rect.top - height - 4);
    }
    menu.style.left = left + 'px';
    menu.style.top = top + 'px';
    sessionMenuNode = menu;
    sessionMenuTrigger = trigger;
    trigger.setAttribute('aria-expanded', 'true');
    var first = menu.querySelector('.menu-item');
    if (first && first.focus) first.focus();
    menu.addEventListener('keydown', function (ev) {
      var items = Array.prototype.slice.call(
        menu.querySelectorAll('.menu-item:not(:disabled)'));
      var at = items.indexOf(document.activeElement);
      if (ev.key === 'ArrowDown') {
        ev.preventDefault();
        if (items.length) items[Math.min(items.length - 1, at + 1)].focus();
      } else if (ev.key === 'ArrowUp') {
        ev.preventDefault();
        if (items.length) items[Math.max(0, at - 1)].focus();
      } else if (ev.key === 'Home') {
        ev.preventDefault();
        if (items.length) items[0].focus();
      } else if (ev.key === 'End') {
        ev.preventDefault();
        if (items.length) items[items.length - 1].focus();
      } else if (ev.key === 'Escape') {
        ev.preventDefault();
        closeSessionMenu();
        if (trigger.focus) trigger.focus();
      } else if (ev.key === 'Tab') {
        closeSessionMenu();
      }
    });
  }

  function sessionAction(kind, label, run) {
    var action = el('button', 'sess-act sess-act-' + kind);
    action.type = 'button';
    /* No native title: WebKitGTK renders it in a separate X window, and that
       window can sit over the very button it describes and eat the click. */
    action.setAttribute('aria-label', label);
    action.innerHTML = ICONS[kind] || '';
    action.addEventListener('click', guard(function (ev) {
      if (ev && ev.stopPropagation) ev.stopPropagation();
      return run(ev);
    }));
    return action;
  }

  function sessionCard(s) {
    var card = el('div', 'sess');
    card.setAttribute('role', 'listitem');
    card.tabIndex = 0;
    var titleText = cleanText(s.title) || '(untitled)';
    card.setAttribute('aria-label', 'Resume ' + titleText + ' from ' + (s.agent || 'agent'));

    var head = el('div', 'sess-head');
    head.appendChild(iconNode(s.agent));
    var title = el('span', 'sess-title', titleText);
    title.title = titleText;
    head.appendChild(title);

    var actions = el('div', 'sess-actions');
    actions.appendChild(sessionAction('branch', 'Resume in Worktree', function () {
      return resumeInWorktree(s);
    }));
    actions.appendChild(sessionAction('chat', 'Resume in New Chat', function () {
      return openSession(s);
    }));
    actions.appendChild(sessionAction('handoff', 'Hand off to agent', function () {
      return openHandoff(s);
    }));
    var more = sessionAction('more', 'More actions', function () {
      openSessionMenu(more, s);
    });
    more.setAttribute('aria-haspopup', 'menu');
    more.setAttribute('aria-expanded', 'false');
    actions.appendChild(more);
    head.appendChild(actions);
    card.appendChild(head);

    var preview = cleanText(s.preview);
    if (preview && titleText && preview.indexOf(titleText.slice(0, 40)) !== 0) {
      card.appendChild(el('div', 'sess-preview', preview));
    }

    var meta = el('div', 'sess-meta');
    meta.appendChild(el('span', 'sess-provider', agentDisplay(s.agent) || s.agent || '?'));
    var msgs = Number(s.messageCount) || 0;
    meta.appendChild(el('span', null, fmtNum(msgs) + (msgs === 1 ? ' msg' : ' msgs')));
    meta.appendChild(el('span', null, relTime(s.updatedAt) || 'unknown'));
    card.appendChild(meta);

    if (s.model || s.branch) {
      var foot = el('div', 'sess-foot');
      if (s.model) {
        var model = el('span', 'sess-model', shortModel(s.model));
        model.title = s.model;
        foot.appendChild(model);
      }
      if (s.branch) {
        var branch = el('span', 'sess-branch');
        branch.innerHTML = ICONS.branch;
        branch.appendChild(el('span', null, s.branch));
        branch.title = s.branch;
        foot.appendChild(branch);
      }
      card.appendChild(foot);
    }

    card.addEventListener('click', guard(function (ev) {
      if (ev.target && ev.target.closest && ev.target.closest('.sess-actions')) return;
      return openSession(s);
    }));
    card.addEventListener('keydown', guard(function (ev) {
      if (ev.key !== 'Enter' && ev.key !== ' ') return;
      if (ev.target && ev.target.closest && ev.target.closest('.sess-actions')) return;
      ev.preventDefault();
      return openSession(s);
    }));
    return card;
  }

  /* Picking a project row scopes the history panel to that folder, so the
     panel shows exactly the project the user just chose. */
  function scopeHistoryToProject(path) {
    path = String(path || '');
    if (!path) return Promise.resolve(null);
    if (hist.scope === 'project' && normPath(hist.project) === normPath(path)) {
      return Promise.resolve(null);
    }
    hist.scope = 'project';
    hist.project = path;
    storeSet('omnix.histScope2', 'project');
    storeSet('omnix.histProject', path);
    renderScopeChips();
    return loadSessions().then(renderHistory);
  }

  function loadSessions() {
    var args = { scope: hist.scope, limit: HISTORY_LIMIT };
    if (hist.scope === 'project') {
      args.cwd = hist.project || launchCwd() || appRoot;
    }
    if (hist.agent) args.agent = hist.agent;
    if (hist.query) args.query = hist.query;

    return rpc('historySessions', args).then(function (res) {
      hist.sessions = res && Array.isArray(res.sessions) ? res.sessions : [];
      hist.total = res && Number.isFinite(Number(res.total)) ? Number(res.total) : hist.sessions.length;
      if (hist.sessions.length) hist.lastSession = hist.sessions[0];
      hist.listError = null;
      setLast('History loaded');
      return res;
    }, function (err) {
      hist.sessions = [];
      hist.total = null;
      hist.listError = 'History listing failed: ' + message(err);
      toast('error', 'History sessions: ' + message(err));
      setLast('History could not load');
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
      setLast('History scan complete');
      renderHistory();
    }, function (err) {
      hist.scanError = 'History scan failed: ' + message(err);
      toast('error', 'History scan: ' + message(err));
      setLast('History scan failed');
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

  /* Sidebar search narrows the project list; Enter hands the query to the
     transcript index so one field covers both jobs. */
  function onSideSearch() {
    var input = $('side-search');
    projFilter = input ? String(input.value || '').trim().toLowerCase() : '';
    renderProjects(savedProjects);
  }

  function onSideSearchKey(ev) {
    var input = $('side-search');
    if (ev.key === 'Escape') {
      if (input) input.value = '';
      projFilter = '';
      renderProjects(savedProjects);
      return;
    }
    if (ev.key !== 'Enter') return;
    ev.preventDefault();
    var query = input ? String(input.value || '').trim() : '';
    openStatic('search');
    var field = $('search-query');
    if (field) field.value = query;
    if (query) runSearch(null);
  }

  /* ---------- command palette ---------- */

  function stopAgentBrowser() {
    call('browserStop').then(function (res) {
      if (res && res.stopped && res.session) {
        toast('ok', 'Sesi browser ' + res.session + ' ditutup' +
          (res.daemon ? '; Lightpanda bersama tetap berjalan' : ''));
      } else if (res && res.stopped) {
        toast('ok', 'Lightpanda bersama dihentikan');
      } else {
        toast('warn', 'Tidak ada sesi browser milik terminal ini.');
      }
    }).catch(function (err) {
      toast('error', 'Stop browser: ' + message(err));
    });
  }

  var paletteItems = [
    { label: 'New Terminal', hint: '', run: function () { createTerminal({ title: 'Terminal' }); } },
    { label: 'New Browser', hint: '', run: function () { openStatic('browser'); } },
    { label: 'New Agent Session', hint: '', run: function () { toggleTabMenu(); } },
    { label: 'Custom CLI', hint: '', run: openCmdModal },
    { label: 'Search History', hint: '', run: function () {
      openStatic('search');
      var input = $('search-query');
      if (input) input.focus();
    } },
    { label: 'Hand Off to Another Agent', hint: '', run: function () { openHandoff(null); } },
    { label: 'Docs', hint: '', run: function () { openStatic('docs'); } },
    { label: 'Agent Settings', hint: '', run: openAgentSettings },
    { label: 'About & Support', hint: 'GPL', run: function () { openSettings('about'); } },
    { label: 'Support Omnix on Tako', hint: 'Coffee', run: openSupport },
    { label: 'Setup Omnix Again', hint: 'Wizard', run: function () { openSetupAgain(); } },
    { label: 'Plugins', hint: 'API 1', run: openPluginsSettings },
    { label: 'Kembangkan Omnix', hint: 'DEV', run: openDevSettings },
    { label: 'Split Terminal', hint: 'Ctrl+Shift+D', run: splitActive },
    { label: 'Usage and Memory', hint: '', run: openUsage },
    { label: 'Stop Agent Browser', hint: 'Lightpanda', run: stopAgentBrowser },
    { label: 'Add Project Folder', hint: '', run: pickProject },
    { label: 'Add Project by Path', hint: '', run: openProjectModal },
    { label: 'Rescan History', hint: '', run: function () { return loadHistory(); } },
    { label: 'Resume Last Session', hint: '', run: resumeLastSession }
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

  function dynamicPaletteItems() {
    var out = [];
    for (var c = 0; c < pluginCommands.length; c++) {
      (function (row) {
        var name = String(row[0] || '');
        var title = String(row[1] || name);
        out.push({
          label: 'Plugin: ' + title,
          hint: name,
          run: function () { return runPluginCommand(name, title); }
        });
      })(pluginCommands[c]);
    }
    for (var p = 0; p < pluginPanels.length; p++) {
      (function (panel) {
        out.push({
          label: 'Panel: ' + (panel.title || panel.name),
          hint: panel.plugin + ':' + panel.name,
          run: function () { return openPluginPanel(panel.plugin, panel.name); }
        });
      })(pluginPanels[p]);
    }
    return out;
  }

  function paletteFilter(query) {
    var q = String(query == null ? '' : query).trim().toLowerCase();
    var items = paletteItems.concat(dynamicPaletteItems());
    paletteMatches = [];
    for (var i = 0; i < items.length; i++) {
      if (!q || items[i].label.toLowerCase().indexOf(q) >= 0) {
        paletteMatches.push(items[i]);
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

  /* Capture phase: xterm stops keydown at its textarea, so a bubble listener
     never sees app shortcuts while a terminal has focus. */
  function onGlobalKey(ev) {
    var key = ev && ev.key ? String(ev.key) : '';
    var mod = ev && (ev.ctrlKey || ev.metaKey);
    function take() {
      ev.preventDefault();
      if (ev.stopPropagation) ev.stopPropagation();
    }
    if (mod && (key === 'k' || key === 'K')) {
      take();
      if (paletteVisible()) paletteClose();
      else paletteOpen();
      return;
    }
    if (mod && ev.shiftKey && (key === 'p' || key === 'P')) {
      take();
      paletteOpen();
      return;
    }
    if (mod && ev.shiftKey && (key === 'd' || key === 'D')) {
      take();
      splitActive();
      return;
    }
    if (key === 'Escape') {
      var pickerMenu = $('handoff-target-menu');
      if (pickerMenu && !pickerMenu.hidden) { take(); closeHandoffPicker(); }
      else if (sessionMenuNode) { take(); closeSessionMenu(); }
      else if (!$('confirm-modal').hidden) { take(); settleConfirm(false); }
      else if (paletteVisible()) { take(); paletteClose(); }
      else if (!$('handoff-modal').hidden) { take(); hideModal($('handoff-modal')); }
      else if (!$('project-modal').hidden) { take(); hideModal($('project-modal')); }
      else if (!$('cmd-modal').hidden) { take(); hideModal($('cmd-modal')); }
      else if (!$('usage-modal').hidden) { take(); hideModal($('usage-modal')); }
      else if (!($('tab-new-menu') || {}).hidden) { take(); closeTabMenu(); }
    }
  }

  /* ---------- panel resize ---------- */

  /* The sidebar and history panel are px-wide CSS variables, so a drag only
     rewrites one custom property. The tabstrip brand reads the same variable,
     which keeps the window bar aligned with the sidebar at any width. */
  var PANEL_CFG = {
    side: { key: 'omnix.sideW', varName: '--side-w', min: 180, max: 420, def: 236 },
    hist: { key: 'omnix.histW', varName: '--hist-w', min: 220, max: 520, def: 292 }
  };

  function panelViewport() {
    var width = Number(window.innerWidth);
    return width > 0 ? width : 1240;
  }

  function panelStored(kind) {
    var width = Number(storeGet(PANEL_CFG[kind].key));
    return width > 0 ? width : 0;
  }

  function panelLimit(kind) {
    var cfg = PANEL_CFG[kind];
    var room = panelViewport() - (kind === 'side' ? 470 : 510);
    return Math.max(cfg.min, Math.min(cfg.max, room));
  }

  function setPanelWidth(kind, px) {
    var cfg = PANEL_CFG[kind];
    var width = Math.round(Number(px));
    if (!isFinite(width) || width <= 0) return 0;
    width = Math.max(cfg.min, Math.min(panelLimit(kind), width));
    var app = $('app');
    if (app && app.style) {
      if (typeof app.style.setProperty === 'function') {
        app.style.setProperty(cfg.varName, width + 'px');
      } else {
        app.style[cfg.varName] = width + 'px';
      }
    }
    return width;
  }

  function clearPanelWidth(kind) {
    var cfg = PANEL_CFG[kind];
    var app = $('app');
    if (app && app.style) {
      if (typeof app.style.removeProperty === 'function') app.style.removeProperty(cfg.varName);
      else delete app.style[cfg.varName];
    }
  }

  function applyStoredPanelWidth(kind) {
    var stored = panelStored(kind);
    if (stored) setPanelWidth(kind, stored);
  }

  function forgetPanelWidth(kind) {
    clearPanelWidth(kind);
    try {
      if (window.localStorage) window.localStorage.removeItem(PANEL_CFG[kind].key);
    } catch (err) { /* private mode */ }
  }

  function currentPanelWidth(kind) {
    var app = $('app');
    var inline = '';
    if (app && app.style && typeof app.style.getPropertyValue === 'function') {
      inline = app.style.getPropertyValue(PANEL_CFG[kind].varName);
    }
    var width = parseFloat(inline);
    if (width > 0) return width;
    var node = kind === 'side' ? $('sidebar') : $('agents-panel');
    var rect = node && node.getBoundingClientRect ? node.getBoundingClientRect() : null;
    return (rect && rect.width) || PANEL_CFG[kind].def;
  }

  function wirePanelResizer(kind) {
    var handle = $(kind + '-resize');
    if (!handle) return;
    var cfg = PANEL_CFG[kind];
    var startX = 0;
    var startW = 0;
    var last = 0;
    var frame = 0;
    var dragging = false;

    function paint(px) {
      last = setPanelWidth(kind, px);
    }

    function begin(ev) {
      if (ev && ev.button != null && ev.button !== 0) return;
      dragging = true;
      startX = Number(ev && ev.clientX) || 0;
      startW = currentPanelWidth(kind);
      last = startW;
      if (handle.classList) handle.classList.add('dragging');
      if (document.body && document.body.classList) document.body.classList.add('is-resizing');
      if (ev && ev.preventDefault) ev.preventDefault();
      window.addEventListener('pointermove', move);
      window.addEventListener('pointerup', stop);
      window.addEventListener('pointercancel', stop);
    }

    function move(ev) {
      if (!dragging) return;
      var delta = (Number(ev && ev.clientX) || 0) - startX;
      var next = kind === 'side' ? startW + delta : startW - delta;
      if (typeof window.requestAnimationFrame === 'function') {
        if (frame) window.cancelAnimationFrame(frame);
        frame = window.requestAnimationFrame(function () { frame = 0; paint(next); });
      } else {
        paint(next);
      }
    }

    function stop() {
      if (!dragging) return;
      dragging = false;
      if (frame) {
        if (typeof window.cancelAnimationFrame === 'function') window.cancelAnimationFrame(frame);
        frame = 0;
      }
      if (handle.classList) handle.classList.remove('dragging');
      if (document.body && document.body.classList) document.body.classList.remove('is-resizing');
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', stop);
      window.removeEventListener('pointercancel', stop);
      if (last) storeSet(cfg.key, String(last));
      refreshBusyDots();
    }

    handle.addEventListener('pointerdown', guard(begin));
    handle.addEventListener('dblclick', guard(function () { forgetPanelWidth(kind); }));
    handle.addEventListener('keydown', guard(function (ev) {
      var step = ev && ev.shiftKey ? 48 : 16;
      var dir = ev && ev.key === 'ArrowRight' ? 1 : ev && ev.key === 'ArrowLeft' ? -1 : 0;
      if (!dir) return;
      if (ev.preventDefault) ev.preventDefault();
      var next = kind === 'side'
        ? currentPanelWidth(kind) + dir * step
        : currentPanelWidth(kind) - dir * step;
      var width = setPanelWidth(kind, next);
      if (width) storeSet(cfg.key, String(width));
    }));
  }

  function wirePanelResizers() {
    wirePanelResizer('side');
    wirePanelResizer('hist');
    var resizeTimer = 0;
    window.addEventListener('resize', function () {
      if (resizeTimer) clearTimeout(resizeTimer);
      resizeTimer = setTimeout(function () {
        resizeTimer = 0;
        var app = $('app');
        if (!app || !app.classList) return;
        if (!app.classList.contains('side-collapsed')) applyStoredPanelWidth('side');
        if (!app.classList.contains('hist-collapsed')) applyStoredPanelWidth('hist');
      }, 120);
    });
    var app = $('app');
    if (!app || !app.classList || !app.classList.contains('side-collapsed')) applyStoredPanelWidth('side');
    if (app && app.classList && !app.classList.contains('hist-collapsed')) applyStoredPanelWidth('hist');
  }

  /* ---------- panel collapse ---------- */

  function setSideCollapsed(collapsed) {
    var app = $('app');
    var btn = $('side-toggle');
    if (app) {
      if (collapsed) app.classList.add('side-collapsed');
      else app.classList.remove('side-collapsed');
    }
    if (collapsed) clearPanelWidth('side');
    else applyStoredPanelWidth('side');
    if (btn) {
      btn.setAttribute('aria-expanded', collapsed ? 'false' : 'true');
      btn.title = collapsed ? 'Expand sidebar' : 'Collapse sidebar';
      btn.setAttribute('aria-label', btn.title);
    }
    storeSet('omnix.sideCollapsed', collapsed ? '1' : '0');
  }

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
    if (collapsed) clearPanelWidth('hist');
    else applyStoredPanelWidth('hist');
    if (rail) {
      rail.setAttribute('aria-expanded', collapsed ? 'false' : 'true');
      rail.setAttribute('aria-current', collapsed ? 'false' : 'true');
    }
    storeSet('omnix.histCollapsed', collapsed ? '1' : '0');
    if (!collapsed) {
      ensureHistory();
      loadWorkers();
    }
  }

  /* Index work starts only while the panel is open. Saved rows render first,
     so a restart shows the existing index before the incremental rescan ends. */
  function ensureHistory() {
    if (hist.loading) return null;
    if (!hist.scan) {
      var ready = hist.sessions.length ? null : loadSessions().then(renderHistory);
      var scan = loadHistory();
      return ready ? Promise.all([ready, scan]) : scan;
    }
    if (!hist.sessions.length) return loadSessions().then(renderHistory);
    return null;
  }

  function setHistoryFilter(open) {
    var box = $('history-agents');
    var btn = $('history-filter');
    if (box) box.hidden = !open;
    if (btn) {
      btn.setAttribute('aria-expanded', open ? 'true' : 'false');
      btn.title = open ? 'Hide Agent Filter' : 'Filter Agents';
      btn.setAttribute('aria-label', btn.title);
    }
    storeSet('omnix.histFilter', open ? '1' : '0');
  }

  function toggleHistorySort() {
    hist.sort = hist.sort === 'asc' ? 'desc' : 'asc';
    storeSet('omnix.histSort', hist.sort);
    renderSortButton();
    renderHistoryList();
  }

  /* ---------- workers / orchestrator ---------- */

  var workersQueued = [];
  var workerSendTarget = null;

  function workerAgentMeta(agent) {
    var node = el('span', 'worker-meta');
    var badge = profileBadge(agent, 'worker-profile');
    if (badge) node.appendChild(badge);
    node.appendChild(document.createTextNode(agent || 'shell'));
    return node;
  }

  function workerRow(tab) {
    var row = el('div', 'worker' + (tab.orchestrator ? ' is-orchestrator' : ''));
    row.setAttribute('role', 'listitem');
    var dot = el('span');
    paintStatus(dot, tab.status || 'idle', statusLabel(tab.status || 'idle', 1));
    row.appendChild(dot);
    var label = tab.handoff || tab.title || tab.paneKey;
    if (tab.parent) label = '\u21b3 ' + label;
    var name = el('span', 'worker-name', label);
    name.title = label;
    row.appendChild(name);
    row.appendChild(workerAgentMeta(tab.agent || 'shell'));
    row.appendChild(el('span', 'worker-state', tab.orchestrator
      ? 'orchestrator'
      : statusLabel(tab.status || 'idle', 1)));

    var open = el('button', 'worker-btn', 'open');
    open.type = 'button';
    open.title = 'Open this pane';
    open.addEventListener('click', guard(function () { activate(tab); }));
    row.appendChild(open);

    if (tab.agent) {
      var send = el('button', 'worker-btn', 'send');
      send.type = 'button';
      send.title = 'Send a message to this worker';
      send.addEventListener('click', guard(function () { openWorkerSend(tab); }));
      row.appendChild(send);
    }
    var close = el('button', 'worker-btn', 'close');
    close.type = 'button';
    close.title = 'Close this pane';
    close.addEventListener('click', guard(function () { closeTab(tab); }));
    row.appendChild(close);
    return row;
  }

  function queuedRow(record) {
    var row = el('div', 'worker is-queued');
    row.setAttribute('role', 'listitem');
    var dot = el('span');
    paintStatus(dot, 'idle', 'queued');
    row.appendChild(dot);
    var name = el('span', 'worker-name', record.name || 'handoff');
    name.title = record.queued_reason || 'queued';
    row.appendChild(name);
    row.appendChild(workerAgentMeta(record.agent || ''));
    row.appendChild(el('span', 'worker-state', 'queued'));
    var close = el('button', 'worker-btn', 'close');
    close.type = 'button';
    close.title = 'Close this handoff';
    close.addEventListener('click', guard(function () {
      call('handoffClose', { name: record.name }).then(function (res) {
        if (res) {
          toast('ok', 'Handoff closed: ' + record.name);
          loadWorkers();
        }
      });
    }));
    row.appendChild(close);
    return row;
  }

  function renderWorkers() {
    var section = $('workers');
    var box = $('workers-list');
    if (!section || !box) return;
    box.textContent = '';
    var rows = [];
    for (var i = 0; i < tabs.length; i++) {
      var tab = tabs[i];
      if (tab.kind !== 'terminal' || tab.closed) continue;
      if (!tab.handoff && !tab.orchestrator && !tab.parent) continue;
      rows.push(tab);
    }
    rows.sort(function (a, b) {
      if (!!a.orchestrator !== !!b.orchestrator) return a.orchestrator ? -1 : 1;
      return String(a.handoff || a.title).localeCompare(String(b.handoff || b.title));
    });
    for (var j = 0; j < rows.length; j++) box.appendChild(workerRow(rows[j]));
    for (var k = 0; k < workersQueued.length; k++) box.appendChild(queuedRow(workersQueued[k]));
    section.hidden = rows.length === 0 && workersQueued.length === 0;
  }

  function loadWorkers() {
    return call('handoffList', {}, { quiet: true }).then(function (res) {
      var list = res && Array.isArray(res.handoffs) ? res.handoffs : [];
      workersQueued = list.filter(function (record) {
        return !record.closed && record.outcome === 'queued';
      });
      renderWorkers();
      return list;
    });
  }

  function openWorkerSend(tab) {
    workerSendTarget = tab;
    var title = $('agent-send-title');
    var target = $('agent-send-target');
    var text = $('agent-send-text');
    var note = $('agent-send-note');
    if (target) target.textContent = tab.handoff || tab.title || tab.paneKey;
    if (note) {
      note.textContent = tab.status === 'approval'
        ? 'Pane is waiting for approval; answer it first.'
        : '';
    }
    if (text) text.value = '';
    showModal($('agent-send-modal'));
    if (text && text.focus) text.focus();
  }

  function sendToWorker() {
    if (!workerSendTarget) return;
    var text = $('agent-send-text');
    var body = text ? text.value : '';
    if (!body.trim()) {
      toast('warn', 'Message is empty.');
      return;
    }
    var tab = workerSendTarget;
    call('agentSend', {
      target: tab.paneKey,
      text: body,
      force: false
    }).then(function (res) {
      if (!res) return;
      hideModal($('agent-send-modal'));
      toast('ok', 'Message sent to ' + (tab.handoff || tab.title));
    });
  }

  function previewGc() {
    call('gc', { dryRun: true }).then(function (res) {
      if (!res) return;
      var lines = Array.isArray(res.actions) ? res.actions : [];
      var box = $('gc-actions');
      if (box) box.textContent = lines.length ? lines.join('\n') : 'Nothing to clean.';
      showModal($('gc-modal'));
    });
  }

  function runGc() {
    call('gc', { dryRun: false }).then(function (res) {
      if (!res) return;
      hideModal($('gc-modal'));
      toast('ok', 'Bersihkan selesai: ' +
        (Array.isArray(res.actions) ? res.actions.length : 0) + ' langkah');
      loadWorkers();
    });
  }

  /* ---------- events from host ---------- */

  EVENTS.ptyData = function (payload) {
    var tab = byPaneKey(payload.paneKey);
    if (!tab) return;
    var bytes = b64ToBytes(payload.dataBase64);
    if (!bytes) return;
    noteOutput(tab, bytesText(bytes));
    if (tab.handoffSend) {
      var send = tab.handoffSend;
      tab.handoffSend = null;
      setTimeout(send, 400);
    }
    if (!tab.term) return;
    tab.term.write(bytes);
  };

  EVENTS.ptyExit = function (payload) {
    var tab = byPaneKey(payload.paneKey);
    if (!tab || tab.exited) return;
    tab.exited = true;
    tab.exitedCode = payload.code == null ? null : payload.code;
    tab.spawned = false;
    tab.handoffSend = null;
    if (tab.settleTimer) {
      clearTimeout(tab.settleTimer);
      tab.settleTimer = 0;
    }
    var code = payload.code == null ? '?' : payload.code;
    if (tab.term) {
      tab.term.write('\r\n\x1b[38;5;244m[pty exit ' + code + ']\x1b[0m\r\n');
    }
    tab.title = tab.title + ' (exited)';
    setTabStatus(tab, code === 0 ? 'done' : 'failed');
    renderTabs();
    refreshBusyDots();
    setLast('Terminal exited (code ' + code + ')');
  };

  EVENTS.paneOpen = function (payload) {
    if (!payload || !payload.paneKey || byPaneKey(payload.paneKey)) return;
    var tab = createTerminal({
      paneKey: payload.paneKey,
      attach: true,
      title: payload.title || payload.handoff || payload.label || payload.agent,
      agent: payload.agent || null,
      cwd: payload.cwd || null,
      handoff: payload.handoff || null,
      parent: payload.parent || null,
      orchestrator: !!payload.orchestrator
    });
    if (!tab) return;
    tab.status = payload.status || 'working';
    paintTabStatus(tab);
    publishPane(tab);
    renderWorkers();
    toast('ok', 'Worker pane opened: ' + (payload.handoff || payload.title || payload.paneKey));
  };

  EVENTS.paneClosed = function (payload) {
    var tab = payload && byPaneKey(payload.paneKey);
    if (tab && !tab.closed) closeTab(tab);
  };

  EVENTS.browserNav = function (payload) {
    if (payload.url) {
      browser.url = String(payload.url);
      var input = $('browser-url');
      if (input) input.value = browser.url;
    }
    if (payload.open && browser.url) {
      browser.engine = 'native';
      var engine = $('browser-engine');
      if (engine) engine.value = 'native';
      openStatic('browser');
      setTimeout(function () { openBrowserNative(); }, 40);
    }
    if (payload.title) setLast('Browser: ' + payload.title);
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
    for (var i = 0; i < agentsInfo.length; i++) {
      var opt = el('option', null, agentDisplay(agentsInfo[i]));
      opt.value = agentsInfo[i].name;
      select.appendChild(opt);
    }
    select.value = fallback || '';
  }

  function ensureDocs() {
    var frame = $('docs-frame');
    if (frame && !frame.getAttribute('src')) frame.setAttribute('src', '/tutorial.html');
  }

  function wire() {
    wirePanelResizers();
    fillAgents($('search-agent'), 'any agent', '');
    fillAgentPicker('opencode2');

    var newProject = $('new-project');
    if (newProject) newProject.addEventListener('click', guard(pickProject));

    var workspaceRefresh = $('workspace-refresh');
    if (workspaceRefresh) workspaceRefresh.addEventListener('click', guard(refreshStatus));

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
        createTerminal({ title: 'Terminal' });
      }));
    }

    var emptyBrowser = $('pane-empty-browser');
    if (emptyBrowser) {
      emptyBrowser.addEventListener('click', guard(function () {
        openStatic('browser');
      }));
    }

    var projectAddIcon = $('project-add-icon');
    if (projectAddIcon) projectAddIcon.addEventListener('click', guard(pickProject));

    var sideSettings = $('side-settings');
    if (sideSettings) sideSettings.addEventListener('click', guard(openAgentSettings));
    var sideSupport = $('side-support');
    if (sideSupport) sideSupport.addEventListener('click', guard(openSupport));

    var setupNextBtn = $('setup-next');
    if (setupNextBtn) setupNextBtn.addEventListener('click', guard(setupNext));
    var setupBackBtn = $('setup-back');
    if (setupBackBtn) setupBackBtn.addEventListener('click', guard(setupBack));
    var setupSkipBtn = $('setup-skip');
    if (setupSkipBtn) setupSkipBtn.addEventListener('click', guard(setupSkip));
    var setupBrowseBtn = $('setup-browse');
    if (setupBrowseBtn) setupBrowseBtn.addEventListener('click', guard(setupBrowse));
    var setupSupportBtn = $('setup-support');
    if (setupSupportBtn) setupSupportBtn.addEventListener('click', guard(openSupport));
    var setupProjectInput = $('setup-project');
    if (setupProjectInput) setupProjectInput.addEventListener('keydown', guard(function (ev) {
      if (ev.key === 'Enter') {
        ev.preventDefault();
        setupNext();
      }
    }));

    var aboutSupport = $('about-support');
    if (aboutSupport) aboutSupport.addEventListener('click', guard(openSupport));
    var aboutRepo = $('about-repo');
    if (aboutRepo) aboutRepo.addEventListener('click', guard(openRepo));
    var aboutSetup = $('about-setup');
    if (aboutSetup) aboutSetup.addEventListener('click', guard(openSetupAgain));

    var winMin = $('win-min');
    if (winMin) winMin.addEventListener('click', guard(function () {
      /* A popup left open would otherwise outlive the window it belongs to. */
      closeHandoffPicker();
      closeSessionMenu();
      call('windowMinimize', {});
    }));

    window.addEventListener('blur', function () {
      closeHandoffPicker();
      closeSessionMenu();
    });
    document.addEventListener('visibilitychange', function () {
      if (!document.hidden) return;
      closeHandoffPicker();
      closeSessionMenu();
    });
    window.addEventListener('resize', function () {
      closeHandoffPicker();
      closeSessionMenu();
    });
    var winMax = $('win-max');
    if (winMax) winMax.addEventListener('click', guard(function () {
      call('windowMaximize', {});
    }));
    var winClose = $('win-close');
    if (winClose) winClose.addEventListener('click', guard(function () {
      call('windowClose', {});
    }));

    /* The tab strip is the window title bar: empty space drags the window,
       double-click maximizes, and real controls keep their own clicks. */
    var strip = $('tabstrip');
    if (strip) {
      strip.addEventListener('mousedown', guard(function (ev) {
        if (ev.button !== 0 || !chromeDrag(ev.target)) return;
        call('windowDrag', {}, { quiet: true });
      }));
      strip.addEventListener('dblclick', guard(function (ev) {
        if (!chromeDrag(ev.target)) return;
        call('windowMaximize', {}, { quiet: true });
      }));
    }

    var edges = document.querySelectorAll ? document.querySelectorAll('#resize-edges .rz') : [];
    for (var edge = 0; edge < edges.length; edge++) {
      edges[edge].addEventListener('mousedown', guard(function (ev) {
        if (ev.button !== 0) return;
        ev.preventDefault();
        call('windowResize', { dir: ev.currentTarget.dataset.dir }, { quiet: true });
      }));
    }

    var appVersion = $('app-version');
    if (appVersion) appVersion.addEventListener('click', guard(function () {
      checkUpdates(true);
    }));
    var updateChip = $('update-chip');
    if (updateChip) updateChip.addEventListener('click', guard(installUpdate));

    setSideCollapsed(storeGet('omnix.sideCollapsed') === '1');

    var sideToggle = $('side-toggle');
    if (sideToggle) sideToggle.addEventListener('click', guard(function () {
      var app = $('app');
      setSideCollapsed(!(app && app.classList.contains('side-collapsed')));
    }));

    var sideSearchBox = $('side-search-box');
    if (sideSearchBox) sideSearchBox.addEventListener('click', function () {
      var app = $('app');
      if (!app || !app.classList.contains('side-collapsed')) return;
      setSideCollapsed(false);
      var input = $('side-search');
      if (input) {
        input.value = '';
        projFilter = '';
        renderProjects(savedProjects);
        if (input.focus) input.focus();
      }
    });

    var sideSearch = $('side-search');
    if (sideSearch) {
      sideSearch.addEventListener('input', guard(onSideSearch));
      sideSearch.addEventListener('keydown', guard(onSideSearchKey));
    }

    var usageOpen = $('usage-open');
    if (usageOpen) usageOpen.addEventListener('click', guard(openUsage));
    var usageStripRefresh = $('usage-strip-refresh');
    if (usageStripRefresh) usageStripRefresh.addEventListener('click', guard(function () {
      return refreshUsage();
    }));
    var usageRefresh = $('usage-refresh');
    if (usageRefresh) usageRefresh.addEventListener('click', guard(function () {
      refreshMemory();
      return refreshUsage();
    }));
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
    if (settingsRefresh) settingsRefresh.addEventListener('click', guard(loadSettingsSection));

    var settingsBack = $('settings-back');
    if (settingsBack) settingsBack.addEventListener('click', guard(function () {
      var tab = tabByKind('settings');
      if (tab) closeTab(tab);
    }));

    var settingsSearch = $('settings-search');
    if (settingsSearch) settingsSearch.addEventListener('input', guard(settingsSearchChanged));

    var settingsNavs = document.querySelectorAll('.settings-nav-item[data-settings]');
    for (var sn = 0; sn < settingsNavs.length; sn++) {
      (function (node) {
        if (!node || !node.dataset) return;
        node.addEventListener('click', guard(function () {
          setSettingsSection(node.dataset.settings);
        }));
      })(settingsNavs[sn]);
    }

    var pluginInstall = $('plugin-install-btn');
    if (pluginInstall) pluginInstall.addEventListener('click', guard(function () {
      var source = $('plugin-install-source');
      return installPlugin(source ? source.value : '');
    }));
    var pluginInstallSource = $('plugin-install-source');
    if (pluginInstallSource) {
      pluginInstallSource.addEventListener('keydown', guard(function (ev) {
        if (ev.key !== 'Enter') return;
        ev.preventDefault();
        return installPlugin(pluginInstallSource.value);
      }));
    }
    var pluginSearch = $('plugin-search-btn');
    if (pluginSearch) pluginSearch.addEventListener('click', guard(function () {
      return searchPluginCatalog();
    }));
    var pluginSearchInput = $('plugin-search');
    if (pluginSearchInput) {
      pluginSearchInput.addEventListener('keydown', guard(function (ev) {
        if (ev.key !== 'Enter') return;
        ev.preventDefault();
        return searchPluginCatalog(pluginSearchInput.value);
      }));
    }
    var pluginDoctor = $('plugin-doctor-btn');
    if (pluginDoctor) pluginDoctor.addEventListener('click', guard(function () {
      return rpc('pluginAction', { action: 'doctor', name: '', names: [] }, 60000).then(function (res) {
        showPluginOutput('Plugin doctor', res, 'Pemeriksaan manifest dan izin');
        return res;
      }, function (err) {
        showPluginError('Plugin doctor', err);
        return null;
      });
    }));

    var devTask = $('dev-task-btn');
    if (devTask) devTask.addEventListener('click', guard(createDevTask));
    var devBuild = $('dev-build-btn');
    if (devBuild) devBuild.addEventListener('click', guard(function () {
      return devJob('Build release', 'devBuild', {});
    }));
    var devTest = $('dev-test-btn');
    if (devTest) devTest.addEventListener('click', guard(function () {
      return devJob('Test', 'devTest', {});
    }));
    var devRun = $('dev-run-btn');
    if (devRun) devRun.addEventListener('click', guard(function () {
      return devJob('Jalankan build', 'devRun', {});
    }));
    var devRestore = $('dev-restore-btn');
    if (devRestore) devRestore.addEventListener('click', guard(function () {
      return rpc('devRestore', {}, 60000).then(function (res) {
        if (!res) return null;
        toast('ok', res.message || 'Binary sebelumnya dikembalikan. Restart Omnix.');
        return loadDev();
      }, function (err) {
        toast('error', 'Kembalikan: ' + message(err));
        return null;
      });
    }));

    var pluginPanelClose = $('plugin-panel-close');
    if (pluginPanelClose) pluginPanelClose.addEventListener('click', guard(closePluginPanel));
    var pluginPanelOk = $('plugin-panel-ok');
    if (pluginPanelOk) pluginPanelOk.addEventListener('click', guard(closePluginPanel));
    var pluginPanelModal = $('plugin-panel-modal');
    if (pluginPanelModal) pluginPanelModal.addEventListener('click', guard(function (ev) {
      if (ev && ev.target === pluginPanelModal) closePluginPanel();
    }));
    var pluginOutputClose = $('plugin-output-close');
    if (pluginOutputClose) pluginOutputClose.addEventListener('click', guard(function () {
      hideModal($('plugin-output-modal'));
    }));
    var pluginOutputOk = $('plugin-output-ok');
    if (pluginOutputOk) pluginOutputOk.addEventListener('click', guard(function () {
      hideModal($('plugin-output-modal'));
    }));
    var pluginOutputModal = $('plugin-output-modal');
    if (pluginOutputModal) pluginOutputModal.addEventListener('click', guard(function (ev) {
      if (ev && ev.target === pluginOutputModal) hideModal($('plugin-output-modal'));
    }));

    var perms = $('agent-default-perms');
    if (perms) perms.addEventListener('change', guard(function () {
      var full = perms.value === 'full';
      return rpc('agentDefaultsSave', { fullAccess: full }).then(function (res) {
        if (!res) {
          if (agentSettingsData && agentSettingsData.defaults) {
            perms.value = agentSettingsData.defaults.fullAccess ? 'full' : 'ask';
          }
          return null;
        }
        if (agentSettingsData) {
          agentSettingsData.defaults = { fullAccess: full };
        }
        renderAgentSettings(agentSettingsData);
        toast('ok', full ? 'New agents start with full access' : 'New agents ask before acting');
        return res;
      });
    }));

    var wsCollapse = $('workspace-collapse');
    if (wsCollapse) wsCollapse.addEventListener('click', guard(toggleWorkspace));

    var wsTitle = $('side-workspace');
    if (wsTitle) {
      wsTitle.addEventListener('focus', guard(function () {
        wsTitle.isEditing = true;
      }));
      wsTitle.addEventListener('blur', guard(saveWorkspaceTitle));
      wsTitle.addEventListener('keydown', guard(function (ev) {
        if (ev.key === 'Enter') {
          ev.preventDefault();
          if (wsTitle.blur) wsTitle.blur();
          else saveWorkspaceTitle();
        } else if (ev.key === 'Escape') {
          ev.preventDefault();
          wsTitle.isEditing = false;
          applyWorkspaceTitle();
          if (wsTitle.blur) wsTitle.blur();
        }
      }));
    }

    var stripCmd = $('strip-command');
    if (stripCmd) stripCmd.addEventListener('click', guard(paletteOpen));


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

    var handoffTargetButton = $('handoff-target');
    if (handoffTargetButton) {
      handoffTargetButton.addEventListener('click', guard(function (ev) {
        if (ev && ev.stopPropagation) ev.stopPropagation();
        toggleHandoffPicker();
      }));
      handoffTargetButton.addEventListener('keydown', guard(function (ev) {
        if (ev.key !== 'ArrowDown' && ev.key !== 'ArrowUp') return;
        ev.preventDefault();
        openHandoffPicker();
      }));
    }
    var handoffTargetMenu = $('handoff-target-menu');
    if (handoffTargetMenu) {
      handoffTargetMenu.addEventListener('keydown', guard(handoffPickerKeydown));
    }

    var confirmOk = $('confirm-ok');
    if (confirmOk) confirmOk.addEventListener('click', guard(function () {
      settleConfirm(true);
    }));
    var confirmCancel = $('confirm-cancel');
    if (confirmCancel) confirmCancel.addEventListener('click', guard(function () {
      settleConfirm(false);
    }));
    var confirmClose = $('confirm-close');
    if (confirmClose) confirmClose.addEventListener('click', guard(function () {
      settleConfirm(false);
    }));
    var confirmModal = $('confirm-modal');
    if (confirmModal) confirmModal.addEventListener('click', guard(function (ev) {
      if (ev && ev.target === confirmModal) settleConfirm(false);
    }));

    document.addEventListener('click', function (ev) {
      var picker = $('handoff-target-picker');
      if (picker && ev.target && !picker.contains(ev.target)) closeHandoffPicker();
      if (sessionMenuNode && ev.target && !sessionMenuNode.contains(ev.target)) {
        closeSessionMenu();
      }
    });

    var sendClose = $('agent-send-close');
    if (sendClose) sendClose.addEventListener('click', guard(function () {
      hideModal($('agent-send-modal'));
    }));
    var sendCancel = $('agent-send-cancel');
    if (sendCancel) sendCancel.addEventListener('click', guard(function () {
      hideModal($('agent-send-modal'));
    }));
    var sendOk = $('agent-send-ok');
    if (sendOk) sendOk.addEventListener('click', guard(sendToWorker));
    var sendModal = $('agent-send-modal');
    if (sendModal) sendModal.addEventListener('click', guard(function (ev) {
      if (ev && ev.target === sendModal) hideModal(sendModal);
    }));

    var gcPreview = $('gc-preview');
    if (gcPreview) gcPreview.addEventListener('click', guard(previewGc));
    var gcClose = $('gc-close');
    if (gcClose) gcClose.addEventListener('click', guard(function () { hideModal($('gc-modal')); }));
    var gcCancel = $('gc-cancel');
    if (gcCancel) gcCancel.addEventListener('click', guard(function () { hideModal($('gc-modal')); }));
    var gcRun = $('gc-run');
    if (gcRun) gcRun.addEventListener('click', guard(runGc));
    var workersRefresh = $('workers-refresh');
    if (workersRefresh) workersRefresh.addEventListener('click', guard(function () {
      loadWorkers();
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
      if ($('palette') && !$('palette').hidden) return;
      var tag = (ev.target && ev.target.tagName || '').toLowerCase();
      if (tag === 'input' || tag === 'textarea' || tag === 'select') return;
      /* Menus, pickers and modals own their arrow keys; pulling focus back to
         the pty here made keyboard navigation impossible. */
      if (ev.target && ev.target.closest &&
          ev.target.closest('[role="menu"], [role="listbox"], .modal, .menu, .picker-menu')) return;
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

    var historyFilter = $('history-filter');
    if (historyFilter) historyFilter.addEventListener('click', guard(function () {
      var box = $('history-agents');
      setHistoryFilter(!(box && !box.hidden));
    }));

    var historySort = $('history-sort');
    if (historySort) historySort.addEventListener('click', guard(toggleHistorySort));

    var collapseAll = $('history-collapse-all');
    if (collapseAll) collapseAll.addEventListener('click', guard(toggleAllHistoryGroups));

    var scopeBox = $('history-scope');
    if (scopeBox && scopeBox.children) {
      for (var s = 0; s < scopeBox.children.length; s++) {
        (function (chip) {
          var scope = chip.dataset ? chip.dataset.scope : null;
          if (!scope) return;
          chip.addEventListener('click', guard(function () {
            if (hist.scope === scope) return;
            hist.scope = scope;
            storeSet('omnix.histScope2', scope);
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

    document.addEventListener('keydown', guard(onGlobalKey), true);

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
    var histCollapsed = storeGet('omnix.histCollapsed');
    setHistoryCollapsed(histCollapsed === null ? true : histCollapsed === '1');
    setHistoryFilter(storeGet('omnix.histFilter') === '1');
    renderHistory();

    refreshAppInfo().then(function () { checkUpdates(false); });
    refreshStatus().then(refreshProjects);
    startMemoryPoll();
    startWorkingSpin();
    setInterval(function () { checkUpdates(false); }, 6 * 60 * 60 * 1000);
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', boot);
  } else {
    boot();
  }
})();
