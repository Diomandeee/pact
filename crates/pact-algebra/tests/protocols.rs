//! P2.2 — a protocol as a validator over graphs.
//!
//! The gate: a hand-built graph that is not a rail is rejected, and the
//! rejection says which rule it broke.
//!
//! # The protocols here are declared, not implemented
//!
//! `rail` and `railpulse` are built in this file out of the [`Rule`]
//! vocabulary, exactly as a user would build theirs. That is the product
//! claim, and it is also what keeps INV-7's reasoning intact one layer up: a
//! `railpulse` shipped inside `pact-algebra` would be privileged over every
//! other protocol the platform is supposed to host. The real one lives in
//! `protocols/`, which unlocks at P2.3.
//!
//! # The graphs here are hand-built
//!
//! Not compiled from a [`Term`]. A word that typechecks produces a graph that
//! validates almost by construction, so validating one proves the compiler and
//! not the validator. What has to be checked is the case that actually
//! matters: a graph someone else built, wrong.

use pact_algebra::{Protocol, Rule, Term, MAX_TRACKS_PER_PHASE, MIN_VIABLE_TRANSITIONS};
use pact_core::{Bag, Distribution, Graph, Node, ResourceEdge, Signature, StateEdge, Type};

fn ty(name: &str) -> Type {
    Type::new(name)
}

/// `rail : M_arch → M_plan`. A distributor, and the invariants a phase carries.
fn rail() -> Protocol {
    Protocol::new("rail")
        .require(Rule::WellFormed)
        .require(Rule::Stage {
            role: "rail",
            reads: ty("M_arch"),
            emits: ty("M_plan"),
        })
        .require(Rule::Expands)
        .require(Rule::BoundedWidth {
            max: MAX_TRACKS_PER_PHASE,
        })
        .require(Rule::GatesBranch {
            min: MIN_VIABLE_TRANSITIONS,
        })
        .require(Rule::Connected)
}

/// `railpulse = ↺(⊗ ▸ * ▸ ⊕)`, as rules rather than as a word.
fn railpulse() -> Protocol {
    rail()
        .require(Rule::Executes)
        .require(Rule::Selects)
        .require(Rule::Repeats)
}

/// A rail: a distributor into a broadcast gate with two tracks.
fn a_rail() -> Graph {
    let mut g = Graph::new();
    g.add(Node::converter(
        "rail",
        Signature::simple("M_arch", "M_plan"),
    ))
    .add(Node::gate("fan", Distribution::Broadcast))
    .add(Node::converter("track_a", Signature::simple("M_plan", "M")))
    .add(Node::converter("track_b", Signature::simple("M_plan", "M")));
    g.connect(ResourceEdge::all("rail", "fan"))
        .connect(ResourceEdge::all("fan", "track_a"))
        .connect(ResourceEdge::all("fan", "track_b"));
    g
}

// ───────────────────────────────────────────────────────── the gate

#[test]
fn a_graph_that_is_not_a_rail_is_rejected_and_the_rule_is_named() {
    // A straight pipeline. It runs, it is well formed, and it is not a rail:
    // nothing fans out. That is the commonest real shape in a skill corpus and
    // the one most likely to be described as a rail in its own documentation.
    let mut pipeline = Graph::new();
    pipeline
        .add(Node::converter(
            "plan",
            Signature::simple("M_arch", "M_plan"),
        ))
        .add(Node::converter("do", Signature::simple("M_plan", "State")));
    pipeline.connect(ResourceEdge::all("plan", "do"));

    let broken = rail()
        .validate(&pipeline)
        .expect_err("a pipeline is not a rail");

    let named: Vec<&str> = broken.iter().map(|v| v.rule).collect();
    assert_eq!(
        named,
        vec!["expands"],
        "exactly the rule it breaks, and no others: {broken:?}"
    );

    // And the violation is actionable without opening the graph.
    let message = broken[0].to_string();
    assert!(
        message.contains("broadcast"),
        "the message must say what was required: {message}"
    );
    assert!(
        message.contains("0 gate(s)"),
        "and what was found instead: {message}"
    );
}

#[test]
fn a_rail_is_accepted() {
    // Without this, a validator that rejected everything would pass the gate.
    rail()
        .validate(&a_rail())
        .expect("a rail must validate against the rail protocol");
}

#[test]
fn every_rule_a_graph_breaks_is_reported_not_just_the_first() {
    // Fixing a graph one round-trip at a time is a guessing game. A graph that
    // is empty breaks almost everything, and the report has to say so at once.
    let broken = railpulse()
        .validate(&Graph::new())
        .expect_err("an empty graph is not a railpulse");

    let named: Vec<&str> = broken.iter().map(|v| v.rule).collect();
    for rule in ["stage", "expands", "executes", "selects", "repeats"] {
        assert!(named.contains(&rule), "{rule} unreported in {named:?}");
    }

    // `Connected` fires too, and it took a wrong assertion here to get its
    // message right. An empty graph has zero components, so it does break the
    // rule; the first version reported "found 0 component(s)", which is true
    // and helps nobody. Saying a graph with no nodes is connected would have
    // been technically defensible and practically misleading, which is worse.
    let parts = broken
        .iter()
        .find(|v| v.rule == "I3 connected")
        .expect("zero components is not one component");
    assert_eq!(parts.found, "the graph has no nodes");
}

// ───────────────────────────────────────────────────────── each rule

#[test]
fn a_missing_stage_names_the_signatures_that_are_present() {
    // "No such stage" sends someone to read every node. The signatures that
    // ARE there are usually enough to see which one was meant, which is the
    // difference between a report and a hint.
    // A rail in every respect except the type it reads.
    let mut wrong_types = Graph::new();
    wrong_types
        .add(Node::converter(
            "rail",
            Signature::simple("Subject", "M_plan"),
        ))
        .add(Node::gate("fan", Distribution::Broadcast))
        .add(Node::converter("a", Signature::simple("M_plan", "M")))
        .add(Node::converter("b", Signature::simple("M_plan", "M")));
    wrong_types
        .connect(ResourceEdge::all("rail", "fan"))
        .connect(ResourceEdge::all("fan", "a"))
        .connect(ResourceEdge::all("fan", "b"));

    let broken = rail().validate(&wrong_types).expect_err("no M_arch reader");
    let stage = broken
        .iter()
        .find(|v| v.rule == "stage")
        .expect("the stage rule must fire");

    assert!(stage.found.contains("Subject"), "{}", stage.found);
    assert!(stage.requires.contains("M_arch"), "{}", stage.requires);
}

#[test]
fn a_gate_wider_than_the_cap_is_rejected_and_the_width_is_named() {
    // I2 is a dimension cap: k tracks implies dim K <= k-1, and it is what
    // keeps a phase hand-checkable.
    let mut wide = Graph::new();
    wide.add(Node::converter(
        "rail",
        Signature::simple("M_arch", "M_plan"),
    ))
    .add(Node::gate("fan", Distribution::Broadcast));
    wide.connect(ResourceEdge::all("rail", "fan"));
    for i in 0..=MAX_TRACKS_PER_PHASE {
        let id = format!("track{i}");
        wide.add(Node::converter(
            id.clone(),
            Signature::simple("M_plan", "M"),
        ));
        wide.connect(ResourceEdge::all("fan", id));
    }

    let broken = rail().validate(&wide).expect_err("one track too many");
    let width = broken
        .iter()
        .find(|v| v.rule == "I2 bounded-width")
        .expect("the cap must fire");

    assert!(
        width
            .found
            .contains(&(MAX_TRACKS_PER_PHASE + 1).to_string()),
        "the actual width must be named: {}",
        width.found
    );
    assert_eq!(width.at, vec!["fan"], "and the gate must be named");
}

#[test]
fn a_gate_that_does_not_branch_is_rejected() {
    // I4. A gate with one output is not a gate, it is a relabelled edge, and a
    // phase built from them has nothing to select over.
    let mut narrow = Graph::new();
    narrow
        .add(Node::converter(
            "rail",
            Signature::simple("M_arch", "M_plan"),
        ))
        .add(Node::gate("fan", Distribution::Broadcast))
        .add(Node::converter("only", Signature::simple("M_plan", "M")));
    narrow
        .connect(ResourceEdge::all("rail", "fan"))
        .connect(ResourceEdge::all("fan", "only"));

    let broken = rail().validate(&narrow).expect_err("a one-way gate");
    let branch = broken
        .iter()
        .find(|v| v.rule == "I4 gates-branch")
        .expect("the floor must fire");
    assert!(branch.found.contains("fan has 1"), "{}", branch.found);
}

#[test]
fn a_phase_in_two_pieces_is_rejected_and_both_pieces_are_named() {
    // I3. Forcing is what makes edges exist, so a phase in two pieces is two
    // phases that have not noticed.
    let mut split = a_rail();
    split
        .add(Node::converter(
            "orphan",
            Signature::simple("M_arch", "M_plan"),
        ))
        .add(Node::pool("elsewhere"));
    split.connect(ResourceEdge::all("orphan", "elsewhere"));

    let broken = rail().validate(&split).expect_err("two components");
    let parts = broken
        .iter()
        .find(|v| v.rule == "I3 connected")
        .expect("connectivity must fire");

    assert!(parts.found.contains("2 component"), "{}", parts.found);
    assert_eq!(parts.at.len(), 2, "one representative per piece: {parts:?}");
}

#[test]
fn a_state_edge_is_enough_to_connect_a_phase() {
    // A state connection moves no resources and still couples what it touches.
    // A component count that ignored them would report a phase as scattered
    // precisely when its coupling is behavioural rather than material, which
    // is the case cross-layer forcing exists to describe.
    let mut coupled = a_rail();
    coupled.add(Node::pool("watcher"));

    assert!(
        rail().validate(&coupled).is_err(),
        "unattached, the watcher is a second component"
    );

    coupled.influence(StateEdge::inhibitor("watcher", "fan", 1));
    rail()
        .validate(&coupled)
        .expect("a state edge connects it just as a resource edge would");
}

#[test]
fn a_dangling_edge_is_a_violation_but_a_starved_converter_is_not() {
    // The distinction `WellFormed` draws. A starved converter is a diagnosis
    // about a system; a dangling edge means the graph does not describe
    // anything. A protocol that rejected the first would reject every fragment
    // with nothing attached to its ends, which is every fragment.
    let starved = a_rail();
    assert!(
        starved
            .validate()
            .iter()
            .any(|d| matches!(d, pact_core::Defect::StarvedConverter { .. })),
        "the fixture does have a starved converter"
    );
    rail()
        .validate(&starved)
        .expect("and that is not a protocol violation");

    let mut dangling = a_rail();
    dangling.connect(ResourceEdge::all("track_a", "nowhere"));
    let violations = rail().validate(&dangling).expect_err("an edge to nothing");
    assert!(
        violations.iter().any(|v| v.rule == "well-formed"),
        "{violations:?}"
    );
}

#[test]
fn iteration_and_repetition_are_different_claims() {
    // `*` says an effector repeats. `↺` says the whole phase does. If
    // `Repeats` accepted any cycle, an inner iteration would satisfy it and
    // the outer loop would never have to close, which is exactly the gap the
    // corpus has.
    let mut inner_only = a_rail();
    // Give track_a a self-loop through a delay: an iterating effector.
    inner_only.add(Node::delay("track_a_loop", 1));
    inner_only
        .connect(ResourceEdge::all("track_a", "track_a_loop"))
        .connect(ResourceEdge::all("track_a_loop", "track_a"));

    let executes = Rule::Executes.check(&inner_only);
    assert!(executes.is_none(), "the delay is on a cycle: {executes:?}");

    let repeats = Rule::Repeats
        .check(&inner_only)
        .expect("no cycle passes through the fan-out");
    assert!(
        repeats.found.contains("none on a cycle"),
        "{}",
        repeats.found
    );

    // Close the phase and both hold.
    inner_only.add(Node::converter(
        "compact",
        Signature::fan_in("M", 2, "M_plan"),
    ));
    inner_only
        .connect(ResourceEdge::all("track_a", "compact"))
        .connect(ResourceEdge::all("track_b", "compact"))
        .connect(ResourceEdge::all("compact", "fan"));

    assert!(Rule::Repeats.check(&inner_only).is_none());
    assert!(Rule::Selects.check(&inner_only).is_none());
}

#[test]
fn a_delay_off_a_cycle_is_a_buffer_not_an_iteration() {
    // The reason `Executes` asks where the delay is rather than whether one
    // exists. A delay that holds work once and passes it on is a buffer, and
    // reporting it as iteration would let a straight pipeline claim a stage it
    // does not have.
    let mut buffered = a_rail();
    buffered.add(Node::delay("wait", 2));
    buffered.connect(ResourceEdge::all("track_a", "wait"));

    let violation = Rule::Executes
        .check(&buffered)
        .expect("a delay off a cycle is not an iteration");
    assert!(
        violation.found.contains("none on a cycle"),
        "{}",
        violation.found
    );
    assert_eq!(violation.at, vec!["wait"]);
}

// ───────────────────────────────────────────────────────── protocol shape

#[test]
fn an_empty_protocol_accepts_everything_and_says_so() {
    // A real and useless protocol. Allowed rather than refused because "no
    // rules yet" is an honest state while authoring one, and a validator
    // reporting success on it is telling the truth.
    let nothing = Protocol::new("nothing");
    assert!(nothing.rules().is_empty());
    assert!(nothing.validate(&Graph::new()).is_ok());
}

#[test]
fn a_protocol_prints_its_own_rules() {
    // Protocols are data. Being able to read one back is what makes that
    // claim mean anything: an unreadable rule set is a trait implementation
    // wearing a struct's clothes.
    let printed = rail().to_string();
    assert!(printed.starts_with("protocol rail"));
    for rule in [
        "expands",
        "I2 bounded-width",
        "I4 gates-branch",
        "I3 connected",
    ] {
        assert!(printed.contains(rule), "{rule} missing from:\n{printed}");
    }
    assert_eq!(rail().rules().len(), 6);
    assert_eq!(railpulse().rules().len(), 9);
}

#[test]
fn the_caps_match_the_spec_file() {
    // Stated as a test as well as a canary, because the canary greps text and
    // this reads the value the code will actually use. Two ways of asking is
    // the point: a constant duplicated across a boundary drifts.
    assert_eq!(MAX_TRACKS_PER_PHASE, 4, "I2, spec/invariants.toml");
    assert_eq!(MIN_VIABLE_TRANSITIONS, 2, "I4, spec/invariants.toml");
}

// ───────────────────────────────────────────────────────── the two layers meet

#[test]
fn a_compiled_word_satisfies_the_protocol_its_shape_implies() {
    // The bridge between L1's two halves. A word built out of ⊗ compiles to a
    // graph with a broadcast gate, so a protocol requiring one accepts it.
    // This is the shape INV-4 (P2.3) will turn into a property over every
    // protocol; here it is one worked case, and it is enough to show the two
    // halves are talking about the same thing.
    let skill = |label: &str, from: &str, to: &str| {
        Term::atom(label, Signature::simple(from, to)).expect("well formed")
    };

    let fan = Term::broadcast(vec![
        skill("track_a", "M_plan", "M"),
        skill("track_b", "M_plan", "M"),
    ])
    .expect("both read M_plan");
    let word = Term::sequence(vec![skill("rail", "M_arch", "M_plan"), fan]).expect("composes");

    let built = word.compile();
    let shape = Protocol::new("has a fan-out")
        .require(Rule::WellFormed)
        .require(Rule::Expands)
        .require(Rule::GatesBranch {
            min: MIN_VIABLE_TRANSITIONS,
        })
        .require(Rule::BoundedWidth {
            max: MAX_TRACKS_PER_PHASE,
        })
        .require(Rule::Connected)
        .require(Rule::Stage {
            role: "rail",
            reads: ty("M_arch"),
            emits: ty("M_plan"),
        });

    shape
        .validate(&built.graph)
        .expect("a ⊗ word must satisfy a protocol requiring a fan-out");
}

#[test]
fn a_word_without_a_fan_out_fails_the_same_protocol() {
    // The other half. Without it the test above is satisfied by a protocol
    // that accepts anything a compiler produces.
    let skill = |label: &str, from: &str, to: &str| {
        Term::atom(label, Signature::simple(from, to)).expect("well formed")
    };
    let straight = Term::sequence(vec![
        skill("rail", "M_arch", "M_plan"),
        skill("pulse", "M_plan", "State"),
    ])
    .expect("composes")
    .compile();

    let broken = Protocol::new("has a fan-out")
        .require(Rule::Expands)
        .validate(&straight.graph)
        .expect_err("a straight word has no fan-out");
    assert_eq!(broken[0].rule, "expands");
}

#[test]
fn validation_ignores_what_a_graph_holds_and_reads_only_its_shape() {
    // Protocols are structural. If a rule depended on the contents of a pool,
    // a graph would validate or not depending on when it was asked, and a
    // check that changes answer mid-run is not a predicate.
    let mut empty_pools = a_rail();
    empty_pools.add(Node::pool("staging"));
    empty_pools.connect(ResourceEdge::all("track_b", "staging"));

    let mut full_pools = a_rail();
    full_pools.add(Node::pool_with("staging", Bag::of("M", 99)));
    full_pools.connect(ResourceEdge::all("track_b", "staging"));

    assert_eq!(
        rail().validate(&empty_pools).is_ok(),
        rail().validate(&full_pools).is_ok(),
        "the same shape must give the same verdict whatever it holds"
    );
}
