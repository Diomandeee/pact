# Finding 0001 — Every chain is half a cycle

*2026-07-30. Corpus: 150 skills, 144 typed, 11 chains.*

## What was measured

Each chain was factored into the six combinators. Types decide, not prose: two
consecutive members that consume the **same** input type are parallel, not
sequential, regardless of what the documentation calls them.

```
▸  sequence    out(A) == in(B)
⊗  broadcast   in(A) == in(B)
⊕  merge       a member consumes M^n
↺  feedback    the last output can reach the first input
*  iterate     the member repeats until a condition
+  choice      conditional routing
```

Canonical cycle: `railpulse = ↺( ⊗ ▸ * ▸ ⊕ )` — expand, execute, select,
repeat.

## Result

| chain | word | complete | typechecks |
|---|---|---|---|
| chain:adversarial-forge | `✗ ⊗ ⊗ ↺` | 2/4 | no |
| chain:creative-engine | `⊗ ⊗ ⊗` | 1/4 | yes |
| chain:deep-research | `⊗ ✗ ⊗ ✗ ⊕` | 2/4 | no |
| chain:diagnose-fix | `✗ ✗ ✗ ⊕ *` | 2/4 | no |
| chain:evolve-ship | `▸ ✗ ⊗ *` | 2/4 | no |
| chain:full-omega | `⊗ ⊗ ⊗ *` | 2/4 | yes |
| chain:genesis | `⊗ ▸ ✗ ✗ ⊗ ⊕` | 2/4 | no |
| chain:idea-to-code | `▸ ▸ ✗ *` | 1/4 | no |
| chain:leverage | `⊗ ⊗ ⊗ ⊗ ✗ ▸ *` | 2/4 | no |
| chain:quality-gate | `⊗ ✗ ✗ ⊕` | 2/4 | no |
| chain:sovereign-loop | `⊗ ✗ ✗ *` | 2/4 | no |

**Not one chain reaches 3 of 4 stages.** The best any of them manage is half
the cycle.

Stage absence across the corpus:

```
↺ repeat    missing in 10 of 11
⊕ select    missing in  7 of 11
* execute   missing in  5 of 11
⊗ expand    missing in  2 of 11
```

**Nine of eleven chains do not typecheck.**

## Reading it

The corpus is a library of half-cycles. It expands reliably, executes
sometimes, converges rarely, and repeats almost never. That is not a criticism
of any individual chain; it is a shape, and the shape has a cause.

Two combinators have no implementation anywhere. `⊕` needs something producing
`M^n` and nothing does, which is why seven chains cannot select and why nine
skills in the wider corpus can never fire. `↺` needs a `State → M_arch`
morphism and nothing does, which is why ten of eleven chains are pipelines
that run once and drain.

The two missing generators are not incidental. They are exactly the two stages
the corpus is missing, and building them would move most of these chains from
2/4 to 4/4 without rewriting any of them.

## What this does not prove

**The falsification test was too weak to have failed.** The predicate used was
"shows at least one canonical stage, or typechecks," and since nearly every
chain contains a `⊗`, almost nothing could have refuted it. The claim survived
a test that could not have killed it, which is close to no evidence at all.

A real test needs a stronger predicate: a chain refutes the claim if its word
contains a combinator pattern that cannot be rewritten into `↺(⊗ ▸ * ▸ ⊕ )`
with some stages set to identity. That rewriting relation has not been defined
yet, and until it is, "every chain is a degenerate railpulse" remains a
conjecture with suggestive evidence rather than a result.

**Member order is approximate.** Chain members were extracted by first mention
in the document, which approximates execution order but does not guarantee it.
The `⊗` detections are robust to this, because two skills consuming the same
type are parallel no matter what order you read them in. The specific
positions of `✗` are not robust, though the *count* of non-composing chains is
stable under reordering, since a type mismatch somewhere does not disappear by
permuting.

## What is robust

- No chain reaches 3/4. Order-independent.
- `↺` absent from 10 of 11. Order-independent.
- 9 of 11 fail to typecheck. Order-independent in count.
- `⊗` dominates every word. Order-independent.

Those four stand regardless of the methodological caveats above.

## Consequence

The next thing to build is not another chain. It is the two missing
combinators, because they are the difference between a library of half-cycles
and a library of cycles.
