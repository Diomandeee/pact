//! P1.5 — the simulator, and the scheduling rules proved end to end.
//!
//! P1.4 proved the rules as pure functions over a snapshot. This proves the
//! same rules through a running backend, which is a different claim: a correct
//! rule can still be wired up wrong, and only one of those two failures is
//! visible from a unit test of the rule.
//!
//! # The step model, stated once
//!
//! Decisions use a snapshot taken at the **start** of the step. Effects apply
//! immediately. So a source that fills a pool on step 0 feeds a drain that
//! reads that pool on step 1, not on step 0. That is simultaneous update, it
//! is what Machinations does, and it is why several assertions here look one
//! step late. The alternative, letting each node see the effects of the nodes
//! before it, makes behaviour depend on insertion order, which is exactly the
//! kind of hidden coupling this whole layer exists to expose.

use pact_core::{
    Activation, Bag, Demand, Distribution, Event, Graph, Kind, Node, ResourceEdge, Signature,
    SimError, Simulator, StateEdge, Type,
};

fn m(n: u64) -> Bag {
    Bag::of("M", n)
}

fn ty(name: &str) -> Type {
    Type::new(name)
}

/// How many of `name` a node holds.
fn at(sim: &Simulator, node: &str, name: &str) -> u64 {
    sim.state().get(node).map_or(0, |b| b.count(&ty(name)))
}

fn count<F: Fn(&Event) -> bool>(trace: &pact_core::Trace, pred: F) -> usize {
    trace.events().filter(|e| pred(e)).count()
}

// ───────────────────────────────────────────── sources and drains

#[test]
fn a_source_creates_and_a_drain_destroys() {
    let mut g = Graph::new();
    g.add(Node::source("s", m(2)))
        .add(Node::pool("p"))
        .add(Node::drain("d"));
    g.connect(ResourceEdge::new("s", "p", 2))
        .connect(ResourceEdge::all("p", "d"));

    let mut sim = Simulator::new(&g, 1);
    let opening = sim.total_held();
    let (trace, ledger) = sim.run(&g, 4).expect("the run must not fail");

    assert_eq!(
        ledger.created().count(&ty("M")),
        8,
        "four steps at two per step"
    );
    assert!(
        ledger.destroyed().count(&ty("M")) > 0,
        "the drain must actually destroy something"
    );
    assert!(
        ledger.conserved(&opening, &sim.total_held()).is_ok(),
        "created minus destroyed must account for what is held"
    );

    // The one-step lag is the step model, not a bug, so it is asserted rather
    // than worked around.
    let first = &trace.steps[0];
    assert!(
        first
            .events
            .iter()
            .any(|e| matches!(e, Event::Created { .. })),
        "the source fills on step zero"
    );
    assert!(
        !first
            .events
            .iter()
            .any(|e| matches!(e, Event::Destroyed { .. })),
        "the drain cannot see what arrived after its own snapshot"
    );
}

#[test]
fn a_source_splits_its_output_and_does_not_multiply_it() {
    // Theorem 1 makes import the only thing that extends a loop's life, so how
    // much enters has to be a property of the source, not of how many wires
    // happen to leave it. The first implementation shared one `emits` bag
    // across every edge, so adding a second output doubled the import.
    let mut g = Graph::new();
    g.add(Node::source("s", m(2)))
        .add(Node::pool("a"))
        .add(Node::pool("b"));
    g.connect(ResourceEdge::new("s", "a", 2))
        .connect(ResourceEdge::new("s", "b", 2));

    let mut sim = Simulator::new(&g, 1);
    let (_, ledger) = sim.run(&g, 1).expect("the run must not fail");

    assert_eq!(
        ledger.created().count(&ty("M")),
        2,
        "two per step means two, whatever the fan-out"
    );
    assert_eq!(at(&sim, "a", "M") + at(&sim, "b", "M"), 2);
}

// ───────────────────────────────────────────── gates

#[test]
fn broadcast_creates() {
    // Each output gets the phase context entire, not a third of it. That means
    // broadcast is the one place outside a source where resources appear, and
    // the ledger has to be told so or conservation reports a leak that is
    // really the design.
    let mut g = Graph::new();
    g.add(Node::pool_with("src", m(1)))
        .add(Node::gate("g", Distribution::Broadcast))
        .add(Node::pool("t1"))
        .add(Node::pool("t2"))
        .add(Node::pool("t3"));
    g.connect(ResourceEdge::all("src", "g"))
        .connect(ResourceEdge::all("g", "t1"))
        .connect(ResourceEdge::all("g", "t2"))
        .connect(ResourceEdge::all("g", "t3"));

    let mut sim = Simulator::new(&g, 1);
    let opening = sim.total_held();
    assert_eq!(opening.count(&ty("M")), 1);

    let (trace, ledger) = sim.run(&g, 1).expect("the run must not fail");

    for target in ["t1", "t2", "t3"] {
        assert_eq!(at(&sim, target, "M"), 1, "{target} must get a full copy");
    }
    assert_eq!(ledger.created().count(&ty("M")), 3, "three copies");
    assert_eq!(
        ledger.destroyed().count(&ty("M")),
        1,
        "the original is consumed by the fan-out"
    );
    assert_eq!(count(&trace, |e| matches!(e, Event::Copied { .. })), 3);
    assert!(
        ledger.conserved(&opening, &sim.total_held()).is_ok(),
        "one in, three out, one destroyed: the books still close"
    );
}

#[test]
fn deterministic_distribution_round_robins() {
    // Three across two outputs splits 2/1, and it splits the same way every
    // run. Landing wherever the arithmetic rounds would be reproducible too,
    // and unfair in a way nobody could predict from the diagram.
    let mut g = Graph::new();
    g.add(Node::pool_with("src", m(3)))
        .add(Node::gate("g", Distribution::Deterministic))
        .add(Node::pool("t1"))
        .add(Node::pool("t2"));
    g.connect(ResourceEdge::all("src", "g"))
        .connect(ResourceEdge::all("g", "t1"))
        .connect(ResourceEdge::all("g", "t2"));

    let mut sim = Simulator::new(&g, 1);
    sim.run(&g, 1).expect("the run must not fail");

    assert_eq!(at(&sim, "t1", "M"), 2);
    assert_eq!(at(&sim, "t2", "M"), 1);
}

#[test]
fn probabilistic_distribution_respects_its_weights() {
    // A zero weight must mean never, not rarely. If it did not, a gate
    // configured to disable a branch would still leak work down it, and the
    // leak would look like noise.
    let mut g = Graph::new();
    g.add(Node::pool_with("src", m(20)))
        .add(Node::gate("g", Distribution::Probabilistic(vec![1, 0])))
        .add(Node::pool("t1"))
        .add(Node::pool("t2"));
    g.connect(ResourceEdge::all("src", "g"))
        .connect(ResourceEdge::all("g", "t1"))
        .connect(ResourceEdge::all("g", "t2"));

    let mut sim = Simulator::new(&g, 0xD1CE);
    sim.run(&g, 1).expect("the run must not fail");

    assert_eq!(at(&sim, "t1", "M"), 20);
    assert_eq!(at(&sim, "t2", "M"), 0, "a zero weight means never");
}

// ───────────────────────────────────────────── delays

#[test]
fn a_delay_releases_on_schedule_and_holds_nothing_visible_meanwhile() {
    // The gate's own words: "delays release on schedule". Both halves matter.
    // Work in flight must be invisible to anything pulling downstream, or a
    // delay is a pool with extra steps.
    let mut g = Graph::new();
    g.add(Node::pool_with("src", m(1)))
        .add(Node::delay("w", 3))
        .add(Node::pool("sink"));
    g.connect(ResourceEdge::all("src", "w"))
        .connect(ResourceEdge::all("w", "sink"));

    let mut sim = Simulator::new(&g, 1);
    let opening = sim.total_held();

    // Step 0: taken in and held.
    sim.run(&g, 1).expect("the run must not fail");
    assert_eq!(at(&sim, "w", "M"), 0, "held work is in flight, not on hand");
    assert_eq!(at(&sim, "src", "M"), 0);
    assert_eq!(
        sim.total_held().count(&ty("M")),
        1,
        "in flight is still held: it has not been destroyed"
    );

    // Steps 1 and 2: still in flight.
    for _ in 0..2 {
        sim.run(&g, 1).expect("the run must not fail");
        assert_eq!(at(&sim, "sink", "M"), 0, "released early");
    }

    // Step 3: released back into the delay, and pushed on the same step.
    sim.run(&g, 1).expect("the run must not fail");
    assert_eq!(at(&sim, "sink", "M"), 1, "released late");
    assert_eq!(at(&sim, "w", "M"), 0, "and it did not linger in the delay");

    assert_eq!(
        sim.total_held(),
        opening,
        "a delay is conserving; nothing was created or destroyed"
    );
}

#[test]
fn a_delay_pushes_released_work_before_taking_more_in() {
    // Found by writing the test above. Pulling first meant released work was
    // held again the same step, so a delay of three never released anything:
    // the work went round forever with a Held event every step and no way to
    // tell which pass it was on.
    let mut g = Graph::new();
    g.add(Node::source("s", m(1)))
        .add(Node::delay("w", 1))
        .add(Node::pool("sink"));
    g.connect(ResourceEdge::all("s", "w"))
        .connect(ResourceEdge::all("w", "sink"));

    let mut sim = Simulator::new(&g, 1);
    sim.run(&g, 6).expect("the run must not fail");

    assert!(
        at(&sim, "sink", "M") > 0,
        "a delay fed every step must eventually deliver something"
    );
}

// ───────────────────────────────────────────── state edges

#[test]
fn an_inhibitor_beats_an_activator_in_a_running_system() {
    // Proved as a rule in P1.4. Proved here as behaviour, because a correct
    // rule wired to the wrong node is a failure a unit test of the rule cannot
    // see.
    let mut g = Graph::new();
    g.add(Node::pool_with("go", m(1)))
        .add(Node::pool_with("stop", m(1)))
        .add(Node::pool_with("input", m(5)))
        .add(Node::converter("work", Signature::simple("M", "State")))
        .add(Node::pool("out"));
    g.connect(ResourceEdge::new("input", "work", 1))
        .connect(ResourceEdge::all("work", "out"));
    g.influence(StateEdge::activator("go", "work", 1))
        .influence(StateEdge::inhibitor("stop", "work", 1));

    let mut sim = Simulator::new(&g, 1);
    let (trace, _) = sim.run(&g, 3).expect("the run must not fail");

    assert_eq!(at(&sim, "out", "State"), 0, "the brake must win");
    assert_eq!(
        at(&sim, "input", "M"),
        5,
        "and nothing upstream was touched"
    );
    assert!(
        count(&trace, |e| matches!(e, Event::Inhibited { .. })) >= 3,
        "and it must say so every step it holds"
    );
}

#[test]
fn releasing_the_brake_lets_the_same_system_run() {
    // The other half. Without it, the test above passes against a converter
    // that never worked for an unrelated reason.
    let mut g = Graph::new();
    g.add(Node::pool_with("go", m(1)))
        .add(Node::pool("stop"))
        .add(Node::pool_with("input", m(5)))
        .add(Node::converter("work", Signature::simple("M", "State")))
        .add(Node::pool("out"));
    g.connect(ResourceEdge::new("input", "work", 1))
        .connect(ResourceEdge::all("work", "out"));
    g.influence(StateEdge::activator("go", "work", 1))
        .influence(StateEdge::inhibitor("stop", "work", 1));

    let mut sim = Simulator::new(&g, 1);
    sim.run(&g, 3).expect("the run must not fail");

    assert!(at(&sim, "out", "State") > 0, "with no brake it must run");
    assert!(at(&sim, "input", "M") < 5, "and it must consume");
}

// ───────────────────────────────────────────── converters

#[test]
fn demand_all_starves_rather_than_half_running() {
    // Half-run is worse than not-run: it produces confident wrong answers
    // downstream. The upstream pool must be untouched, because a converter
    // that took what it could and stalled has already spent the resources.
    let mut g = Graph::new();
    g.add(Node::pool_with("input", m(1)))
        .add(Node::converter(
            "merge",
            Signature::fan_in("M", 3, "M_review"),
        ))
        .add(Node::pool("out"));
    g.connect(ResourceEdge::all("input", "merge"))
        .connect(ResourceEdge::all("merge", "out"));

    let mut sim = Simulator::new(&g, 1);
    let (trace, ledger) = sim.run(&g, 2).expect("the run must not fail");

    assert_eq!(at(&sim, "input", "M"), 1, "nothing may be taken");
    assert_eq!(at(&sim, "out", "M_review"), 0);
    assert!(ledger.created().is_empty());
    assert!(
        count(&trace, |e| matches!(e, Event::Starved { .. })) >= 2,
        "and starvation is recorded, because it is the diagnosis"
    );
}

#[test]
fn demand_any_takes_what_it_can_get() {
    let mut g = Graph::new();
    g.add(Node::pool_with("input", m(1)))
        .add(
            Node::converter("loose", Signature::fan_in("M", 3, "M_review"))
                .with_demand(Demand::Any),
        )
        .add(Node::pool("out"));
    g.connect(ResourceEdge::all("input", "loose"))
        .connect(ResourceEdge::all("loose", "out"));

    let mut sim = Simulator::new(&g, 1);
    sim.run(&g, 2).expect("the run must not fail");

    assert_eq!(at(&sim, "input", "M"), 0, "it took what was there");
    assert!(at(&sim, "out", "M_review") > 0);
}

#[test]
fn a_converter_splits_its_output_and_does_not_duplicate_it() {
    // Only a broadcast gate duplicates, and it says so through `Copied`. The
    // first implementation shared one `produced` bag across every outgoing
    // edge, so a converter with two outputs emitted twice what its signature
    // promised.
    let mut g = Graph::new();
    let doubling = Signature {
        consumes: m(1),
        produces: Bag::of("M_plan", 2),
    };
    g.add(Node::pool_with("input", m(1)))
        .add(Node::converter("c", doubling))
        .add(Node::pool("a"))
        .add(Node::pool("b"));
    g.connect(ResourceEdge::all("input", "c"))
        .connect(ResourceEdge::new("c", "a", 1))
        .connect(ResourceEdge::new("c", "b", 1));

    let mut sim = Simulator::new(&g, 1);
    let (_, ledger) = sim.run(&g, 2).expect("the run must not fail");

    assert_eq!(
        ledger.created().count(&ty("M_plan")),
        2,
        "the signature promised two, so two exist"
    );
    assert_eq!(at(&sim, "a", "M_plan"), 1);
    assert_eq!(at(&sim, "b", "M_plan"), 1);
}

#[test]
fn a_converter_wired_to_nothing_reports_what_it_actually_emitted() {
    // Otherwise the ledger is told two `M_plan` were created, none exist, and
    // conservation reports a leak whose real cause is an accounting error one
    // layer down. The failure would be pinned on the engine, not the wiring.
    let mut g = Graph::new();
    g.add(Node::pool_with("input", m(1)))
        .add(Node::converter("nowhere", Signature::simple("M", "M_plan")));
    g.connect(ResourceEdge::all("input", "nowhere"));

    let mut sim = Simulator::new(&g, 1);
    let opening = sim.total_held();
    let (_, ledger) = sim.run(&g, 2).expect("the run must not fail");

    assert!(
        ledger.created().is_empty(),
        "nothing was emitted, so nothing was created"
    );
    assert!(ledger.conserved(&opening, &sim.total_held()).is_ok());
}

// ───────────────────────────────────────────── pools

#[test]
fn two_take_everything_edges_do_not_both_claim_the_whole_pool() {
    // The first implementation snapshotted a pool's holdings once and reused
    // it for every outgoing edge, so the second `Rate::All` transfer ran
    // against a pool the first had already emptied and failed outright.
    let mut g = Graph::new();
    g.add(Node::pool_with("p", m(4)).with_activation(Activation::Automatic))
        .add(Node::pool("a"))
        .add(Node::pool("b"));
    g.connect(ResourceEdge::all("p", "a"))
        .connect(ResourceEdge::all("p", "b"));

    let mut sim = Simulator::new(&g, 1);
    let opening = sim.total_held();
    let (_, ledger) = sim.run(&g, 1).expect("the run must not fail");

    assert_eq!(at(&sim, "a", "M"), 4, "the first edge takes everything");
    assert_eq!(at(&sim, "b", "M"), 0, "and the second finds nothing left");
    assert!(ledger.conserved(&opening, &sim.total_held()).is_ok());
}

#[test]
fn a_passive_pool_never_pushes() {
    // What makes a pool storage rather than a pump. If it pushed, every pool
    // in an imported corpus would drain itself on step one and every pressure
    // measurement would read zero.
    let mut g = Graph::new();
    g.add(Node::pool_with("p", m(4))).add(Node::pool("a"));
    g.connect(ResourceEdge::all("p", "a"));

    let mut sim = Simulator::new(&g, 1);
    sim.run(&g, 3).expect("the run must not fail");

    assert_eq!(at(&sim, "p", "M"), 4);
    assert_eq!(at(&sim, "a", "M"), 0);
}

// ───────────────────────────────────────────── failure and shape

#[test]
fn an_over_commitment_fails_loudly_with_both_bags() {
    // The scheduler plans against a snapshot; the interpreter is the authority
    // on what is there. A plan that over-commits has to be an error, not a
    // silent clamp, or negative stock becomes representable.
    let mut sim = Simulator::new(&Graph::new(), 1);
    let err = pact_core::Interpreter::destroy(&mut sim, "empty", &m(3))
        .expect_err("taking three from nothing must fail");

    let SimError::Insufficient { at, wanted, held } = err;
    assert_eq!(at, "empty");
    assert_eq!(wanted.count(&ty("M")), 3);
    assert_eq!(held.count(&ty("M")), 0, "the error carries what was there");
}

#[test]
fn a_zero_seed_is_not_a_constant_sequence() {
    // Zero is the xorshift fixed point: it maps to itself forever. Passing
    // seed 0 must not silently produce the same "random" value every call,
    // because the run would look reproducible for the wrong reason.
    let mut g = Graph::new();
    g.add(Node::pool_with("src", m(40)))
        .add(Node::gate("g", Distribution::Probabilistic(vec![1, 1])))
        .add(Node::pool("t1"))
        .add(Node::pool("t2"));
    g.connect(ResourceEdge::all("src", "g"))
        .connect(ResourceEdge::all("g", "t1"))
        .connect(ResourceEdge::all("g", "t2"));

    let mut sim = Simulator::new(&g, 0);
    sim.run(&g, 1).expect("the run must not fail");

    assert!(
        at(&sim, "t1", "M") > 0 && at(&sim, "t2", "M") > 0,
        "even weights over forty draws must reach both branches, got {} and {}",
        at(&sim, "t1", "M"),
        at(&sim, "t2", "M")
    );
}

#[test]
fn an_edge_between_two_active_nodes_is_driven_from_one_side_only() {
    // The defect that forced the push/pull rule. A gate pushes along its
    // outputs and a converter pulls along its inputs, so the edge between them
    // was driven from both ends: the converter planned a pull against the
    // start-of-step snapshot, the gate had already pushed the same resources
    // away, and the pull failed against a node the push had emptied.
    //
    // The rule is that storage is pulled from and everything active pushes, so
    // this edge is the gate's. Ordering cannot fix it, because the two
    // mechanisms disagree about who owns the edge rather than about when.
    let mut g = Graph::new();
    g.add(Node::pool_with("src", m(4)))
        .add(Node::gate("g", Distribution::Deterministic))
        .add(Node::converter("c", Signature::simple("M", "State")))
        .add(Node::pool("out"));
    g.connect(ResourceEdge::all("src", "g"))
        .connect(ResourceEdge::all("g", "c"))
        .connect(ResourceEdge::all("c", "out"));

    let mut sim = Simulator::new(&g, 1);
    let opening = sim.total_held();
    let (_, ledger) = sim
        .run(&g, 5)
        .expect("an edge between two active nodes must not double-move");

    assert!(at(&sim, "out", "State") > 0, "the chain must actually run");
    assert!(ledger.conserved(&opening, &sim.total_held()).is_ok());
}

#[test]
fn work_pushed_into_a_delay_is_still_delayed() {
    // The other half of the delay fix. Released work leaves, and only released
    // work: pushing out everything on hand would push out work that arrived
    // the same step and never delay it at all.
    let mut g = Graph::new();
    g.add(Node::pool_with("src", m(1)).with_activation(Activation::Automatic))
        .add(Node::delay("w", 2))
        .add(Node::pool("sink"));
    g.connect(ResourceEdge::all("src", "w"))
        .connect(ResourceEdge::all("w", "sink"));

    let mut sim = Simulator::new(&g, 1);

    sim.run(&g, 1).expect("the run must not fail");
    assert_eq!(
        at(&sim, "sink", "M"),
        0,
        "pushed in and held, not passed on"
    );
    sim.run(&g, 1).expect("the run must not fail");
    assert_eq!(at(&sim, "sink", "M"), 0, "still in flight");
    sim.run(&g, 1).expect("the run must not fail");
    assert_eq!(at(&sim, "sink", "M"), 1, "released after exactly two steps");
}

#[test]
fn every_node_kind_can_be_driven() {
    // A cheap guard against a kind the scheduler silently ignores. A gate that
    // does nothing looks identical to a gate with nothing to do, and the
    // difference only shows up as a pressure number nobody can explain.
    let mut g = Graph::new();
    g.add(Node::source("s", m(2)))
        .add(Node::pool("p").with_activation(Activation::Automatic))
        .add(Node::gate("g", Distribution::Deterministic))
        .add(Node::delay("w", 1))
        .add(Node::converter("c", Signature::simple("M", "State")))
        .add(Node::drain("d"));
    g.connect(ResourceEdge::all("s", "p"))
        .connect(ResourceEdge::all("p", "g"))
        .connect(ResourceEdge::all("g", "w"))
        .connect(ResourceEdge::all("w", "c"))
        .connect(ResourceEdge::all("c", "d"));

    let mut sim = Simulator::new(&g, 7);
    let opening = sim.total_held();
    let (trace, ledger) = sim.run(&g, 10).expect("the chain must run");

    for kind in ["Created", "Moved", "Held", "Released", "Converted"] {
        let seen = trace.events().any(|e| {
            matches!(
                (kind, e),
                ("Created", Event::Created { .. })
                    | ("Moved", Event::Moved { .. })
                    | ("Held", Event::Held { .. })
                    | ("Released", Event::Released { .. })
                    | ("Converted", Event::Converted { .. })
            )
        });
        assert!(seen, "nothing in ten steps produced a {kind} event");
    }
    assert!(ledger.conserved(&opening, &sim.total_held()).is_ok());

    // And the vocabulary is exhaustive: every Kind above appears in the graph.
    let kinds: Vec<&str> = g
        .nodes()
        .iter()
        .map(|n| match n.kind {
            Kind::Pool { .. } => "pool",
            Kind::Source { .. } => "source",
            Kind::Drain => "drain",
            Kind::Converter { .. } => "converter",
            Kind::Gate { .. } => "gate",
            Kind::Delay { .. } => "delay",
        })
        .collect();
    for expected in ["pool", "source", "drain", "converter", "gate", "delay"] {
        assert!(kinds.contains(&expected), "{expected} missing from fixture");
    }
}
