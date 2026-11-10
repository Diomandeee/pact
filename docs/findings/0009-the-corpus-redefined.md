# Finding 0009 — the corpus, redefined by its own analysis

**Date:** 2026-11-03. **Corpus:** `~/.claude/skills`, 157 typed skills before,
162 after. **Oracles:** `decompose.py`, `prompt_algebra.py`,
`prompt_economy.py` (all in `divergent-rail/scripts/`), and the Rust
`pact check` / `pact decompose` on a second machine agreeing with them.

## What was measured

Finding 0001 said every chain was half a cycle, and 0002 built the three
generators the economy was short of. Nobody then went back and fixed the
chains. Three months later the corpus had grown by seven skills and the
chain picture had not moved: 10 of 13 did not typecheck, one reached 4/4,
and the reason was structural rather than careless.

| | before | after |
|---|---|---|
| typed skills | 157 | 162 |
| skill folders PACT could not read (no type block) | 4 | 0 |
| types in the economy | 12 | 11 (`M_analytical` gone) |
| chains that typecheck end to end | 3 of 13 | **13 of 13** |
| chains at 4/4 | 1 | 1 (`chain:generational`) |
| chains that declare their members | 1 (by accident of prose) | 13 |
| `M_review` producers → consumers | 7 → **0** | 8 → 1 (`triage`) |
| dead-end types | `M_analytical`, `M_review`, `Report`, `silent` | `Report`, `silent` (sanctioned: what a person takes away) |
| source-only types | `Subject` | `Subject` (correct, unchanged) |
| converters that can never fire | 0 | 0 |
| feedback loops in the economy | 12 | 19 |
| `M_creative` pressure | 16.5 (33 → 2) | 17 (34 → 2), untouched, see below |

## Three causes, three fixes

**1. The word came from the prose.** Members were scanned by first mention,
so the bare word "chain" — itself a skill, `Subject → M` — was a member of
eleven chains, always last, always wanting `Subject` from a member that
yielded `State` or `M_review`. Every one of those was a `✗` the chain's
author never wrote. The fix is a declaration: `members: [a, (b | c), d]` in
the chain's SOOP-2 block, with `(b | c)` marking a fan-out. The declaration
decides the shape; the types decide whether the shape is legal. A declared
sequence whose pair does not compose is `✗`; a declared fan-out whose
branches read different types is `✗`; a declared name that is not a skill
is `✗`. The prose scan stays for a chain that declares nothing, so the
oracle and the engine still agree on an undeclared corpus. (Gate P3.4.)

**2. Reviews went nowhere.** Seven skills produced `M_review` and nothing
consumed it, so every chain that reviewed something ended there and
`chain:diagnose-fix` — `explore → meta-review → pulse` — could not
typecheck because `pulse` reads `M_plan` and a review is not a plan. The
missing morphism is `M_review → M_plan`, built as `triage`: every finding
gets a disposition (fix, defer with a reason, reject with evidence,
already), the fixes get an order, and the result is a rail-shaped plan.
This is the fourth generator the corpus needed after 0002's three, and it
was found the same way: by asking which dead end had the most producers.

**3. Mid-chain skills were typed as if they always ran first.** `creative:forge`,
`vid:cinematic`, `evo-cubed:evolve`, `divergent-rail`, `sys:research`,
`truth-contracts` all declared `Subject` (or `M_arch`) as input, so nothing
upstream could feed them. They work on material. They now read `M`, and
composition honours refinement (`M_creative ⇒ M`, spec rule 2, previously
written and unchecked). The person's subject is lifted exactly once, by the
`chain` runner (`Subject → M`), which is the one lift the corpus has.
`meta-review` and `meta:amr` moved from `M^n` to `M` for the symmetric
reason: each reviews one artifact with internal lenses; the fan-in they were
typed for is `merge`'s stage, not theirs. `meta:hydra` reads `State` (it
builds on the world as it is) and iterates; `meta:omega` reads `M` and
iterates. Iteration is now declared (`iterates: true`) on the five skills
the hard-coded list named, and the list stays as the fallback. (Gate P3.5.)

## The words, after

```
1/4 ok   chain:adversarial-forge    ▸ ▸ ↺       evo-cubed:evolve ▸ meta:adversarial ▸ creative:forge
0/4 ok   chain:creative-engine      ▸ ▸         tie:gen ▸ creative:forge ▸ vid:cinematic
0/4 ok   chain:deep-research        ▸ ▸         explore ▸ sys:research ▸ meta:amr
1/4 ok   chain:diagnose-fix         ▸ ▸ ▸ *     explore ▸ meta-review ▸ triage ▸ pulse*
1/4 ok   chain:evolve-ship          ▸ ▸ *       evo-cubed:evolve ▸ divergent-rail ▸ pulse*
1/4 ok   chain:full-omega           ▸ ▸ *       meta:crucible ▸ meta:omega* ▸ meta:hydra*
4/4 ok   chain:generational         ▸ ⊗ ▸ ▸ ▸ ▸ ⊕ * ↺
0/4 ok   chain:genesis              ▸ ▸ ▸ ▸ ▸   explore ▸ evo ▸ forge ▸ rail ▸ meta-review ▸ triage
1/4 ok   chain:idea-to-code         ▸ ▸ *       creative:forge ▸ divergent-rail ▸ pulse*
2/4 ok   chain:leverage             ⊗ ▸ ▸ ▸ *   ⊗(memory-runtime, context-recovery-ops) ▸ truth-contracts ▸ rail ▸ pulse*
2/4 ok   chain:pact-pulse           ▸ ▸ * ↺     pulse* ▸ compact ▸ divergent-rail ↺
2/4 ok   chain:quality-gate         ⊗ ⊗ ↺       ⊗(meta-review, meta:amr, meta:adversarial)
2/4 ok   chain:sovereign-loop       ▸ ▸ ▸ ▸ * ↺ meta:hydra* ▸ compact ▸ meta:adversarial ▸ triage ▸ pulse* ↺
```

Twelve of thirteen are still degenerate, and that is now a true statement
about what they are rather than about how they were documented. Ten have
no fan-out, twelve do not select, eight do not repeat. The pipelines
(`creative-engine`, `deep-research`, `genesis`) are pipelines: Theorem 1
says a chain rooted at `Subject` cannot close, because `Subject` is what a
person brings. The loops close: `pact-pulse` and `sovereign-loop` return to
their own input through `compact` and `triage`, which did not exist in July
and September respectively.

## What this did not touch

- `M_creative` at 34 producers into 2 consumers is the largest pressure in
  the economy and was left alone. It is the "creative material nobody
  consumes" problem, and the consumers it needs (`creative:forge` now reads
  `M`, so by refinement it is a third) are a design question about what the
  34 generators are *for*, not a typing fix.
- The economy was still nominal here. Refinement was honoured in composition
  only; a consumer of `M` was not counted as fed by `M_*` producers in the
  pressure and never-fire numbers. Closed by P3.6, re-measured below.
- `Report` and `silent` remain dead ends on purpose and the analysis still
  lists them, because the engine has no notion of a sanctioned drain. Whether
  it should is a vocabulary question (SKILL_TYPES v1.1 says they are the
  partner of `Subject`); the engine reporting them keeps that decision
  visible rather than baked in.

## Re-measured with refinement (P3.6, 2026-11-05)

The economy now counts one free coercion `M_x ⇒ M` per produced subtype: one
more consumer of `M_x`, one more producer of `M`, an edge `M_x → M` in the
type graph, a `coerce:M_x` converter in the engine graph. The engine
(`economy_degrees`, `to_graph`, dynamics, never-fire) and the oracle
(`prompt_economy.py`) widened in the same change and still agree
(`matches_python`, `soop`); the category (dead ends, source-only, degrees)
stays nominal, so P3.1 diffs against `prompt_algebra.py` unchanged.
Measured with `pact check` on one machine and the oracle on another, same corpus, 162
skills:

| | nominal (above) | with refinement |
|---|---|---|
| `M_creative` pressure | 17 (34 → 2) | **11.33** (34 → 3) |
| `M_review` pressure | 8 (8 → 1) | **4** (8 → 2) |
| `M` producers → consumers | 51 → 36 (1.42) | 56 → 36 (1.56) |
| `M_plan` pressure | 0.35 (6 → 17) | 0.33 (6 → 18) |
| `M_arch` pressure | 2.5 (5 → 2) | 1.67 (5 → 3) |
| feedback loops | 19 | **36** |
| converters that can never fire | 0 | 0 |
| dead-end types | `Report`, `silent` | `Report`, `silent` |

What moved and what did not. The loop count nearly doubles because every
skill that reads `M` and emits a subtype now closes a two-step loop through
its coercion; those are real (the output is readable by the next `M`
reader), and they are the shortest kind. `M_creative` is still the largest
pressure by a wide margin: refinement adds one consumer (the coercion), not
the consumers the 34 generators need. The design question in the bullet
above stands, now measured against the economy composition actually uses.
