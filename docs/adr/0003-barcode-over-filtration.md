# ADR 0003 — Persistence lives in L2, over a filtration

**Status:** accepted, 2026-07-30
**Layer:** L2

## Context

The barcode is the system's actual output: the structures that survived being
compressed across generations. It had no home in the layering, and the obvious
wrong answers were to put it in the engine (where it would privilege one
protocol) or in the CLI (where it would be a rendering rather than a result).

## Decision

Persistence is an analysis, alongside statics, dynamics, and structure. It
operates on a `Filtration`: an ordered sequence of graphs **plus the maps
between them**, not on graphs alone.

`is_simplicial` becomes a precondition on admitting a step into the filtration,
not a separate check run afterwards.

## Consequences

**Good.** A fabricated face is rejected at the door rather than discovered in
the report. That is the difference between a hallucination detector and a
hallucination log.

**Good.** Persistence gets the same status as the other three lenses. None is
privileged, and a user who only wants pressure never pays for homology.

**Costly.** The filtration has to carry the vertex maps, which means whoever
produces a generation must also produce the map. That is real bookkeeping and
it cannot be inferred after the fact, because inferring it is exactly the
fabrication being guarded against.

## What would falsify this

If in practice nobody ever has more than one generation, persistence is
theatre and the barcode is a single column. The test is Phase 3 of the original
plan: three generations with at least one dead bar. A barcode where nothing
dies is not selecting, it is accumulating.
