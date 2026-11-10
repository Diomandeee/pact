# ADR 0002 — A protocol is a validator; the generator is derived

**Status:** accepted, 2026-07-30
**Layer:** L1

## Context

A protocol like `railpulse` can be modelled two ways: as something that
*produces* a graph of the right shape, or as a predicate that *decides* whether
a given graph has that shape. Both are useful and they are different APIs.

## Decision

A protocol is fundamentally a predicate. `Protocol::validate(&Graph) -> Result`
is the primary interface. `Protocol::emit(args) -> Graph` is a convenience,
and INV-4 requires `validate(emit(x))` to hold for all `x`, enforced by
property test.

## Consequences

**Good.** The main use case survives: pointing the tool at a graph that already
exists and asking whether it is a legal rail. Under a generator-first design
that question has no answer.

**Good.** The generator is checkable against the validator. Under the
reverse ordering, nothing checks the generator at all.

**Costly.** Two implementations per protocol instead of one, and they can drift.
The property test is what stops that, and it only stops it if the generated
inputs are varied enough to be interesting.

## What would falsify this

If validators turn out to be so much harder to write than generators that
protocols stop being authored, the ordering is wrong in practice even if it is
right in principle. Signal to watch: a protocol whose validator is more than
roughly three times the size of its generator.
