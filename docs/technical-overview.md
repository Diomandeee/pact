# PACT technical overview

Prompt Algebra and Composition Theory: typed flow graphs for prompt and
agent systems, plus the analyses that run over them. This document
describes the architecture, the public API surface, the invariants and
their mechanical checks, and the governance that builds it. For *why* it
exists, see `.governance/0.1.1_PROJECT_CHARTER.md`. For the math of the
canonical protocol, see `SPEC.md`.

## Layering

Dependencies point one way: `L4 → L3 → L2 → L1 → L0`. `pact-core`
depends on nothing but `std` (INV-1, canaried). Only L4 crates
(`pact-cli`, `pact-server`) may take third-party dependencies (INV-8,
canaried). Analysis sits above import (L2 after L3) deliberately:
metrics over synthetic graphs never meet a contradicting fact.

| Layer | Crate | Contents |
|---|---|---|
| L0 engine | `pact-core` | `token.rs` (`Type`, `Bag`), `node.rs`, `edge.rs`, `graph.rs`, `interpret.rs` (`Interpreter` trait, `Scheduler`, `Ledger`), `sim.rs` |
| L1 algebra | `pact-algebra` | `combinator.rs`, `word.rs`, `protocol.rs` |
| L2 analysis | `pact-analysis` | `statics.rs`, `dynamics.rs`, `homology.rs`, `persistence.rs`, `stuck.rs`, `repertoire.rs` |
| L3 import | `pact-corpus` | `soop.rs`, `chains.rs` (frontmatter → graph) |
| L4 surface | `pact-cli` | `main.rs` (`check`, `decompose`, `simulate`, `export`, `tiers`, `stuck`), `export.rs`, `tiers.rs` |
| L4 surface | `pact-server` | HTTP API, Convex mirror (`mirror.rs`), views |
| L4 surface | `pact-wasm` | browser build of the engine |
| L4 surface | `pact-reader` | model reads bodies into closed vocabularies (`prompt.rs`, `openai.rs`, `cache.rs`) |

## L0: engine (`pact-core`)

Resources are typed (`Type`, a newtype over `String`) and held in
multisets (`Bag`). Six node kinds — Pool, Source, Drain, Converter,
Gate, Delay — each expressing something the others cannot. Two edge
kinds:

- **Resource edges** move work: `ResourceEdge::new(from, to, n)`,
  `::all(...)` for unbounded draw, with `Rate::resolve(available)` and
  `Rate::appetite()`.
- **State edges** modify behaviour without moving work: `activator`,
  `trigger`, `inhibitor` (plus Modifier). Cross-layer forcing lives
  here.

`Graph::{new, add, connect, influence, validate}` — `validate()` returns
`Vec<Defect>` covering the six structural defects: dangling, duplicate,
starved, unbounded, orphan, sink.

Execution is split three ways. `eligible(graph, tick)` and
`inhibited(graph, tick)` are pure predicate functions. The
`Interpreter` trait consumes eligibility into `Event`s collected in a
`Trace`. `Scheduler` is written once against the trait; `sim.rs`
provides the `Simulator` (activation modes, state edges, delays,
gates). The `Ledger` records every event and `conserved(opening,
closing)` checks `created[t] == destroyed[t] + held[t]` **per type**
(INV-2) — asserted after every simulation step in property tests,
because an aggregate check passes while two types swap.

Reproducibility (INV-3): same graph + same seed = byte-identical trace.
Iteration order is `BTreeMap` everywhere; a `HashMap` in the hot path
is a violation even if the test happens to pass.

## L1: algebra (`pact-algebra`)

Six combinators: `▸` sequence, `⊗` broadcast, `⊕` merge, `↺` feedback,
`+` choice, `*` iterate. A **word** is a chain in combinators, e.g.
`chain:genesis` is `⊗ ▸ ✗ ✗ ⊗ ⊕`, and `railpulse` is `↺(⊗ ▸ * ▸ ⊕)`
(expand, execute, select, repeat). A **protocol** is a predicate on
graphs plus typed ports — and per INV-4 every protocol ships a
validator with its generator, with property tests asserting
`validate(emit(args)).is_ok()`: generation never precedes validation,
because the main use case is judging graphs that already exist.

INV-7: `pact-core` contains no `Rail`, `Pulse`, `Phase`, `Track`, or
`Generation` outside doc comments (canaried by grep). Protocols live in
`protocols/` (`railpulse.rs`), `#[path]`-included by their gate tests
and compiled into nothing that ships.

## L2: analysis (`pact-analysis`)

- **`statics`** — composability, dead ends (`never_fires`), source-only
  types. Gate: matches `prompt_algebra.py` on the live corpus.
- **`dynamics`** — per-type flow: inflow, outflow, pressure
  (`Machination`: Source/Drain/Pool), feedback loops, unbounded pools,
  plus a `verdict()`. Gate: matches `prompt_economy.py`.
- **`homology`** — `b₀`/`b₁` over GF(2) on the track nerve (vertices =
  tracks, simplices = tracks sharing a named artifact). `b₀ > 1` means
  a plan is a disjoint union pretending to be a plan. Gate: nine
  hand-checkable complexes, each also diffed against `loop_harness.py`.
- **`persistence`** — filtration, barcode, simplicial-map admission
  (fabrications kept out at the door, not flagged in the report).
- **`stuck`** (P9.1) — `StuckDetector`: fires on the second of two
  consecutive `REJECT_CORRECT`s, stays lit while the streak holds, any
  other move resets. Unknown labels break the streak. Gate: oracle
  parity over 218 vendored sessions plus 8 unit tests.
- **`repertoire`** (P9.2) — `Maneuver`: the 7 redirect classes plus
  `Continuation` as an exhaustive enum; unclassifiable input is `None`
  (do not intervene), never a ninth bucket. Gate: 170 human-baked rows
  plus an independent derivation diff.

Every L2/L3 gate reads "matches the Python" against a live or vendored
corpus — differential tests, with the standing rule that when Rust and
Python disagree, the Python is not assumed wrong (twice so far it was
right). INV-5: no check may call a model, hit a network, or read a
clock; analysis tests run with no network namespace in CI.

## L3: import (`pact-corpus`)

`soop.rs` parses SOOP-2 frontmatter into `Graph`s; `chains.rs` imports
chains as words with round-trip `graph → word → graph`. Six hand-typed
frontmatter fields per skill (~200 of ~3,000 bytes read); `load`,
`categorise`, `factor_all`, `to_graph` are the entry points.

## L4: surfaces

**CLI** (`pact`, no arg-parsing dependency — four subcommands over an
optional path, hand-split):

- `pact check [path]` — dead ends, starved skills, pressure, verdict.
  Exit code is the finding (non-zero when something cannot fire).
- `pact decompose [path]` — how every chain factors, what is missing.
- `pact simulate [path]` — run the corpus, report conservation.
- `pact export [path]` — mermaid diagram on stdout.
- `pact tiers [path]` — what the engine answered alone vs escalated,
  with reasons (P5.3: measures the model-call share).
- `pact stuck` — stdin JSON lines
  `{"session", "idx", "move"}` → stdout fire lines per the P9.1 rule;
  malformed lines counted on stderr, never fatal. Reads stdin, not the
  skills corpus. (P9.4; exercised by `scripts/watch-stuck`.)

**Server** (`pact-server`): HTTP API plus the Convex mirror — the sole
Convex writer, over HTTP, required to work with the backend
unreachable. Note: the binary embeds the wasm artifact via
`include_bytes!`, so `cargo test --workspace` fails unless the
`wasm32-unknown-unknown` release build exists first (pre-existing;
build wasm before testing the server).

**Reader** (`pact-reader`, P8): prompt + closed vocabulary +
content-keyed cache — a model infers types for untyped bodies; frontmatter
is stripped before the model sees it or the measurement reads the
answer off the page.

## Invariants and canaries

Every invariant ships a mechanical detector running in CI. Summary;
full statements in `.governance/0.1.3_ASSUMPTIONS_INVARIANTS.md`.

| ID | Rule | Canary |
|---|---|---|
| INV-1 | `pact-core` pure: no I/O, net, fs, clock | `cargo tree -p pact-core` empty; grep for `std::{fs,net,io,time,process}` |
| INV-2 | Conservation per type, every step | `Ledger::conserved()` in property tests |
| INV-3 | Same seed → byte-identical trace | double-run trace equality; `BTreeMap` order |
| INV-4 | Protocols validate before generating | `validate(emit(args)).is_ok()` property tests |
| INV-5 | Checks deterministic; never call models | no async/net in analysis; no-netns CI tests |
| INV-6 | No stubs pass gates | CI greps `todo!`, `unimplemented!`, `#[ignore]`; artifact-exists gates |
| INV-7 | No protocol primitives in core | grep for rail/pulse/phase/track/generation |
| INV-8 | Third-party deps only at L4 | `cargo tree -p pact-{analysis,algebra,corpus} --depth 1` |

Assumptions (believed, falsifiable, recorded with confidence and
evidence): ASM-1, every non-trivial chain factors as a degenerate
railpulse (low confidence; 11/11 show ≥1 canonical stage).

## Governance mechanics

- `scripts/pulse next|board|remaining|verify` — what is next (item +
  gate + what it opens), ticked/unticked, ordered remainder,
  canaries + lock check + tests as one verdict.
- `scripts/pz status|open|check|scaffold` — open/next/far and why,
  unlock a gated file, verify nothing was edited while locked,
  create every locked file.
- `scripts/canaries.sh` — every invariant's detector.
- `.governance/0.3_LIVING_CHECKLIST.md` — phases P0–P9; an item ticks
  because its gate command passed, with counts and date plus the
  defects found on the way. P9 (stuck trigger, repertoire, trigger
  A/B with scored round on file, live watcher) is the
  trajectory work; P6.5 deploy has passed.
- `.governance/0.4_UNLOCK_MAP.toml` — every expected file with
  `unlocks_after` and `why`. Editing a locked file fails `pz check`.
- `docs/adr/` — one file per consequential decision, each carrying
  what would falsify it. `docs/findings/` — measured results.

## Scripts of record (P9)

- `crates/pact-analysis/tests/fixtures/live_gate_ab.{md,py}` —
  pre-registered trigger A/B protocol + harness (`--smoke`, `--embed`,
  `--offline`); `--offline` needs a private corpus.
- `scripts/watch-stuck` — `--init/--sweep/--backfill/--selftest`;
  classify in Python, decide via `pact stuck`, fire records to gitignored
  `var/stuck-fires.jsonl`. Read-only on `~/.claude/prompt-logs`.
