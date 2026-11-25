// The 3D scenes of the site, in plain WebGL (no library): the particles of
// the hero, which go from a sphere to a wave as the page scrolls, and the
// moving horizon of the footer.

const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;

function shader(gl, type, src) {
  const s = gl.createShader(type);
  gl.shaderSource(s, src);
  gl.compileShader(s);
  if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s));
  return s;
}

function program(gl, vs, fs) {
  const p = gl.createProgram();
  gl.attachShader(p, shader(gl, gl.VERTEX_SHADER, vs));
  gl.attachShader(p, shader(gl, gl.FRAGMENT_SHADER, fs));
  gl.linkProgram(p);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p));
  return p;
}

// Runs `frame(t)` while the canvas is on screen and the tab visible.
function loop(canvas, frame) {
  let visible = true;
  let raf = 0;
  const start = performance.now();
  const tick = (now) => {
    raf = 0;
    if (!visible || document.hidden) return;
    frame((now - start) / 1000);
    if (!reduced) raf = requestAnimationFrame(tick);
  };
  const wake = () => {
    if (!raf && visible && !document.hidden) raf = requestAnimationFrame(tick);
  };
  new IntersectionObserver((e) => {
    visible = e[0].isIntersecting;
    wake();
  }).observe(canvas);
  document.addEventListener("visibilitychange", wake);
  wake();
  return wake;
}

function fit(canvas, gl) {
  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  const w = Math.max(1, Math.floor(canvas.clientWidth * dpr));
  const h = Math.max(1, Math.floor(canvas.clientHeight * dpr));
  if (canvas.width !== w || canvas.height !== h) {
    canvas.width = w;
    canvas.height = h;
  }
  if (gl) gl.viewport(0, 0, w, h);
  return [w, h];
}

// ----- the hero: particles ------------------------------------------------

const HERO_VS = `#version 300 es
precision highp float;
in vec3 aSphere;
in vec2 aGrid;
in float aSeed;
uniform float uTime;
uniform float uMorph;
uniform vec2 uMouse;
uniform float uAspect;
uniform float uScale;
out float vGlow;
out float vMix;

mat3 rotY(float a) { float c = cos(a), s = sin(a); return mat3(c, 0., -s, 0., 1., 0., s, 0., c); }
mat3 rotX(float a) { float c = cos(a), s = sin(a); return mat3(1., 0., 0., 0., c, s, 0., -s, c); }

void main() {
  float t = uTime;
  // A sphere that breathes with a rhythm...
  float beat = 0.06 * sin(t * 2.2 + aSeed * 6.2831) + 0.04 * sin(t * 3.7 + aSphere.y * 4.0);
  vec3 sphere = aSphere * (1.0 + beat);
  // ...and a wave: a grid whose height follows several sines, like sound.
  float x = aGrid.x * 2.4, z = aGrid.y * 2.4;
  float y = 0.28 * sin(x * 2.1 + t * 1.6) * cos(z * 1.7 - t * 1.1)
          + 0.12 * sin((x + z) * 3.3 - t * 2.4);
  vec3 wave = vec3(x, y - 0.15, z);
  float m = smoothstep(0.0, 1.0, uMorph);
  vec3 p = mix(sphere, wave, m);
  p = rotY(t * 0.12 + uMouse.x * 0.6) * rotX(0.35 + uMouse.y * 0.35 - m * 0.25) * p;
  float depth = p.z + 3.2;
  gl_Position = vec4(p.x * uScale / uAspect, p.y * uScale, 0.0, depth * 0.55);
  gl_PointSize = (2.6 + 2.0 * fract(aSeed * 13.7)) * (3.2 / depth) * uScale * 1.6;
  vGlow = clamp(1.6 - depth * 0.33, 0.25, 1.0);
  vMix = fract(aSeed * 7.31) * 0.5 + (p.y + 1.0) * 0.25;
}`;

const HERO_FS = `#version 300 es
precision highp float;
in float vGlow;
in float vMix;
out vec4 color;
void main() {
  vec2 c = gl_PointCoord - 0.5;
  float d = length(c);
  if (d > 0.5) discard;
  float a = smoothstep(0.5, 0.0, d);
  vec3 violet = vec3(0.49, 0.36, 1.0);
  vec3 cyan = vec3(0.0, 0.83, 1.0);
  vec3 rose = vec3(1.0, 0.36, 0.66);
  vec3 col = mix(mix(violet, cyan, vMix), rose, smoothstep(0.75, 1.0, vMix));
  color = vec4(col * (0.55 + a), a * vGlow);
}`;

export function hero(canvas) {
  const gl = canvas.getContext("webgl2", { antialias: true, premultipliedAlpha: false, alpha: true });
  if (!gl) {
    canvas.classList.add("no-webgl");
    return;
  }
  const prog = program(gl, HERO_VS, HERO_FS);
  const n = matchMedia("(max-width: 700px)").matches ? 4200 : 9000;
  const sphere = new Float32Array(n * 3);
  const grid = new Float32Array(n * 2);
  const seed = new Float32Array(n);
  const side = Math.ceil(Math.sqrt(n));
  const golden = Math.PI * (3 - Math.sqrt(5));
  for (let i = 0; i < n; i++) {
    // Points spread evenly on the sphere (Fibonacci lattice).
    const y = 1 - (i / (n - 1)) * 2;
    const r = Math.sqrt(1 - y * y);
    const th = golden * i;
    sphere.set([Math.cos(th) * r, y, Math.sin(th) * r], i * 3);
    grid.set([((i % side) / side - 0.5) * 2, (Math.floor(i / side) / side - 0.5) * 2], i * 2);
    seed[i] = Math.random();
  }
  const attr = (name, data, size) => {
    const loc = gl.getAttribLocation(prog, name);
    const buf = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buf);
    gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
    gl.enableVertexAttribArray(loc);
    gl.vertexAttribPointer(loc, size, gl.FLOAT, false, 0, 0);
  };
  const vao = gl.createVertexArray();
  gl.bindVertexArray(vao);
  attr("aSphere", sphere, 3);
  attr("aGrid", grid, 2);
  attr("aSeed", seed, 1);
  const u = (name) => gl.getUniformLocation(prog, name);
  const uTime = u("uTime"), uMorph = u("uMorph"), uMouse = u("uMouse"), uAspect = u("uAspect"), uScale = u("uScale");
  let mouse = [0, 0], target = [0, 0], morph = 0;
  addEventListener("pointermove", (e) => {
    target = [(e.clientX / innerWidth - 0.5) * 2, (e.clientY / innerHeight - 0.5) * 2];
  }, { passive: true });
  gl.enable(gl.BLEND);
  gl.blendFunc(gl.SRC_ALPHA, gl.ONE);
  loop(canvas, (t) => {
    const [w, h] = fit(canvas, gl);
    mouse = [mouse[0] + (target[0] - mouse[0]) * 0.05, mouse[1] + (target[1] - mouse[1]) * 0.05];
    const rect = canvas.getBoundingClientRect();
    const goal = Math.min(1, Math.max(0, -rect.top / (rect.height * 0.7)));
    morph += (goal - morph) * 0.08;
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.useProgram(prog);
    gl.bindVertexArray(vao);
    gl.uniform1f(uTime, t);
    gl.uniform1f(uMorph, morph);
    gl.uniform2f(uMouse, mouse[0], mouse[1]);
    gl.uniform1f(uAspect, w / h);
    gl.uniform1f(uScale, w / h < 1 ? 1.25 : 1.55);
    gl.drawArrays(gl.POINTS, 0, n);
  });
}

// ----- the footer: a horizon of lines ------------------------------------

export function horizon(canvas) {
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  loop(canvas, (t) => {
    const [w, h] = fit(canvas, null);
    ctx.clearRect(0, 0, w, h);
    const horizonY = h * 0.38;
    // The sun, behind the grid.
    const sun = ctx.createRadialGradient(w / 2, horizonY, 0, w / 2, horizonY, h * 0.55);
    sun.addColorStop(0, "rgba(255, 92, 168, 0.55)");
    sun.addColorStop(0.35, "rgba(124, 92, 255, 0.28)");
    sun.addColorStop(1, "rgba(0, 0, 0, 0)");
    ctx.fillStyle = sun;
    ctx.fillRect(0, 0, w, h);
    ctx.lineWidth = Math.max(1, w / 1400);
    // Lines going away: their spacing shrinks with the distance.
    const speed = (t * 0.35) % 1;
    for (let i = 0; i < 18; i++) {
      const z = (i + 1 - speed) / 18;
      const y = horizonY + (h - horizonY) * Math.pow(z, 2.2);
      ctx.strokeStyle = `rgba(61, 214, 255, ${0.08 + z * 0.5})`;
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(w, y);
      ctx.stroke();
    }
    // Lines toward the vanishing point.
    for (let i = -16; i <= 16; i++) {
      const x = w / 2 + i * (w / 16);
      ctx.strokeStyle = `rgba(155, 132, 255, ${0.35 - Math.abs(i) * 0.016})`;
      ctx.beginPath();
      ctx.moveTo(w / 2 + i * 6, horizonY);
      ctx.lineTo(x * 1.0 + i * w * 0.06, h);
      ctx.stroke();
    }
    const fade = ctx.createLinearGradient(0, horizonY - 2, 0, horizonY + h * 0.18);
    fade.addColorStop(0, "rgba(10, 10, 18, 1)");
    fade.addColorStop(1, "rgba(10, 10, 18, 0)");
    ctx.fillStyle = fade;
    ctx.fillRect(0, horizonY - 2, w, h * 0.18);
  });
}
