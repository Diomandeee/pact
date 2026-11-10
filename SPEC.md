# The mathematics of railpulse

**`rail ∥ pulse`** — a generational loop where planning expands, execution
fills, and compression selects. The output is a barcode: the set of structures
that survived being compressed.

Named from Mohamed's own notation, `divergent-rail │ pulse`. The bar is the
composition operator, and it is the whole idea.

**railpulse is one protocol. PACT is the platform that hosts it.** This
document is the math of that one protocol, and it is deliberately not the
platform's spec: the whole point of PACT is that other protocols can be
authored against the same engine, and a document that treats railpulse as the
subject would quietly privilege it. INV-7 enforces the same separation in
code — `pact-core` names no protocol concept, and railpulse lives in
`protocols/`.

Version 0.1.0. Sections 1 through 3 are the contract. Section 4 says where it
is implemented and how far along that is.

---

## 1. Definitions

**D1 (Complex).** A *phase* is the nerve of a cover. Vertices are tracks. A set
of tracks spans a simplex when they share a named artifact. The complex `K` is
closed downward: if three tracks jointly resolved, any two of them did.

**D2 (Rail).** `rail : M_arch → M_plan`. A Distributor. It produces `K` with
simplices **asserted**, not filled.

**D3 (Pulse).** `pulse : M_plan → State`. An Effector. An iteration attaches an
`n`-cell along its boundary; the boundary is the set of already-completed
prerequisite iterations. Signals: `CONTINUE` (attached), `COMPLETE` (the cycle
bounds), `BLOCKED` (see P2).

**D4 (Compaction).** `compact : State → M_arch`. A simplicial map `K_n → K_{n+1}`.
It may collapse vertices. It may not create a face absent from the source.
**This morphism does not exist in the current skill corpus. It is generator #1
to build.**

**D5 (Barcode).** The generations form a persistence module
`K_1 → K_2 → K_3 → …`. A *bar* is a homology class with a birth and a death
generation. The barcode is the system's output; the complexes are scaffolding.

---

## 2. The theorem

**Theorem 1 (Import Necessity).**
*A railpulse loop with no external vertex injection halts in finitely many
generations.*

**Proof sketch.** Invariant 2 caps a phase at `k = 4` tracks, so its complex has
at most `2^k − 1 = 15` non-empty faces; the configuration space of one phase is
finite. Invariant 1 requires the induced map on homology
`H_*(K_n) → H_*(K_{n+1})` to be non-isomorphic at every generation. A strictly
changing sequence in a finite space cannot be infinite. Therefore some
generation `N` admits no legal successor, and the loop halts. ∎

**Corollary.** External input is not an optimization. It is the only source of
new vertices, and therefore the only thing that extends the loop's life. Any
implementation must make external injection a first-class operation, not an
escape hatch.

*Status: this is the one result here with real mathematical content. What
follows are definitions and their immediate consequences, which do operational
work but are not theorems. Labelling them as such would not survive scrutiny,
and this document is meant to survive scrutiny.*

---

## 3. Propositions

**P1 (Filling).** A rail asserts simplices; only pulse fills them. Therefore
`compact ∘ rail` selects over unfilled faces, and every resulting bar is
vacuous. **Pulse is necessary between rail and compaction for the barcode to
mean anything.**

**P2 (Blocked).** `BLOCKED` ⟺ the attaching map's image is not contained in the
existing skeleton. Consequence: retrying a blocked iteration cannot succeed.
Only building the boundary can. Blocked and failed are different kinds.

**P3 (Fabrication).** A face in `K_{n+1}` with no preimage in `K_n` was invented
by the compaction. This is checkable, and it is the only structural detector of
a hallucinated connection.

**P4 (Hardening).** A phase is hard when its non-bounding cycles bound, i.e.
when the filling artifacts exist. `b₁ → 0` is the completion criterion, not a
checked task list.

**P5 (Invariants as topology).**
`I1` → the induced map is not an isomorphism.
`I2` → `dim K ≤ k − 1`.
`I3` → `b₀ = 1`; forcing is what makes edges exist.
`I4` → no sinks in the phase graph.

**P6 (Determinism boundary).** The predicates are deterministic. The effects
are not, and never will be, because an iteration is a model doing work. All
contract enforcement lives on the predicate side.

---

## 4. Where this is implemented

The engine is `pact-core`, and it knows nothing about any of the above. It
knows pools, sources, drains, converters, gates, delays, and two kinds of
connection. `rail`, `pulse`, `compact` and `merge` are words in a composition
algebra one layer up, and railpulse itself is a word in that algebra, defined
in `protocols/`.

```
L4  crates/pact-cli        CLI, diagram export
L3  crates/pact-corpus     SOOP-2 frontmatter and chains -> Graph
L2  crates/pact-analysis   statics, dynamics, homology, persistence
L1  crates/pact-algebra    the six combinators, protocols
L0  crates/pact-core       typed tokens, nodes, two edge kinds, simulation
```

Dependencies point one way, `L4 → L3 → L2 → L1 → L0`, and **`pact-core`
depends on nothing.** Not on serde, not on a logging crate, not on a clock.
INV-1 has a canary and it runs in CI.

`spec/invariants.toml` carries the locked constants and its own
`spec_version`. A change to `k`, to the invariant set, or to the signal
semantics is a spec bump, not a patch release. A barcode is only comparable
across generations that share a spec version.

### The execution plan is not here

It lives in `.governance/0.3_LIVING_CHECKLIST.md`, with a Gate on every item
and a tick only when that Gate's command passed. It used to be duplicated in
this file, and the copy here had already drifted: it named five crates, two of
which (`-economy`, `-harness`) were folded into `-analysis` and the harness
layer before a line was written.

Two copies of a plan is how one of them goes stale, and the stale one is
always the one someone reads.

Current state is whatever these print:

```bash
./scripts/pz status         what is open, what is next, what is far and why
./scripts/canaries.sh       do the invariants still hold
```

---

## 5. What this does not claim

Type agreement is necessary for a chain to work and nowhere near sufficient.
Two skills can compose perfectly and produce nonsense. PACT finds the chains
that **cannot** work, which is a smaller and far more useful set than the ones
that might.

It says nothing about quality. Determinism buys the predicates and never the
judgment. That gap is permanent, and it is why the barcode, not the harness, is
the output.
