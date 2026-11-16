/* Smoke test for app.js: stubs the WebView, drives reply/emit, asserts no throw. */
'use strict';

const assert = require('node:assert');

function fakeEl(tag) {
  const node = {
    tagName: tag,
    children: [],
    className: '',
    textContent: '',
    value: '',
    hidden: false,
    title: '',
    disabled: false,
    type: '',
    dataset: {},
    attrs: {},
    style: {},
    handlers: {},
    classList: { add() {}, remove() {}, contains() { return false; } },
    appendChild(child) { node.children.push(child); return child; },
    removeChild(child) { node.children = node.children.filter((x) => x !== child); return child; },
    addEventListener(type, fn) { node.handlers[type] = fn; },
    setAttribute(key, val) { node.attrs[key] = val; },
    getAttribute(key) { return Object.prototype.hasOwnProperty.call(node.attrs, key) ? node.attrs[key] : null; },
    focus() {},
    getBoundingClientRect() { return { x: 10, y: 20, width: 400, height: 300 }; }
  };
  return node;
}

const byId = new Map();
[
  'toasts', 'status-last', 'status-left', 'status-right', 'app-version', 'tab-list',
  'workspace-list', 'panes', 'new-terminal', 'new-browser',
  'browser-url', 'browser-engine', 'browser-open', 'browser-native', 'browser-text',
  'search-form', 'search-query', 'search-agent', 'search-conversation', 'search-results', 'search-meta',
  'handoff-form', 'handoff-name', 'handoff-agent', 'handoff-prompt', 'handoff-base', 'handoff-result',
  'docs-frame'
].forEach((id) => byId.set(id, fakeEl(id)));

const paneByKind = {
  browser: fakeEl('section'),
  search: fakeEl('section'),
  handoff: fakeEl('section'),
  docs: fakeEl('section')
};

const posted = [];

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
  getElementById(id) { return byId.get(id) || null; },
  createElement(tag) { return fakeEl(tag); },
  createTextNode(text) { return { textContent: text }; },
  querySelector(sel) {
    const match = /data-kind="([a-z]+)"/.exec(sel);
    return match ? (paneByKind[match[1]] || null) : null;
  },
  querySelectorAll() { return []; },
  addEventListener() {}
};

require('./app.js');

(async function () {
  assert.ok(window.__omnix && typeof window.__omnix.reply === 'function', 'reply exposed');
  assert.ok(typeof window.__omnix.emit === 'function', 'emit exposed');

  assert.strictEqual(posted.length, 3, 'boot sent appInfo, status, ptySpawn');
  posted.forEach((req) => {
    assert.strictEqual(typeof req.id, 'number', 'numeric id');
    assert.strictEqual(typeof req.cmd, 'string', 'string cmd');
    assert.strictEqual(typeof req.args, 'object', 'object args');
  });
  assert.deepStrictEqual(posted.map((r) => r.cmd), ['appInfo', 'status', 'ptySpawn']);
  assert.strictEqual(posted[2].args.paneKey, 'tab:1', 'first terminal pane key');

  assert.ok(!posted.some((r) => r.cmd === 'ptyResize'), 'no resize before spawn reply');

  const [infoReq, statusReq, spawnReq] = posted;

  window.__omnix.reply(infoReq.id, true, '{"version":"0.1.0","root":"/repo","engine":{"lightpanda":true,"version":"9.9"}}');
  window.__omnix.reply(statusReq.id, false, '{"code":"internal","message":"boom"}');
  window.__omnix.reply(spawnReq.id, true, '{"paneKey":"tab:1","pid":42}');
  await new Promise((r) => setImmediate(r));

  assert.match(byId.get('status-left').textContent, /omnix 0\.1\.0/, 'version rendered');
  assert.match(byId.get('status-last').textContent, /pid 42/, 'spawn pid recorded');
  assert.ok(byId.get('toasts').children.length >= 1, 'rejected status toasted');

  // Defensive paths must never throw.
  window.__omnix.reply(9999, true, '{}');
  window.__omnix.reply(9998, false, 'not json');
  window.__omnix.emit('nope', '{}');
  window.__omnix.emit('ptyData', '{"paneKey":"tab:1","dataBase64":"aGk="}');
  window.__omnix.emit('ptyData', '{"paneKey":"tab:1","dataBase64":"@@@"}');
  window.__omnix.emit('ptyExit', '{"paneKey":"tab:1","code":0}');
  window.__omnix.emit('browserNav', 'not json');
  window.__omnix.emit('toast', '{"level":"warn","message":"host says hi"}');

  assert.match(byId.get('status-last').textContent, /code 0/, 'ptyExit rendered');
  assert.ok(byId.get('toasts').children.length >= 2, 'toast event rendered');

  // Search panel: rpc args shape, then empty result set.
  byId.get('search-query').value = 'blank restore';
  byId.get('search-conversation').checked = true;
  byId.get('search-agent').value = 'codex';
  await byId.get('search-form').handlers.submit({ preventDefault() {} });
  const searchReq = posted[posted.length - 1];
  assert.strictEqual(searchReq.cmd, 'historySearch');
  assert.deepStrictEqual(searchReq.args, {
    query: 'blank restore', limit: 20, agent: 'codex', conversation: true
  });
  window.__omnix.reply(searchReq.id, true, '{"hits":[{"agent":"codex","sessionId":"s-1","title":"t","cwd":"/r","resumeCommand":"codex resume s-1","snippet":"snip"}]}');
  await new Promise((r) => setImmediate(r));
  assert.match(byId.get('search-meta').textContent, /1 hit for "blank restore"/);

  // Handoff panel: required fields, then rendering.
  byId.get('handoff-name').value = 'auth-fix';
  byId.get('handoff-prompt').value = 'fix it';
  byId.get('handoff-base').value = 'main';
  await byId.get('handoff-form').handlers.submit({ preventDefault() {} });
  const handoffReq = posted[posted.length - 1];
  assert.strictEqual(handoffReq.cmd, 'handoff');
  assert.deepStrictEqual(handoffReq.args, {
    name: 'auth-fix', prompt: 'fix it', agent: 'codex', base: 'main'
  });
  window.__omnix.reply(handoffReq.id, true, '{"outcome":"unconfirmed","worktree":"/w","branch":"omnix/auth-fix","brief":"/b.md"}');
  await new Promise((r) => setImmediate(r));
  assert.strictEqual(byId.get('handoff-result').hidden, false, 'handoff result shown');
  assert.ok(byId.get('handoff-result').children.length >= 3, 'handoff rows rendered');

  // Browser tab: scheme guard, native bounds, close on tab switch, lightpanda fetch.
  byId.get('new-browser').handlers.click({ preventDefault() {} });
  byId.get('browser-url').value = 'ftp://example.com';
  byId.get('browser-open').handlers.click({ preventDefault() {} });
  await new Promise((r) => setImmediate(r));
  assert.ok(!posted.some((r) => r.cmd === 'browserOpen'), 'ftp URL rejected');

  byId.get('browser-url').value = 'example.com';
  byId.get('browser-engine').value = 'native';
  byId.get('browser-open').handlers.click({ preventDefault() {} });
  await new Promise((r) => setImmediate(r));
  const openReq = posted[posted.length - 1];
  assert.strictEqual(openReq.cmd, 'browserOpen');
  assert.strictEqual(openReq.args.url, 'https://example.com', 'scheme added');
  assert.deepStrictEqual(openReq.args.rect, { x: 10, y: 20, width: 400, height: 300 }, 'content rect sent');
  window.__omnix.reply(openReq.id, true, '{"url":"https://example.com"}');
  await new Promise((r) => setImmediate(r));

  byId.get('tab-list').children[0].handlers.click({ stopPropagation() {} });
  await new Promise((r) => setImmediate(r));
  assert.ok(posted.some((r) => r.cmd === 'browserClose'), 'browser closed on tab switch');

  byId.get('browser-engine').value = 'lightpanda';
  byId.get('browser-engine').handlers.change({});
  byId.get('browser-open').handlers.click({ preventDefault() {} });
  await new Promise((r) => setImmediate(r));
  const fetchReq = posted[posted.length - 1];
  assert.strictEqual(fetchReq.cmd, 'browserFetch');
  assert.deepStrictEqual(fetchReq.args, { url: 'https://example.com', dump: 'markdown' });
  window.__omnix.reply(fetchReq.id, true, '{"text":"# page"}');
  await new Promise((r) => setImmediate(r));
  assert.strictEqual(byId.get('browser-text').textContent, '# page', 'fetched text rendered');

  console.log('selftest ok: rpc, reply, emit, search, handoff, browser');
  process.exit(0);
})();
