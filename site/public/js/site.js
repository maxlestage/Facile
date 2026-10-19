// The movements of the site: 3D tilt of the cards, 3D reveal on scroll, the
// cube of the stack, the benchmark's bars, counters, tabs, copy buttons.
// Everything still reads without JavaScript, and nothing moves for those who
// asked their system for less motion.

import { hero, horizon } from "./scene.js";

const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
const fine = matchMedia("(pointer: fine)").matches;
const $$ = (s, root = document) => Array.from(root.querySelectorAll(s));

// ----- the 3D scenes ------------------------------------------------------

for (const c of $$("canvas[data-scene=hero]")) {
  try {
    hero(c);
  } catch (e) {
    console.warn(e);
  }
}
for (const c of $$("canvas[data-scene=horizon]")) horizon(c);

// ----- cards that lean toward the pointer --------------------------------

if (fine && !reduced) {
  for (const el of $$("[data-tilt]")) {
    const max = Number(el.dataset.tilt) || 10;
    let raf = 0;
    el.addEventListener("pointermove", (e) => {
      const r = el.getBoundingClientRect();
      const x = (e.clientX - r.left) / r.width;
      const y = (e.clientY - r.top) / r.height;
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => {
        el.style.setProperty("--rx", `${(0.5 - y) * max}deg`);
        el.style.setProperty("--ry", `${(x - 0.5) * max}deg`);
        el.style.setProperty("--gx", `${x * 100}%`);
        el.style.setProperty("--gy", `${y * 100}%`);
        el.classList.add("tilting");
      });
    });
    el.addEventListener("pointerleave", () => {
      cancelAnimationFrame(raf);
      el.style.setProperty("--rx", "0deg");
      el.style.setProperty("--ry", "0deg");
      el.classList.remove("tilting");
    });
  }
}

// ----- reveal on scroll, in depth ----------------------------------------

const reveal = new IntersectionObserver(
  (entries) => {
    for (const e of entries) {
      if (!e.isIntersecting) continue;
      e.target.classList.add("visible");
      reveal.unobserve(e.target);
    }
  },
  { threshold: 0, rootMargin: "0px 0px -8% 0px" },
);
$$("[data-reveal]").forEach((el, i) => {
  el.style.setProperty("--delay", `${(Number(el.dataset.reveal) || 0) * 90}ms`);
  if (reduced) el.classList.add("visible");
  else reveal.observe(el);
  void i;
});

// ----- numbers that count up ----------------------------------------------

const counters = new IntersectionObserver((entries) => {
  for (const e of entries) {
    if (!e.isIntersecting) continue;
    counters.unobserve(e.target);
    const el = e.target;
    const end = Number(el.dataset.count);
    const decimals = Number(el.dataset.decimals || 0);
    const fmt = (v) => v.toLocaleString(document.documentElement.lang || "fr", { minimumFractionDigits: decimals, maximumFractionDigits: decimals });
    if (reduced) {
      el.textContent = fmt(end);
      continue;
    }
    const t0 = performance.now();
    const step = (now) => {
      const k = Math.min(1, (now - t0) / 1400);
      el.textContent = fmt(end * (1 - Math.pow(1 - k, 3)));
      if (k < 1) requestAnimationFrame(step);
    };
    requestAnimationFrame(step);
  }
});
$$("[data-count]").forEach((el) => counters.observe(el));

// ----- the cube of the stack: turns alone, and with the pointer ----------

for (const cube of $$("[data-cube]")) {
  let rx = -18, ry = 32, vx = 0, vy = reduced ? 0 : 0.18, dragging = false, last = null;
  const scene = cube.parentElement;
  const draw = () => {
    cube.style.transform = `rotateX(${rx}deg) rotateY(${ry}deg)`;
  };
  scene.addEventListener("pointerdown", (e) => {
    dragging = true;
    last = [e.clientX, e.clientY];
    scene.setPointerCapture(e.pointerId);
  });
  scene.addEventListener("pointermove", (e) => {
    if (!dragging) return;
    vx = (e.clientY - last[1]) * -0.25;
    vy = (e.clientX - last[0]) * 0.25;
    rx += vx;
    ry += vy;
    last = [e.clientX, e.clientY];
    draw();
  });
  const stop = () => {
    dragging = false;
  };
  scene.addEventListener("pointerup", stop);
  scene.addEventListener("pointercancel", stop);
  // Faces that tell which part of Facile they are, by keyboard too.
  for (const b of $$("[data-face]", scene.closest("section") || document)) {
    b.addEventListener("click", () => {
      const [x, y] = b.dataset.face.split(",").map(Number);
      rx = x;
      ry = y;
      vx = 0;
      vy = 0;
      draw();
    });
  }
  const spin = () => {
    if (!dragging) {
      vx *= 0.95;
      vy = vy * 0.95 + (reduced ? 0 : 0.18) * 0.05;
      rx += vx;
      ry += vy;
      draw();
    }
    requestAnimationFrame(spin);
  };
  draw();
  if (!reduced) requestAnimationFrame(spin);
}

// ----- tabs of the code windows --------------------------------------------

for (const tabs of $$("[data-tabs]")) {
  const buttons = $$("[role=tab]", tabs);
  const panels = $$("[role=tabpanel]", tabs);
  const show = (i) => {
    buttons.forEach((b, j) => {
      b.setAttribute("aria-selected", String(i === j));
      b.tabIndex = i === j ? 0 : -1;
    });
    panels.forEach((p, j) => (p.hidden = i !== j));
  };
  buttons.forEach((b, i) => {
    b.addEventListener("click", () => show(i));
    b.addEventListener("keydown", (e) => {
      if (e.key === "ArrowRight") buttons[(i + 1) % buttons.length].focus(), show((i + 1) % buttons.length);
      if (e.key === "ArrowLeft") buttons[(i - 1 + buttons.length) % buttons.length].focus(), show((i - 1 + buttons.length) % buttons.length);
    });
  });
  show(0);
}

// ----- copy buttons ---------------------------------------------------------

for (const pre of $$("pre.code, [data-copy]")) {
  const text = pre.dataset.copy || pre.innerText;
  const b = document.createElement("button");
  b.className = "copy";
  b.type = "button";
  b.textContent = document.documentElement.lang === "en" ? "Copy" : "Copier";
  b.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(text.trim());
      b.textContent = document.documentElement.lang === "en" ? "Copied ✓" : "Copié ✓";
      setTimeout(() => (b.textContent = document.documentElement.lang === "en" ? "Copy" : "Copier"), 1600);
    } catch (_) {}
  });
  (pre.closest(".window, .terminal") || pre).appendChild(b);
}

// ----- the header: shadow once the page scrolled, menu on phones ---------

const header = document.querySelector(".site-header");
const progress = document.querySelector(".progress");
const onScroll = () => {
  const y = scrollY;
  if (header) header.classList.toggle("scrolled", y > 8);
  if (progress) {
    const max = document.documentElement.scrollHeight - innerHeight;
    progress.style.transform = `scaleX(${max > 0 ? y / max : 0})`;
  }
};
addEventListener("scroll", onScroll, { passive: true });
onScroll();

const toggle = document.querySelector(".menu-toggle");
if (toggle) {
  toggle.addEventListener("click", () => {
    const open = toggle.getAttribute("aria-expanded") !== "true";
    toggle.setAttribute("aria-expanded", String(open));
    document.body.classList.toggle("menu-open", open);
  });
  for (const a of $$(".site-nav a")) a.addEventListener("click", () => {
    toggle.setAttribute("aria-expanded", "false");
    document.body.classList.remove("menu-open");
  });
}

// ----- buttons that follow the pointer a little ----------------------------

if (fine && !reduced) {
  for (const b of $$("[data-magnetic]")) {
    b.addEventListener("pointermove", (e) => {
      const r = b.getBoundingClientRect();
      b.style.transform = `translate(${(e.clientX - r.left - r.width / 2) * 0.18}px, ${(e.clientY - r.top - r.height / 2) * 0.28}px)`;
    });
    b.addEventListener("pointerleave", () => (b.style.transform = ""));
  }
}

// ----- the menu of the docs: folded on phones, open beside the text --------

const wide = matchMedia("(min-width: 861px)");
for (const d of $$(".docs-nav details")) {
  const fit = () => (d.open = wide.matches);
  fit();
  wide.addEventListener("change", fit);
  // On larger screens the summary is only a title: the menu stays open.
  d.addEventListener("toggle", () => { if (wide.matches && !d.open) d.open = true; });
}

// ----- the table of contents of the docs follows the reading ---------------

const tocLinks = $$(".toc a[href^='#']");
if (tocLinks.length) {
  const byId = new Map(tocLinks.map((a) => [decodeURIComponent(a.hash.slice(1)), a]));
  const spy = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (!e.isIntersecting) continue;
        tocLinks.forEach((a) => a.classList.remove("active"));
        const a = byId.get(e.target.id);
        if (a) a.classList.add("active");
      }
    },
    { rootMargin: "-20% 0px -70% 0px" },
  );
  for (const id of byId.keys()) {
    const h = document.getElementById(id);
    if (h) spy.observe(h);
  }
}
