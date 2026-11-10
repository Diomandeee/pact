//! P1.2 — the node vocabulary and both edge kinds.
//!
//! This gate deliberately does not touch `Graph`. Its first draft did, which
//! was circular: `graph.rs` unlocks after P1.2. Validation belongs to P1.3.
//!
//! What is checked here is that the vocabulary means what it says. A `Kind`
//! whose constructor quietly disagrees with its documented default is the
//! kind of defect that surfaces four layers up as an economy that does not
//! balance, and by then nobody looks here.

use pact_core::{Activation, Bag, Demand, Distribution, Kind, Node, Rate, Signature, Type};
use pact_core::{Effect, ResourceEdge, StateEdge};

#[test]
fn every_kind_is_constructible_with_its_declared_defaults() {
    // The defaults are load-bearing, not cosmetic. A converter defaults to
    // Demand::All because half-run is worse than not-run; a drain defaults to
    // Any because a drain that refuses partial input stops being a drain.
    let pool = Node::pool("p");
    assert!(matches!(pool.kind, Kind::Pool { .. }));
    assert_eq!(
        pool.activation,
        Activation::Passive,
        "a pool must not act on its own"
    );
    assert!(pool.is_storage());

    let source = Node::source("s", Bag::of("M", 2));
    assert!(matches!(source.kind, Kind::Source { .. }));
    assert_eq!(source.activation, Activation::Automatic);
    assert!(!source.is_storage(), "a source holds nothing");

    let drain = Node::drain("d");
    assert!(matches!(drain.kind, Kind::Drain));
    assert_eq!(
        drain.demand,
        Demand::Any,
        "a drain must take what it can get"
    );

    let converter = Node::converter("c", Signature::simple("M_plan", "State"));
    assert!(matches!(converter.kind, Kind::Converter { .. }));
    assert_eq!(
        converter.demand,
        Demand::All,
        "a converter defaults to all-or-nothing: half-run is worse than not-run"
    );

    let gate = Node::gate("g", Distribution::Broadcast);
    assert!(matches!(gate.kind, Kind::Gate { .. }));
    assert_eq!(gate.demand, Demand::Any);
    assert!(!gate.is_storage(), "a gate must not accumulate");

    let delay = Node::delay("w", 3);
    assert!(matches!(delay.kind, Kind::Delay { steps: 3 }));
    assert!(delay.is_storage(), "work in flight is held somewhere");
}

#[test]
fn only_pools_and_delays_store() {
    // The conservation ledger sums storage. If a non-storage node could hold
    // resources, the sum would miss them and INV-2 would report a leak that
    // is really an accounting hole.
    let storing = [Node::pool("p"), Node::delay("w", 1)];
    let passing = [
        Node::source("s", Bag::of("M", 1)),
        Node::drain("d"),
        Node::converter("c", Signature::simple("M", "State")),
        Node::gate("g", Distribution::Deterministic),
    ];
    assert!(storing.iter().all(Node::is_storage));
    assert!(passing.iter().all(|n| !n.is_storage()));
}

#[test]
fn signature_lossiness_agrees_with_totals() {
    let lossy = Signature::fan_in("M", 3, "M_review");
    assert!(lossy.is_lossy(), "3 in and 1 out is lossy");
    assert_eq!(lossy.consumes.total(), 3);
    assert_eq!(lossy.produces.total(), 1);

    let neutral = Signature::simple("M_plan", "State");
    assert!(!neutral.is_lossy(), "1 in and 1 out is not lossy");

    // A signature that produces more than it eats is legal. Broadcast does
    // exactly this, and calling it lossy would invert the meaning.
    let generative = Signature {
        consumes: Bag::of("M", 1),
        produces: {
            let mut b = Bag::of("M", 1);
            b.add(Type::new("M_creative"), 1);
            b
        },
    };
    assert!(!generative.is_lossy());
}

#[test]
fn converter_appetite_is_its_consumed_bag() {
    let converter = Node::converter("c", Signature::fan_in("M", 4, "M_review"));
    let appetite = converter.appetite().expect("a converter has an appetite");
    assert_eq!(appetite.count(&Type::new("M")), 4);

    // Nothing else does. Asking a pool what it eats is a category error, and
    // returning zero instead of None would let a caller treat it as a
    // converter that happens to want nothing.
    assert!(Node::pool("p").appetite().is_none());
    assert!(Node::drain("d").appetite().is_none());
    assert!(Node::gate("g", Distribution::Broadcast)
        .appetite()
        .is_none());
}

#[test]
fn rate_never_exceeds_availability_and_appetite_ignores_it() {
    let fixed = Rate::Fixed(5);
    assert_eq!(fixed.resolve(10), 5, "fixed takes its share");
    assert_eq!(fixed.resolve(3), 3, "fixed cannot take what is not there");
    assert_eq!(fixed.resolve(0), 0);
    // Appetite is what a node WANTS, independent of what exists. Demand::All
    // needs that distinction to decide whether it may fire at all.
    assert_eq!(fixed.appetite(), 5, "appetite ignores availability");

    let all = Rate::All;
    assert_eq!(all.resolve(7), 7);
    assert_eq!(all.resolve(0), 0);
    assert_eq!(all.appetite(), 1, "All wants at least something");

    for available in 0..64u64 {
        for n in 0..8u64 {
            assert!(
                Rate::Fixed(n).resolve(available) <= available,
                "resolve exceeded availability: rate {n}, available {available}"
            );
        }
        assert!(Rate::All.resolve(available) <= available);
    }
}

#[test]
fn builders_are_chainable_without_losing_earlier_settings() {
    let node = Node::pool("p")
        .with_activation(Activation::Triggered)
        .with_demand(Demand::Any)
        .with_meta("track", "db")
        .with_meta("role", "convergence");

    assert_eq!(node.activation, Activation::Triggered);
    assert_eq!(node.demand, Demand::Any);
    assert_eq!(node.meta.get("track").map(String::as_str), Some("db"));
    assert_eq!(
        node.meta.get("role").map(String::as_str),
        Some("convergence")
    );
    assert_eq!(node.id, "p", "chaining must not disturb identity");
}

#[test]
fn resource_edges_carry_their_type() {
    let edge = ResourceEdge::new("a", "b", 2).carrying("M_plan");
    assert_eq!(edge.from, "a");
    assert_eq!(edge.to, "b");
    assert_eq!(edge.rate, Rate::Fixed(2));
    assert_eq!(edge.label.as_deref(), Some("M_plan"));

    let sweep = ResourceEdge::all("b", "c");
    assert_eq!(sweep.rate, Rate::All);
    assert!(
        sweep.label.is_none(),
        "an unlabelled edge must not invent a type"
    );
}

#[test]
fn state_edges_are_distinct_effects() {
    // The two edge kinds are orthogonal on purpose. Collapsing them is what
    // makes most systems diagrams useless: you cannot tell whether A feeds B
    // or merely influences B.
    assert_eq!(
        StateEdge::activator("a", "b", 1).effect,
        Effect::Activator { at_least: 1 }
    );
    assert_eq!(
        StateEdge::trigger("a", "b", 2).effect,
        Effect::Trigger { at_least: 2 }
    );
    assert_eq!(
        StateEdge::inhibitor("a", "b", 3).effect,
        Effect::Inhibitor { at_least: 3 }
    );

    // An activator and an inhibitor at the same threshold are not equal.
    // If they compared equal, a brake and an accelerator would be
    // interchangeable, and the inhibitor-wins rule would be unstatable.
    assert_ne!(
        StateEdge::activator("a", "b", 1).effect,
        StateEdge::inhibitor("a", "b", 1).effect
    );
}

#[test]
fn broadcast_is_distinguishable_from_splitting() {
    // Broadcast gives every output a full copy and therefore CREATES
    // resources; deterministic splitting divides them. Conservation accounts
    // for these differently, so the two must never be conflated.
    assert_ne!(Distribution::Broadcast, Distribution::Deterministic);
    assert_ne!(
        Distribution::Probabilistic(vec![1, 1]),
        Distribution::Probabilistic(vec![1, 2])
    );
}
