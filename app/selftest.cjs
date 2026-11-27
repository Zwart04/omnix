/* Smoke test for app.js: stubs the WebView, drives reply/emit, asserts no throw.
   Covers rpc shape, reply/emit resilience, historyScan/historySessions render,
   historyProjects tree, scope + agent chips, session resume, palette, tab strip. */
'use strict';

const assert = require('node:assert');

function fakeEl(tag) {
  let text = '';
  const node = {
    tagName: tag,
    children: [],
    className: '',
    value: '',
    checked: false,
    hidden: false,
    title: '',
    disabled: false,
    type: '',
    id: '',
    dataset: {},
    attrs: {},
    style: {},
    handlers: {},
    parentNode: null,
    classList: {
      set: new Set(),
      add(c) { this.set.add(c); },
      remove(c) { this.set.delete(c); },
      toggle(c, force) {
        const on = force === undefined ? !this.set.has(c) : !!force;
        if (on) this.set.add(c); else this.set.delete(c);
        return on;
      },
      contains(c) { return this.set.has(c); }
    },
    appendChild(child) {
      if (child.parentNode && child.parentNode !== node) {
        child.parentNode.children = child.parentNode.children.filter((x) => x !== child);
      }
      child.parentNode = node;
      node.children.push(child);
      return child;
    },
    removeChild(child) {
      node.children = node.children.filter((x) => x !== child);
      child.parentNode = null;
      return child;
    },
    insertBefore(child, before) {
      const at = node.children.indexOf(before);
      node.children.splice(at < 0 ? node.children.length : at, 0, child);
      child.parentNode = node;
      return child;
    },
    addEventListener(type, fn) { node.handlers[type] = fn; },
    setAttribute(key, val) { node.attrs[key] = String(val); },
    getAttribute(key) {
      return Object.prototype.hasOwnProperty.call(node.attrs, key) ? node.attrs[key] : null;
    },
    removeAttribute(key) { delete node.attrs[key]; },
    querySelector() { return null; },
    focus() { global.document.activeElement = node; },
    getBoundingClientRect() { return { x: 10, y: 20, width: 400, height: 300 }; }
  };
  // browser semantics: assigning textContent replaces every child
  Object.defineProperty(node, 'textContent', {
    get() { return text; },
    set(value) {
      text = value == null ? '' : String(value);
      node.children.length = 0;
    },
    enumerable: true
  });
  return node;
}

const byId = new Map();
[
  'app', 'app-version', 'sidebar', 'workspace-list', 'tab-list', 'panes', 'tabstrip',
  'conflict-banner',
  'side-resize', 'hist-resize',
  'tabstrip-tabs', 'new-project', 'pane-empty', 'pane-empty-terminal', 'pane-empty-browser',
  'project-modal', 'project-close', 'project-cancel', 'project-add', 'project-path', 'project-error',
  'tab-new', 'tab-new-menu', 'strip-command', 'tab-new-terminal', 'tab-new-browser',
  'browser-url', 'browser-open', 'browser-engine', 'browser-native', 'browser-text',
  'search-form', 'search-query', 'search-conversation', 'search-agent', 'search-results', 'search-meta',
  'handoff-form', 'handoff-name', 'handoff-agent', 'handoff-base', 'handoff-prompt', 'handoff-result',
  'docs-frame', 'toasts', 'statusbar', 'status-left', 'update-chip',
  'history-list', 'history-search', 'history-meta', 'history-count', 'history-rescan',
  'history-agents', 'history-scope', 'history-collapse', 'history-expand', 'agents-panel',
  'history-filter', 'history-sort', 'history-collapse-all', 'history-time',
  'project-tree', 'project-refresh', 'project-add-icon', 'side-search', 'side-toggle', 'side-settings',
  'side-support', 'rail-agents',
  'win-min', 'win-max', 'win-close',
  'side-workspace', 'workspace-group', 'workspace-group-body', 'workspace-collapse',
  'workspace-refresh',
  'usage-chip', 'usage-open', 'usage-strip', 'usage-strip-refresh', 'usage-modal', 'usage-close',
  'usage-refresh', 'usage-list', 'usage-note',
  'memory-chip', 'memory-strip', 'memory-modal', 'memory-close', 'memory-refresh',
  'memory-total', 'memory-ai', 'memory-list', 'memory-ram',
  'handoff-modal', 'handoff-close', 'handoff-cancel', 'handoff-start',
  'handoff-target', 'handoff-source-title', 'handoff-source-sub', 'handoff-cwd',
  'fallback-modal', 'fallback-close', 'fallback-cancel', 'fallback-save',
  'fallback-agent', 'fallback-command', 'fallback-auto', 'fallback-error', 'fallback-sub',
  'team-modal', 'team-close', 'team-cancel', 'team-save', 'team-start', 'team-add-member',
  'team-members', 'team-orchestrator', 'team-on-quota', 'team-fallback-btn',
  'team-project', 'team-task', 'team-error',
  'workers', 'workers-list', 'workers-refresh', 'gc-preview',
  'agent-send-modal', 'agent-send-title', 'agent-send-target', 'agent-send-text',
  'agent-send-note', 'agent-send-close', 'agent-send-cancel', 'agent-send-ok',
  'gc-modal', 'gc-title', 'gc-close', 'gc-actions', 'gc-cancel', 'gc-run',
  'agent-settings', 'agent-settings-meta', 'agent-settings-refresh',
  'agent-settings-path', 'agent-default-perms', 'settings-back', 'settings-search',
  'settings-agents', 'settings-plugins', 'settings-dev', 'settings-about',
  'about-version', 'about-setup-state', 'about-support', 'about-repo', 'about-setup',
  'setup-modal', 'setup-step-0', 'setup-step-1', 'setup-step-2', 'setup-step-3',
  'setup-dot-0', 'setup-dot-1', 'setup-dot-2', 'setup-dot-3',
  'setup-project', 'setup-project-error', 'setup-browse', 'setup-agents',
  'setup-agents-note', 'setup-support', 'setup-step-note',
  'setup-skip', 'setup-back', 'setup-next',
  'plugin-install-source', 'plugin-install-btn', 'plugin-search', 'plugin-search-btn',
  'plugin-doctor-btn', 'plugin-catalog-note', 'plugin-list', 'plugin-catalog',
  'plugin-panel-modal', 'plugin-panel-title', 'plugin-panel-note', 'plugin-panel-frame',
  'plugin-panel-source', 'plugin-panel-close', 'plugin-panel-ok',
  'plugin-output-modal', 'plugin-output-title', 'plugin-output-pre',
  'plugin-output-note', 'plugin-output-close', 'plugin-output-ok',
  'dev-brief', 'dev-agent', 'dev-task-btn', 'dev-task-meta',
  'dev-build-btn', 'dev-test-btn', 'dev-run-btn', 'dev-restore-btn',
  'dev-output', 'dev-tasks', 'dev-builds',
  'cmd-modal', 'cmd-close', 'cmd-cancel', 'cmd-start', 'cmd-line', 'cmd-cwd', 'cmd-error',
  'palette', 'palette-input', 'palette-list'
].forEach((id) => byId.set(id, fakeEl(id)));

byId.get('palette').hidden = true;

// scope chips ship in index.html; the stub rebuilds that initial DOM
['workspace', 'project', 'all'].forEach((scope) => {
  const chip = fakeEl('button');
  chip.className = 'seg-item';
  chip.dataset.scope = scope;
  byId.get('history-scope').appendChild(chip);
});

const paneByKind = {
  browser: fakeEl('section'),
  search: fakeEl('section'),
  handoff: fakeEl('section'),
  docs: fakeEl('section'),
  settings: fakeEl('section')
};

const panelButtons = ['search', 'handoff', 'docs', 'settings'].map((panel) => {
  const node = fakeEl('button');
  node.dataset.panel = panel;
  return node;
});

const settingsNavButtons = ['agents', 'plugins', 'dev', 'about'].map((section) => {
  const node = fakeEl('button');
  node.dataset.settings = section;
  if (section === 'agents') node.classList.add('is-active');
  return node;
});

const posted = [];
const docHandlers = {};

const storage = new Map();
storage.set('omnix.sideW', '310');
storage.set('omnix.histW', '340');

global.window = {
  localStorage: {
    getItem(key) { return storage.has(key) ? storage.get(key) : null; },
    setItem(key, value) { storage.set(key, String(value)); },
    removeItem(key) { storage.delete(key); }
  },
  ipc: { postMessage(raw) { posted.push(JSON.parse(raw)); } },
  atob(value) { return Buffer.from(value, 'base64').toString('binary'); },
  TextDecoder: class FakeDecoder {
    decode(bytes) { return Buffer.from(bytes).toString('utf8'); }
  },
  addEventListener() {},
  Terminal: class FakeTerminal {
    constructor() {
      this.cols = 80;
      this.rows = 24;
      this.dataHandlers = [];
      this.resizeHandlers = [];
      this.written = [];
    }
    open() {}
    loadAddon() {}
    onData(fn) { this.dataHandlers.push(fn); }
    onResize(fn) { this.resizeHandlers.push(fn); }
    write(data) { this.written.push(data); }
    dispose() {}
  },
  FitAddon: { FitAddon: class FakeFit { fit() {} } }
};

global.document = {
  readyState: 'complete',
  activeElement: null,
  getElementById(id) { return byId.get(id) || null; },
  createElement(tag) { return fakeEl(tag); },
  createTextNode(text) { return { textContent: text }; },
  querySelector(sel) {
    const match = /data-kind="([a-z]+)"/.exec(sel);
    return match ? (paneByKind[match[1]] || null) : null;
  },
  querySelectorAll(sel) {
    if (sel.indexOf('.panel-btn') === 0) return panelButtons;
    if (sel.indexOf('.settings-nav-item') === 0) return settingsNavButtons;
    return [];
  },
  addEventListener(type, fn, capture) { docHandlers[type] = fn; docHandlers[type + ':capture'] = !!capture; }
};
document.activeElement = byId.get('strip-command');

function all(node, pred, out = []) {
  if (!node) return out;
  if (pred(node)) out.push(node);
  for (const child of node.children || []) all(child, pred, out);
  return out;
}

function byClass(id, cls) {
  return all(byId.get(id), (n) => String(n.className).split(/\s+/).includes(cls));
}

function noop() {}

function tick() {
  return new Promise((r) => setImmediate(r));
}

function lastReq(cmd) {
  for (let i = posted.length - 1; i >= 0; i--) {
    if (posted[i].cmd === cmd) return posted[i];
  }
  return null;
}

require('./app.js');

(async function () {
  // ---- rpc contract and boot order ----
  assert.ok(window.__omnix && typeof window.__omnix.reply === 'function', 'reply exposed');
  assert.ok(typeof window.__omnix.emit === 'function', 'emit exposed');

  assert.deepStrictEqual(
    posted.map((r) => r.cmd).slice(0, 2),
    ['appInfo', 'status'],
    'boot sends appInfo and status only; the index walk waits for the panel'
  );
  posted.forEach((req) => {
    assert.strictEqual(typeof req.id, 'number', 'numeric id');
    assert.strictEqual(typeof req.cmd, 'string', 'string cmd');
    assert.strictEqual(typeof req.args, 'object', 'object args');
  });
  assert.ok(!posted.some((r) => r.cmd === 'ptySpawn'), 'no terminal spawned at boot');

  const [infoReq, statusReq] = posted;

  window.__omnix.reply(infoReq.id, true,
    '{"version":"0.1.0","root":"/repo","engine":{"lightpanda":true,"version":"9.9"},' +
    '"agents":[{"name":"codex","bin":"codex"},{"name":"claude","bin":"claude"}],' +
    '"pluginCommands":[["hello:hello","Hello"]],' +
    '"pluginPanels":[{"plugin":"hello","name":"status","title":"Status"}],' +
    '"setup":{"needed":false,"complete":true,"completedAt":1700000000}}');
  window.__omnix.reply(statusReq.id, true,
    '{"sessions":0,"messages":0,"dbBytes":0,"workspaces":[]}');
  await tick();

  const bootProjectsReq = lastReq('projects');
  assert.ok(bootProjectsReq, 'boot lists the saved projects');
  window.__omnix.reply(bootProjectsReq.id, true, JSON.stringify({
    projects: [
      { path: '/home/you/code/omnix', name: 'omnix' },
      { path: '/home/you/code/demo-app', name: 'demo-app' }
    ]
  }));
  await tick();

  // saved projects and history settle together, then the sidebar renders once
  const bootHistReq = lastReq('historyProjects');
  assert.ok(bootHistReq, 'history fills in session stats');
  window.__omnix.reply(bootHistReq.id, true, JSON.stringify({
    root: '/repo',
    projects: [
      {
        path: '/home/you/code/omnix', name: 'omnix', sessions: 12, messages: 3400,
        updatedAt: Math.floor(Date.now() / 1000) - 7200, agents: ['claude', 'codex']
      },
      {
        path: '/home/you/code/demo-app', name: 'demo-app', sessions: 108, messages: 44000,
        updatedAt: Math.floor(Date.now() / 1000) - 120, agents: ['claude', 'codex', 'opencode']
      },
      {
        path: '/tmp/not-a-project', name: 'ghost', sessions: 3,
        updatedAt: Math.floor(Date.now() / 1000) - 60, agents: ['codex']
      }
    ]
  }));
  await tick();

  const bootRows = byClass('workspace-list', 'proj-row');
  assert.strictEqual(bootRows.length, 2, 'only saved projects render, never every indexed folder');
  assert.strictEqual(byId.get('app').style['--side-w'], '310px',
    'the saved sidebar width applies at boot');
  assert.strictEqual(all(bootRows[0], (n) => n.className === 'ws-name')[0].textContent, 'demo-app',
    'projects sorted by indexed updatedAt desc');
  assert.strictEqual(all(bootRows[1], (n) => n.className === 'ws-name')[0].textContent, 'omnix',
    'older project second');
  const bootIcons = byClass('workspace-list', 'icon');
  assert.strictEqual(bootIcons.length, 2, 'one folder mark per saved project');
  assert.ok(bootIcons.every((n) => /^<svg viewBox=/.test(n.innerHTML || '')), 'icons are inline SVGs');
  // one workspace title, editable and collapsible; history only decorates projects
  assert.strictEqual(byId.get('side-workspace').textContent, 'repo', 'sidebar names the root');
  byId.get('workspace-collapse').handlers.click(noop);
  assert.strictEqual(byId.get('workspace-group-body').hidden, true, 'workspace collapses');
  byId.get('workspace-collapse').handlers.click(noop);
  assert.strictEqual(byId.get('workspace-group-body').hidden, false, 'workspace expands');

  // sidebar search narrows the project list without touching saved state
  byId.get('side-search').value = 'demo';
  byId.get('side-search').handlers.input(noop);
  assert.strictEqual(byClass('workspace-list', 'proj-row').length, 1, 'search filters projects');
  byId.get('side-search').handlers.keydown({ key: 'Escape' });
  assert.strictEqual(byClass('workspace-list', 'proj-row').length, 2, 'escape clears the filter');

  // the left sidebar collapses to an icon rail and comes back
  byId.get('side-toggle').handlers.click(noop);
  assert.ok(byId.get('app').classList.contains('side-collapsed'), 'sidebar collapses to icons');
  byId.get('side-toggle').handlers.click(noop);
  assert.ok(!byId.get('app').classList.contains('side-collapsed'), 'sidebar expands again');

  byId.get('workspace-refresh').handlers.click(noop);
  await tick();
  const wsReq = posted[posted.length - 1];
  assert.strictEqual(wsReq.cmd, 'status', 'workspace refresh asks for status');
  window.__omnix.reply(wsReq.id, true, JSON.stringify({
    root: '/repo', sessions: 0, messages: 0, dbBytes: 0,
    workspaces: [
      { path: '/home/you/code/omnix', branch: 'main', sleeping: false, sessions: [
        { paneKey: 'x:1', agent: 'codex', sessionId: 'sess-1', cwd: '/home/you/code/omnix',
          resumeCommand: 'codex resume sess-1', saved_at: Math.floor(Date.now() / 1000) - 90 }
      ] },
      { path: '/tmp/not-a-project', branch: 'ghost', sleeping: true, sessions: [] }
    ]
  }));
  await tick();
  assert.strictEqual(byClass('workspace-list', 'proj-row').length, 2,
    'unpinned tracked folders never join the sidebar');
  const projChev = byClass('workspace-list', 'proj-chev')
    .find((n) => !n.classList.contains('is-empty'));
  assert.ok(projChev, 'tracked sessions expose a twisty');
  projChev.handlers.click({ stopPropagation: noop });
  assert.strictEqual(byClass('workspace-list', 'proj-branch')[0].textContent, 'main',
    'expanded project shows its branch');
  assert.strictEqual(byClass('workspace-list', 'sess-row').length, 1,
    'project expands into its tracked session');
  assert.strictEqual(
    JSON.parse(window.localStorage.getItem('omnix.projExpanded'))['/home/you/code/omnix'],
    true, 'expanded project state persists');

  // removing a project only touches the saved list
  const removeBtn = byClass('workspace-list', 'proj-remove')[0];
  assert.ok(removeBtn, 'every project row has a remove control');
  removeBtn.handlers.click({ stopPropagation: noop });
  await tick();
  const removeReq = posted[posted.length - 1];
  assert.strictEqual(removeReq.cmd, 'projectRemove', 'remove asks the host');
  assert.deepStrictEqual(removeReq.args, { path: '/home/you/code/demo-app' }, 'remove sends the path');
  window.__omnix.reply(removeReq.id, true, JSON.stringify({
    projects: [{ path: '/home/you/code/omnix', name: 'omnix' }]
  }));
  await tick();
  assert.strictEqual(byClass('workspace-list', 'proj-row').length, 1, 'row disappears after remove');

  // opening the agents panel starts the index walk
  byId.get('rail-agents').handlers.click(noop());
  await tick();
  const scanReq = posted.filter((r) => r.cmd === 'historyScan').pop();
  assert.ok(scanReq, 'expanding Agents starts the scan');
  window.__omnix.reply(scanReq.id, true, JSON.stringify({
    files: 290,
    messages: 44846,
    skipped: 0,
    unchanged: 288,
    db: '/repo/.omnix/history.sqlite',
    agents: [
      { agent: 'codex', sessions: 14, messages: 3540 },
      { agent: 'claude', sessions: 108, messages: 22000 },
      { agent: 'opencode', sessions: 168, messages: 19306 }
    ],
    roots: [{ agent: 'codex', path: '/home/you/.codex/sessions' }]
  }));
  await tick();

  assert.strictEqual(byId.get('status-left').textContent, 'repo', 'status bar names the workspace');
  assert.match(byId.get('app-version').textContent, /v0\.1\.0/, 'version rendered in the sidebar footer');

  // ---- historySessions: scope arg, count, cards ----
  const sessionsReq = posted[posted.length - 1];
  assert.strictEqual(sessionsReq.cmd, 'historySessions', 'scan is followed by historySessions');
  assert.deepStrictEqual(sessionsReq.args, { scope: 'all', limit: 500 }, 'default scope is all');

  window.__omnix.reply(sessionsReq.id, true, JSON.stringify({
    scope: 'all',
    root: '/repo',
    total: 290,
    sessions: [
      {
        agent: 'claude', sessionId: 'uuid-1', title: 'wire indexed history',
        cwd: '/repo', updatedAt: Math.floor(Date.now() / 1000) - 420,
        messageCount: 299, resumeCommand: 'claude --resume uuid-1',
        model: 'claude/opus-4', branch: 'master',
        preview: 'add a right panel that lists indexed sessions'
      },
      {
        agent: 'codex', sessionId: 'uuid-2', title: 'fix pty resize',
        cwd: '/repo', updatedAt: Math.floor(Date.now() / 1000) - 9000,
        messageCount: 41, resumeCommand: 'codex resume uuid-2',
        preview: 'terminal rows drop when the window shrinks'
      },
      {
        agent: 'opencode', sessionId: 'uuid-3', title: 'sidebar tree',
        cwd: '/repo/tools/repo', updatedAt: Math.floor(Date.now() / 1000) - 300000,
        messageCount: 7, resumeCommand: 'opencode --session uuid-3',
        preview: 'project tree rows'
      }
    ]
  }));
  await tick();

  assert.strictEqual(byId.get('history-count').textContent, '290 sessions', 'session count text');
  assert.strictEqual(byId.get('history-meta').textContent, 'History refreshed',
    'status line stays short like the reference panel');
  assert.match(byId.get('history-meta').title,
    /^Updated 290 files \u00b7 44,846 new messages/,
    'scan detail lives in the status tooltip');
  assert.ok(byId.get('history-time').textContent, 'scan time renders on the right');

  const groups = byClass('history-list', 'hist-group');
  assert.strictEqual(groups.length, 2, 'sessions group by project');
  assert.deepStrictEqual(
    groups.map((g) => all(g, (n) => n.className === 'hist-group-name')[0].textContent),
    ['repo', 'repo \u00b7 tools'],
    'same-basename folders stay separate groups and name their parent');
  assert.strictEqual(
    all(groups[0], (n) => n.className === 'hist-group-count')[0].textContent, '2',
    'group header carries its session count');

  // group collapse is a real toggle and survives a redraw
  let groupHead = all(groups[0], (n) => n.className === 'hist-group-head')[0];
  groupHead.handlers.click(noop());
  let sessionsBox = all(byClass('history-list', 'hist-group')[0],
    (n) => n.className === 'hist-sessions')[0];
  assert.strictEqual(sessionsBox.hidden, true, 'group collapses');
  assert.strictEqual(
    JSON.parse(window.localStorage.getItem('omnix.histGroups'))['/repo'], true,
    'collapsed group state is saved');
  groupHead = all(byClass('history-list', 'hist-group')[0],
    (n) => n.className === 'hist-group-head')[0];
  groupHead.handlers.click(noop());
  sessionsBox = all(byClass('history-list', 'hist-group')[0],
    (n) => n.className === 'hist-sessions')[0];
  assert.strictEqual(sessionsBox.hidden, false, 'group expands again');

  // collapse-all collapses every group, then restores them
  byId.get('history-collapse-all').handlers.click(noop());
  let heads = byClass('history-list', 'hist-group-head');
  assert.deepStrictEqual(heads.map((h) => h.getAttribute('aria-expanded')),
    ['false', 'false'], 'collapse all closes every group');
  byId.get('history-collapse-all').handlers.click(noop());
  heads = byClass('history-list', 'hist-group-head');
  assert.deepStrictEqual(heads.map((h) => h.getAttribute('aria-expanded')),
    ['true', 'true'], 'collapse all toggles back to expanded');

  // newest/oldest sort flips card order
  byId.get('history-sort').handlers.click(noop());
  assert.strictEqual(byId.get('history-sort').dataset.order, 'asc', 'sort flips to oldest');
  let titles = byClass('history-list', 'sess-title').map((n) => n.textContent);
  assert.strictEqual(titles[titles.length - 1], 'wire indexed history',
    'oldest first puts the freshest session last');
  assert.strictEqual(window.localStorage.getItem('omnix.histSort'), 'asc', 'sort order persists');
  byId.get('history-sort').handlers.click(noop());
  titles = byClass('history-list', 'sess-title').map((n) => n.textContent);
  assert.strictEqual(titles[0], 'wire indexed history', 'newest first is the default');

  // agent filter row is hidden until the funnel asks for it
  assert.strictEqual(byId.get('history-agents').hidden, true, 'agent filter starts hidden');
  byId.get('history-filter').handlers.click(noop());
  assert.strictEqual(byId.get('history-agents').hidden, false, 'funnel reveals the agent filter');
  assert.strictEqual(window.localStorage.getItem('omnix.histFilter'), '1', 'filter state persists');
  byId.get('history-filter').handlers.click(noop());

  const agentChips = byId.get('history-agents').children;
  assert.strictEqual(agentChips.length, 4, 'All agents plus one chip per scanned agent');
  assert.strictEqual(agentChips[1].textContent, 'codex 14', 'agent chip carries its count');
  assert.strictEqual(agentChips[2].textContent, 'claude 108', 'agent chip carries its count');

  const cards = byClass('history-list', 'sess');
  assert.strictEqual(cards.length, 3, 'one card per session');
  const metaRow = all(cards[0], (n) => n.className === 'sess-meta')[0];
  assert.deepStrictEqual(metaRow.children.map((c) => c.textContent),
    ['Claude Code', '299 msgs', '7m ago'], 'card meta row');
  const foot = all(cards[0], (n) => n.className === 'sess-foot')[0];
  assert.ok(foot, 'card has a model/branch footer');
  assert.strictEqual(all(foot, (n) => n.className === 'sess-model')[0].textContent, 'opus-4',
    'model name is shortened to its last segment');
  const branchChip = all(foot, (n) => n.className === 'sess-branch')[0];
  assert.strictEqual(all(branchChip, (n) => n !== branchChip && n.tagName === 'span')[0].textContent,
    'master', 'branch chip carries the branch');


  // ---- scope chip sends the right scope ----
  const scopeChips = byId.get('history-scope').children;
  scopeChips[1].handlers.click(noop());
  await tick();
  let req = posted[posted.length - 1];
  assert.strictEqual(req.cmd, 'historySessions', 'scope switch reloads sessions');
  assert.strictEqual(req.args.scope, 'project', 'project chip sends scope project');
  assert.strictEqual(req.args.cwd, '/repo', 'project scope scopes to the active folder');
  window.__omnix.reply(req.id, true, '{"scope":"project","root":"/repo","total":0,"sessions":[]}');
  await tick();
  assert.strictEqual(byClass('history-list', 'empty').length, 1, 'empty state after empty result');

  scopeChips[2].handlers.click(noop());
  await tick();
  req = posted[posted.length - 1];
  assert.strictEqual(req.args.scope, 'all', 'all chip sends scope all');
  assert.strictEqual(window.localStorage.getItem('omnix.histScope2'), 'all',
    'scope choice persists across restarts');
  window.__omnix.reply(req.id, true, JSON.stringify({
    scope: 'all',
    root: '/repo',
    total: 1,
    sessions: [{
      agent: 'codex', sessionId: 'uuid-9', title: 'resume me', cwd: '/repo',
      updatedAt: Math.floor(Date.now() / 1000) - 60, messageCount: 12,
      resumeCommand: 'codex resume uuid-9', preview: 'preview'
    }]
  }));
  await tick();

  // ---- agent chip filters ----
  byId.get('history-agents').children[1].handlers.click(noop());
  await tick();
  req = posted[posted.length - 1];
  assert.strictEqual(req.args.agent, 'codex', 'agent chip sends agent filter');
  window.__omnix.reply(req.id, true, '{"scope":"all","root":"/repo","total":0,"sessions":[]}');
  await tick();
  assert.strictEqual(byId.get('history-count').textContent, '0 sessions',
    'summary follows the agent filter, not the global scan');
  byId.get('history-agents').children[0].handlers.click(noop());
  await tick();
  req = posted[posted.length - 1];
  assert.ok(!('agent' in req.args), 'All agents clears the filter');
  window.__omnix.reply(req.id, true, JSON.stringify({
    scope: 'all',
    root: '/repo',
    total: 1,
    sessions: [{
      agent: 'codex', sessionId: 'uuid-9', title: 'resume me', cwd: '/repo',
      updatedAt: Math.floor(Date.now() / 1000) - 60, messageCount: 12,
      resumeCommand: 'codex resume uuid-9', preview: 'preview'
    }]
  }));
  await tick();

  // ---- debounced search ----
  byId.get('history-search').value = 'resize';
  byId.get('history-search').handlers.input(noop());
  await new Promise((r) => setTimeout(r, 260));
  req = posted[posted.length - 1];
  assert.strictEqual(req.cmd, 'historySessions', 'search reloads sessions');
  assert.strictEqual(req.args.query, 'resize', 'debounced query forwarded');
  window.__omnix.reply(req.id, true, '{"scope":"all","root":"/repo","total":0,"sessions":[]}');
  await tick();
  assert.strictEqual(byId.get('history-count').textContent, '0 sessions',
    'summary follows the search result count');
  assert.strictEqual(byClass('history-list', 'empty').length, 1,
    'search miss shows the empty state');

  // ---- Resume button spawns pty with agent + sessionId ----
  byId.get('history-search').value = '';
  byId.get('history-search').handlers.input(noop());
  await new Promise((r) => setTimeout(r, 260));
  req = posted[posted.length - 1];
  window.__omnix.reply(req.id, true, JSON.stringify({
    scope: 'all',
    root: '/repo',
    total: 1,
    sessions: [{
      agent: 'claude', sessionId: 'uuid-1', title: 'wire indexed history', cwd: '/repo',
      updatedAt: Math.floor(Date.now() / 1000) - 420, messageCount: 299,
      resumeCommand: 'claude --resume uuid-1', preview: 'preview'
    }]
  }));
  await tick();

  const resumeBtn = byClass('history-list', 'sess')[0];
  assert.ok(resumeBtn, 'session card renders');
  resumeBtn.handlers.click({ target: resumeBtn });
  const spawn = posted[posted.length - 1];
  assert.strictEqual(spawn.cmd, 'ptySpawn', 'resume spawns a pty');
  assert.strictEqual(spawn.args.agent, 'claude', 'spawn carries the agent');
  assert.strictEqual(spawn.args.sessionId, 'uuid-1', 'spawn carries the session id');
  assert.strictEqual(spawn.args.paneKey, 'h:claude:uuid-1', 'spawn carries a stable pane key');
  window.__omnix.reply(spawn.id, true, '{"paneKey":"h:claude:uuid-1","pid":77}');
  await tick();

  // ---- tab strip create and close ----
  assert.strictEqual(byId.get('tabstrip-tabs').children.length, 1, 'tab strip shows the new tab');
  const stripClose = all(byId.get('tabstrip-tabs'), (n) => n.className === 'strip-close')[0];
  stripClose.handlers.click({ stopPropagation: noop() });
  await tick();
  const kill = posted[posted.length - 1];
  assert.strictEqual(kill.cmd, 'ptyKill', 'closing a tab kills its pty');
  assert.strictEqual(byId.get('tabstrip-tabs').children.length, 0, 'tab strip emptied');
  window.__omnix.reply(kill.id, true, '{"ok":true}');
  await tick();

  // ---- a closed panel tab can be reopened from the rail ----
  panelButtons[0].handlers.click(noop());
  assert.strictEqual(byId.get('tabstrip-tabs').children.length, 1, 'rail opens a panel tab');
  all(byId.get('tabstrip-tabs'), (n) => n.className === 'strip-close')[0]
    .handlers.click({ stopPropagation: noop() });
  await tick();
  assert.strictEqual(byId.get('tabstrip-tabs').children.length, 0, 'panel tab closed');
  panelButtons[0].handlers.click(noop());
  assert.strictEqual(byId.get('tabstrip-tabs').children.length, 1,
    'panel pane survives its tab being closed');
  all(byId.get('tabstrip-tabs'), (n) => n.className === 'strip-close')[0]
    .handlers.click({ stopPropagation: noop() });
  await tick();

  // ---- palette: open, filter, enter ----
  byId.get('strip-command').handlers.click(noop());
  assert.strictEqual(byId.get('palette').hidden, false, 'palette opens from the rail');
  byId.get('palette-input').value = 'New Terminal';
  byId.get('palette-input').handlers.input(noop());
  assert.strictEqual(byId.get('palette-list').children.length, 1, 'filter narrows the list');
  assert.match(byId.get('palette-list').children[0].textContent, /New Terminal/, 'match label');
  byId.get('palette-input').handlers.keydown({
    key: 'Enter', preventDefault: noop
  });
  await tick();
  assert.strictEqual(byId.get('palette').hidden, true, 'enter closes the palette');
  const paletteSpawn = posted[posted.length - 1];
  assert.strictEqual(paletteSpawn.cmd, 'ptySpawn', 'enter runs the matched command');
  window.__omnix.reply(paletteSpawn.id, true, '{"paneKey":"tab:1","pid":78}');
  await tick();

  // ---- split terminal ----
  byId.get('strip-command').handlers.click(noop());
  byId.get('palette-input').value = 'Split Terminal';
  byId.get('palette-input').handlers.input(noop());
  assert.strictEqual(byId.get('palette-list').children.length, 1, 'split command found');
  byId.get('palette-input').handlers.keydown({ key: 'Enter', preventDefault: noop });
  await tick();
  const splitSpawn = posted[posted.length - 1];
  assert.strictEqual(splitSpawn.cmd, 'ptySpawn', 'split spawns a second pty');
  assert.strictEqual(splitSpawn.args.paneKey, 'tab:2', 'split pane key increments');
  window.__omnix.reply(splitSpawn.id, true, '{"paneKey":"tab:2","pid":79}');
  await tick();
  const splitWraps = byClass('panes', 'pane-split');
  assert.strictEqual(splitWraps.length, 1, 'panes wrapped side by side');
  assert.strictEqual(splitWraps[0].children.length, 2, 'both terminals stay mounted');
  const floatSplits = byClass('panes', 'pane-float-split');
  assert.strictEqual(floatSplits.length, 2, 'each pane carries a split control');
  assert.strictEqual(floatSplits[0].disabled, false, 'a pair can split again');
  const paneHeads = byClass('panes', 'pane-head');
  assert.strictEqual(paneHeads.length, 2, 'each split pane has a header');
  assert.strictEqual(byClass('panes', 'pane-head-time').length, 0,
    'the header skips the uptime clock');
  assert.strictEqual(byClass('panes', 'pane-head-btn').length, 8,
    'header carries role, split, handoff, and close');
  const layoutPickers = byClass('panes', 'pane-head-layout');
  assert.strictEqual(layoutPickers.length, 2, 'each pane carries a layout picker');
  assert.strictEqual(layoutPickers[0].value, 'auto', 'layout starts on auto');
  layoutPickers[0].value = 'cols';
  layoutPickers[0].handlers.change();
  assert.strictEqual(splitWraps[0].dataset.layout, 'cols', 'columns layout applies to the group');
  assert.strictEqual(layoutPickers[1].value, 'cols', 'layout choice syncs across panes');
  layoutPickers[1].value = 'rows';
  layoutPickers[1].handlers.change();
  assert.strictEqual(splitWraps[0].dataset.layout, 'rows', 'stack layout replaces columns');
  assert.strictEqual(layoutPickers[0].value, 'rows', 'layout syncs both ways');
  layoutPickers[0].value = 'auto';
  layoutPickers[0].handlers.change();
  assert.strictEqual(splitWraps[0].dataset.layout, 'auto', 'auto layout restores the grid');
  byClass('panes', 'pane-head-handoff')[0].handlers.click({ stopPropagation: noop });
  await tick();
  assert.strictEqual(byId.get('handoff-modal').hidden, false, 'header handoff opens the modal');
  byId.get('handoff-modal').hidden = true;
  // a third pane joins the same group from the floating split control
  floatSplits[1].handlers.click({ stopPropagation: noop });
  await tick();
  const thirdSpawn = posted[posted.length - 1];
  assert.strictEqual(thirdSpawn.cmd, 'ptySpawn', 'third pane spawns');
  window.__omnix.reply(thirdSpawn.id, true, '{"paneKey":"tab:3","pid":80}');
  await tick();
  assert.strictEqual(splitWraps[0].children.length, 3, 'three panes share one wrapper');
  assert.strictEqual(splitWraps[0].dataset.panes, '3', 'wrapper records the pane count');
  byClass('panes', 'pane-float-split')[2].handlers.click({ stopPropagation: noop });
  await tick();
  const fourthSpawn = posted[posted.length - 1];
  assert.strictEqual(fourthSpawn.cmd, 'ptySpawn', 'fourth pane spawns');
  window.__omnix.reply(fourthSpawn.id, true, '{"paneKey":"tab:4","pid":81}');
  await tick();
  assert.strictEqual(splitWraps[0].children.length, 4, 'four panes share one wrapper');
  assert.strictEqual(splitWraps[0].dataset.panes, '4', 'wrapper records four panes');
  const fourPanes = byClass('panes', 'pane-head-layout');
  fourPanes[0].value = 'cols';
  fourPanes[0].handlers.change();
  assert.strictEqual(splitWraps[0].dataset.layout, 'cols', 'four panes can stack in columns');
  assert.ok(byClass('panes', 'pane-float-split').every((b) => b.disabled),
    'the split control stops at four panes');
  byClass('panes', 'pane-head-close')[3].handlers.click({ stopPropagation: noop });
  await tick();
  const fourthKill = posted[posted.length - 1];
  assert.strictEqual(fourthKill.cmd, 'ptyKill', 'closing the fourth pane kills its pty');
  assert.strictEqual(splitWraps[0].children.length, 3, 'three panes remain after the cap test');
  window.__omnix.reply(fourthKill.id, true, '{"ok":true}');
  await tick();
  const thirdClose = byClass('tabstrip-tabs', 'strip-close')[2];
  thirdClose.handlers.click({ stopPropagation: noop });
  await tick();
  const splitKill = posted[posted.length - 1];
  assert.strictEqual(splitKill.cmd, 'ptyKill', 'pane close control kills its pty');
  assert.strictEqual(splitWraps[0].children.length, 2, 'closing keeps the remaining pair');
  window.__omnix.reply(splitKill.id, true, '{"ok":true}');
  await tick();

  // a pane header can close its own pane; one survivor drops the split wrapper
  const headClose = byClass('panes', 'pane-head-close')[1];
  headClose.handlers.click({ stopPropagation: noop });
  await tick();
  const headKill = posted[posted.length - 1];
  assert.strictEqual(headKill.cmd, 'ptyKill', 'header close kills its pty');
  assert.strictEqual(byClass('panes', 'pane-split').length, 0, 'last pair unwraps to one pane');
  window.__omnix.reply(headKill.id, true, '{"ok":true}');
  await tick();

  // ---- history panel closes and reopens from the window bar ----
  byId.get('history-collapse').handlers.click(noop);
  assert.ok(byId.get('app').classList.contains('hist-collapsed'), 'history panel collapses');
  byId.get('rail-agents').handlers.click(noop);
  assert.ok(!byId.get('app').classList.contains('hist-collapsed'), 'history panel reopens');
  assert.strictEqual(byId.get('app').style['--hist-w'], '340px',
    'the saved history width comes back when the panel reopens');

  // ---- settings / agents ----
  panelButtons[3].handlers.click(noop());
  await tick();
  const settingsReq = posted[posted.length - 1];
  assert.strictEqual(settingsReq.cmd, 'agentSettings', 'settings pane loads agent settings');
  window.__omnix.reply(settingsReq.id, true, JSON.stringify({
    path: '/repo/.omnix/agents.json',
    defaults: { fullAccess: false },
    agents: [
      { name: 'claude', bin: 'claude', available: true, enabled: true, args: '', env: '',
        fullAccessFlag: '--dangerously-skip-permissions' },
      { name: 'codex', bin: 'codex', available: true, enabled: false, args: '--fast', env: '',
        fullAccessFlag: '--dangerously-bypass-approvals-and-sandbox' },
      { name: 'gemini', bin: 'gemini', available: false, enabled: true, args: '', env: '',
        fullAccessFlag: '--yolo' }
    ]
  }));
  await tick();
  assert.strictEqual(byClass('agent-settings', 'agent-row').length, 3,
    'installed and available agents both render');
  assert.match(byId.get('agent-settings-meta').textContent, /2 installed .* 1 available/,
    'meta counts installed and missing');
  assert.match(byId.get('agent-settings-path').textContent, /agents\.json/, 'settings path shown');
  const codexRow = all(byId.get('agent-settings'),
    (n) => n.dataset && n.dataset.name === 'codex')[0];
  assert.ok(String(codexRow.className).indexOf('is-disabled') >= 0, 'disabled agent row dims');
  const codexToggle = all(codexRow, (n) => n.type === 'checkbox')[0];
  assert.strictEqual(codexToggle.checked, false, 'toggle mirrors disabled state');
  codexToggle.checked = true;
  codexToggle.handlers.change(noop);
  await tick();
  const enableReq = posted[posted.length - 1];
  assert.strictEqual(enableReq.cmd, 'agentEnable', 'toggle asks the host');
  assert.deepStrictEqual(enableReq.args, { agent: 'codex', enabled: true }, 'toggle sends agent state');
  window.__omnix.reply(enableReq.id, true, '{"ok":true,"enabled":true}');
  await tick();

  byId.get('agent-default-perms').value = 'full';
  byId.get('agent-default-perms').handlers.change(noop);
  await tick();
  const defaultsReq = posted[posted.length - 1];
  assert.strictEqual(defaultsReq.cmd, 'agentDefaultsSave', 'permissions select saves');
  assert.deepStrictEqual(defaultsReq.args, { fullAccess: true }, 'full access forwarded');
  window.__omnix.reply(defaultsReq.id, true, '{"ok":true,"defaults":{"fullAccess":true}}');
  await tick();

  byId.get('settings-search').value = 'gemini';
  byId.get('settings-search').handlers.input(noop);
  assert.strictEqual(byClass('agent-settings', 'agent-row').length, 1,
    'settings search filters both lists');
  byId.get('settings-search').value = '';
  byId.get('settings-search').handlers.input(noop);

  // ---- settings / plugins ----
  settingsNavButtons[1].handlers.click(noop);
  await tick();
  const pluginsReq = lastReq('plugins');
  assert.ok(pluginsReq, 'plugins section asks for installed plugins');
  assert.strictEqual(byId.get('settings-plugins').hidden, false, 'plugins pane visible');
  assert.strictEqual(byId.get('settings-agents').hidden, true, 'agents pane hidden');
  window.__omnix.reply(pluginsReq.id, true, JSON.stringify({
    catalog: 'https://example.test/index.json',
    path: '/home/you/.omnix/plugins',
    plugins: [
      {
        name: 'hello', version: '1.0.0', enabled: true, installed: true,
        path: '/p/hello', kinds: ['command', 'panel'], permissions: 'read .; network api',
        requested: ['read', 'network'], granted: ['read'], sha256: 'a'.repeat(64),
        source: 'local', description: 'Says hello', author: 'Test Author', license: 'MIT', api: 1
      },
      {
        name: 'broken', installed: false, enabled: true, kind: [],
        requested: [], granted: [], error: 'bad manifest'
      }
    ]
  }));
  await tick();
  assert.strictEqual(byClass('plugin-list', 'plugin-row').length, 2,
    'installed and broken plugin folders render');
  assert.strictEqual(byClass('plugin-list', 'perm-chip').length, 2,
    'requested permissions become chips');
  assert.strictEqual(byClass('plugin-list', 'perm-chip')
    .filter((n) => String(n.className).indexOf('is-granted') >= 0).length, 1,
    'granted permission is marked');
  const panelAction = byClass('plugin-list', 'btn')
    .find((n) => /Buka panel/.test(n.textContent));
  assert.ok(panelAction, 'panel contribution offers a button');
  panelAction.handlers.click(noop);
  await tick();
  const panelReq = lastReq('pluginPanel');
  assert.deepStrictEqual(panelReq.args, { name: 'hello', panel: 'status' },
    'panel request names plugin and panel');
  window.__omnix.reply(panelReq.id, true,
    '{"plugin":"hello","name":"status","title":"Status","url":"/plugin-panel/hello/index.html"}');
  await tick();
  assert.strictEqual(byId.get('plugin-panel-modal').hidden, false, 'panel modal opens');
  assert.strictEqual(byId.get('plugin-panel-frame').getAttribute('src'),
    '/plugin-panel/hello/index.html', 'panel frame points at the plugin file');
  byId.get('plugin-panel-ok').handlers.click(noop);
  assert.strictEqual(byId.get('plugin-panel-modal').hidden, true, 'panel modal closes');

  byId.get('plugin-install-source').value = '/tmp/hello-plugin';
  byId.get('plugin-install-btn').handlers.click(noop);
  await tick();
  const installReq = lastReq('pluginAction');
  assert.strictEqual(installReq.args.action, 'install', 'install action is sent');
  assert.strictEqual(installReq.args.name, '/tmp/hello-plugin', 'install source is sent');
  window.__omnix.reply(installReq.id, false, '{"code":"internal","message":"not found"}');
  await tick();

  byId.get('plugin-search').value = 'hello';
  byId.get('plugin-search-btn').handlers.click(noop);
  await tick();
  const catalogReq = lastReq('pluginAction');
  assert.strictEqual(catalogReq.args.action, 'search', 'catalog search action is sent');
  assert.strictEqual(catalogReq.args.name, 'hello', 'catalog query is sent');
  window.__omnix.reply(catalogReq.id, true, JSON.stringify({
    catalog: 'https://example.test/index.json',
    plugins: [{
      name: 'hello', version: '1.0.0', author: 'Test Author', license: 'MIT',
      description: 'Says hello', kinds: ['command'], permissions: 'read .',
      sha256: 'b'.repeat(64), installed: null
    }]
  }));
  await tick();
  assert.strictEqual(byClass('plugin-catalog', 'catalog-row').length, 1,
    'catalog search renders a result');
  assert.match(byClass('plugin-catalog', 'plugin-desc')[0].textContent, /Says hello/,
    'catalog result carries the description');

  // ---- settings / dev ----
  settingsNavButtons[2].handlers.click(noop);
  await tick();
  const devReq = lastReq('devStatus');
  assert.ok(devReq, 'dev section asks for build status');
  assert.strictEqual(byId.get('settings-dev').hidden, false, 'dev pane visible');
  window.__omnix.reply(devReq.id, true, JSON.stringify({
    builds: [{
      id: 'dev-1', worktree: '/repo', branch: 'main', commit: 'abc123',
      profile: 'release', binary: '/home/you/.omnix/builds/dev-1/omnix',
      sha256: 'c'.repeat(64), size: 2097152, built_at: Math.floor(Date.now() / 1000) - 60
    }],
    installed: {},
    previous: null,
    devHome: '/home/you/.omnix/dev/home',
    tasks: [{
      name: 'dev-task-1', agent: 'codex', worktree: '/repo/.omnix/worktrees/dev-1',
      branch: 'omnix/dev-1', status: 'working', outcome: 'journaled'
    }]
  }));
  await tick();
  assert.strictEqual(byClass('dev-builds', 'dev-row').length, 1, 'dev build renders');
  assert.strictEqual(byClass('dev-tasks', 'dev-row').length, 1, 'dev task renders');
  assert.strictEqual(byId.get('dev-restore-btn').disabled, true,
    'restore waits for a previous binary');

  byId.get('dev-brief').value = 'add a small status panel';
  byId.get('dev-agent').value = 'codex';
  byId.get('dev-task-btn').handlers.click(noop);
  await tick();
  const taskReq = lastReq('devTask');
  assert.strictEqual(taskReq.args.brief, 'add a small status panel', 'brief is sent');
  assert.strictEqual(taskReq.args.agent, 'codex', 'agent is sent');
  window.__omnix.reply(taskReq.id, true, JSON.stringify({
    record: {
      name: 'dev-task-2', agent: 'codex', worktree: '/repo/.omnix/worktrees/dev-2',
      branch: 'omnix/dev-2', status: 'journaled', outcome: 'journaled'
    },
    path: '/repo/.omnix/handoffs/dev-2.md',
    brief: '# Omnix development task\n\nadd a small status panel'
  }));
  await tick();
  const devSpawn = lastReq('ptySpawn');
  assert.ok(devSpawn, 'dev task opens a terminal');
  assert.strictEqual(devSpawn.args.agent, 'codex', 'dev task uses the chosen agent');
  assert.strictEqual(devSpawn.args.cwd, '/repo/.omnix/worktrees/dev-2',
    'dev task starts in its worktree');
  window.__omnix.reply(devSpawn.id, true, '{"paneKey":"dev:dev-task-2","pid":91}');
  await tick();
  const deliveredReq = lastReq('devTaskDelivered');
  assert.ok(deliveredReq, 'opening the pane marks the task delivered');
  assert.strictEqual(deliveredReq.args.name, 'dev-task-2');
  window.__omnix.reply(deliveredReq.id, true, '{"ok":true}');
  await tick();
  const devReload = lastReq('devStatus');
  window.__omnix.reply(devReload.id, true,
    '{"builds":[],"installed":{},"previous":null,"devHome":"/h","tasks":[]}');
  await tick();

  byId.get('dev-build-btn').handlers.click(noop);
  await tick();
  const buildReq = lastReq('devBuild');
  assert.ok(buildReq, 'build button asks the host to build');
  window.__omnix.reply(buildReq.id, true, JSON.stringify({
    id: 'dev-2', worktree: '/repo', branch: 'main', commit: 'def456',
    profile: 'release', binary: '/home/you/.omnix/builds/dev-2/omnix',
    sha256: 'd'.repeat(64), size: 2097152, built_at: Math.floor(Date.now() / 1000)
  }));
  await tick();
  const buildReload = lastReq('devStatus');
  window.__omnix.reply(buildReload.id, true,
    '{"builds":[],"installed":{},"previous":null,"devHome":"/h","tasks":[]}');
  await tick();
  window.__omnix.emit('paneClosed', { paneKey: 'dev:dev-task-2' });
  await tick();
  const devKill = lastReq('ptyKill');
  if (devKill) {
    window.__omnix.reply(devKill.id, true, '{"ok":true}');
    await tick();
  }

  // keyboard: arrows move the active row, escape closes, focus returns
  assert.strictEqual(docHandlers['keydown:capture'], true,
    'app shortcuts listen before xterm swallows the key');
  docHandlers.keydown({ key: 'k', ctrlKey: true, preventDefault: noop });
  assert.strictEqual(byId.get('palette').hidden, false, 'Ctrl+K opens the palette');
  const rows = byId.get('palette-list').children;
  assert.strictEqual(rows.length, 23, 'all commands listed with an empty query');
  byId.get('palette-input').handlers.keydown({ key: 'ArrowDown', preventDefault: noop });
  assert.strictEqual(byId.get('palette-list').children[1].getAttribute('aria-selected'), 'true',
    'arrow down moves the selection');
  byId.get('palette-input').handlers.keydown({ key: 'Escape', preventDefault: noop });
  assert.strictEqual(byId.get('palette').hidden, true, 'escape closes the palette');
  assert.strictEqual(document.activeElement, byId.get('strip-command'),
    'focus returns to the element that opened the palette');

  // ---- defensive paths must never throw ----
  window.__omnix.reply(9999, true, '{}');
  window.__omnix.reply(9998, false, 'not json');
  window.__omnix.emit('nope', '{}');
  window.__omnix.emit('ptyData', '{"paneKey":"tab:1","dataBase64":"aGk="}');
  window.__omnix.emit('ptyData', '{"paneKey":"tab:1","dataBase64":"@@@"}');
  window.__omnix.emit('ptyExit', '{"paneKey":"tab:1","code":0}');
  window.__omnix.emit('browserNav', 'not json');
  window.__omnix.emit('toast', '{"level":"warn","message":"host says hi"}');
  await tick();

  assert.ok(byId.get('toasts').children.length >= 1, 'toast event rendered');

  // the AI memory budget warns once when the terminal tree crosses it
  docHandlers.visibilitychange();
  await tick();
  const memReq = lastReq('memory');
  assert.ok(memReq, 'visibility refresh asks for memory');
  window.__omnix.reply(memReq.id, true, JSON.stringify({
    available: true, pid: 1,
    app: { rss: 104857600, processes: 3 },
    ai: { rss: 10485760, processes: 1 },
    other: { rss: 94371840, processes: 2 },
    budget: { limitBytes: 1048576, over: true },
    panes: [],
    conflicts: [{
      kind: 'folder', value: '/repo',
      message: '2 agent berjalan di folder yang sama: /repo',
      panes: [{ paneKey: 'tab:1', agent: 'codex' }, { paneKey: 'tab:2', agent: 'claude' }]
    }]
  }));
  await tick();
  const budgetToasts = () => byId.get('toasts').children
    .filter((n) => /budget/.test(String(n.textContent)));
  assert.strictEqual(budgetToasts().length, 1, 'over-budget toast appears');
  assert.strictEqual(byId.get('conflict-banner').hidden, false,
    'shared-folder warning banner is visible');
  const moveButton = all(byId.get('conflict-banner'),
    (n) => n.tagName === 'button' && /Move to worktree/.test(String(n.textContent)))[0];
  assert.ok(moveButton, 'the banner offers a move-to-worktree action');
  docHandlers.visibilitychange();
  await tick();
  const memAgain = lastReq('memory');
  window.__omnix.reply(memAgain.id, true, JSON.stringify({
    available: true, pid: 1,
    app: { rss: 104857600, processes: 3 },
    ai: { rss: 10485760, processes: 1 },
    other: { rss: 94371840, processes: 2 },
    budget: { limitBytes: 1048576, over: true },
    panes: []
  }));
  await tick();
  assert.strictEqual(budgetToasts().length, 1, 'over-budget warning is one-shot');

  // ---- error state ----
  byId.get('history-rescan').handlers.click(noop());
  await tick();
  const rescanReq = posted[posted.length - 1];
  assert.strictEqual(rescanReq.cmd, 'historyScan', 'rescan re-runs the scan');
  window.__omnix.reply(rescanReq.id, false, '{"code":"internal","message":"db locked"}');
  await tick();

  const listingAfterFail = posted[posted.length - 1];
  assert.strictEqual(listingAfterFail.cmd, 'historySessions', 'listing still runs after a failed scan');
  window.__omnix.reply(listingAfterFail.id, true, '{"scope":"all","root":"/repo","total":0,"sessions":[]}');
  await tick();

  assert.strictEqual(byClass('history-list', 'error').length, 1, 'error state rendered');
  assert.match(byClass('history-list', 'error')[0].textContent, /db locked/, 'error carries the message');

  // ---- remaining panels still work ----
  byId.get('search-query').value = 'blank restore';
  byId.get('search-conversation').checked = true;
  byId.get('search-agent').value = 'codex';
  await byId.get('search-form').handlers.submit({ preventDefault: noop });
  const searchReq = posted[posted.length - 1];
  assert.strictEqual(searchReq.cmd, 'historySearch');
  assert.deepStrictEqual(searchReq.args, {
    query: 'blank restore', limit: 20, agent: 'codex', conversation: true
  });
  window.__omnix.reply(searchReq.id, true, '{"hits":[{"agent":"codex","sessionId":"s-1","title":"t","cwd":"/r","resumeCommand":"codex resume s-1","snippet":"snip"}]}');
  await tick();
  assert.match(byId.get('search-meta').textContent, /1 hit for "blank restore"/);

  panelButtons[1].handlers.click(noop());
  assert.strictEqual(byId.get('handoff-modal').hidden, false, 'handoff modal opens');
  assert.match(byId.get('handoff-source-title').textContent, /wire indexed history/,
    'modal picks the last session');
  byId.get('handoff-target').value = 'codex';
  byId.get('handoff-start').handlers.click(noop());
  const handoffReq = posted[posted.length - 1];
  assert.strictEqual(handoffReq.cmd, 'handoffAgent');
  assert.strictEqual(handoffReq.args.agent, 'codex');
  assert.strictEqual(handoffReq.args.sourceAgent, 'claude');
  assert.strictEqual(handoffReq.args.sessionId, 'uuid-1');
  assert.strictEqual(handoffReq.args.context, 'focused');
  window.__omnix.reply(handoffReq.id, true,
    '{"agent":"codex","cwd":"/repo","brief":"/repo/.omnix/handoffs/1-to-codex.md",' +
    '"prompt":"# Handoff to codex\\n\\n## Stopping point\\n\\ncontinue","messages":12}');
  await tick();
  const handoffSpawn = posted[posted.length - 1];
  assert.strictEqual(handoffSpawn.cmd, 'ptySpawn', 'handoff starts a new agent session');
  assert.strictEqual(handoffSpawn.args.agent, 'codex', 'handoff runs the target CLI');
  assert.strictEqual(handoffSpawn.args.cwd, '/repo');
  window.__omnix.reply(handoffSpawn.id, true, '{"paneKey":"hf:codex:1","pid":79}');
  await tick();
  assert.strictEqual(byId.get('handoff-modal').hidden, true, 'handoff modal closes');

  // the brief is typed into the new agent once its TUI prints the first frame
  window.__omnix.emit('ptyData', {
    paneKey: handoffSpawn.args.paneKey,
    dataBase64: Buffer.from('agent ready').toString('base64')
  });
  await new Promise((r) => setTimeout(r, 450));
  const handoffWrite = posted.filter((r) => r.cmd === 'ptyWrite').pop();
  assert.ok(handoffWrite, 'handoff types the brief into the new terminal');
  assert.ok(handoffWrite.args.data.startsWith('\x1b[200~'), 'brief arrives as a bracketed paste');
  assert.match(handoffWrite.args.data, /# Handoff to codex/, 'the prompt carries the transcript');
  assert.ok(handoffWrite.args.data.endsWith('\x1b[201~'),
    'the brief is one paste without an early enter');
  window.__omnix.reply(handoffWrite.id, true, '{"ok":true}');
  await new Promise((r) => setTimeout(r, 1350));
  const handoffSubmit = posted.filter((r) => r.cmd === 'ptyWrite').pop();
  assert.strictEqual(handoffSubmit.args.data, '\r',
    'the prompt is submitted after the paste settles');

  byId.get('tab-new').handlers.click(noop());
  byId.get('tab-new-menu').children[1].handlers.click(noop());
  byId.get('browser-url').value = 'ftp://example.com';
  byId.get('browser-open').handlers.click(noop());
  await tick();
  assert.ok(!posted.some((r) => r.cmd === 'browserOpen'), 'ftp URL rejected');

  byId.get('browser-url').value = 'example.com';
  byId.get('browser-engine').value = 'native';
  byId.get('browser-open').handlers.click(noop());
  await tick();
  const openReq = posted[posted.length - 1];
  assert.strictEqual(openReq.cmd, 'browserOpen');
  assert.strictEqual(openReq.args.url, 'https://example.com', 'scheme added');
  assert.deepStrictEqual(openReq.args.rect, { x: 10, y: 20, width: 400, height: 300 }, 'content rect sent');
  window.__omnix.reply(openReq.id, true, '{"url":"https://example.com"}');
  await tick();

  // adding a project: native folder dialog -> projectAdd -> refresh -> terminal there
  byId.get('new-project').handlers.click(noop());
  await tick();
  const pickReq = posted[posted.length - 1];
  assert.strictEqual(pickReq.cmd, 'pickFolder', '+ Project asks the host for a folder');
  window.__omnix.reply(pickReq.id, true, JSON.stringify({ path: '/home/you/code/newrepo' }));
  await tick();
  const addReq = posted[posted.length - 1];
  assert.strictEqual(addReq.cmd, 'projectAdd', 'picked folder is added');
  assert.deepStrictEqual(addReq.args, { path: '/home/you/code/newrepo' }, 'add sends the folder');
  window.__omnix.reply(addReq.id, true, JSON.stringify({
    projects: [
      { path: '/home/you/code/omnix', name: 'omnix' },
      { path: '/home/you/code/newrepo', name: 'newrepo' }
    ]
  }));
  await tick();
  const addProjectsReq = lastReq('projects');
  assert.ok(addProjectsReq, 'add refreshes the list');
  window.__omnix.reply(addProjectsReq.id, true, JSON.stringify({
    projects: [
      { path: '/home/you/code/omnix', name: 'omnix' },
      { path: '/home/you/code/omnix/packages/tools', name: 'omnix' },
      { path: '/home/you/code/newrepo', name: 'newrepo' }
    ]
  }));
  await tick();
  assert.strictEqual(posted[posted.length - 1].cmd, 'historyProjects', 'add reloads session stats');
  window.__omnix.reply(posted[posted.length - 1].id, true, JSON.stringify({ projects: [] }));
  await tick();
  const spawnReq = lastReq('ptySpawn');
  assert.ok(spawnReq, 'add opens a terminal for the new project');
  assert.strictEqual(spawnReq.args.cwd, '/home/you/code/newrepo', 'terminal starts in the folder');
  window.__omnix.reply(spawnReq.id, true, '{"paneKey":"tab:new","pid":90}');
  await tick();
  const addHistReq = lastReq('historySessions');
  assert.ok(addHistReq, 'new project scopes history');
  assert.strictEqual(addHistReq.args.scope, 'project', 'new project switches scope');
  assert.strictEqual(addHistReq.args.cwd, '/home/you/code/newrepo',
    'history follows the project folder');
  window.__omnix.reply(addHistReq.id, true,
    '{"scope":"project","root":"/repo","total":0,"sessions":[]}');
  await tick();
  assert.strictEqual(byClass('workspace-list', 'proj-row').length, 3, 'new row is listed');

  // duplicate basenames name their parent, and nested projects own separate tabs
  const projRows = byClass('workspace-list', 'proj-row');
  const labels = projRows.map((n) => all(n, (x) => String(x.className) === 'ws-name')[0].textContent);
  assert.ok(labels.indexOf('omnix \u00b7 code') >= 0, 'parent project label is disambiguated');
  assert.ok(labels.indexOf('omnix \u00b7 packages') >= 0, 'nested project label is disambiguated');

  const toolsRow = projRows.find((n) => String(n.attrs['aria-label'] || '').includes('/home/you/code/omnix/packages/tools'));
  assert.ok(toolsRow, 'nested project row is present');
  const beforeTools = posted.length;
  toolsRow.handlers.click(noop());
  await tick();
  const toolsReqs = posted.slice(beforeTools);
  const toolsSpawn = toolsReqs.find((r) => r.cmd === 'ptySpawn');
  const toolsHist = toolsReqs.find((r) => r.cmd === 'historySessions');
  assert.ok(toolsSpawn, 'nested project opens its own terminal');
  assert.strictEqual(toolsSpawn.args.cwd, '/home/you/code/omnix/packages/tools',
    'nested project cwd is used');
  assert.ok(toolsHist, 'nested project filters history');
  assert.strictEqual(toolsHist.args.cwd, '/home/you/code/omnix/packages/tools',
    'history follows the nested project');
  window.__omnix.reply(toolsSpawn.id, true, '{"paneKey":"tab:tools","pid":90}');
  window.__omnix.reply(toolsHist.id, true,
    '{"scope":"project","root":"/repo","total":0,"sessions":[]}');
  await tick();

  // new terminals inherit the folder of the project the user is working in
  const omnixRow = projRows.find((n) => String(n.attrs['aria-label'] || '').endsWith('/home/you/code/omnix'));
  assert.ok(omnixRow, 'omnix project row is present');
  const beforeOmnix = posted.length;
  omnixRow.handlers.click(noop());
  await tick();
  const omnixReqs = posted.slice(beforeOmnix);
  const projectSpawn = omnixReqs.find((r) => r.cmd === 'ptySpawn');
  const omnixHist = omnixReqs.find((r) => r.cmd === 'historySessions');
  assert.ok(projectSpawn,
    'clicking the parent project must not reuse the nested tab');
  assert.strictEqual(projectSpawn.args.cwd, '/home/you/code/omnix', 'project cwd is used');
  assert.ok(omnixHist, 'parent project filters history');
  assert.strictEqual(omnixHist.args.cwd, '/home/you/code/omnix',
    'history follows the parent project');
  window.__omnix.reply(projectSpawn.id, true, '{"paneKey":"tab:9","pid":91}');
  window.__omnix.reply(omnixHist.id, true,
    '{"scope":"project","root":"/repo","total":0,"sessions":[]}');
  await tick();

  // a terminal in a nested project must not also appear under its parent
  const projWraps = byClass('workspace-list', 'proj');
  function wrapFor(path) {
    return projWraps.find((w) => {
      const row = all(w, (n) => String(n.className).split(/\s+/).includes('proj-row'))[0];
      return row && String(row.attrs['aria-label'] || '').endsWith(' \u2014 ' + path);
    });
  }
  const parentWrap = wrapFor('/home/you/code/omnix');
  const toolsWrap = wrapFor('/home/you/code/omnix/packages/tools');
  assert.ok(parentWrap && toolsWrap, 'parent and nested project rows exist');
  const parentLabels = all(parentWrap, (n) => n.className === 'sess-row')
    .map((n) => all(n, (x) => String(x.className) === 'inv-title')[0].textContent);
  assert.ok(parentLabels.indexOf('omnix \u00b7 packages') < 0,
    'nested project terminal stays out of the parent row');
  assert.strictEqual(all(toolsWrap, (n) => n.className === 'sess-row').length, 1,
    'nested project lists its own terminal');

  // tabs are scoped: the strip shows only the project that is active
  let stripTitles = all(byId.get('tabstrip-tabs'), (n) => n.className === 'strip-title');
  assert.strictEqual(stripTitles.length, 1,
    'switching project leaves only that project tab in the strip');
  assert.ok(String(stripTitles[0].title).indexOf('/home/you/code/omnix') >= 0,
    'tab tooltip carries the active project path');
  assert.ok(String(stripTitles[0].title).indexOf('/packages/tools') < 0,
    'nested project tab is hidden while the parent project is active');
  const activeRows = byClass('workspace-list', 'proj').filter((n) =>
    String(n.className).split(/\s+/).includes('is-active'));
  assert.strictEqual(activeRows.length, 1, 'exactly one project is marked active');
  assert.ok(String(activeRows[0].children[0].attrs['aria-label'] || '').endsWith('/home/you/code/omnix'),
    'the active marker follows the clicked project');

  // the sidebar spinner turns only while the pane is printing
  const liveDot = byClass('workspace-list', 'work-dot')[0];
  assert.ok(liveDot, 'live project shows a status mark');
  window.__omnix.emit('ptyData', JSON.stringify({
    paneKey: projectSpawn.args.paneKey, dataBase64: 'aGk='
  }));
  await new Promise((resolve) => setTimeout(resolve, 800));
  assert.ok(byClass('workspace-list', 'is-working').length >= 1,
    'printing pane animates its project mark');

  const beforeReuse = posted.length;
  omnixRow.handlers.click(noop());
  await tick();
  assert.strictEqual(posted.length, beforeReuse,
    'clicking an open project activates its tab instead of spawning another');

  // tab marks: working, approval, failed, done
  const statusMarks = byClass('tabstrip-tabs', 'tab-status');
  assert.ok(statusMarks.length >= 1, 'terminal tabs carry a status mark');
  assert.ok(statusMarks.some((n) => /is-working/.test(n.className)),
    'a printing terminal spins in the tab strip');
  window.__omnix.emit('ptyData', {
    paneKey: projectSpawn.args.paneKey,
    dataBase64: Buffer.from('Do you want to proceed?\n❯ 1. Yes\n  2. No').toString('base64')
  });
  await tick();
  assert.ok(byClass('tabstrip-tabs', 'is-approval').length >= 1,
    'an approval question turns the tab amber');
  window.__omnix.emit('ptyExit', { paneKey: projectSpawn.args.paneKey, code: 2 });
  await tick();
  assert.ok(byClass('tabstrip-tabs', 'is-failed').length >= 1,
    'a non-zero exit marks the tab failed');

  byId.get('tab-new-menu').hidden = true;
  byId.get('tab-new').handlers.click(noop());
  const agentRow = all(byId.get('tab-new-menu'), (n) =>
    String(n.className) === 'menu-item' && all(n, (x) =>
      String(x.className) === 'menu-label' && /claude/i.test(x.textContent || '')).length)[0];
  assert.ok(agentRow, 'detected agent appears in the + menu');
  agentRow.handlers.click(noop());
  await tick();
  const agentSpawn = posted[posted.length - 1];
  assert.strictEqual(agentSpawn.cmd, 'ptySpawn', 'agent starts from the + menu');
  assert.strictEqual(agentSpawn.args.cwd, '/home/you/code/omnix',
    'agent inherits the project folder instead of the home directory');
  window.__omnix.reply(agentSpawn.id, true, '{"paneKey":"tab:10","pid":92}');
  await tick();

  toolsRow.handlers.click(noop());
  await tick();
  stripTitles = all(byId.get('tabstrip-tabs'), (n) => n.className === 'strip-title');
  assert.strictEqual(stripTitles.length, 1, 'returning to a project swaps the strip back');
  assert.ok(String(stripTitles[0].title).indexOf('/packages/tools') >= 0,
    'nested project tab returns when its row is active');

  // a static panel is one DOM node: it follows the project that opened it
  function stripCount() {
    return all(byId.get('tabstrip-tabs'), (n) => n.className === 'strip-title').length;
  }
  omnixRow.handlers.click(noop());
  await tick();
  const omnixTabs = stripCount();
  panelButtons[0].handlers.click(noop());
  await tick();
  assert.strictEqual(stripCount(), omnixTabs + 1, 'panel opens in the active project');
  toolsRow.handlers.click(noop());
  await tick();
  assert.strictEqual(stripCount(), 1, 'panel does not follow the scope');
  panelButtons[0].handlers.click(noop());
  await tick();
  assert.strictEqual(stripCount(), 2, 'panel moves to the new project');
  omnixRow.handlers.click(noop());
  await tick();
  assert.strictEqual(stripCount(), omnixTabs, 'panel left with its project');
  toolsRow.handlers.click(noop());
  await tick();
  assert.strictEqual(stripCount(), 2, 'panel returns with its project');

  // update notification: the host names the release, the chip downloads it
  const updateReq = posted.find((r) => r.cmd === 'updateCheck');
  assert.ok(updateReq, 'boot checks for updates');
  window.__omnix.reply(updateReq.id, true,
    '{"available":true,"current":"0.6.0","version":"0.6.1"}');
  await tick();
  assert.strictEqual(byId.get('update-chip').hidden, false, 'update chip appears');
  assert.strictEqual(byId.get('update-chip').textContent, 'Update 0.6.1');
  const beforeUpdate = posted.length;
  byId.get('update-chip').handlers.click(noop());
  assert.strictEqual(posted[posted.length - 1].cmd, 'updateInstall', 'chip downloads the release');
  window.__omnix.reply(posted[posted.length - 1].id, true,
    '{"ok":true,"quit":false,"message":"opened"}');
  await tick();
  assert.strictEqual(byId.get('update-chip').hidden, true, 'chip clears after install');
  assert.strictEqual(posted.length, beforeUpdate + 1, 'one install request');

  // usage modal: one limit line per agent, not aggregate token totals
  byId.get('usage-open').handlers.click(noop());
  await tick();
  const usageReq = lastReq('usage');
  assert.ok(usageReq, 'usage modal reads quotas');
  window.__omnix.reply(usageReq.id, true, JSON.stringify({
    ram: { totalBytes: 16 * 1024 * 1024 * 1024, usedBytes: 6 * 1024 * 1024 * 1024 },
    agents: [
      { agent: 'codex', plan: 'free', windows: [
        { kind: 'primary', usedPercent: 100, resetsInSeconds: 900, windowMinutes: 300 },
        { kind: 'secondary', usedPercent: 11, resetsInSeconds: 4000, windowMinutes: 10080 }
      ] },
      { agent: 'antigravity', plan: null, windows: [
        { kind: 'gemini-weekly', label: 'Gemini Models \u00b7 Weekly',
          usedPercent: 1.254, resetsInSeconds: 460800, windowMinutes: 10080 },
        { kind: 'gemini-5h', label: 'Gemini Models \u00b7 5h',
          usedPercent: 0, resetsInSeconds: 900, windowMinutes: 300 }
      ] }
    ]
  }));
  await tick();
  const limits = byClass('usage-list', 'usage-row');
  assert.strictEqual(limits.length, 2, 'one quota line per agent');
  const codexName = all(limits[0], (n) => n.className === 'usage-name')[0];
  assert.strictEqual(codexName.textContent, 'Codex', 'agent name is readable');
  const codexPlan = all(limits[0], (n) => n.className === 'usage-plan')[0];
  assert.match(codexPlan.textContent, /Free/, 'plan is shown');
  const codexWhen = all(limits[0], (n) => n.className === 'usage-window')[0];
  assert.match(codexWhen.textContent, /resets in/, 'reset countdown is shown');
  const codexPct = all(limits[0], (n) => n.className === 'usage-window-pct')[0];
  assert.strictEqual(codexPct.textContent, '100% used', 'the tightest window wins');
  const agPct = all(limits[1], (n) => n.className === 'usage-window-pct')[0];
  assert.strictEqual(agPct.textContent, '1% used', 'antigravity reports remaining quota');

  // the same snapshot fills the compact strip in the status bar
  assert.strictEqual(byId.get('usage-chip').hidden, false, 'AI chip appears with data');
  const stripItems = byClass('usage-strip', 'usage-item');
  assert.strictEqual(stripItems.length, 2, 'bottom bar lists every agent');
  const stripPcts = all(byId.get('usage-strip'), (n) => n.className === 'usage-pct');
  assert.deepStrictEqual(stripPcts.map((n) => n.textContent), ['100%', '1%'],
    'bottom bar shows the tightest window percentage');
  assert.strictEqual(all(byId.get('usage-strip'), (n) => n.className === 'usage-tag').length, 0,
    'the bar leaves window tags to the menu');
  assert.strictEqual(all(byId.get('usage-strip'), (n) => n.className === 'usage-reset').length, 0,
    'the bar leaves reset times to the menu');
  const stripMarks = byClass('usage-strip', 'usage-mark');
  assert.strictEqual(stripMarks.length, 2, 'each entry has a brand mark');
  const beforeUsage = posted.length;
  byId.get('usage-strip-refresh').handlers.click(noop());
  assert.strictEqual(posted.length, beforeUsage + 1, 'strip refresh reads quotas again');
  assert.strictEqual(posted[posted.length - 1].cmd, 'usage', 'refresh uses the usage RPC');
  window.__omnix.reply(posted[posted.length - 1].id, true,
    '{"ram":{},"agents":[]}');
  await tick();
  assert.strictEqual(byId.get('usage-chip').hidden, false,
    'the AI entry stays visible without data');
  const emptyEntry = byClass('usage-strip', 'usage-item-empty')[0];
  assert.ok(emptyEntry, 'the empty AI entry keeps a dash');
  const emptyText = all(emptyEntry,
    (n) => n !== emptyEntry && n.className === 'usage-pct')[0];
  assert.strictEqual(emptyText.textContent, 'AI –', 'the empty entry reads AI -');
  byId.get('usage-close').handlers.click(noop());
  assert.strictEqual(byId.get('usage-modal').hidden, true, 'usage menu closes');

  // memory has its own menu and its own RPC, separate from AI quotas
  byId.get('memory-chip').handlers.click(noop());
  await tick();
  assert.strictEqual(byId.get('usage-modal').hidden, true, 'memory is not the usage modal');
  assert.strictEqual(byId.get('memory-modal').hidden, false, 'memory menu opens');
  const memOpenReq = lastReq('memory');
  assert.ok(memOpenReq, 'memory menu refreshes memory');
  window.__omnix.reply(memOpenReq.id, true, JSON.stringify({
    available: true, pid: 1,
    app: { rss: 157286400, processes: 4 },
    ai: { rss: 52428800, processes: 2 },
    other: { rss: 104857600, processes: 2 },
    budget: { limitBytes: 1048576, over: false },
    panes: [],
    ram: { totalBytes: 16 * 1024 * 1024 * 1024, usedBytes: 8 * 1024 * 1024 * 1024 }
  }));
  await tick();
  assert.strictEqual(byId.get('memory-strip').textContent, '150 MB',
    'memory chip shows app RSS');
  assert.strictEqual(byId.get('memory-total').textContent, '150 MB in 4 processes',
    'memory menu shows the process total');
  assert.strictEqual(byId.get('memory-ai').textContent, 'AI 50 MB \u00b7 2 processes',
    'AI share has its own tag');
  assert.strictEqual(byId.get('memory-ram').textContent,
    'System memory 8.0 GB of 16.0 GB (50%)', 'system RAM comes from the memory RPC');
  byId.get('memory-close').handlers.click(noop());
  assert.strictEqual(byId.get('memory-modal').hidden, true, 'memory menu closes');

  // workers: the CLI's pane registry, queued handoffs, send, and invoked cleanup
  const listReq = lastReq('handoffList');
  assert.ok(listReq, 'opening the panel lists handoffs');
  window.__omnix.reply(listReq.id, true, JSON.stringify({
    handoffs: [
      { name: 'tertunda', agent: 'codex', outcome: 'queued', closed: false,
        queued_reason: '2 worker(s) active (max 2)' },
      { name: 'selesai', agent: 'claude', outcome: 'delivered', closed: true }
    ]
  }));
  await tick();
  assert.strictEqual(byId.get('workers').hidden, false, 'queued handoff shows a worker row');
  assert.strictEqual(byClass('workers-list', 'is-queued').length, 1, 'only open queued rows render');

  window.__omnix.emit('paneOpen', {
    paneKey: 'ho:tugas', title: 'tugas', handoff: 'tugas', agent: 'codex',
    cwd: '/repo/.omnix/worktrees/tugas', status: 'working'
  });
  await tick();
  const opened = lastReq('paneState');
  assert.strictEqual(opened.args.paneKey, 'ho:tugas', 'attached pane publishes its state');
  assert.strictEqual(opened.args.handoff, 'tugas', 'pane registry carries the handoff name');
  const workerRows = byClass('workers-list', 'worker');
  assert.ok(workerRows.length >= 2, 'live worker and queued handoff both listed');

  const sendBtn = all(byId.get('workers-list'),
    (n) => n.className === 'worker-btn' && n.textContent === 'send')[0];
  assert.ok(sendBtn, 'a live agent worker offers send');
  sendBtn.handlers.click(noop());
  assert.strictEqual(byId.get('agent-send-modal').hidden, false, 'send modal opens');
  byId.get('agent-send-text').value = 'lanjutkan';
  byId.get('agent-send-ok').handlers.click(noop());
  const sendReq = lastReq('agentSend');
  assert.strictEqual(sendReq.args.target, 'ho:tugas', 'send targets the pane key');
  assert.strictEqual(sendReq.args.text, 'lanjutkan', 'send carries the message');
  window.__omnix.reply(sendReq.id, true, '{"ok":true,"paneKey":"ho:tugas","state":"working"}');
  await tick();
  assert.strictEqual(byId.get('agent-send-modal').hidden, true, 'send modal closes on success');

  byId.get('gc-preview').handlers.click(noop());
  const gcPreviewReq = lastReq('gc');
  assert.strictEqual(gcPreviewReq.args.dryRun, true, 'cleanup opens with a dry run');
  window.__omnix.reply(gcPreviewReq.id, true,
    '{"dryRun":true,"actions":["close selesai (merged)"]}');
  await tick();
  assert.strictEqual(byId.get('gc-modal').hidden, false, 'cleanup preview opens');
  assert.match(byId.get('gc-actions').textContent, /merged/, 'preview lists the action');
  byId.get('gc-run').handlers.click(noop());
  const gcRunReq = lastReq('gc');
  assert.strictEqual(gcRunReq.args.dryRun, false, 'run applies the same cleanup');
  window.__omnix.reply(gcRunReq.id, true, '{"dryRun":false,"actions":["closed selesai"]}');
  await tick();
  assert.strictEqual(byId.get('gc-modal').hidden, true, 'cleanup closes after running');

  window.__omnix.emit('paneClosed', { paneKey: 'ho:tugas' });
  await tick();
  assert.strictEqual(byClass('workers-list', 'is-orchestrator').length, 0,
    'closed pane leaves the registry');

  // ---- settings / about ----
  settingsNavButtons[3].handlers.click(noop);
  await tick();
  assert.strictEqual(byId.get('settings-about').hidden, false, 'about pane visible');
  assert.strictEqual(byId.get('settings-dev').hidden, true, 'dev pane hidden');
  assert.match(byId.get('about-version').textContent, /v0\.1\.0/, 'about shows the version');
  byId.get('about-support').handlers.click(noop);
  const supportReq = lastReq('supportOpen');
  assert.strictEqual(supportReq.cmd, 'supportOpen', 'support opens the fixed Tako page');
  window.__omnix.reply(supportReq.id, true, '{"ok":true,"url":"https://tako.id/Zwart04/gift"}');
  await tick();

  // ---- first-run setup ----
  docHandlers.keydown({ key: 'k', ctrlKey: true, preventDefault: noop });
  byId.get('palette-input').value = 'Setup Omnix Again';
  byId.get('palette-input').handlers.input(noop());
  byId.get('palette-input').handlers.keydown({ key: 'Enter', preventDefault: noop });
  await tick();
  const resetSetupReq = lastReq('setupReset');
  assert.ok(resetSetupReq, 'replaying setup resets the one-time flag');
  window.__omnix.reply(resetSetupReq.id, true,
    '{"ok":true,"setup":{"needed":true,"complete":false,' +
    '"supportUrl":"https://tako.id/Zwart04/gift"}}');
  await tick();
  assert.strictEqual(byId.get('setup-modal').hidden, false, 'wizard opens on demand');
  assert.strictEqual(byId.get('setup-step-0').hidden, false, 'wizard starts at welcome');
  byId.get('setup-next').handlers.click(noop());
  assert.strictEqual(byId.get('setup-step-1').hidden, false, 'project step follows');
  byId.get('setup-next').handlers.click(noop());
  assert.strictEqual(byId.get('setup-step-2').hidden, false, 'agent step follows');
  const setupRows = all(byId.get('setup-agents'),
    (n) => String(n.className).split(/\s+/).indexOf('setup-agent') >= 0);
  assert.ok(setupRows.length >= 2, 'installed agents appear in setup');
  byId.get('setup-next').handlers.click(noop());
  assert.strictEqual(byId.get('setup-step-3').hidden, false, 'support step follows');
  assert.match(byId.get('setup-step-note').textContent, /4 of 4/, 'step counter is honest');
  byId.get('setup-next').handlers.click(noop());
  await tick();
  const setupDoneReq = lastReq('setupComplete');
  assert.ok(setupDoneReq, 'finish records the setup state');
  window.__omnix.reply(setupDoneReq.id, true,
    '{"ok":true,"setup":{"needed":false,"complete":true,"completedAt":1700000100}}');
  await tick();
  assert.strictEqual(byId.get('setup-modal').hidden, true, 'wizard closes after finishing');

  // window controls talk to the host; the WebView draws no fake chrome
  const beforeWindow = posted.length;
  byId.get('win-min').handlers.click(noop());
  byId.get('win-max').handlers.click(noop());
  byId.get('win-close').handlers.click(noop());
  assert.deepStrictEqual(posted.slice(beforeWindow).map((r) => r.cmd),
    ['windowMinimize', 'windowMaximize', 'windowClose'], 'window controls reach the host');

  console.log('selftest ok: rpc, reply/emit, sidebar, projects+sessions, settings, splits, palette, tabs, updates');
  process.exit(0);
})().catch((err) => {
  console.error(err && err.stack ? err.stack : err);
  process.exit(1);
});
