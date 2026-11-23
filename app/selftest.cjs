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
  'tabstrip-tabs', 'new-project', 'pane-empty', 'pane-empty-terminal', 'pane-empty-browser',
  'project-modal', 'project-close', 'project-cancel', 'project-add', 'project-path', 'project-error',
  'tab-new', 'tab-new-menu', 'strip-command', 'tab-new-terminal', 'tab-new-browser',
  'browser-url', 'browser-open', 'browser-engine', 'browser-native', 'browser-text',
  'search-form', 'search-query', 'search-conversation', 'search-agent', 'search-results', 'search-meta',
  'handoff-form', 'handoff-name', 'handoff-agent', 'handoff-base', 'handoff-prompt', 'handoff-result',
  'docs-frame', 'toasts', 'statusbar', 'status-left', 'status-right', 'status-last', 'update-chip',
  'history-list', 'history-search', 'history-meta', 'history-count', 'history-rescan',
  'history-agents', 'history-scope', 'history-collapse', 'history-expand', 'agents-panel',
  'history-filter', 'history-sort', 'history-collapse-all', 'history-time',
  'project-tree', 'project-refresh', 'project-add-icon', 'side-search', 'side-toggle', 'side-settings', 'rail-agents',
  'win-min', 'win-max', 'win-close',
  'side-workspace', 'workspace-group', 'workspace-group-body', 'workspace-collapse',
  'workspace-refresh',
  'usage-chip', 'usage-open', 'usage-strip', 'usage-strip-refresh', 'usage-modal', 'usage-close',
  'usage-refresh', 'usage-ram', 'usage-list', 'usage-note',
  'handoff-modal', 'handoff-close', 'handoff-cancel', 'handoff-start',
  'handoff-target', 'handoff-source-title', 'handoff-source-sub', 'handoff-cwd',
  'agent-settings', 'agent-settings-meta', 'agent-settings-refresh',
  'agent-settings-path', 'agent-default-perms', 'settings-back', 'settings-search',
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

const posted = [];
const docHandlers = {};

const storage = new Map();

global.window = {
  localStorage: {
    getItem(key) { return storage.has(key) ? storage.get(key) : null; },
    setItem(key, value) { storage.set(key, String(value)); },
    removeItem(key) { storage.delete(key); }
  },
  ipc: { postMessage(raw) { posted.push(JSON.parse(raw)); } },
  atob(value) { return Buffer.from(value, 'base64').toString('binary'); },
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
    '"agents":[{"name":"codex","bin":"codex"},{"name":"claude","bin":"claude"}]}');
  window.__omnix.reply(statusReq.id, true,
    '{"sessions":0,"messages":0,"dbBytes":0,"workspaces":[]}');
  await tick();

  const bootProjectsReq = lastReq('projects');
  assert.ok(bootProjectsReq, 'boot lists the saved projects');
  window.__omnix.reply(bootProjectsReq.id, true, JSON.stringify({
    projects: [
      { path: '/home/zwart/git/omnix', name: 'omnix' },
      { path: '/home/zwart/git/crm-sb', name: 'crm-sb' }
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
        path: '/home/zwart/git/omnix', name: 'omnix', sessions: 12, messages: 3400,
        updatedAt: Math.floor(Date.now() / 1000) - 7200, agents: ['claude', 'codex']
      },
      {
        path: '/home/zwart/git/crm-sb', name: 'crm-sb', sessions: 108, messages: 44000,
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
  assert.strictEqual(all(bootRows[0], (n) => n.className === 'ws-name')[0].textContent, 'crm-sb',
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
  byId.get('side-search').value = 'crm';
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
      { path: '/home/zwart/git/omnix', branch: 'main', sleeping: false, sessions: [
        { paneKey: 'x:1', agent: 'codex', sessionId: 'sess-1', cwd: '/home/zwart/git/omnix',
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
    JSON.parse(window.localStorage.getItem('omnix.projExpanded'))['/home/zwart/git/omnix'],
    true, 'expanded project state persists');

  // removing a project only touches the saved list
  const removeBtn = byClass('workspace-list', 'proj-remove')[0];
  assert.ok(removeBtn, 'every project row has a remove control');
  removeBtn.handlers.click({ stopPropagation: noop });
  await tick();
  const removeReq = posted[posted.length - 1];
  assert.strictEqual(removeReq.cmd, 'projectRemove', 'remove asks the host');
  assert.deepStrictEqual(removeReq.args, { path: '/home/zwart/git/crm-sb' }, 'remove sends the path');
  window.__omnix.reply(removeReq.id, true, JSON.stringify({
    projects: [{ path: '/home/zwart/git/omnix', name: 'omnix' }]
  }));
  await tick();
  assert.strictEqual(byClass('workspace-list', 'proj-row').length, 1, 'row disappears after remove');

  // opening the agents panel starts the index walk
  byId.get('rail-agents').handlers.click(noop());
  await tick();
  const scanReq = posted[posted.length - 1];
  assert.strictEqual(scanReq.cmd, 'historyScan', 'expanding Agents starts the scan');
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
    roots: [{ agent: 'codex', path: '/home/zwart/.codex/sessions' }]
  }));
  await tick();

  assert.match(byId.get('status-left').textContent, /omnix 0\.1\.0/, 'version rendered');

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
    ['299 msgs', '7m ago', 'claude'], 'card meta row');


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

  const resumeBtn = byClass('history-list', 'sess-main')[0];
  assert.ok(resumeBtn, 'session card renders');
  resumeBtn.handlers.click(noop());
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
  assert.strictEqual(byClass('panes', 'pane-head-btn').length, 6,
    'header carries split, handoff, and close');
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

  // keyboard: arrows move the active row, escape closes, focus returns
  assert.strictEqual(docHandlers['keydown:capture'], true,
    'app shortcuts listen before xterm swallows the key');
  docHandlers.keydown({ key: 'k', ctrlKey: true, preventDefault: noop });
  assert.strictEqual(byId.get('palette').hidden, false, 'Ctrl+K opens the palette');
  const rows = byId.get('palette-list').children;
  assert.strictEqual(rows.length, 15, 'all commands listed with an empty query');
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

  assert.match(byId.get('status-last').textContent, /code 0/, 'ptyExit rendered');
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
    panes: []
  }));
  await tick();
  const budgetToasts = () => byId.get('toasts').children
    .filter((n) => /budget/.test(String(n.textContent)));
  assert.strictEqual(budgetToasts().length, 1, 'over-budget toast appears');
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
    '{"agent":"codex","cwd":"/repo","brief":"/repo/.omnix/handoffs/1-to-codex.md","messages":12}');
  await tick();
  const handoffSpawn = posted[posted.length - 1];
  assert.strictEqual(handoffSpawn.cmd, 'ptySpawn', 'handoff starts a new agent session');
  assert.strictEqual(handoffSpawn.args.command, 'codex', 'handoff runs the target CLI');
  assert.strictEqual(handoffSpawn.args.cwd, '/repo');
  window.__omnix.reply(handoffSpawn.id, true, '{"paneKey":"hf:codex:1","pid":79}');
  await tick();
  assert.strictEqual(byId.get('handoff-modal').hidden, true, 'handoff modal closes');

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
  window.__omnix.reply(pickReq.id, true, JSON.stringify({ path: '/home/zwart/git/newrepo' }));
  await tick();
  const addReq = posted[posted.length - 1];
  assert.strictEqual(addReq.cmd, 'projectAdd', 'picked folder is added');
  assert.deepStrictEqual(addReq.args, { path: '/home/zwart/git/newrepo' }, 'add sends the folder');
  window.__omnix.reply(addReq.id, true, JSON.stringify({
    projects: [
      { path: '/home/zwart/git/omnix', name: 'omnix' },
      { path: '/home/zwart/git/newrepo', name: 'newrepo' }
    ]
  }));
  await tick();
  const addProjectsReq = lastReq('projects');
  assert.ok(addProjectsReq, 'add refreshes the list');
  window.__omnix.reply(addProjectsReq.id, true, JSON.stringify({
    projects: [
      { path: '/home/zwart/git/omnix', name: 'omnix' },
      { path: '/home/zwart/git/omnix/packages/tools', name: 'omnix' },
      { path: '/home/zwart/git/newrepo', name: 'newrepo' }
    ]
  }));
  await tick();
  assert.strictEqual(posted[posted.length - 1].cmd, 'historyProjects', 'add reloads session stats');
  window.__omnix.reply(posted[posted.length - 1].id, true, JSON.stringify({ projects: [] }));
  await tick();
  const spawnReq = posted[posted.length - 1];
  assert.strictEqual(spawnReq.cmd, 'ptySpawn', 'add opens a terminal for the new project');
  assert.strictEqual(spawnReq.args.cwd, '/home/zwart/git/newrepo', 'terminal starts in the folder');
  window.__omnix.reply(spawnReq.id, true, '{"paneKey":"tab:new","pid":90}');
  await tick();
  assert.strictEqual(byClass('workspace-list', 'proj-row').length, 3, 'new row is listed');

  // duplicate basenames name their parent, and nested projects own separate tabs
  const projRows = byClass('workspace-list', 'proj-row');
  const labels = projRows.map((n) => all(n, (x) => String(x.className) === 'ws-name')[0].textContent);
  assert.ok(labels.indexOf('omnix \u00b7 git') >= 0, 'parent project label is disambiguated');
  assert.ok(labels.indexOf('omnix \u00b7 packages') >= 0, 'nested project label is disambiguated');

  const toolsRow = projRows.find((n) => n.title === '/home/zwart/git/omnix/packages/tools');
  assert.ok(toolsRow, 'nested project row is present');
  toolsRow.handlers.click(noop());
  await tick();
  const toolsSpawn = posted[posted.length - 1];
  assert.strictEqual(toolsSpawn.cmd, 'ptySpawn', 'nested project opens its own terminal');
  assert.strictEqual(toolsSpawn.args.cwd, '/home/zwart/git/omnix/packages/tools',
    'nested project cwd is used');
  window.__omnix.reply(toolsSpawn.id, true, '{"paneKey":"tab:tools","pid":90}');
  await tick();

  // new terminals inherit the folder of the project the user is working in
  const omnixRow = projRows.find((n) => n.title === '/home/zwart/git/omnix');
  assert.ok(omnixRow, 'omnix project row is present');
  omnixRow.handlers.click(noop());
  await tick();
  const projectSpawn = posted[posted.length - 1];
  assert.strictEqual(projectSpawn.cmd, 'ptySpawn',
    'clicking the parent project must not reuse the nested tab');
  assert.strictEqual(projectSpawn.args.cwd, '/home/zwart/git/omnix', 'project cwd is used');
  window.__omnix.reply(projectSpawn.id, true, '{"paneKey":"tab:9","pid":91}');
  await tick();

  // a terminal in a nested project must not also appear under its parent
  const projWraps = byClass('workspace-list', 'proj');
  function wrapFor(path) {
    return projWraps.find((w) => {
      const row = all(w, (n) => String(n.className).split(/\s+/).includes('proj-row'))[0];
      return row && row.title === path;
    });
  }
  const parentWrap = wrapFor('/home/zwart/git/omnix');
  const toolsWrap = wrapFor('/home/zwart/git/omnix/packages/tools');
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
  assert.ok(String(stripTitles[0].title).indexOf('/home/zwart/git/omnix') >= 0,
    'tab tooltip carries the active project path');
  assert.ok(String(stripTitles[0].title).indexOf('/packages/tools') < 0,
    'nested project tab is hidden while the parent project is active');
  const activeRows = byClass('workspace-list', 'proj').filter((n) =>
    String(n.className).split(/\s+/).includes('is-active'));
  assert.strictEqual(activeRows.length, 1, 'exactly one project is marked active');
  assert.ok(activeRows[0].children[0].title === '/home/zwart/git/omnix',
    'the active marker follows the clicked project');

  // the sidebar spinner turns only while the pane is printing
  const liveDot = byClass('workspace-list', 'work-dot')[0];
  assert.ok(liveDot, 'live project shows a status mark');
  window.__omnix.emit('ptyData', JSON.stringify({
    paneKey: projectSpawn.args.paneKey, dataBase64: 'aGk='
  }));
  await new Promise((resolve) => setTimeout(resolve, 800));
  assert.ok(byClass('workspace-list', 'busy').length >= 1,
    'printing pane animates its project mark');

  const beforeReuse = posted.length;
  omnixRow.handlers.click(noop());
  await tick();
  assert.strictEqual(posted.length, beforeReuse,
    'clicking an open project activates its tab instead of spawning another');

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
  assert.strictEqual(agentSpawn.args.cwd, '/home/zwart/git/omnix',
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
  const limits = byClass('usage-list', 'usage-limit');
  assert.strictEqual(limits.length, 2, 'one quota line per agent');
  const codexName = all(limits[0], (n) => n.className === 'usage-agent-name')[0];
  assert.strictEqual(codexName.textContent, 'Codex', 'agent name is readable');
  const codexSub = all(limits[0], (n) => n.className === 'usage-limit-sub')[0];
  assert.match(codexSub.textContent, /Free plan/, 'plan is shown');
  assert.match(codexSub.textContent, /resets in/, 'reset countdown is shown');
  const codexPct = all(limits[0], (n) => n.className === 'usage-window-pct')[0];
  assert.strictEqual(codexPct.textContent, '100%', 'the tightest window wins');
  const agPct = all(limits[1], (n) => n.className === 'usage-window-pct')[0];
  assert.strictEqual(agPct.textContent, '1%', 'antigravity reports remaining quota');

  // the same snapshot fills the Orca-style strip in the status bar
  const stripItems = byClass('usage-strip', 'usage-item');
  assert.strictEqual(stripItems.length, 2, 'bottom bar lists every agent');
  const stripPcts = all(byId.get('usage-strip'), (n) => n.className === 'usage-pct');
  assert.deepStrictEqual(stripPcts.map((n) => n.textContent), ['100%', '1%'],
    'bottom bar shows the tightest window percentage');
  const stripTags = all(byId.get('usage-strip'), (n) => n.className === 'usage-tag');
  assert.strictEqual(stripTags.length, 1, 'only antigravity needs a window tag');
  assert.strictEqual(stripTags[0].textContent, 'Gemini wk', 'window label is compact');
  const stripMarks = byClass('usage-strip', 'usage-mark');
  assert.strictEqual(stripMarks.length, 2, 'each entry has a brand mark');
  const stripResets = byClass('usage-strip', 'usage-reset');
  assert.strictEqual(stripResets.length, 1, 'codex keeps the reset countdown');
  assert.strictEqual(stripResets[0].textContent, '15m', 'reset countdown is compact');
  const beforeUsage = posted.length;
  byId.get('usage-strip-refresh').handlers.click(noop());
  assert.strictEqual(posted.length, beforeUsage + 1, 'strip refresh reads quotas again');
  assert.strictEqual(posted[posted.length - 1].cmd, 'usage', 'refresh uses the usage RPC');
  window.__omnix.reply(posted[posted.length - 1].id, true,
    '{"ram":{},"agents":[]}');
  await tick();
  assert.strictEqual(byClass('usage-strip', 'usage-strip-empty').length, 1,
    'an empty snapshot leaves a dash');

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
