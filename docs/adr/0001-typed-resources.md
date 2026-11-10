# ADR 0001 — Resources are typed tokens, not counters

**Status:** accepted, 2026-07-30
**Layer:** L0

## Context

A Machinations pool holds an integer. The classic model is a single
undifferentiated resource, and every published example works that way.

The corpus this engine exists to analyse is a type system. `M_creative` has 33
producers and 2 consumers; `M_plan` has 4 and 16. Those are different facts
about different things.

## Decision

A resource is a token carrying a `Type`. A pool holds a `Bag`, which is a
multiset. Conservation is checked per type. A converter carries a `Signature`:
a multiset consumed, a multiset produced.

## Consequences

**Good.** A converter's signature is literally a skill's SOOP-2 block, so the
corpus importer is a translation rather than an interpretation. Per-type
pressure becomes computable, and pressure is the single most useful number the
economy lens produces. `Demand::All` becomes meaningful: a review needing three
`M` and having two has not run.

**Costly.** Conservation must be checked per type rather than in aggregate. An
aggregate check would pass while two types silently swapped, so the property
test is strictly more work and the failure it catches is invisible otherwise.

**Constraining.** `Type` is a newtype over `String` so it can never be confused
with a node id. Both are strings; they are not the same kind of thing, and
every bug where they got mixed would be invisible.

## What would falsify this

If a real corpus turns out to have one meaningful resource type and the
typing is ceremony, the multiset machinery is pure cost. The test: import a
second corpus that is not Mohamed's and see whether its type graph has more
than two or three objects. Twelve objects on the first corpus is one data
point, not a pattern.
