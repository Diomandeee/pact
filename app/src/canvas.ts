// The canvas: layout, and what a frame draws.
//
// Split deliberately into a pure half and a rendering half. Everything above
// `render()` is a function of data with no DOM in it, which is what lets the
// gate check that **the visible token count matches the ledger** without a
// browser. A renderer that can only be checked by looking at it is one whose
// arithmetic nobody checks.
//
// ── the prototype's bug, and why it cannot happen here ──
//
// The prototype stored `level` ON the node objects that `layout()` rebuilt.
// Any relayout — a resize, a window drag — silently reset every pool to zero
// while the data stayed correct, so the canvas showed a corpus at rest and
// nothing anywhere reported an error.
//
// Geometry and state are separate maps here. `layout()` cannot touch a level
// because it does not have one, which removes the class rather than the
// instance.

import type { Flow, Frame, Graph, GraphNode, Run } from "./types.ts";

export interface Point {
  x: number;
  y: number;
}

/// Where every type sits. Produced by layout, and never carrying state.
export type Layout = Map<string, Point>;

/// What every type holds. Produced by a frame, and never carrying geometry.
export type Levels = Map<string, number>;

// ── layout ────────────────────────────────────────────────────────────────

/// Depth from the sources, by breadth-first walk over the type graph.
///
/// A ring forces every path through the middle and produces a hairball; the
/// corpus has a natural order — `Subject → M → M_plan → State → M_arch` — and
/// laying it out left to right by depth reads as the pipeline-with-cycles it
/// actually is.
export function depths(graph: Graph): Map<string, number> {
  const next = new Map<string, string[]>();
  for (const e of graph.edges) {
    if (!next.has(e.from)) next.set(e.from, []);
    next.get(e.from)!.push(e.to);
  }

  const depth = new Map<string, number>();
  const queue: string[] = graph.nodes
    .filter((n) => n.kind === "source")
    .map((n) => n.id);
  // A corpus with no source-only type would leave everything unplaced, so the
  // most-produced type stands in. That is a degenerate corpus rather than an
  // error, and refusing to draw it would say less than drawing it.
  if (queue.length === 0 && graph.nodes.length > 0) {
    const busiest = [...graph.nodes].sort((a, b) => b.inflow - a.inflow)[0];
    queue.push(busiest.id);
  }
  for (const id of queue) depth.set(id, 0);

  while (queue.length > 0) {
    const id = queue.shift()!;
    const d = depth.get(id)!;
    for (const to of next.get(id) ?? []) {
      if (!depth.has(to)) {
        depth.set(to, d + 1);
        queue.push(to);
      }
    }
  }
  // Anything unreachable from a source sits after everything else, which is
  // itself the finding: nothing produces what it reads.
  const deepest = Math.max(0, ...depth.values());
  for (const n of graph.nodes) {
    if (!depth.has(n.id)) depth.set(n.id, deepest + 1);
  }
  return depth;
}

/// Place every type. Columns are depth, rows are ordered by how much flows in.
export function layout(graph: Graph, width: number, height: number): Layout {
  const depth = depths(graph);
  const columns = new Map<number, GraphNode[]>();
  for (const n of graph.nodes) {
    const d = depth.get(n.id) ?? 0;
    if (!columns.has(d)) columns.set(d, []);
    columns.get(d)!.push(n);
  }

  const keys = [...columns.keys()].sort((a, b) => a - b);
  const pad = 110;
  const span = Math.max(1, keys.length - 1);
  const out: Layout = new Map();

  for (const [i, d] of keys.entries()) {
    const column = columns.get(d)!.sort((a, b) => b.inflow - a.inflow);
    const x = pad + (i / span) * (width - pad * 2);
    for (const [j, n] of column.entries()) {
      const slot = (j + 1) / (column.length + 1);
      out.set(n.id, { x, y: pad * 0.5 + slot * (height - pad) });
    }
  }
  return out;
}

// ── what a frame holds ────────────────────────────────────────────────────

/// The levels a frame reports, as a map the renderer reads.
export function levelsOf(frame: Frame): Levels {
  return new Map(frame.levels);
}

/// How many units the canvas will draw as tokens for this frame.
///
/// The number the gate compares against the ledger. If the renderer drew a
/// capped or sampled subset, this would report the cap and the two would
/// agree — so it counts what is actually drawn, and the cap lives in
/// `particlesFor` where it is visible.
export function visibleTokens(frame: Frame): number {
  let total = 0;
  for (const flow of frame.flows) total += flow.n;
  return total;
}

/// How much the whole system holds, which conservation is about.
export function heldTokens(frame: Frame): number {
  let total = 0;
  for (const [, n] of frame.levels) total += n;
  return total;
}

/// A pool's radius. Area, not radius, tracks the level.
///
/// A pool holding a hundred is ten times the area of one holding ten, which is
/// what the eye reads. Scaling the radius linearly would make it a hundred
/// times the area and swamp the canvas.
export function poolRadius(level: number): number {
  return 9 + Math.sqrt(Math.max(0, level)) * 3.4;
}

/// One drawn unit in transit.
export interface Particle {
  from: string;
  to: string;
  t: number;
  self: boolean;
}

/// The particles a flow contributes.
///
/// Staggered so they read as a stream rather than a clump, and capped: a flow
/// of two hundred would draw two hundred dots on one arc and read as a solid
/// line. The cap is stated here rather than hidden, and `visibleTokens` counts
/// the real total so the gate compares against the ledger and not against the
/// cap.
export const PARTICLE_CAP = 12;

export function particlesFor(flow: Flow): Particle[] {
  const drawn = Math.min(flow.n, PARTICLE_CAP);
  const out: Particle[] = [];
  for (let i = 0; i < drawn; i++) {
    out.push({
      from: flow.from,
      to: flow.to,
      // Spread across the first third so they leave as a stream.
      t: (i / Math.max(1, drawn)) * 0.33,
      // A self-flow has no direction to travel; drawn as a loop above the
      // node instead of collapsing to a point underneath it, which is what
      // the prototype did and why nothing appeared to move.
      self: flow.from === flow.to,
    });
  }
  return out;
}

/// The last `window` levels for one type, for a sparkline.
///
/// A pool climbing for thirty steps looks identical to one that just filled
/// without this, and accumulation was the whole argument for the medium.
export function history(run: Run, ty: string, upto: number, window = 30): number[] {
  const from = Math.max(0, upto - window + 1);
  const out: number[] = [];
  for (let i = from; i <= upto && i < run.frames.length; i++) {
    const found = run.frames[i].levels.find(([name]) => name === ty);
    out.push(found ? found[1] : 0);
  }
  return out;
}

/// What the header reports. The starvation figure is first-class here because
/// it is the most striking fact about the corpus and the prototype buried it
/// in eleven-pixel grey at the bottom.
export interface Headline {
  step: number;
  fired: number;
  starved: number;
  held: number;
  moving: number;
}

export function headlineOf(run: Run, step: number): Headline {
  const frame = run.frames[step];
  return {
    step,
    fired: frame.fired.length,
    starved: frame.starved,
    held: heldTokens(frame),
    moving: visibleTokens(frame),
  };
}
