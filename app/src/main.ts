// The rendering half.
//
// `canvas.ts` is pure so the gate can check the arithmetic without a browser.
// This is the part that needs one: it owns the DOM, the animation frame, and
// nothing else. Every number it draws came from a function over there.

import { api, type Meta } from "./engine.ts";
import type { Graph, Run } from "./types.ts";
import {
  headlineOf,
  history,
  type Layout,
  layout,
  type Levels,
  levelsOf,
  type Particle,
  particlesFor,
  poolRadius,
} from "./canvas.ts";

const COL = {
  source: "#a3e635",
  pool: "#4cc9d9",
  hot: "#f97316",
  drain: "#8b5cf6",
  ink: "#ece7dc",
  dim: "#7b7568",
  // Bright enough to read. The prototype drew these at #22242c on #0b0c10 and
  // made the densest information on the canvas the least visible thing on it.
  edge: "#39404e",
  dead: "#2c2f39",
};

const el = <T extends HTMLElement>(id: string) =>
  document.getElementById(id) as T;

// ── state: geometry and levels never share an object ──────────────────────

let graph: Graph | null = null;
let run: Run | null = null;
let places: Layout = new Map();
let levels: Levels = new Map();
let particles: Particle[] = [];
let step = 0;
let playing = false;
// Deno types setInterval as returning Timeout and the DOM types it as a
// number. Neither is wrong; taking the return type of the thing being called
// is right in both.
let timer: ReturnType<typeof setInterval> | undefined;

const canvas = el<HTMLCanvasElement>("c");
const ctx = canvas.getContext("2d")!;
let W = 0, H = 0;

function resize() {
  const dpr = globalThis.devicePixelRatio || 1;
  W = canvas.clientWidth;
  H = canvas.clientHeight;
  canvas.width = W * dpr;
  canvas.height = H * dpr;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  // Recomputes geometry only. Levels live in their own map and a relayout
  // cannot reach them, which is the prototype's bug made impossible.
  if (graph) places = layout(graph, W, H);
}
globalThis.addEventListener("resize", resize);

function colourOf(kind: string, hot: boolean): string {
  if (kind === "source") return COL.source;
  if (kind === "drain") return COL.drain;
  return hot ? COL.hot : COL.pool;
}

// ── drawing ───────────────────────────────────────────────────────────────

function curve(a: { x: number; y: number }, b: { x: number; y: number }) {
  const dx = b.x - a.x, dy = b.y - a.y;
  return { cx: (a.x + b.x) / 2 - dy * 0.12, cy: (a.y + b.y) / 2 + dx * 0.12 };
}

function pointOn(
  a: { x: number; y: number },
  b: { x: number; y: number },
  t: number,
) {
  const { cx, cy } = curve(a, b);
  const u = 1 - t;
  return {
    x: u * u * a.x + 2 * u * t * cx + t * t * b.x,
    y: u * u * a.y + 2 * u * t * cy + t * t * b.y,
  };
}

/// A self-flow travels a loop above its node rather than collapsing to a point
/// underneath it, which is what the prototype did and why those arcs never
/// appeared to move.
function pointOnLoop(at: { x: number; y: number }, r: number, t: number) {
  const angle = -Math.PI / 2 + t * Math.PI * 2;
  return {
    x: at.x + Math.cos(angle) * (r + 14),
    y: at.y + Math.sin(angle) * (r + 14) - (r + 14),
  };
}

function sparkline(at: { x: number; y: number }, past: number[], hot: boolean) {
  if (past.length < 2) return;
  const top = Math.max(...past, 1);
  const w = 46, h = 14;
  const x0 = at.x - w / 2, y0 = at.y + 30;
  ctx.beginPath();
  past.forEach((v, i) => {
    const x = x0 + (i / (past.length - 1)) * w;
    const y = y0 + h - (v / top) * h;
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

  // Bundled edges. One arrow per type pair; 150 separate arrows over 12 nodes
  // is a hairball, and the question anyone has is how much moves from M_plan
  // to State, not which of the thirty skills did it.
  const bundles = new Map<
    string,
    { from: string; to: string; n: number; dead: number }
  >();
  for (const e of graph.edges) {
    if (e.from === e.to) continue;
    const key = `${e.from} ${e.to}`;
    const found = bundles.get(key) ?? { from: e.from, to: e.to, n: 0, dead: 0 };
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
    ctx.setLineDash(b.dead === b.n ? [3, 5] : []);
    ctx.stroke();
    ctx.setLineDash([]);
  }

  for (const p of particles) {
    const a = places.get(p.from), z = places.get(p.to);
    if (!a || !z) continue;
    const q = p.self
      ? pointOnLoop(a, poolRadius(levels.get(p.from) ?? 0), p.t)
      : pointOn(a, z, p.t);
    // Bright and with a halo. At 2.6px flat these were technically drawn and
    // practically invisible, which for the one element whose whole job is to
    // show motion is the same as not drawing them.
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
  // Slower than a step, so a token is in flight when the next frame lands and
// the canvas reads as continuous rather than as a slideshow.
  for (const p of particles) p.t += 0.028;
  particles = particles.filter((p) => p.t < 1);
  draw();
  requestAnimationFrame(frameLoop);
}

// ── transport ─────────────────────────────────────────────────────────────

function show(i: number) {
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
  el<HTMLInputElement>("scrub").value = String(step);
}

function play() {
  playing = !playing;
  el("play").textContent = playing ? "pause" : "play";
  if (playing) {
    timer = setInterval(
      () => show(step >= run!.frames.length - 1 ? 0 : step + 1),
      300,
    );
  } else {
    clearInterval(timer);
  }
}

// ── boot ──────────────────────────────────────────────────────────────────

(async () => {
  try {
    const [meta, g, r]: [Meta, Graph, Run] = await Promise.all([
      api.meta(),
      api.graph(),
      api.trace(),
    ]);
    graph = g;
    run = r;

    el("meta").textContent =
      `${meta.skills} skills · ${g.nodes.length} types · seed ${r.seed} · ` +
      (r.conserved ? "conserved" : "LEAKS");
    el<HTMLInputElement>("scrub").max = String(r.frames.length - 1);

    resize();
    show(0);
    frameLoop();
    play();

    el("play").onclick = play;
    el("rewind").onclick = () => show(0);
    el<HTMLInputElement>("scrub").oninput = (e) => {
      if (playing) play();
      show(Number((e.target as HTMLInputElement).value));
    };
  } catch (error) {
    el("meta").textContent = `could not read the corpus: ${error}`;
  }
})();
