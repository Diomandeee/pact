//! P2.3 — INV-4, a generator satisfies its own validator.
//!
//! # The property, stated so it can fail
//!
//! For every protocol: `emit()` either **refuses** or produces a graph that
//! `validate()` accepts. Never a third outcome.
//!
//! The refusal half is the interesting one. A generator that only ever
//! succeeded would be satisfying the property by having no hard cases, and
//! rule sets do contradict each other: `Expands` with `GatesBranch { min: 3 }`
//! and `BoundedWidth { max: 1 }` requires a gate that branches at least three
//! ways and at most one. An honest generator says so instead of emitting
//! something that fails its own check.
//!
//! Note the `Expands`. Without it the same two width rules are satisfiable, by
//! a graph with no gates at all, where both hold vacuously. That distinction
//! was written the wrong way round here first and the code was right: refusing
//! the pair alone would have been the generator inventing a conflict that is
//! not there.
//!
//! # Why this is not circular
//!
//! It would be, if `emit` called `validate`. Emit-then-check makes INV-4 hold
//! by construction and reduces the property to "a filter filters", which is
//! true of every filter and interesting about none. So there is a structural
//! guard here as well as the property: `Protocol::emit` is grepped for any
//! mention of the validator.
//!
//! That guard is a check against the obvious version of the mistake, not a
//! proof — a future author could reach the validator through a helper. It is
//! cheap, it catches what would actually happen, and saying which of those two
//! it is costs one sentence.
//!
//! # Generated over rule sets, not over graphs
//!
//! Random graphs would test the validator. Random *rule sets* test the
//! generator against combinations nobody designed for, which is where the
//! interactions live: a width cap constrains what the fan-out rule may build,
//! and a repeat rule needs the fan-out to have somewhere to send work.

use pact_algebra::{Protocol, Rule, Unsatisfiable, MAX_TRACKS_PER_PHASE, MIN_VIABLE_TRANSITIONS};
use pact_core::Type;

const CASES: u64 = 1_000;

/// Deterministic rule-set generation. A failing case has to be reproducible
/// from its index alone.
struct Gen(u64);

impl Gen {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1))
    }

    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 11
    }

    fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next() % n
        }
    }

    fn chance(&mut self, one_in: u64) -> bool {
        self.below(one_in) == 0
    }

    /// An index or a small count. Narrowed in one place, which is the one
    /// place that can say why it cannot truncate: `below` returns strictly
    /// less than its argument, and every argument here is a small literal.
    fn small(&mut self, n: u64) -> usize {
        usize::try_from(self.below(n)).unwrap_or(0)
    }

    /// A protocol over a random subset of the vocabulary.
    ///
    /// Widths range past the spec caps on purpose, and both directions: a
    /// generator that only ever saw legal numbers would never meet the
    /// contradiction that makes the refusal half of the property real.
    fn protocol(&mut self, case: u64) -> Protocol {
        const TYPES: [&str; 4] = ["M", "M_plan", "State", "M_arch"];
        let mut p = Protocol::new(format!("generated-{case}"));

        for _ in 0..self.below(3) {
            let reads = TYPES[self.small(4)];
            let emits = TYPES[self.small(4)];
            p = p.require(Rule::Stage {
                role: "generated",
                reads: Type::new(reads),
                emits: Type::new(emits),
            });
        }
        if self.chance(2) {
            p = p.require(Rule::Expands);
        }
        if self.chance(2) {
            p = p.require(Rule::Selects);
        }
        if self.chance(2) {
            p = p.require(Rule::Executes);
        }
        if self.chance(3) {
            p = p.require(Rule::Repeats);
        }
        if self.chance(2) {
            p = p.require(Rule::BoundedWidth { max: self.small(6) });
        }
        if self.chance(2) {
            p = p.require(Rule::GatesBranch { min: self.small(6) });
        }
        if self.chance(2) {
            p = p.require(Rule::Connected);
        }
        if self.chance(2) {
            p = p.require(Rule::WellFormed);
        }
        p
    }
}

// ─────────────────────────────────────────────────────────── the property

#[test]
fn every_protocol_emits_something_it_accepts_or_refuses_to_emit() {
    let mut failures = Vec::new();
    let mut emitted = 0u64;
    let mut refused = 0u64;

    for case in 0..CASES {
        let mut gen = Gen::new(case);
        let protocol = gen.protocol(case);

        match protocol.emit() {
            Ok(graph) => {
                emitted += 1;
                if let Err(broken) = protocol.validate(&graph) {
                    // Collect rather than panic on the first, so one run names
                    // every broken combination instead of one at a time.
                    failures.push(format!(
                        "case {case}: emitted a graph its own protocol rejects\n  {}\n  {}",
                        protocol
                            .rules()
                            .iter()
                            .map(Rule::id)
                            .collect::<Vec<_>>()
                            .join(", "),
                        broken
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join("\n  ")
                    ));
                }
            }
            Err(_) => refused += 1,
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {CASES} case(s) broke INV-4:\n{}",
        failures.len(),
        failures
            .iter()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );

    // Non-vacuity, both ways. A generator that refused everything would
    // satisfy the property and be useless; one that never refused would mean
    // the corpus of cases never contained a contradiction, and the refusal
    // half would be untested.
    assert!(
        emitted > CASES / 2,
        "only {emitted} of {CASES} emitted: the generator is refusing too much"
    );
    assert!(
        refused > 0,
        "no case was contradictory: the refusal half of INV-4 went untested"
    );
}

#[test]
fn the_generator_never_consults_the_validator() {
    // The structural guard. Emit-then-check would make the property above hold
    // by construction and reduce it to "a filter filters".
    let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/protocol.rs"))
        .expect("protocol.rs must be readable to check its own shape");

    let start = source
        .find("pub fn emit(&self)")
        .expect("emit must exist to be checked");
    let open = source[start..].find('{').expect("emit must have a body") + start;

    let mut depth = 0usize;
    let mut end = open;
    for (offset, ch) in source[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = open + offset;
                    break;
                }
            }
            _ => {}
        }
    }
    let body = &source[open..=end];

    for forbidden in ["validate", ".check(", "Violation"] {
        assert!(
            !body.contains(forbidden),
            "`{forbidden}` appears in emit's body, which makes INV-4 circular:\n{body}"
        );
    }
}

// ─────────────────────────────────────────────────────────── refusals

#[test]
fn contradictory_widths_are_only_contradictory_once_a_gate_is_required() {
    // This test was written the other way round first, and the code was right.
    //
    // `GatesBranch { min: 3 }` says every gate branches at least three ways.
    // `BoundedWidth { max: 1 }` says none branches more than once. Together
    // they describe an impossible gate — and a graph with NO gates satisfies
    // both, vacuously. So the pair alone is satisfiable, and refusing it would
    // have been the generator inventing a conflict that is not there.
    let vacuous = Protocol::new("no gates at all")
        .require(Rule::GatesBranch { min: 3 })
        .require(Rule::BoundedWidth { max: 1 });
    let witness = vacuous
        .emit()
        .expect("a graph with no gates breaks neither rule");
    vacuous
        .validate(&witness)
        .expect("and both rules hold over no gates");

    // Add anything that forces a gate to exist and the contradiction bites.
    let impossible = vacuous.require(Rule::Expands);
    let broken = impossible
        .emit()
        .expect_err("now a gate must exist, and no gate can be both");

    assert_eq!(broken, Unsatisfiable::Width { min: 3, max: 1 });
    let message = broken.to_string();
    assert!(message.contains('3') && message.contains('1'), "{message}");
}

#[test]
fn a_cycle_through_a_gate_that_cannot_branch_is_refused() {
    // The contradiction that is not visible from the two width numbers alone.
    // `Repeats` needs the fan-out to have somewhere to send work, and a gate
    // with no outgoing edge is on no cycle.
    let broken = Protocol::new("closed loop")
        .require(Rule::Repeats)
        .require(Rule::BoundedWidth { max: 0 })
        .emit()
        .expect_err("a cycle needs at least one branch");

    assert_eq!(broken, Unsatisfiable::ClosedFanOut { max: 0 });
    assert!(broken.to_string().contains("cycle"), "{broken}");
}

#[test]
fn a_zero_width_cap_alone_is_satisfiable() {
    // Without this, the refusal above would be satisfied by refusing every
    // `BoundedWidth { max: 0 }`. A fan-out wired to nothing is a legal answer
    // to `Expands`, which only asks that a broadcast gate exist.
    let graph = Protocol::new("wide as nothing")
        .require(Rule::Expands)
        .require(Rule::BoundedWidth { max: 0 })
        .emit()
        .expect("a gate with no branches still exists");

    Protocol::new("wide as nothing")
        .require(Rule::Expands)
        .require(Rule::BoundedWidth { max: 0 })
        .validate(&graph)
        .expect("and it satisfies the rule that asked for it");
}

// ─────────────────────────────────────────────────────────── witnesses

#[test]
fn the_witness_for_railpulse_is_a_railpulse() {
    // The worked case behind the property. Every rule of the canonical cycle
    // at once, which is where the interactions are: the width cap constrains
    // the fan-out, and the repeat rule needs that fan-out to be on a cycle.
    let railpulse = Protocol::new("railpulse")
        .require(Rule::WellFormed)
        .require(Rule::Stage {
            role: "rail",
            reads: Type::new("M_arch"),
            emits: Type::new("M_plan"),
        })
        .require(Rule::Expands)
        .require(Rule::Executes)
        .require(Rule::Selects)
        .require(Rule::Repeats)
        .require(Rule::BoundedWidth {
            max: MAX_TRACKS_PER_PHASE,
        })
        .require(Rule::GatesBranch {
            min: MIN_VIABLE_TRANSITIONS,
        })
        .require(Rule::Connected);

    let witness = railpulse
        .emit()
        .expect("the canonical cycle is satisfiable");
    railpulse
        .validate(&witness)
        .expect("and its witness satisfies it");

    // And the witness is a graph someone could read: it has the fan-out the
    // rules asked for, at the width they asked for.
    let gates: Vec<_> = witness
        .nodes()
        .iter()
        .filter(|n| matches!(n.kind, pact_core::Kind::Gate { .. }))
        .collect();
    assert_eq!(gates.len(), 1, "one fan-out");
    assert_eq!(
        witness.outgoing(&gates[0].id).len(),
        MIN_VIABLE_TRANSITIONS,
        "as narrow as the floor allows, and no wider"
    );
}

#[test]
fn an_empty_protocol_emits_a_graph_it_accepts() {
    // The degenerate end. An empty protocol accepts everything, so anything
    // would do; emitting nothing at all would fail `Connected` the moment
    // someone added it, so the witness is one node rather than zero.
    let nothing = Protocol::new("nothing");
    let witness = nothing.emit().expect("nothing to contradict");
    nothing.validate(&witness).expect("and nothing to break");
    assert_eq!(witness.nodes().len(), 1, "a hub and no more");

    // Adding only `Connected` must still work, which it would not if the
    // witness were empty.
    let connected = Protocol::new("connected").require(Rule::Connected);
    let witness = connected.emit().expect("satisfiable");
    connected
        .validate(&witness)
        .expect("one node is one component");
}

#[test]
fn declaration_order_does_not_change_the_verdict() {
    // A protocol whose meaning depends on the order its rules were written is
    // not a predicate. Two `BoundedWidth` rules take the strictest, not the
    // last, and the same for the floor.
    let strict_first = Protocol::new("a")
        .require(Rule::BoundedWidth { max: 2 })
        .require(Rule::BoundedWidth { max: 5 })
        .require(Rule::Expands);
    let strict_last = Protocol::new("b")
        .require(Rule::Expands)
        .require(Rule::BoundedWidth { max: 5 })
        .require(Rule::BoundedWidth { max: 2 });

    let first = strict_first.emit().expect("satisfiable");
    let last = strict_last.emit().expect("satisfiable");

    let width = |g: &pact_core::Graph| {
        g.nodes()
            .iter()
            .filter(|n| matches!(n.kind, pact_core::Kind::Gate { .. }))
            .map(|n| g.outgoing(&n.id).len())
            .max()
            .unwrap_or(0)
    };
    assert_eq!(width(&first), width(&last));
    strict_first.validate(&first).expect("valid either way");
    strict_last.validate(&last).expect("valid either way");
}

#[test]
fn a_witness_is_labelled_as_one() {
    // Provenance, so a witness is never mistaken for someone's real system in
    // a report. Every node it invents says where it came from.
    let witness = Protocol::new("labelled")
        .require(Rule::Expands)
        .require(Rule::Selects)
        .require(Rule::Executes)
        .emit()
        .expect("satisfiable");

    for node in witness.nodes() {
        assert!(
            node.meta.contains_key("witness"),
            "{} is unlabelled",
            node.id
        );
    }
}
