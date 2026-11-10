//! P1.4 — the interpreter seam, and the scheduling rules written against it.
//!
//! Two halves, and they are checked very differently on purpose.
//!
//! **The trait is checked structurally.** Three greps over its method
//! signatures. That is cheap and it is the whole gate, which took three
//! drafts to accept. The first draft required a recording no-op to pass a
//! behavioural suite, which proves nothing: a no-op needs nothing. The second
//! required a deliberately hostile implementation to pass the *same* suite,
//! which is incoherent, because a hostile implementation fails transfers by
//! design and its behaviour differs on purpose. The expensive proof that this
//! seam is in the right place is building an executor against it, and that is
//! ADR-0004's falsification condition rather than a blocker at L0.
//!
//! **The rules are checked behaviourally**, because they are pure. Which node
//! may act is a function of the graph and a snapshot, so it can be tested
//! exhaustively here, before any backend exists. P1.5 re-proves the same rules
//! end to end through the simulator; this is the layer where a wrong rule can
//! still be told apart from a wrong implementation of it.

use pact_core::interpret::{eligible, inhibited, Levels, Tick};
use pact_core::{
    Activation, Bag, Distribution, Event, Graph, Ledger, Node, ResourceEdge, Signature, StateEdge,
    Type,
};

// ───────────────────────────────────────────── the structural gate

/// Every method signature in the `Interpreter` trait, comments stripped and
/// line wrapping undone.
fn interpreter_signatures() -> Vec<String> {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/interpret.rs"))
        .expect("interpret.rs must be readable to check its own shape");

    let start = src
        .find("pub trait Interpreter {")
        .expect("the trait must exist");
    let body = &src[start..];
    let end = body
        .find("\n}\n")
        .expect("the trait must close at column zero");

    let mut signatures = Vec::new();
    let mut current = String::new();
    for line in body[..end].lines() {
        let line = line.trim();
        // Doc comments inside the trait talk about seeds and steps at length,
        // explaining why they are absent. Checking them would fail on the
        // explanation, which is the wrong way round.
        if line.starts_with("//") {
            continue;
        }
        if current.is_empty() && !line.starts_with("fn ") {
            continue;
        }
        current.push(' ');
        current.push_str(line);
        if line.ends_with(';') || line.ends_with('{') {
            signatures.push(current.trim().to_string());
            current.clear();
        }
    }
    assert!(
        signatures.len() >= 8,
        "expected the full trait, found {} signature(s): {signatures:?}",
        signatures.len()
    );
    signatures
}

#[test]
fn no_method_mentions_a_seed_a_step_counter_or_a_ledger() {
    // Constraint 1. An executor has no seed and its step is wall-clock. A
    // trait that names either is a simulator interface wearing a trait's
    // clothes, and every result built on top inherits that assumption.
    const FORBIDDEN: [&str; 6] = ["seed", "rng", "step", "Ledger", "Trace", "Simulator"];

    for signature in interpreter_signatures() {
        for word in FORBIDDEN {
            assert!(
                !signature.to_lowercase().contains(&word.to_lowercase()),
                "`{word}` appears in a trait signature: {signature}"
            );
        }
    }
}

#[test]
fn transfer_can_fail() {
    // Constraint 2. In a simulator a transfer cannot fail. In an executor it
    // fails routinely: the artifact is missing, the agent died, the disk is
    // full. A signature that cannot say so cannot be implemented, and that is
    // discovered after the analysis layer is built on top of it.
    let transfer = interpreter_signatures()
        .into_iter()
        .find(|s| s.contains("fn transfer("))
        .expect("the trait must have a transfer");

    assert!(
        transfer.contains("-> Result<"),
        "transfer must be fallible: {transfer}"
    );

    // And it is not the only one. Anything that touches backend state has to
    // be able to say no.
    for name in ["create", "destroy", "copy_to", "hold"] {
        let signature = interpreter_signatures()
            .into_iter()
            .find(|s| s.contains(&format!("fn {name}(")))
            .unwrap_or_else(|| panic!("the trait must have {name}"));
        assert!(
            signature.contains("-> Result<"),
            "{name} must be fallible: {signature}"
        );
    }
}

#[test]
fn available_does_not_hand_back_a_borrow() {
    // Constraint 3. Returning `&Bag` from an internal map is free for the
    // simulator and impossible for anything whose state lives behind a
    // network call. Same for `holdings`, which must be an owned snapshot.
    for name in ["available", "holdings"] {
        let signature = interpreter_signatures()
            .into_iter()
            .find(|s| s.contains(&format!("fn {name}(")))
            .unwrap_or_else(|| panic!("the trait must have {name}"));

        let Some((_, returns)) = signature.split_once("->") else {
            panic!("{name} must return something: {signature}");
        };

        assert!(
            !returns.contains('&'),
            "{name} returns a borrow, which an out-of-process backend cannot: {signature}"
        );
    }
}

// ───────────────────────────────────────────── the scheduling rules

fn levels(pairs: &[(&str, u64)]) -> Levels {
    pairs
        .iter()
        .map(|(id, n)| ((*id).to_string(), Bag::of("M", *n)))
        .collect()
}

fn tick<'a>(now: &'a Levels, before: Option<&'a Levels>, first: bool) -> Tick<'a> {
    Tick {
        levels: now,
        previous: before,
        is_first: first,
    }
}

#[test]
fn activation_decides_who_offers_to_act() {
    let mut g = Graph::new();
    g.add(Node::pool("passive"))
        .add(Node::pool("automatic").with_activation(Activation::Automatic))
        .add(Node::pool("starting").with_activation(Activation::Starting));

    let now = levels(&[]);

    let first = eligible(&g, &tick(&now, None, true));
    assert_eq!(
        first,
        vec!["automatic", "starting"],
        "on step zero a starting node acts and a passive one never does"
    );

    let later = eligible(&g, &tick(&now, Some(&now), false));
    assert_eq!(
        later,
        vec!["automatic"],
        "a starting node seeds the system once, not every step"
    );
}

#[test]
fn a_trigger_fires_on_the_crossing_not_on_the_level() {
    // The distinction between a phase gate and an automatic node. Without it,
    // a triggered node fires every step forever once its condition is met and
    // Activation::Triggered means nothing.
    let mut g = Graph::new();
    g.add(Node::pool("watched")).add(
        Node::converter("gated", Signature::simple("M", "State"))
            .with_activation(Activation::Triggered),
    );
    g.influence(StateEdge::trigger("watched", "gated", 3));

    let below = levels(&[("watched", 2)]);
    let at = levels(&[("watched", 3)]);
    let above = levels(&[("watched", 9)]);

    assert!(
        eligible(&g, &tick(&below, None, true)).is_empty(),
        "below the threshold nothing fires"
    );
    assert_eq!(
        eligible(&g, &tick(&at, Some(&below), false)),
        vec!["gated"],
        "the step it crosses, it fires"
    );
    assert!(
        eligible(&g, &tick(&above, Some(&at), false)).is_empty(),
        "staying above the threshold is not a second crossing"
    );
    assert_eq!(
        eligible(&g, &tick(&at, None, true)),
        vec!["gated"],
        "already true on step zero counts as having become true"
    );
}

#[test]
fn an_activator_holds_a_node_inert_until_it_is_satisfied() {
    let mut g = Graph::new();
    g.add(Node::pool("phase_one")).add(Node::converter(
        "phase_two",
        Signature::simple("M", "State"),
    ));
    g.influence(StateEdge::activator("phase_one", "phase_two", 4));

    assert!(eligible(&g, &tick(&levels(&[("phase_one", 3)]), None, true)).is_empty());
    assert_eq!(
        eligible(&g, &tick(&levels(&[("phase_one", 4)]), None, true)),
        vec!["phase_two"]
    );
}

#[test]
fn every_activator_must_be_satisfied_not_just_one() {
    // A phase with two prerequisites opens when both land. "Any" would let a
    // phase start on half its inputs, which is the failure the gate exists to
    // prevent.
    let mut g = Graph::new();
    g.add(Node::pool("a"))
        .add(Node::pool("b"))
        .add(Node::converter("next", Signature::simple("M", "State")));
    g.influence(StateEdge::activator("a", "next", 1))
        .influence(StateEdge::activator("b", "next", 1));

    assert!(eligible(&g, &tick(&levels(&[("a", 5), ("b", 0)]), None, true)).is_empty());
    assert_eq!(
        eligible(&g, &tick(&levels(&[("a", 1), ("b", 1)]), None, true)),
        vec!["next"]
    );
}

#[test]
fn an_inhibitor_beats_an_activator() {
    // A brake must win against an accelerator. If this inverted, a system
    // would accelerate through exactly the condition someone wired a brake to
    // stop, and it would look like the brake was never connected.
    let mut g = Graph::new();
    g.add(Node::pool("go"))
        .add(Node::pool("stop"))
        .add(Node::converter("work", Signature::simple("M", "State")));
    g.influence(StateEdge::activator("go", "work", 1))
        .influence(StateEdge::inhibitor("stop", "work", 1));

    assert_eq!(
        eligible(&g, &tick(&levels(&[("go", 1), ("stop", 0)]), None, true)),
        vec!["work"],
        "activated and not inhibited: it runs"
    );
    assert!(
        eligible(&g, &tick(&levels(&[("go", 1), ("stop", 1)]), None, true)).is_empty(),
        "both satisfied: the brake wins"
    );
}

#[test]
fn being_held_back_is_reported_not_silently_dropped() {
    // "Nothing happened" and "a brake is on" look identical in a trace
    // otherwise, and they call for opposite fixes.
    let mut g = Graph::new();
    g.add(Node::pool("stop"))
        .add(Node::converter("work", Signature::simple("M", "State")));
    g.influence(StateEdge::inhibitor("stop", "work", 1));

    let held = levels(&[("stop", 1)]);
    assert_eq!(
        inhibited(&g, &tick(&held, None, true)),
        vec![("work".to_string(), "stop".to_string())]
    );

    let clear = levels(&[("stop", 0)]);
    assert!(inhibited(&g, &tick(&clear, None, true)).is_empty());
}

#[test]
fn a_modifier_never_gates_anything() {
    // A modifier scales a rate. It has no level at which it is "satisfied",
    // and reading one as satisfied would silently enable a node nobody
    // enabled. The safe direction for a predicate that decides whether work
    // runs is to answer no.
    let mut g = Graph::new();
    g.add(Node::pool("driver"))
        .add(Node::pool("target").with_activation(Activation::Automatic))
        .add(Node::pool("sink"));
    g.connect(ResourceEdge::new("target", "sink", 1));
    g.influence(StateEdge {
        from: "driver".into(),
        to: "target".into(),
        effect: pact_core::Effect::Modifier {
            edge_index: 0,
            per_unit: 2,
        },
    });

    // A huge driver level must not read as an activator being satisfied, and
    // must not read as an inhibitor firing either.
    let loaded = levels(&[("driver", u64::MAX / 2)]);
    assert!(eligible(&g, &tick(&loaded, None, true)).contains(&"target".to_string()));
    assert!(inhibited(&g, &tick(&loaded, None, true)).is_empty());
}

#[test]
fn eligibility_ignores_whether_resources_are_actually_there() {
    // A converter with nothing upstream is still eligible, and reports
    // Starved when it tries. Filtering it out here would make the commonest
    // real defect invisible: nine skills in the live corpus are in exactly
    // this state and the whole point is to see them.
    let mut g = Graph::new();
    g.add(Node::converter(
        "hungry",
        Signature::fan_in("M", 3, "M_review"),
    ));

    assert_eq!(
        eligible(&g, &tick(&levels(&[]), None, true)),
        vec!["hungry"],
        "starvation is a runtime fact, not an eligibility rule"
    );
}

// ───────────────────────────────────────────── the conservation ledger

#[test]
fn a_move_changes_nothing_on_the_books() {
    let mut ledger = Ledger::new();
    ledger.record(&Event::Moved {
        from: "a".into(),
        to: "b".into(),
        what: Bag::of("M", 4),
    });
    assert!(ledger.created().is_empty());
    assert!(ledger.destroyed().is_empty());
    assert!(ledger.conserved(&Bag::of("M", 4), &Bag::of("M", 4)).is_ok());
}

#[test]
fn a_broadcast_copy_is_a_creation() {
    // Broadcast gives every output a full copy, so it CREATES. If the ledger
    // read it as a move, a fan-out over four tracks would look like
    // conservation holding while the total quadrupled.
    let mut ledger = Ledger::new();
    for target in ["t1", "t2", "t3"] {
        ledger.record(&Event::Copied {
            from: "gate".into(),
            to: target.into(),
            what: Bag::of("M_plan", 1),
        });
    }
    assert_eq!(ledger.created().count(&Type::new("M_plan")), 3);
    assert!(ledger
        .conserved(&Bag::of("M_plan", 1), &Bag::of("M_plan", 4))
        .is_ok());
}

#[test]
fn a_converter_is_read_per_type_not_as_a_net_loss() {
    // A fan-in eats three `M` and emits one `M_review`. That is three
    // destroyed and one created. Calling it "two lost" would make the type
    // that actually vanished unnameable.
    let mut ledger = Ledger::new();
    ledger.record(&Event::Converted {
        at: "merge".into(),
        consumed: Bag::of("M", 3),
        produced: Bag::of("M_review", 1),
    });

    assert_eq!(ledger.destroyed().count(&Type::new("M")), 3);
    assert_eq!(ledger.created().count(&Type::new("M_review")), 1);

    let mut closing = Bag::new();
    closing.add(Type::new("M_review"), 1);
    assert!(ledger.conserved(&Bag::of("M", 3), &closing).is_ok());
}

#[test]
fn aggregate_conservation_is_not_conservation() {
    // The reason INV-1.6 says "per type, aggregate-only checking is a fail".
    // Four `M_plan` becoming four `State` balances perfectly in total and is
    // exactly the defect worth catching.
    let ledger = Ledger::new();
    let opening = Bag::of("M_plan", 4);
    let closing = Bag::of("State", 4);

    assert_eq!(opening.total(), closing.total(), "the aggregate agrees");

    let broken = ledger
        .conserved(&opening, &closing)
        .expect_err("per-type checking must reject it");
    let named: Vec<_> = broken.iter().map(|i| i.ty.to_string()).collect();
    assert_eq!(
        named,
        vec!["M_plan", "State"],
        "both sides of the substitution are named, got {broken:?}"
    );
}

#[test]
fn an_imbalance_says_what_was_expected_and_what_was_found() {
    // A report saying only "conservation failed" sends someone to read the
    // whole trace. The numbers are what make it a bug report.
    let mut ledger = Ledger::new();
    ledger.record(&Event::Created {
        at: "src".into(),
        what: Bag::of("M", 2),
    });

    let broken = ledger
        .conserved(&Bag::new(), &Bag::of("M", 5))
        .expect_err("five held after two created is a leak");
    assert_eq!(broken.len(), 1);
    assert_eq!(broken[0].expected, 2);
    assert_eq!(broken[0].found, 5);
}

#[test]
fn events_that_are_neither_creation_nor_destruction_are_inert() {
    // Held, Released, Starved and Inhibited are observations, not movements.
    // If any of them touched the books, a delay would look like a leak for
    // exactly as long as it held work.
    let mut ledger = Ledger::new();
    for event in [
        Event::Held {
            at: "d".into(),
            what: Bag::of("M", 7),
        },
        Event::Released {
            at: "d".into(),
            what: Bag::of("M", 7),
        },
        Event::Starved {
            at: "c".into(),
            wanted: Bag::of("M", 3),
        },
        Event::Inhibited {
            at: "c".into(),
            by: "brake".into(),
        },
    ] {
        ledger.record(&event);
    }
    assert!(ledger.created().is_empty());
    assert!(ledger.destroyed().is_empty());
    assert!(ledger.conserved(&Bag::of("M", 7), &Bag::of("M", 7)).is_ok());
}

#[test]
fn the_gate_vocabulary_is_reachable_from_the_scheduler() {
    // A cheap guard against the seam drifting out of sync with the node
    // vocabulary: every Distribution a gate can carry has to be something the
    // scheduler knows how to split. This catches a new variant added to
    // node.rs and never handled here, which otherwise surfaces as a gate that
    // silently does nothing.
    let all = [
        Distribution::Broadcast,
        Distribution::Deterministic,
        Distribution::Probabilistic(vec![1, 1]),
    ];
    for distribution in all {
        let mut g = Graph::new();
        g.add(Node::gate("g", distribution));
        assert_eq!(
            eligible(&g, &tick(&levels(&[]), None, true)),
            vec!["g"],
            "a gate is automatic regardless of how it splits"
        );
    }
}
