# ADR 0004 — Simulation and execution share one graph, two interpreters

**Status:** accepted, 2026-07-30
**Layer:** L0

## Context

The tool needs to simulate a system before it runs, and also to run real rails
with real agents. These could be separate representations with separate
scheduling, or one representation with two readings.

## Decision

One graph. The scheduler that decides which node acts on which step is written
once, against an `Interpreter` trait. `Simulator` moves synthetic tokens
against an in-memory ledger. An executor moves real work by dispatching agents
and observing artifacts on disk. They differ in what a step means and where
tokens come from; they share the vocabulary and every invariant.

## Consequences

**Good.** Simulation is worthless unless it predicts execution, and a shared
scheduler is the strongest available guarantee that it does.

**Good.** One protocol definition governs both. A rail that validates for the
simulator validates for the executor.

**Costly and honest.** Execution is nondeterministic and simulation is not. The
answer is P6: determinism lives on the predicates, never the effects. Whether
an iteration was *allowed* to run is always deterministic; what it produces
never is.

**Risky.** This is the most ambitious decision in the set. If the trait ends up
shaped like the simulator, the executor will not fit it and the sharing becomes
a lie told in the type system.

The gate for P1.4 exists for exactly this, and it took three drafts.

A recording no-op proves nothing: a no-op needs nothing, so it satisfies a
simulator-shaped trait trivially. Requiring a hostile implementation to pass
the same behavioural suite is incoherent: a hostile impl fails transfers by
design, so its behaviour differs on purpose.

What survives is structural and cheap. The trait's signatures must be unable
to express simulator-only assumptions: no seed or step counter in any method,
`transfer` returns `Result` because an executor's transfer fails routinely,
and `available` does not hand back a borrow of an in-memory map. That catches
the leak without demanding a second implementation before the first one
exists.

The expensive proof is building the executor. That belongs below, as
falsification, not above as a blocker. A gate that makes early work
disproportionately hard relative to the risk it retires is a bad gate even
when the risk is real.

## What would falsify this

Build the executor. If more than roughly a third of the scheduler needs
`if simulating` branches, the abstraction is wrong and the two should split.
Recorded now so that outcome reads as evidence rather than as failure.
