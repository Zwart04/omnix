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
      child.parentNode = node;
      node.children.push(child);
      return child;
    },
    removeChild(child) {
      node.children = node.children.filter((x) => x !== child);
      child.parentNode = null;
      return child;
    },
    addEventListener(type, fn) { node.handlers[type] = fn; },
    setAttribute(key, val) { node.attrs[key] = String(val); },
    getAttribute(key) {
      return Object.prototype.hasOwnProperty.call(node.attrs, key) ? node.attrs[key] : null;
    },
    removeAttribute(key) { delete node.attrs[key]; },
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
  'tabstrip-tabs', 'new-terminal', 'new-browser', 'pane-empty', 'pane-empty-terminal',
  'tab-new', 'tab-new-menu', 'strip-command', 'tab-new-terminal', 'tab-new-browser',
  'browser-url', 'browser-open', 'browser-engine', 'browser-native', 'browser-text',
  'search-form', 'search-query', 'search-conversation', 'search-agent', 'search-results', 'search-meta',
  'handoff-form', 'handoff-name', 'handoff-agent', 'handoff-base', 'handoff-prompt', 'handoff-result',
  'docs-frame', 'toasts', 'statusbar', 'status-left', 'status-right', 'status-last',
  'history-list', 'history-search', 'history-meta', 'history-count', 'history-rescan',
  'history-agents', 'history-scope', 'history-collapse', 'history-expand', 'agents-panel',
  'project-tree', 'project-refresh', 'cmd-btn', 'rail-agents',
  'handoff-modal', 'handoff-close', 'handoff-cancel', 'handoff-start',
  'handoff-target', 'handoff-source-title', 'handoff-source-sub', 'handoff-cwd',
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
  docs: fakeEl('section')
};

const panelButtons = ['search', 'handoff', 'docs'].map((panel) => {
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
  assert.strictEqual(bootProjectsReq.cmd, 'historyProjects', 'boot lists cached projects');
  window.__omnix.reply(bootProjectsReq.id, true, JSON.stringify({ root: '/repo', projects: [] }));
  await tick();

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
    /^Scanned 290 files \u00b7 44,846 messages \u00b7 /, 'scan status line');

  const agentChips = byId.get('history-agents').children;
  assert.strictEqual(agentChips.length, 4, 'All agents plus one chip per scanned agent');
  assert.strictEqual(agentChips[1].textContent, 'codex 14', 'agent chip carries its count');
  assert.strictEqual(agentChips[2].textContent, 'claude 108', 'agent chip carries its count');

  const cards = byClass('history-list', 'sess');
  assert.strictEqual(cards.length, 3, 'one card per session');
  const metaRow = all(cards[0], (n) => n.className === 'sess-meta')[0];
  assert.deepStrictEqual(metaRow.children.map((c) => c.textContent),
    ['299 msgs', '7m ago', 'claude'], 'card meta row');

  // ---- historyProjects fallback tree ----
  const projectsReq = posted[posted.length - 1];
  assert.strictEqual(projectsReq.cmd, 'historyProjects', 'empty workspaces fall back to historyProjects');
  window.__omnix.reply(projectsReq.id, true, JSON.stringify({
    root: '/repo',
    projects: [
      {
        path: '/home/zwart/git/omnix', name: 'omnix', sessions: 12, messages: 3400,
        updatedAt: Math.floor(Date.now() / 1000) - 7200, agents: ['claude', 'codex']
      },
      {
        path: '/home/zwart/git/crm-sb', name: 'crm-sb', sessions: 108, messages: 44000,
        updatedAt: Math.floor(Date.now() / 1000) - 120, agents: ['claude', 'codex', 'opencode']
      }
    ]
  }));
  await tick();
  const projRows = byClass('workspace-list', 'proj-row');
  assert.strictEqual(projRows.length, 2, 'one row per project');
  assert.strictEqual(all(projRows[0], (n) => n.className === 'ws-name')[0].textContent, 'crm-sb',
    'projects sorted by updatedAt desc');
  assert.strictEqual(all(projRows[1], (n) => n.className === 'ws-name')[0].textContent, 'omnix',
    'older project second');
  assert.strictEqual(projRows[0].children[1].children[0].textContent, '108 sessions',
    'project row carries its session count');
  assert.strictEqual(all(projRows[0], (n) => n.className === 'proj-time')[0].textContent, '2m ago',
    'project row carries relative time');
  const agentIcons = byClass('workspace-list', 'icon');
  assert.strictEqual(agentIcons.length, 5, 'agent icons rendered');
  assert.ok(agentIcons.every((n) => /^<svg viewBox=/.test(n.innerHTML || '')), 'icons are brand SVGs');

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
  byId.get('palette-input').value = 'terminal';
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

  // keyboard: arrows move the active row, escape closes, focus returns
  docHandlers.keydown({ key: 'k', ctrlKey: true, preventDefault: noop });
  assert.strictEqual(byId.get('palette').hidden, false, 'Ctrl+K opens the palette');
  const rows = byId.get('palette-list').children;
  assert.strictEqual(rows.length, 9, 'all commands listed with an empty query');
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

  byId.get('new-browser').handlers.click(noop());
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

  console.log('selftest ok: rpc, reply/emit, scan+sessions+projects, chips, resume, palette, tabs');
  process.exit(0);
})().catch((err) => {
  console.error(err && err.stack ? err.stack : err);
  process.exit(1);
});
