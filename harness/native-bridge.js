// Bridge yang di-inject ke WebView native.
// Kontrak: setiap method mengembalikan JSON string supaya aman lewat evaluate_script_with_callback.
(() => {
  const refs = new Map();
  let nextRef = 1;
  let generation = 0;

  const roleOf = (el) => {
    const tag = el.tagName.toLowerCase();
    if (tag === "a") return "link";
    if (tag === "button") return "button";
    if (tag === "input") return el.type || "textbox";
    if (/^h[1-6]$/.test(tag)) return "heading";
    return tag;
  };

  const nameOf = (el) =>
    (el.getAttribute("aria-label") || el.innerText || el.value || el.placeholder || "")
      .trim()
      .replace(/\s+/g, " ")
      .slice(0, 200);

  const assign = (el) => {
    const ref = `@e${nextRef++}`;
    refs.set(ref, el);
    return ref;
  };

  const walk = (root, interactive, depth, out) => {
    if (depth < 0 || out.length > 2000) return;
    for (const el of root.children) {
      const isControl = ["a", "button", "input", "select", "textarea"].includes(
        el.tagName.toLowerCase(),
      );
      if (!interactive || isControl) {
        out.push({
          ref: assign(el),
          role: roleOf(el),
          name: nameOf(el),
          ...(el.href ? { url: el.href } : {}),
        });
      }
      walk(el, interactive, depth - 1, out);
    }
  };

  const snapshot = ({ interactive = true, depth = 8 } = {}) => {
    generation += 1;
    refs.clear();
    nextRef = 1;
    const tree = [];
    walk(document.body, interactive, depth, tree);
    return JSON.stringify({
      url: location.href,
      title: document.title,
      generation,
      tree,
      truncated: tree.length > 2000,
    });
  };

  const byRef = (ref) => {
    const el = refs.get(ref);
    if (!el || !el.isConnected) throw new Error("stale_ref");
    return el;
  };

  globalThis.__omnix = {
    snapshot,
    read: ({ selector = "body", maxChars = 20000 } = {}) => {
      const el = document.querySelector(selector) ?? document.body;
      const text = el.innerText ?? "";
      return JSON.stringify({
        text: text.slice(0, maxChars),
        truncated: text.length > maxChars,
      });
    },
    goto: ({ url }) => {
      if (!/^https?:/i.test(url)) throw new Error("blocked_scheme");
      location.href = url;
      return JSON.stringify({ url });
    },
    click: ({ ref }) => {
      byRef(ref).click();
      return JSON.stringify({ ok: true });
    },
    fill: ({ ref, value }) => {
      const el = byRef(ref);
      el.focus();
      el.value = value;
      el.dispatchEvent(new Event("input", { bubbles: true }));
      el.dispatchEvent(new Event("change", { bubbles: true }));
      return JSON.stringify({ ok: true });
    },
    press: ({ key }) => {
      document.activeElement?.dispatchEvent(
        new KeyboardEvent("keydown", { key, bubbles: true }),
      );
      document.activeElement?.dispatchEvent(
        new KeyboardEvent("keyup", { key, bubbles: true }),
      );
      return JSON.stringify({ ok: true });
    },
  };
})();
