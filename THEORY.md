# A working theory

What PACT is a claim *about*, whether that claim is defensible yet, and what
would settle it. Written 2026-07-31 against the corpus as it stands: 150
skills, 12 types, 13 chains.

This is a living document and it is allowed to be wrong. `SPEC.md` states what
the system proves. This states what the system is *for*, which is a different
and less certain thing.

---

## Abstract, first draft

> Practitioners who work with large language models accumulate prompt
> libraries: dozens to hundreds of natural-language instructions, written over
> months, composed into chains by hand. These are software artifacts in every
> respect except that nothing checks them. A chain whose steps cannot compose
> fails in prose, or worse, appears to succeed.
>
> We present PACT, which reads a prompt library as a typed resource-flow
> network. Each prompt is a converter with declared input and output types,
> each type is a pool, and each chain is a word in a six-operator algebra.
> Over that structure the system computes predicates that need no model:
> which prompts can never fire, which types are produced and never consumed,
> which chains fail to typecheck, and which return to their own precondition.
>
> Applied to a 150-prompt library accumulated over six months by one
> practitioner, the analysis found nine prompts that could never fire, three
> types entering the economy with no producer, and twelve of thirteen chains
> that ran once and stopped for want of a morphism back to their own start.
> Acting on the findings took never-fires to zero, closed the first chain, and
> raised feedback loops from five to twelve. Ten of thirteen chains still fail
> to typecheck end to end, and one type is produced by 33 prompts and consumed
> by 2.
>
> The architectural claim is a division of labour: a language model supplies
> the type assignment, which cannot be verified, and a deterministic engine
> checks composition, which can. A model may hallucinate that a prompt emits a
> given type; it cannot hallucinate that the result composes. We argue this
> makes structural analysis a cheap and sound filter over a class of model
> errors, characterise exactly which errors it cannot catch, and give a
> permutation test that measures how much of the filter's power comes from the
> type vocabulary rather than from the corpus.

Everything in that abstract is measured except the last paragraph, which is
the thesis and is currently unmeasured. That gap is the whole state of play.

---

## 1. The object of study

The interesting move is not the formalism. It is what the formalism is pointed
at.

Workflow analysis normally takes a **designed** artifact: a business process
someone drew, a pipeline someone specified. A prompt library is not designed.
It **accrues**. Skills get written when a need appears, chains get assembled
from whatever was nearby, and nothing forces last month's vocabulary to agree
with this month's. After six months it is a codebase nobody refactored and
nobody compiled.

That makes it a good subject for the same reason legacy code is: the defects
are real, unintentional, and unknown to the author. The corpus here had nine
prompts that could not fire under any input. Their author did not know.

The second reason it is a good subject is that the field has no tooling for
it. Prompt libraries are managed as folders. There is no linter, no dead-code
pass, no dependency graph, no notion of a build that fails.

---

## 2. What is being formalised

A corpus becomes a graph:

| corpus | graph |
|---|---|
| type (`M_plan`, `State`, `Subject`) | pool |
| skill | converter, with a signature |
| chain | word over six operators |
| a type nothing produces | source |
| a type nothing consumes | drain, or a dead end |

Six operators: `▸` sequence, `⊗` broadcast, `⊕` merge, `↺` feedback, `+`
choice, `*` iterate. The canonical shape of a working chain in this corpus is

```
railpulse = ↺( ⊗ ▸ * ▸ ⊕ )
```

expand, execute, select, and return. A chain is scored `n/4` on how many of
those four stages it has. Twelve of thirteen chains were missing at least one,
and eleven were missing the return.

The predicates the engine computes, none of which need a model:

- **liveness** — can this converter ever fire? (van der Aalst's soundness for
  workflow nets, in different clothes)
- **boundary** — which types enter with no producer, which leave with no
  consumer
- **pressure** — producers-to-consumers ratio per type, as a measure of where
  work accumulates
- **composability** — does each consecutive pair of a chain typecheck
- **homology over GF(2)** — `b₀` components, `b₁` non-bounding cycles. `b₁ → 0`
  is what hardening looks like
- **conservation** — nothing is created or destroyed except at sources and
  drains, checked per type

---

## 3. The central claim

State it precisely, because its value is entirely in the precision.

> **Let a model assign types to untyped prompts. The assignment is judgment
> and cannot be verified. Composition over the assignment is a predicate and
> can be. A model can hallucinate that a prompt emits `M_plan`; it cannot
> hallucinate that the result composes with what comes next.**

The consequence that makes it worth writing down: **inference accuracy can be
mediocre and the system still useful**, because wrong inferences do not fail
silently. They surface as dead converters, unreachable types, and chains that
stop typechecking. The symbolic layer is not validating content. It is
validating *composition*, which is the only thing it can validate soundly.

This is a neurosymbolic architecture with an unusually modest symbolic half,
and the modesty is the point. Most neurosymbolic systems ask the symbolic
layer to check whether the model is *right*. This one asks only whether the
model is *consistent*, which is decidable.

### The limitation, stated up front

The filter catches an inference error **only when the error produces a type
mismatch**. An error that yields a plausible and composable typing passes
silently. So the filter's power is a property of the *vocabulary*, not of the
engine: if every prompt in a corpus is `Subject → State`, nothing is ever
caught, and the analysis is theatre.

This is the sharpest thing in the theory and it cuts both ways. It gives a
falsifiable quantity: **how discriminating is a type vocabulary?** And it says
plainly that a corpus can be typed in a way that makes PACT useless.

---

## 4. Prior art, honestly

Every component is borrowed. The composition and the object are what is new.

| this | exists as |
|---|---|
| resource-flow economies with pools, converters, gates | **Machinations** (Dormans, 2012), for game economy design |
| liveness, "can this task ever fire" | **workflow-net soundness** (van der Aalst, 1997) |
| the underlying execution semantics | Petri nets, place/transition |
| typed input/output on a prompt | **DSPy signatures** — the closest prior art by far |
| composition of prompts into pipelines | LangChain, DSPy, AutoGen, and every agent framework |
| dead code, unreachable symbols, dependency cycles | ordinary static analysis |
| `b₀`/`b₁` over a network | standard TDA |

**Where it differs from DSPy, which is the comparison a reviewer reaches for
first.** DSPy types a module so it can be *compiled and optimised*. The type
is machinery for one module's execution. PACT types a prompt so the *library*
can be analysed as a network, and never executes anything. Nobody, as far as
I can find, treats an accumulated prompt library as an artifact with emergent
structure and runs corpus-level static analysis over it.

**Where it differs from workflow nets.** Soundness analysis assumes a designed
process with a defined start and end. A prompt corpus has neither. The
questions differ accordingly: not "does this process terminate correctly" but
"what did six months of accretion actually build, and does it close."

An honest framing of the contribution is therefore *systems and empirical*,
not *theoretical*. There is no new theorem here. There is a new place to point
old theorems, and a measurement of what happens when you do.

---

## 5. Evidence so far

**One corpus. One author. Six months of accretion** (first commit 2026-01-31),
150 typed skills, 12 types, 13 chains.

Before and after acting on the analysis:

| | before | now |
|---|---|---|
| prompts that can never fire | **9** | **0** |
| types entering with no producer | `M^n`, `M_code`, `Subject` | `Subject` only |
| feedback loops | 5 | **12** |
| chains at 4/4 | 0 of 12 | **1** of 13 |

What the corpus still shows, and these are the interesting ones because
nobody has fixed them yet:

```
dead ends            M_analytical, M_review, Report, silent
pressure             M_creative   16.5   33 producers into 2 consumers
                     State         3.69  48 producers into 13 consumers
chains               10 of 13 do not typecheck end to end
missing stages       ↺ repeat 11 · ⊕ select 7 · * execute 5 · ⊗ expand 3
```

`M_creative` at 16.5 is the finding I would lead a paper with. Thirty-three
prompts produce creative output and two consume it. That is a library that
generates far more than it can select from, and it is invisible without the
network view.

**The tier split**, which is the thesis in miniature: of 189 questions the
system can be asked about this corpus, **176 are answered from structure alone
and 13 escalate to a model** (93%). The 13 all have the same reason recorded
against them: *quality is not a property of a signature.*

### What that evidence is not

It is a single-subject case study by the tool's author on the tool's author's
data, with no control, no second corpus, and no model in the loop yet. Every
number above is a description of one artifact. None of it generalises, and it
would be dishonest to present it as if it did.

---

## 6. The experiments that would settle it

Ranked by how much they buy per unit of effort.

### E1 — the permutation test. Cheap, decisive, and runnable today.

Randomly permute the type assignments among the 150 skills and recompute the
analysis. Repeat a few thousand times to get a null distribution for: chains
that typecheck, dead converters, `b₁`, and pressure.

**If the real corpus is a sharp outlier, the typing carries information and
composability is a filter with teeth. If it is not, §3's limitation has
swallowed the thesis** and the honest conclusion is that this vocabulary is
too coarse to catch anything.

It needs no model, no new theory, and only existing code. It produces a
p-value. **A reviewer will ask for this first, and so should we.** It is also
the one experiment that can falsify the project cheaply, which is exactly why
it should run before more is built on top.

### E2 — the filter's precision and recall over model errors. The central table.

Once P8.2 and P8.3 land, we have model inferences and ground truth on the same
150 skills. Then partition the wrong inferences:

|  | flagged by the engine | passed silently |
|---|---|---|
| **inference wrong** | caught | **the dangerous cell** |
| **inference right** | false alarm | correct |

The claim in §3 predicts the top-left is large and the bottom-right small. The
size of the top-right cell is the honest limit of the whole approach, and
publishing it is what makes the paper credible rather than promotional.

### E3 — N > 1.

Run on other people's prompt libraries. Public collections exist. Two or three
foreign corpora turn a case study into a finding, and would answer the
question the case study cannot: **do these defects occur in libraries their
author did not write with this analysis in mind?**

### E4 — does the analysis change what gets built?

Longitudinal, and the hardest. The corpus already changed once in response to
its own analysis (§5), which is suggestive and is also the author responding
to his own tool. A weaker but real version: measure whether chains authored
after the analysis score higher at 4/4 than chains authored before.

### E5 — the executor.

Still the largest unproven claim in the project, and deliberately out of scope
here.

---

## 7. Threats to validity

- **The author is the subject.** The corpus was typed by the person who wrote
  the type system. E3 is the only real answer.
- **The vocabulary might be doing the work, or might be doing none.** E1
  settles it.
- **Members are extracted from prose.** A chain's word depends partly on how
  its documentation is written, because the importer finds members by scanning
  for known skill names. This is faithfully reproduced from the Python oracle
  and is a real threat to any chain-level number.
- **Hand-typing is the ground truth, and hand-typing is fallible.** The
  accuracy measurement in P8.3 measures agreement with a human, not
  correctness. `merge` was typed wrong by its own author for weeks.
- **`n/4` completeness encodes one opinion** about what a good chain looks
  like. It is defensible and it is not derived from anything.
- **Selection effect.** A corpus that had no structure would not have prompted
  its author to build this, so the one corpus we have is the one most likely
  to look structured.

---
