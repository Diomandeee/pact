// The engine, from the browser's side.
//
// Two ways in, and which one is used is the difference between a page that
// reports and an instrument that responds:
//
//   fetch()  — the server ran it. One round trip, whole trace, no interaction.
//   wasm()   — the same engine, here, per frame. Editing needs this.
//
// Both reach `pact_core::Simulator`. There is no TypeScript simulator and
// there must never be one: two engines drift, and P7.1's gate exists to make
// that impossible rather than merely discouraged.

import type {
  Chains,
  Graph,
  Overview,
  Run,
  SkillDetail,
} from "./types.ts";

export type { Chains, Graph, Overview, Run, SkillDetail };

/// Where the numbers came from. Shown, because a page reporting on a corpus
/// without naming it is one somebody eventually reads against the wrong one.
export interface Meta {
  root: string;
  skills: number;
}

async function json<T>(path: string): Promise<T> {
  const response = await fetch(path);
  if (!response.ok) {
    throw new Error(`${path} answered ${response.status}`);
  }
  return await response.json() as T;
}

/// Everything the server computed, in one load.
export const api = {
  meta: () => json<Meta>("/api/meta"),
  overview: () => json<Overview>("/api/overview"),
  chains: () => json<Chains>("/api/chains"),
  graph: () => json<Graph>("/api/graph"),
  trace: () => json<Run>("/api/trace"),
  skill: (name: string) =>
    json<SkillDetail>(`/api/skill/${encodeURIComponent(name)}`),
};

// ── the wasm side ─────────────────────────────────────────────────────────

/// A graph as the engine's wire form describes one.
///
/// Mirrors `pact_wasm::GraphSpec`. Hand-written rather than generated because
/// `pact-wasm` has no serve path and therefore no `view.rs` to derive from;
/// when editing lands and the spec starts changing, it moves under the same
/// generator as everything else.
export interface GraphSpec {
  nodes: NodeSpec[];
  edges: EdgeSpec[];
}

export interface NodeSpec {
  id: string;
  kind: "pool" | "source" | "drain" | "converter" | "gate" | "delay";
  bag?: [string, number][];
  consumes?: [string, number][];
  produces?: [string, number][];
  distribution?: "broadcast" | "probabilistic" | "deterministic";
  weights?: number[];
  steps?: number;
  activation?: "automatic" | "triggered" | "starting" | "passive";
}

export interface EdgeSpec {
  from: string;
  to: string;
  rate?: number;
}

/// What a run reports. Mirrors `pact_wasm::RunReport`.
export interface RunReport {
  seed: number;
  steps: number;
  events: string[];
  created: [string, number][];
  destroyed: [string, number][];
  conserved: boolean;
}

interface Exports {
  memory: WebAssembly.Memory;
  alloc: (len: number) => number;
  dealloc: (ptr: number, len: number) => void;
  run_buffer: (
    ptr: number,
    len: number,
    seed: bigint,
    steps: number,
  ) => number;
}

/// The engine, loaded into this page.
///
/// No `wasm-bindgen`: the boundary is a flat buffer, so this is the raw
/// `WebAssembly` API and about thirty lines. A dependency whose job is
/// marshalling two strings would be the largest thing here.
export class Engine {
  #exports: Exports;

  private constructor(exports: Exports) {
    this.#exports = exports;
  }

  static async load(url = "/pact_wasm.wasm"): Promise<Engine> {
    const response = await fetch(url);
    if (!response.ok) {
      throw new Error(`the engine did not load: ${response.status}`);
    }
    const { instance } = await WebAssembly.instantiate(
      await response.arrayBuffer(),
      {},
    );
    return new Engine(instance.exports as unknown as Exports);
  }

  /// Run a graph and report what happened.
  ///
  /// The same call the differential test makes natively. If this and the
  /// server ever disagree, one of them is not the engine.
  run(spec: GraphSpec, seed: number, steps: number): RunReport {
    const { memory, alloc, dealloc, run_buffer } = this.#exports;

    const input = new TextEncoder().encode(JSON.stringify(spec));
    const inPtr = alloc(input.length);
    new Uint8Array(memory.buffer, inPtr, input.length).set(input);

    const outPtr = run_buffer(inPtr, input.length, BigInt(seed), steps);
    // `[u32 little-endian length][utf-8 bytes]`, because a wasm export returns
    // one value and the caller needs both.
    const len = new DataView(memory.buffer).getUint32(outPtr, true);
    const text = new TextDecoder().decode(
      new Uint8Array(memory.buffer, outPtr + 4, len),
    );

    dealloc(inPtr, input.length);
    dealloc(outPtr, 4 + len);

    const report = JSON.parse(text) as RunReport | { error: string };
    if ("error" in report) {
      throw new Error(report.error);
    }
    return report;
  }
}

// A field the page reads, named here so a rename in Rust has something to
// break. Without a use site, generated types drift silently: the file changes,
// nothing references the old name, and `deno check` stays green while the page
// reads a field that is never sent.
export function headline(o: Overview): string {
  const dead = o.never_fires.length;
  return `${o.skills} skills, ${o.types} types, ${dead} that can never fire`;
}
