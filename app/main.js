// src/engine.ts
async function json(path) {
  const response = await fetch(path);
  if (!response.ok) {
    throw new Error(`${path} answered ${response.status}`);
  }
  return await response.json();
}
var api = {
  meta: () => json("/api/meta"),
  overview: () => json("/api/overview"),
  chains: () => json("/api/chains"),
  graph: () => json("/api/graph"),
  trace: () => json("/api/trace"),
  skill: (name) => json(`/api/skill/${encodeURIComponent(name)}`)
};

// src/canvas.ts
function depths(graph2) {
  const next = /* @__PURE__ */ new Map();
  for (const e of graph2.edges) {
    if (!next.has(e.from)) next.set(e.from, []);
    next.get(e.from).push(e.to);
  }
  const depth = /* @__PURE__ */ new Map();
  const queue = graph2.nodes.filter((n) => n.kind === "source").map((n) => n.id);
  if (queue.length === 0 && graph2.nodes.length > 0) {
    const busiest = [
      ...graph2.nodes
    ].sort((a, b) => b.inflow - a.inflow)[0];
    queue.push(busiest.id);
  }
  for (const id of queue) depth.set(id, 0);
  while (queue.length > 0) {
    const id = queue.shift();
    const d = depth.get(id);
    for (const to of next.get(id) ?? []) {
      if (!depth.has(to)) {
        depth.set(to, d + 1);
        queue.push(to);
      }
    }
  }
  const deepest = Math.max(0, ...depth.values());
  for (const n of graph2.nodes) {
    if (!depth.has(n.id)) depth.set(n.id, deepest + 1);
  }
  return depth;
}
function layout(graph2, width, height) {
  const depth = depths(graph2);
  const columns = /* @__PURE__ */ new Map();
  for (const n of graph2.nodes) {
    const d = depth.get(n.id) ?? 0;
    if (!columns.has(d)) columns.set(d, []);
    columns.get(d).push(n);
  }
  const keys = [
    ...columns.keys()
  ].sort((a, b) => a - b);
  const pad = 110;
  const span = Math.max(1, keys.length - 1);
  const out = /* @__PURE__ */ new Map();
  for (const [i, d] of keys.entries()) {
    const column = columns.get(d).sort((a, b) => b.inflow - a.inflow);
    const x = pad + i / span * (width - pad * 2);
    for (const [j, n] of column.entries()) {
      const slot = (j + 1) / (column.length + 1);
      out.set(n.id, {
        x,
        y: pad * 0.5 + slot * (height - pad)
      });
    }
  }
  return out;
}
function levelsOf(frame) {
  return new Map(frame.levels);
}
function visibleTokens(frame) {
  let total = 0;
  for (const flow of frame.flows) total += flow.n;
  return total;
}
function heldTokens(frame) {
  let total = 0;
  for (const [, n] of frame.levels) total += n;
  return total;
}
function poolRadius(level) {
  return 9 + Math.sqrt(Math.max(0, level)) * 3.4;
}
var PARTICLE_CAP = 12;
function particlesFor(flow) {
  const drawn = Math.min(flow.n, PARTICLE_CAP);
  const out = [];
  for (let i = 0; i < drawn; i++) {
    out.push({
      from: flow.from,
      to: flow.to,
      // Spread across the first third so they leave as a stream.
      t: i / Math.max(1, drawn) * 0.33,
      // A self-flow has no direction to travel; drawn as a loop above the
      // node instead of collapsing to a point underneath it, which is what
      // the prototype did and why nothing appeared to move.
      self: flow.from === flow.to
    });
  }
  return out;
}
function history(run2, ty, upto, window = 30) {
  const from = Math.max(0, upto - window + 1);
  const out = [];
  for (let i = from; i <= upto && i < run2.frames.length; i++) {
    const found = run2.frames[i].levels.find(([name]) => name === ty);
    out.push(found ? found[1] : 0);
  }
  return out;
}
function headlineOf(run2, step2) {
  const frame = run2.frames[step2];
  return {
    step: step2,
    fired: frame.fired.length,
    starved: frame.starved,
    held: heldTokens(frame),
    moving: visibleTokens(frame)
  };
}

// src/main.ts
var COL = {
  source: "#a3e635",
  pool: "#4cc9d9",
  hot: "#f97316",
  drain: "#8b5cf6",
  ink: "#ece7dc",
  dim: "#7b7568",
  // Bright enough to read. The prototype drew these at #22242c on #0b0c10 and
  // made the densest information on the canvas the least visible thing on it.
  edge: "#39404e",
  dead: "#2c2f39"
};
var el = (id) => document.getElementById(id);
var graph = null;
var run = null;
var places = /* @__PURE__ */ new Map();
var levels = /* @__PURE__ */ new Map();
var particles = [];
var step = 0;
var playing = false;
var timer;
var canvas = el("c");
var ctx = canvas.getContext("2d");
var W = 0;
var H = 0;
function resize() {
  const dpr = globalThis.devicePixelRatio || 1;
  W = canvas.clientWidth;
  H = canvas.clientHeight;
  canvas.width = W * dpr;
  canvas.height = H * dpr;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  if (graph) places = layout(graph, W, H);
}
globalThis.addEventListener("resize", resize);
function colourOf(kind, hot) {
  if (kind === "source") return COL.source;
  if (kind === "drain") return COL.drain;
  return hot ? COL.hot : COL.pool;
}
function curve(a, b) {
  const dx = b.x - a.x, dy = b.y - a.y;
  return {
    cx: (a.x + b.x) / 2 - dy * 0.12,
    cy: (a.y + b.y) / 2 + dx * 0.12
  };
}
function pointOn(a, b, t) {
  const { cx, cy } = curve(a, b);
  const u = 1 - t;
  return {
    x: u * u * a.x + 2 * u * t * cx + t * t * b.x,
    y: u * u * a.y + 2 * u * t * cy + t * t * b.y
  };
}
function pointOnLoop(at, r, t) {
  const angle = -Math.PI / 2 + t * Math.PI * 2;
  return {
    x: at.x + Math.cos(angle) * (r + 14),
    y: at.y + Math.sin(angle) * (r + 14) - (r + 14)
  };
}
function sparkline(at, past, hot) {
  if (past.length < 2) return;
  const top = Math.max(...past, 1);
  const w = 46, h = 14;
  const x0 = at.x - w / 2, y0 = at.y + 30;
  ctx.beginPath();
  past.forEach((v, i) => {
    const x = x0 + i / (past.length - 1) * w;
    const y = y0 + h - v / top * h;
    if (i === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  });
  ctx.strokeStyle = hot ? COL.hot : COL.dim;
  ctx.lineWidth = 1.2;
  ctx.globalAlpha = 0.85;
  ctx.stroke();
  ctx.globalAlpha = 1;
}
function draw() {
  ctx.clearRect(0, 0, W, H);
  if (!graph || !run) return;
  const bundles = /* @__PURE__ */ new Map();
  for (const e of graph.edges) {
    if (e.from === e.to) continue;
    const key = `${e.from} ${e.to}`;
    const found = bundles.get(key) ?? {
      from: e.from,
      to: e.to,
      n: 0,
      dead: 0
    };
    found.n++;
    if (e.dead) found.dead++;
    bundles.set(key, found);
  }
  for (const b of bundles.values()) {
    const a = places.get(b.from), z = places.get(b.to);
    if (!a || !z) continue;
    const { cx, cy } = curve(a, z);
    ctx.beginPath();
    ctx.moveTo(a.x, a.y);
    ctx.quadraticCurveTo(cx, cy, z.x, z.y);
    ctx.lineWidth = Math.min(5, 0.7 + b.n * 0.2);
    ctx.strokeStyle = b.dead === b.n ? COL.dead : COL.edge;
    ctx.setLineDash(b.dead === b.n ? [
      3,
      5
    ] : []);
    ctx.stroke();
    ctx.setLineDash([]);
  }
  for (const p of particles) {
    const a = places.get(p.from), z = places.get(p.to);
    if (!a || !z) continue;
    const q = p.self ? pointOnLoop(a, poolRadius(levels.get(p.from) ?? 0), p.t) : pointOn(a, z, p.t);
    ctx.beginPath();
    ctx.arc(q.x, q.y, 6.5, 0, Math.PI * 2);
    ctx.fillStyle = "#ece7dc22";
    ctx.fill();
    ctx.beginPath();
    ctx.arc(q.x, q.y, 3.2, 0, Math.PI * 2);
    ctx.fillStyle = "#ffffff";
    ctx.fill();
  }
  for (const n of graph.nodes) {
    const at = places.get(n.id);
    if (!at) continue;
    const level = levels.get(n.id) ?? 0;
    const r = poolRadius(level);
    const col = colourOf(n.kind, n.under_pressure);
    ctx.beginPath();
    ctx.arc(at.x, at.y, r, 0, Math.PI * 2);
    ctx.fillStyle = `${col}1f`;
    ctx.fill();
    ctx.strokeStyle = col;
    ctx.lineWidth = n.under_pressure ? 2.2 : 1.2;
    ctx.stroke();
    ctx.font = "12px ui-monospace, Menlo, monospace";
    ctx.textAlign = "center";
    ctx.fillStyle = COL.ink;
    ctx.fillText(n.id, at.x, at.y - r - 8);
    if (level > 0) {
      ctx.fillStyle = col;
      ctx.font = "13px ui-monospace, Menlo, monospace";
      ctx.fillText(String(level), at.x, at.y + 5);
    }
    sparkline(at, history(run, n.id, step, 30), n.under_pressure);
  }
}
function frameLoop() {
  for (const p of particles) p.t += 0.028;
  particles = particles.filter((p) => p.t < 1);
  draw();
  requestAnimationFrame(frameLoop);
}
function show(i) {
  if (!run) return;
  step = Math.max(0, Math.min(run.frames.length - 1, i));
  const frame = run.frames[step];
  levels = levelsOf(frame);
  particles = frame.flows.flatMap(particlesFor);
  const head = headlineOf(run, step);
  el("n-step").textContent = String(head.step);
  el("n-fired").textContent = String(head.fired);
  el("n-starved").textContent = String(head.starved);
  el("n-held").textContent = String(head.held);
  el("scrub").value = String(step);
}
function play() {
  playing = !playing;
  el("play").textContent = playing ? "pause" : "play";
  if (playing) {
    timer = setInterval(() => show(step >= run.frames.length - 1 ? 0 : step + 1), 300);
  } else {
    clearInterval(timer);
  }
}
(async () => {
  try {
    const [meta, g, r] = await Promise.all([
      api.meta(),
      api.graph(),
      api.trace()
    ]);
    graph = g;
    run = r;
    el("meta").textContent = `${meta.skills} skills \xB7 ${g.nodes.length} types \xB7 seed ${r.seed} \xB7 ` + (r.conserved ? "conserved" : "LEAKS");
    el("scrub").max = String(r.frames.length - 1);
    resize();
    show(0);
    frameLoop();
    play();
    el("play").onclick = play;
    el("rewind").onclick = () => show(0);
    el("scrub").oninput = (e) => {
      if (playing) play();
      show(Number(e.target.value));
    };
  } catch (error) {
    el("meta").textContent = `could not read the corpus: ${error}`;
  }
})();
