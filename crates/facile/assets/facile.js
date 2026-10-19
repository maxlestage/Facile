// Facile in the browser: brings the islands of a page to life.
//
// An island is a component written in Rythme (`client/name.ry`). The server
// sent its first view as HTML; here the Rythme interpreter, compiled to
// WebAssembly, runs the same file: each event gives a message to `update`,
// `view` gives the new view, and only what changed is written in the page.

const WASM_URL = "/_facile/facile.wasm";
const LANGS = ["en", "fr", "es", "de", "pt", "zh"];
const SVG = "http://www.w3.org/2000/svg";

let wasm = null;
const loaded = new Map(); // component name -> Promise of its loading
const encoder = new TextEncoder();
const decoder = new TextDecoder();

function langIndex() {
  const l = (document.documentElement.lang || navigator.language || "en").slice(0, 2).toLowerCase();
  const i = LANGS.indexOf(l);
  return i < 0 ? 0 : i;
}

let instantiating = null;

// One module for the whole page, whichever island asks first.
function instantiate() {
  if (!instantiating) instantiating = compile();
  return instantiating;
}

async function compile() {
  const imports = {
    env: {
      // The clock and the time zone, for `now()` and `today()` in components.
      rythme_now: () => Date.now(),
      rythme_offset: (ms) => -new Date(ms).getTimezoneOffset(),
    },
  };
  let result;
  try {
    result = await WebAssembly.instantiateStreaming(fetch(WASM_URL), imports);
  } catch (_) {
    // A server that does not say `application/wasm`.
    const bytes = await (await fetch(WASM_URL)).arrayBuffer();
    result = await WebAssembly.instantiate(bytes, imports);
  }
  wasm = result.instance.exports;
  return wasm;
}

// Writes a text in the module's memory: [pointer, length].
function put(text) {
  const bytes = encoder.encode(text);
  const ptr = wasm.facile_alloc(bytes.length);
  new Uint8Array(wasm.memory.buffer, ptr, bytes.length).set(bytes);
  return [ptr, bytes.length];
}

// The JSON answer of the last call.
function out() {
  const ptr = wasm.facile_out_ptr();
  const len = wasm.facile_out_len();
  const answer = JSON.parse(decoder.decode(new Uint8Array(wasm.memory.buffer, ptr, len)));
  if (answer.log) console.log(answer.log);
  if (answer.error) console.error("Facile:\n" + answer.error);
  return answer;
}

function load(name, src) {
  if (!loaded.has(name)) {
    loaded.set(
      name,
      (async () => {
        const [code, ok] = await Promise.all([fetch(src).then((r) => r.text()), instantiate()]);
        void ok;
        const [np, nl] = put(name);
        const [sp, sl] = put(code);
        wasm.facile_load(np, nl, sp, sl, langIndex());
        const answer = out();
        if (answer.error) throw new Error(answer.error);
      })(),
    );
  }
  return loaded.get(name);
}

// ----- the virtual DOM --------------------------------------------------

const PROPS = new Set(["value", "checked", "selected", "disabled"]);

function setAttr(el, k, v) {
  if (PROPS.has(k) && k in el) {
    el[k] = k === "value" ? v : v !== null;
    if (k !== "value") {
      if (v === null) el.removeAttribute(k);
      else el.setAttribute(k, "");
    }
    return;
  }
  if (v === null) el.removeAttribute(k);
  else el.setAttribute(k, v);
}

function create(v, island, svg) {
  if (typeof v === "string") return document.createTextNode(v);
  if (v.r !== undefined) {
    const span = document.createElement("span");
    span.style.display = "contents";
    span.innerHTML = v.r;
    return span;
  }
  const inSvg = svg || v.t === "svg";
  const el = inSvg ? document.createElementNS(SVG, v.t) : document.createElement(v.t);
  for (const [k, val] of Object.entries(v.a || {})) setAttr(el, k, val);
  bind(el, v.e || {}, island);
  for (const c of v.c || []) el.appendChild(create(c, island, inSvg && v.t !== "foreignObject"));
  return el;
}

// Events: the element keeps the number of each handler; one listener per event name.
function bind(el, events, island) {
  const old = el.__facile || {};
  for (const name of Object.keys(old)) {
    if (!(name in events)) {
      el.removeEventListener(name, old[name].listener);
      delete old[name];
    }
  }
  for (const [name, handler] of Object.entries(events)) {
    if (old[name]) {
      old[name].handler = handler;
      continue;
    }
    const entry = { handler, listener: null };
    entry.listener = (e) => island.event(e, name, entry.handler);
    el.addEventListener(name, entry.listener);
    old[name] = entry;
  }
  el.__facile = old;
}

function same(a, b) {
  if (typeof a === "string" || typeof b === "string") return typeof a === typeof b;
  if ((a.r !== undefined) !== (b.r !== undefined)) return false;
  return a.t === b.t && (a.a || {}).key === (b.a || {}).key;
}

function patch(parent, node, old, now, island, svg) {
  if (!same(old, now)) {
    const fresh = create(now, island, svg);
    parent.replaceChild(fresh, node);
    return;
  }
  if (typeof now === "string") {
    if (old !== now) node.nodeValue = now;
    return;
  }
  if (now.r !== undefined) {
    if (old.r !== now.r) node.innerHTML = now.r;
    return;
  }
  const inSvg = svg || now.t === "svg";
  const oa = old.a || {};
  const na = now.a || {};
  for (const k of Object.keys(oa)) if (!(k in na)) setAttr(node, k, null);
  for (const [k, v] of Object.entries(na)) {
    if (oa[k] !== v || (PROPS.has(k) && k in node && String(node[k]) !== v)) setAttr(node, k, v);
  }
  bind(node, now.e || {}, island);
  patchChildren(node, old.c || [], now.c || [], island, inSvg && now.t !== "foreignObject");
}

function patchChildren(el, olds, nows, island, svg) {
  const nodes = Array.from(el.childNodes);
  const n = Math.min(olds.length, nows.length);
  for (let i = 0; i < n; i++) patch(el, nodes[i], olds[i], nows[i], island, svg);
  for (let i = olds.length; i < nows.length; i++) el.appendChild(create(nows[i], island, svg));
  for (let i = olds.length - 1; i >= nows.length; i--) el.removeChild(nodes[i]);
}

// ----- islands ----------------------------------------------------------

function eventValue(e, name) {
  const t = e.target;
  if (name === "submit") {
    e.preventDefault();
    const data = {};
    for (const [k, v] of new FormData(t).entries()) data[k] = typeof v === "string" ? v : v.name;
    return [JSON.stringify(data), 1];
  }
  if (name.startsWith("key")) return [e.key, 0];
  if (t && t.type === "checkbox") return [String(t.checked), 0];
  if (t && "value" in t && t.value !== undefined && name !== "click") return [String(t.value), 0];
  return ["", 0];
}

class Island {
  constructor(el) {
    this.el = el;
    this.name = el.dataset.component;
    this.id = -1;
    this.view = null;
  }

  async start() {
    await load(this.name, this.el.dataset.src);
    const [np, nl] = put(this.name);
    const [pp, pl] = put(this.el.dataset.props || "{}");
    wasm.facile_mount(np, nl, pp, pl);
    this.apply(out(), true);
    this.el.dataset.live = "";
  }

  apply(answer, first) {
    if (answer.error) return;
    if (answer.id !== undefined) this.id = answer.id;
    const kids = answer.vdom && answer.vdom.t === "facile-fragment" ? answer.vdom.c || [] : [answer.vdom];
    if (first) {
      // The server's HTML is the same view: built again with its handlers.
      this.el.replaceChildren(...kids.map((k) => create(k, this, false)));
    } else {
      patchChildren(this.el, this.view, kids, this, false);
    }
    this.view = kids;
    for (const effect of answer.effects || []) this.run(effect);
  }

  event(e, name, handler) {
    const [value, kind] = eventValue(e, name);
    const [vp, vl] = put(value);
    wasm.facile_event(this.id, handler, vp, vl, kind);
    this.apply(out(), false);
  }

  callback(cb, result) {
    const [vp, vl] = put(JSON.stringify(result));
    wasm.facile_callback(this.id, cb, vp, vl);
    this.apply(out(), false);
  }

  async run(effect) {
    switch (effect.effect) {
      case "fetch": {
        try {
          const init = { method: effect.method, headers: { accept: "application/json" } };
          if (effect.body !== undefined) {
            init.body = JSON.stringify(effect.body);
            init.headers["content-type"] = "application/json";
          }
          const r = await fetch(effect.url, init);
          const text = await r.text();
          let data = null;
          try {
            data = text ? JSON.parse(text) : null;
          } catch (_) {
            data = text;
          }
          if (r.ok) this.callback(effect.cb, { ok: data });
          else this.callback(effect.cb, { err: (data && data.error) || r.status + " " + r.statusText });
        } catch (err) {
          this.callback(effect.cb, { err: String(err.message || err) });
        }
        break;
      }
      case "after":
        setTimeout(() => this.callback(effect.cb, { tick: true }), effect.ms);
        break;
      case "save":
        try {
          localStorage.setItem("facile:" + effect.key, JSON.stringify(effect.value));
        } catch (_) {}
        break;
      case "load": {
        let v = null;
        try {
          v = localStorage.getItem("facile:" + effect.key);
        } catch (_) {}
        this.callback(effect.cb, v === null ? { none: true } : { some: JSON.parse(v) });
        break;
      }
      case "navigate":
        location.assign(effect.url);
        break;
      case "focus": {
        const el = document.querySelector(effect.selector);
        if (el) el.focus();
        break;
      }
    }
  }
}

function start(root) {
  const islands = root.querySelectorAll("facile-island:not([data-live])");
  if (!islands.length) return;
  const begin = (el) => new Island(el).start().catch((err) => console.error("Facile:", err));
  // Islands come to life when they are about to be seen. An island has no
  // box of its own (`display: contents`): its first element is watched.
  const owner = new Map();
  const seen = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (!e.isIntersecting) continue;
        seen.unobserve(e.target);
        begin(owner.get(e.target));
      }
    },
    { rootMargin: "200px" },
  );
  for (const el of islands) {
    const watched = el.firstElementChild;
    if (!watched || typeof IntersectionObserver === "undefined") {
      begin(el);
      continue;
    }
    owner.set(watched, el);
    seen.observe(watched);
  }
}

if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", () => start(document));
else start(document);

export { start };
