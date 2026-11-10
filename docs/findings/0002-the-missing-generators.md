# Finding 0002 — the corpus needed three generators, not two

**Date:** 2026-07-31
**Gate:** P2.5
**Oracles:** `prompt_economy.py`, `decompose.py`, `prompt_algebra.py`

## What was measured

| | before | after |
|---|---|---|
| converters that can never fire | **9** | **0** |
| source-only types (consumed, never produced) | `M^n`, `M_code`, `Subject` | `Subject` |
| feedback loops in the economy | 5 | 12 |
| chains at 4/4 railpulse completeness | 0 of 12 | **1** of 13 |
| chains that typecheck end to end | 2 of 12 | 3 of 13 |
| `State` pressure | 4.36 | 3.69 |

`Subject` remaining as the only source-only type is correct, not residual: it
is what a person brings, and Theorem 1 says external import is the only thing
that extends a loop's life. A corpus where `Subject` had a producer would be
one that had invented its own inputs.

## The checklist said two generators. There were three.

The plan named `merge : M^n → M` and `compact : State → M_arch`. Those close
two of the three holes. The third only shows up if you ask *why* nine
converters were dead rather than seven:

```
M^n has 0 producers, 7 consumers
  art:convergent  meta-review  meta:amr  syn:core
  syn:emergent    syn:fusion   syn:radical

M_code has 0 producers, 2 consumers
  ops:git  swiftui-pro
```

Seven plus two is nine. `merge` reaches the seven. Nothing reached the other
two, and the plan had not noticed they were a separate hole.

### `emit:code`, and the reading it rejects

The alternative was that `M_code` is a phantom type and its two consumers
should read `State` like everything else. That would also have made them
reachable, with less work, and it is genuinely arguable.

It was rejected because the two types answer different questions. `State` says
work occurred. `M_code` says *here is the thing, at this path, in this shape*.
Collapsing them makes "did it run" and "what did it produce" the same
question, and a corpus that cannot tell those apart cannot tell a successful
run from a successful run that produced nothing.

**What would falsify this:** `M_code` ends up with exactly one producer and its
consumers keep reaching for `State` anyway.

## `merge` was typed wrong first, and the corpus said so

The first version read `M → M^n`, on the reasoning that a fan-in gathers
analyses.

The corpus disagreed, unanimously: **every skill that consumes `M_plan`
produces `State`**, without exception. Parallel tracks are executions, and what
they hand back is a changed world, not an analysis. A merge typed `M → M^n`
sits off the execution path entirely, and no loop can route through it.

Retyped to `State → M^n`. Feedback loops went 9 → 11 on that change alone,
which is the shape of a step that had been in the wrong place.

Gathering analyses is a real operation. It is a different one.

## Building a generator does not put it in a chain

This is the part worth remembering. After all three generators existed, the
decomposition was **unchanged**: still 0 chains at 4/4, still `↺` missing from
11 of 12.

`decompose.py` reads a chain's members from the skill names its own
documentation mentions. Making a skill exist makes it *available*; it does not
make anyone use it. Corpus-level holes and chain-level holes are different
holes and they close by different means.

So `chain:generational` was authored: the first sequence in this corpus that
closes.

```
compact ▸ divergent-rail ▸ ⊗( pulse* , ops:autopilot ) ▸ merge ▸ syn:core ▸ ops:cognitive-twin ↺
  State       M_arch            M_plan     M_plan          State    M^n         M              State
```

4/4, and it typechecks at every step — one of three in the corpus that does.

## A weakness in the oracle, found by using it

`decompose.py` extracts members **positionally from prose**. Any known skill
name anywhere in a chain's documentation joins its member list, in order of
first mention.

The first draft of `chain:generational` scored 3/4 and did not typecheck, not
because the sequence was wrong but because the closing prose mentioned two
unrelated skills and the bare word "chain", which is itself a skill name. The
factoring changed when the paragraph did.

That means **a chain's word depends on how its documentation is written, not
only on what it does.** P3.2 has to reproduce this behaviour exactly to match
the oracle, which means reproducing the weakness. Worth knowing before the
differential test starts disagreeing for reasons that have nothing to do with
the rewrite.

## What this does not prove

That any of the nine now-reachable converters produce anything useful. They can
fire; that is a statement about types and nothing else. Seven of them are
synthesis skills whose value is entirely in what they synthesise, and the type
system has no opinion about that and never will.

Nor does it prove `chain:generational` runs well. It proves it *closes* — that
the last thing it produces is the first thing it needs. Whether a second
generation is better than the first is what the barcode is for, and the barcode
does not exist yet.
