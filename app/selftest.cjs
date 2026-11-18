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
  'docs-frame', 'toasts', 'statusbar', 'status-left', 'status-right', 'status-last',
  'history-list', 'history-search', 'history-meta', 'history-count', 'history-rescan',
  'history-agents', 'history-scope', 'history-collapse', 'history-expand', 'agents-panel',
  'project-tree', 'project-refresh', 'project-add-icon', 'cmd-btn', 'side-settings', 'rail-agents',
  'side-workspace', 'workspace-group', 'workspace-group-body', 'workspace-collapse',
  'workspace-count', 'workspace-refresh', 'project-count', 'rail-agents-count',
  'usage-chip', 'usage-modal', 'usage-close', 'usage-refresh', 'usage-ram', 'usage-list', 'usage-note',
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

global.window = {
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
  addEventListener(type, fn) { docHandlers[type] = fn; }
};
document.activeElement = byId.get('cmd-btn');

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

  const bootProjectsReq = posted[posted.length - 1];
  assert.strictEqual(bootProjectsReq.cmd, 'projects', 'boot lists the saved projects');
  window.__omnix.reply(bootProjectsReq.id, true, JSON.stringify({
    projects: [
      { path: '/home/zwart/git/omnix', name: 'omnix' },
      { path: '/home/zwart/git/crm-sb', name: 'crm-sb' }
    ]
  }));
  await tick();

  // saved projects render immediately; history only decorates them
  const bootHistReq = posted[posted.length - 1];
  assert.strictEqual(bootHistReq.cmd, 'historyProjects', 'history fills in session stats');
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
  assert.strictEqual(
    all(bootRows[0], (n) => n.className === 'proj-meta')[0].children[0].textContent,
    '108 sessions', 'saved project carries its indexed session count');
  assert.strictEqual(all(bootRows[0], (n) => n.className === 'proj-time')[0].textContent, '2m ago',
    'saved project carries its indexed relative time');
  assert.strictEqual(byId.get('project-count').textContent, '2', 'project count follows the list');
  const bootIcons = byClass('workspace-list', 'icon');
  assert.strictEqual(bootIcons.length, 7, 'two folder marks plus five agent marks');
  assert.ok(bootIcons.every((n) => /^<svg viewBox=/.test(n.innerHTML || '')), 'icons are inline SVGs');
  // one workspace title, editable and collapsible; history only decorates projects
  assert.strictEqual(byId.get('side-workspace').textContent, 'repo', 'sidebar names the root');
  byId.get('workspace-collapse').handlers.click(noop);
  assert.strictEqual(byId.get('workspace-group-body').hidden, true, 'workspace collapses');
  byId.get('workspace-collapse').handlers.click(noop);
  assert.strictEqual(byId.get('workspace-group-body').hidden, false, 'workspace expands');

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
  assert.strictEqual(byId.get('workspace-count').textContent, '2', 'workspace count follows projects');
  const projChev = byClass('workspace-list', 'proj-chev')
    .find((n) => !n.classList.contains('is-empty'));
  assert.ok(projChev, 'tracked sessions expose a twisty');
  projChev.handlers.click({ stopPropagation: noop });
  assert.strictEqual(byClass('workspace-list', 'sess-row').length, 1,
    'project expands into its tracked session');

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
  assert.deepStrictEqual(sessionsReq.args, { scope: 'all', limit: 80 }, 'default scope is all');

  window.__omnix.reply(sessionsReq.id, true, JSON.stringify({
    scope: 'all',
    root: '/repo',
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
        cwd: '/repo', updatedAt: Math.floor(Date.now() / 1000) - 300000,
        messageCount: 7, resumeCommand: 'opencode --session uuid-3',
        preview: 'project tree rows'
      }
    ]
  }));
  await tick();

  assert.strictEqual(byId.get('history-count').textContent, '290 sessions', 'session count text');
  assert.match(byId.get('history-meta').textContent,
    /^290 updated \u00b7 288 unchanged \u00b7 44,846 new messages \u00b7 /,
    'incremental scan status line');
  assert.strictEqual(byId.get('rail-agents-count').hidden, false, 'agents badge appears with history');
  assert.strictEqual(byId.get('rail-agents-count').textContent, '290', 'badge carries the session total');

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
  window.__omnix.reply(req.id, true, '{"scope":"project","root":"/repo","sessions":[]}');
  await tick();
  assert.strictEqual(byClass('history-list', 'empty').length, 1, 'empty state after empty result');

  scopeChips[2].handlers.click(noop());
  await tick();
  req = posted[posted.length - 1];
  assert.strictEqual(req.args.scope, 'all', 'all chip sends scope all');
  window.__omnix.reply(req.id, true, JSON.stringify({
    scope: 'all',
    root: '/repo',
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
  window.__omnix.reply(req.id, true, '{"scope":"all","root":"/repo","sessions":[]}');
  await tick();
  byId.get('history-agents').children[0].handlers.click(noop());
  await tick();
  req = posted[posted.length - 1];
  assert.ok(!('agent' in req.args), 'All agents clears the filter');
  window.__omnix.reply(req.id, true, JSON.stringify({
    scope: 'all',
    root: '/repo',
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
  window.__omnix.reply(req.id, true, '{"scope":"all","root":"/repo","sessions":[]}');
  await tick();

  // ---- Resume button spawns pty with agent + sessionId ----
  byId.get('history-search').value = '';
  byId.get('history-search').handlers.input(noop());
  await new Promise((r) => setTimeout(r, 260));
  req = posted[posted.length - 1];
  window.__omnix.reply(req.id, true, JSON.stringify({
    scope: 'all',
    root: '/repo',
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
  byId.get('cmd-btn').handlers.click(noop());
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
  byId.get('cmd-btn').handlers.click(noop());
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
  const paneControls = byClass('panes', 'pane-controls');
  assert.strictEqual(paneControls.length, 2, 'each pane carries split and close controls');
  assert.strictEqual(paneControls[0].children[0].disabled, false, 'a pair can split again');
  // a third pane joins the same group
  byId.get('cmd-btn').handlers.click(noop);
  byId.get('palette-input').value = 'Split Terminal';
  byId.get('palette-input').handlers.input(noop);
  byId.get('palette-input').handlers.keydown({ key: 'Enter', preventDefault: noop });
  await tick();
  const thirdSpawn = posted[posted.length - 1];
  assert.strictEqual(thirdSpawn.cmd, 'ptySpawn', 'third pane spawns');
  window.__omnix.reply(thirdSpawn.id, true, '{"paneKey":"tab:3","pid":80}');
  await tick();
  assert.strictEqual(splitWraps[0].children.length, 3, 'three panes share one wrapper');
  assert.strictEqual(splitWraps[0].dataset.panes, '3', 'wrapper records the pane count');
  const thirdControls = byClass('panes', 'pane-controls')[2];
  thirdControls.children[1].handlers.click({ stopPropagation: noop });
  await tick();
  const splitKill = posted[posted.length - 1];
  assert.strictEqual(splitKill.cmd, 'ptyKill', 'pane close control kills its pty');
  assert.strictEqual(splitWraps[0].children.length, 2, 'closing keeps the remaining pair');
  window.__omnix.reply(splitKill.id, true, '{"ok":true}');
  await tick();

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
  docHandlers.keydown({ key: 'k', ctrlKey: true, preventDefault: noop });
  assert.strictEqual(byId.get('palette').hidden, false, 'Ctrl+K opens the palette');
  const rows = byId.get('palette-list').children;
  assert.strictEqual(rows.length, 14, 'all commands listed with an empty query');
  byId.get('palette-input').handlers.keydown({ key: 'ArrowDown', preventDefault: noop });
  assert.strictEqual(byId.get('palette-list').children[1].getAttribute('aria-selected'), 'true',
    'arrow down moves the selection');
  byId.get('palette-input').handlers.keydown({ key: 'Escape', preventDefault: noop });
  assert.strictEqual(byId.get('palette').hidden, true, 'escape closes the palette');
  assert.strictEqual(document.activeElement, byId.get('cmd-btn'),
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

  // ---- error state ----
  byId.get('history-rescan').handlers.click(noop());
  await tick();
  const rescanReq = posted[posted.length - 1];
  assert.strictEqual(rescanReq.cmd, 'historyScan', 'rescan re-runs the scan');
  window.__omnix.reply(rescanReq.id, false, '{"code":"internal","message":"db locked"}');
  await tick();

  const listingAfterFail = posted[posted.length - 1];
  assert.strictEqual(listingAfterFail.cmd, 'historySessions', 'listing still runs after a failed scan');
  window.__omnix.reply(listingAfterFail.id, true, '{"scope":"all","root":"/repo","sessions":[]}');
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
  assert.strictEqual(posted[posted.length - 1].cmd, 'projects', 'add refreshes the list');
  window.__omnix.reply(posted[posted.length - 1].id, true, JSON.stringify({
    projects: [
      { path: '/home/zwart/git/omnix', name: 'omnix' },
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
  assert.strictEqual(byClass('workspace-list', 'proj-row').length, 2, 'new row is listed');
  console.log('selftest ok: rpc, reply/emit, sidebar, projects+sessions, settings, splits, palette, tabs');
  process.exit(0);
})().catch((err) => {
  console.error(err && err.stack ? err.stack : err);
  process.exit(1);
});
