// P7.3 — the visible token count matches the ledger.
//
// Run with `deno test --allow-net app/tests/`. It reads the live corpus
// through the running server, because a canvas checked against a fixture is
// checked against something nobody looks at.

import {
  depths,
  headlineOf,
  heldTokens,
  history,
  layout,
  levelsOf,
  PARTICLE_CAP,
  particlesFor,
  poolRadius,
  visibleTokens,
} from "../src/canvas.ts";
import type { Frame, Graph, Run } from "../src/types.ts";

const BASE = Deno.env.get("PACT_BASE") ?? "http://127.0.0.1:8730";

async function live<T>(path: string): Promise<T | null> {
  try {
    const r = await fetch(BASE + path);
    if (!r.ok) return null;
    return await r.json() as T;
  } catch {
    return null;
  }
}

function skip(why: string) {
  console.log(`SKIPPED: ${why}`);
  console.log("  start it with: cargo run -p pact-server 127.0.0.1:8730");
}

Deno.test("the visible token count matches the ledger", async () => {
  const run = await live<Run>("/api/trace");
  if (!run) return skip("no server");

  // The gate. Every unit the canvas draws as a token is a unit the engine
  // reported moving, and every unit the engine reported is drawn. A canvas
  // showing more or fewer than the engine accounts for is animating a fiction.
  for (const [i, frame] of run.frames.entries()) {
    const drawn = visibleTokens(frame);
    const ledger = frame.flows.reduce((sum, f) => sum + f.n, 0);
    if (drawn !== ledger) {
      throw new Error(
        `frame ${i}: the canvas would draw ${drawn} tokens and the engine ` +
          `reported ${ledger} moving`,
      );
    }
  }

  // And the run was not inert, or the equality above is 0 === 0 sixty times.
  const total = run.frames.reduce((n, f) => n + visibleTokens(f), 0);
  if (total < 50) {
    throw new Error(`only ${total} tokens moved across the whole run`);
  }
});

Deno.test("the particle cap does not hide units from the count", async () => {
  const run = await live<Run>("/api/trace");
  if (!run) return skip("no server");

  // `particlesFor` caps how many dots one arc draws, because two hundred on
  // one curve reads as a solid line. The cap must not reach `visibleTokens`,
  // or the gate above would compare the cap against the cap and always pass.
  let capped = 0;
  for (const frame of run.frames) {
    for (const flow of frame.flows) {
      const drawn = particlesFor(flow).length;
      if (drawn > PARTICLE_CAP) {
        throw new Error(`a flow drew ${drawn} particles, above the cap`);
      }
      if (flow.n > PARTICLE_CAP) capped++;
      if (drawn !== Math.min(flow.n, PARTICLE_CAP)) {
        throw new Error(`a flow of ${flow.n} drew ${drawn}`);
      }
    }
  }
  console.log(`  ${capped} flow(s) exceeded the cap and are counted in full`);
});

Deno.test("the count is the real total even when the cap bites", () => {
  // Written because the live check above turned out to be VACUOUS against
  // this failure mode: no flow in the current corpus exceeds the cap, so
  // `min(n, CAP)` equals `n` everywhere and a renderer that reported the cap
  // instead of the total would have passed unchanged.
  //
  // A gate that cannot fail is not a gate. A synthetic frame with a flow far
  // over the cap makes it fail for the right reason on any corpus.
  const frame: Frame = {
    levels: [["M", 500]],
    flows: [
      { from: "M", to: "State", n: 500, via: "flood" },
      { from: "State", to: "M", n: 3, via: "trickle" },
    ],
    fired: ["flood", "trickle"],
    starved: 0,
  };

  const counted = visibleTokens(frame);
  if (counted !== 503) {
    throw new Error(
      `503 units moved and the canvas would report ${counted}. ` +
        "If this equals the cap, the count is reporting what it drew rather " +
        "than what the engine did.",
    );
  }

  const drawn = particlesFor(frame.flows[0]).length;
  if (drawn !== PARTICLE_CAP) {
    throw new Error(`a flow of 500 drew ${drawn} particles, not the cap`);
  }
  if (drawn >= counted) {
    throw new Error("the cap did not bite, so this test proves nothing");
  }
});

Deno.test("a level survives a relayout", async () => {
  const [graph, run] = await Promise.all([
    live<Graph>("/api/graph"),
    live<Run>("/api/trace"),
  ]);
  if (!graph || !run) return skip("no server");

  // The prototype's bug, as a test. It stored level on the objects `layout()`
  // rebuilt, so any resize silently zeroed every pool while the data stayed
  // correct. Geometry and state are separate maps now, and this is what says
  // so.
  const levels = levelsOf(run.frames[28]);
  const before = new Map(levels);

  layout(graph, 1600, 900);
  layout(graph, 800, 600);
  layout(graph, 2400, 1400);

  for (const [ty, n] of before) {
    if (levels.get(ty) !== n) {
      throw new Error(`${ty} was ${n} and a relayout made it ${levels.get(ty)}`);
    }
  }

  // And the levels are not all zero, or the assertion is vacuous.
  const held = [...levels.values()].reduce((a, b) => a + b, 0);
  if (held === 0) throw new Error("every pool is empty at step 28");
});

Deno.test("a pool's area tracks its level", async () => {
  // Area, not radius. A pool holding a hundred should look ten times one
  // holding ten, which is what the eye reads; scaling the radius linearly
  // would make it a hundred times and swamp the canvas.
  const small = poolRadius(10) - 9;
  const large = poolRadius(1000) - 9;
  const ratio = large / small;
  if (Math.abs(ratio - 10) > 0.01) {
    throw new Error(`a 100x level gave a ${ratio.toFixed(2)}x radius, not 10x`);
  }
  if (poolRadius(0) !== 9) throw new Error("an empty pool must still be visible");
  if (poolRadius(-5) !== 9) throw new Error("a negative level must not vanish");
});

Deno.test("layout places every type, and once", async () => {
  const graph = await live<Graph>("/api/graph");
  if (!graph) return skip("no server");

  const placed = layout(graph, 1600, 900);
  if (placed.size !== graph.nodes.length) {
    throw new Error(`${graph.nodes.length} types, ${placed.size} placed`);
  }
  for (const n of graph.nodes) {
    const p = placed.get(n.id);
    if (!p) throw new Error(`${n.id} was not placed`);
    if (!Number.isFinite(p.x) || !Number.isFinite(p.y)) {
      throw new Error(`${n.id} was placed at ${p.x},${p.y}`);
    }
    if (p.x < 0 || p.x > 1600 || p.y < 0 || p.y > 900) {
      throw new Error(`${n.id} was placed off-canvas at ${p.x},${p.y}`);
    }
  }

  // Sources sit at depth zero. If they did not, the left-to-right reading
  // would be arbitrary and a ring would be no worse.
  const depth = depths(graph);
  for (const n of graph.nodes) {
    if (n.kind === "source" && depth.get(n.id) !== 0) {
      throw new Error(`${n.id} is a source at depth ${depth.get(n.id)}`);
    }
  }
});

Deno.test("a self-flow is drawn as a loop, not a point", async () => {
  const run = await live<Run>("/api/trace");
  if (!run) return skip("no server");

  // The prototype collapsed `A -> A` to a single point underneath the node,
  // which is why nothing appeared to move on those arcs.
  const selfFlows = run.frames
    .flatMap((f) => f.flows)
    .filter((f) => f.from === f.to);
  if (selfFlows.length === 0) {
    console.log("  no self-flows in this corpus; rule still asserted below");
  }
  for (const flow of selfFlows) {
    for (const p of particlesFor(flow)) {
      if (!p.self) throw new Error(`${flow.via} is a self-flow and is not marked`);
    }
  }
  const straight = { from: "M", to: "State", n: 3, via: "x" };
  for (const p of particlesFor(straight)) {
    if (p.self) throw new Error("a straight flow was marked as a loop");
  }
});

Deno.test("history is what makes accumulation legible", async () => {
  const run = await live<Run>("/api/trace");
  if (!run) return skip("no server");

  // A pool climbing for thirty steps looks identical to one that just filled
  // without this.
  const hottest = run.frames[run.frames.length - 1].levels
    .slice()
    .sort((a, b) => b[1] - a[1])[0];
  if (!hottest) throw new Error("no types");

  const past = history(run, hottest[0], run.frames.length - 1, 30);
  if (past.length === 0) throw new Error("no history");
  if (past[past.length - 1] !== hottest[1]) {
    throw new Error("the last point of the history is not the current level");
  }
  console.log(`  ${hottest[0]}: ${past.slice(-8).join(" → ")}`);
});

Deno.test("the headline reports what the frame reports", async () => {
  const run = await live<Run>("/api/trace");
  if (!run) return skip("no server");

  const step = 28;
  const head = headlineOf(run, step);
  const frame = run.frames[step];

  if (head.fired !== frame.fired.length) throw new Error("fired disagrees");
  if (head.starved !== frame.starved) throw new Error("starved disagrees");
  if (head.held !== heldTokens(frame)) throw new Error("held disagrees");
  if (head.moving !== visibleTokens(frame)) throw new Error("moving disagrees");

  // The number the prototype buried in eleven-pixel grey at the bottom.
  console.log(
    `  step ${step}: ${head.fired} fired, ${head.starved} starved, ` +
      `${head.held} held, ${head.moving} moving`,
  );
});
