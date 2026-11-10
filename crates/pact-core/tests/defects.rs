//! P1.3 — structural defects.
//!
//! The checklist states this gate as "a fixture graph containing all six
//! returns all six." That is necessary and it is not sufficient: a `validate`
//! that returned every defect unconditionally would pass it. So the gate here
//! is three-sided, and the second and third sides are the ones with teeth:
//!
//!   1. a graph containing all six returns exactly six, one of each
//!   2. a clean graph returns none
//!   3. each defect in isolation returns exactly itself
//!
//! (3) is what rules out a detector that fires on the wrong shape. A validator
//! that reported `Sink` for every pool would satisfy (1) and fail (3).
//!
//! Each fixture below adds its own dedicated nodes rather than mutating the
//! clean graph's. That is not fastidiousness: the first draft reused the
//! clean graph's pool for both the unbounded case and the sink case, and the
//! extra outflow pushed the inflow/outflow ratio back under the threshold, so
//! `UnboundedPool` silently stopped firing in the combined fixture. Fixtures
//! that share nodes test their interaction, not the thing they name.

use pact_core::{Bag, Defect, Graph, Node, ResourceEdge, Signature, PRESSURE_RATIO};

/// Source, converter, pool, drain. Nothing wrong with it.
fn clean() -> Graph {
    let mut g = Graph::new();
    g.add(Node::source("s", Bag::of("M", 1)))
        .add(Node::converter("c", Signature::simple("M", "M_plan")))
        .add(Node::pool("p"))
        .add(Node::drain("d"));
    g.connect(ResourceEdge::new("s", "c", 1))
        .connect(ResourceEdge::new("c", "p", 1))
        .connect(ResourceEdge::new("p", "d", 1));
    g
}

/// An edge naming a node that was never added.
fn add_dangling(g: &mut Graph) {
    g.add(Node::pool("de"));
    g.connect(ResourceEdge::new("de", "ghost", 1));
}

/// The same id added twice. Isolated, so nothing else can fire on it.
fn add_duplicate(g: &mut Graph) {
    g.add(Node::pool("twin")).add(Node::pool("twin"));
}

/// A converter nothing feeds. Nine skills in the real corpus are here.
fn add_starved(g: &mut Graph) {
    g.add(Node::converter("cs", Signature::simple("M", "M_review")));
}

/// Pressure: producers at the ratio, one consumer.
fn add_unbounded(g: &mut Graph) {
    g.add(Node::pool("hot")).add(Node::drain("hot_out"));
    for i in 0..PRESSURE_RATIO {
        let feeder = format!("f{i}");
        g.add(Node::pool(feeder.clone()));
        g.connect(ResourceEdge::new(feeder, "hot", 1));
    }
    g.connect(ResourceEdge::new("hot", "hot_out", 1));
}

/// A source wired to nothing.
fn add_orphan_source(g: &mut Graph) {
    g.add(Node::source("os", Bag::of("M", 1)));
}

/// Work arrives and cannot leave.
fn add_sink(g: &mut Graph) {
    g.add(Node::pool("upstream")).add(Node::pool("trap"));
    g.connect(ResourceEdge::new("upstream", "trap", 1));
}

/// A named mutation that introduces exactly one defect.
type Fixture = (&'static str, fn(&mut Graph));

fn kind_of(d: &Defect) -> &'static str {
    match d {
        Defect::DanglingEdge { .. } => "dangling",
        Defect::DuplicateNode { .. } => "duplicate",
        Defect::StarvedConverter { .. } => "starved",
        Defect::UnboundedPool { .. } => "unbounded",
        Defect::OrphanSource { .. } => "orphan",
        Defect::Sink { .. } => "sink",
    }
}

#[test]
fn a_clean_graph_has_no_defects() {
    // Side (2). Without this the whole suite is satisfied by a validator that
    // returns everything, and "found 6 defects" would mean nothing.
    assert_eq!(
        clean().validate(),
        vec![],
        "a well-formed graph must be silent"
    );
}

#[test]
fn all_six_defects_are_detected_together() {
    // Side (1), the gate as the checklist states it.
    let mut g = clean();
    add_dangling(&mut g);
    add_duplicate(&mut g);
    add_starved(&mut g);
    add_unbounded(&mut g);
    add_orphan_source(&mut g);
    add_sink(&mut g);

    let defects = g.validate();
    let mut kinds: Vec<_> = defects.iter().map(kind_of).collect();
    kinds.sort_unstable();

    assert_eq!(
        kinds,
        vec![
            "dangling",
            "duplicate",
            "orphan",
            "sink",
            "starved",
            "unbounded"
        ],
        "expected exactly one of each, got {defects:?}"
    );
}

#[test]
fn each_defect_fires_alone() {
    // Side (3). A detector that fires on the wrong shape passes side (1) and
    // dies here.
    let cases: [Fixture; 6] = [
        ("dangling", add_dangling),
        ("duplicate", add_duplicate),
        ("starved", add_starved),
        ("unbounded", add_unbounded),
        ("orphan", add_orphan_source),
        ("sink", add_sink),
    ];

    for (name, apply) in cases {
        let mut g = clean();
        apply(&mut g);
        let defects = g.validate();
        assert_eq!(
            defects.len(),
            1,
            "{name}: expected one defect, got {defects:?}"
        );
        assert_eq!(kind_of(&defects[0]), name, "{name}: wrong defect reported");
    }
}

#[test]
fn a_dangling_edge_names_the_endpoint_that_is_missing() {
    // The report has to be actionable. "Something is dangling" sends the
    // author back to read every edge by hand.
    let mut g = Graph::new();
    g.add(Node::pool("real"));
    g.connect(ResourceEdge::new("real", "ghost", 1));

    assert_eq!(
        g.validate(),
        vec![Defect::DanglingEdge {
            from: "real".into(),
            to: "ghost".into(),
            missing: "ghost".into(),
        }]
    );

    // Both endpoints missing is two findings, not one, because they are two
    // separate authoring mistakes that happen to share an edge.
    let mut g = Graph::new();
    g.connect(ResourceEdge::new("nowhere", "neither", 1));
    assert_eq!(g.validate().len(), 2);
}

#[test]
fn state_edges_dangle_too() {
    // A state connection moves no resources, which makes it easy to forget it
    // can point at nothing. Cross-layer forcing that silently addresses a
    // node that does not exist is a phase gate that never opens.
    use pact_core::StateEdge;

    let mut g = Graph::new();
    g.add(Node::pool("watched"));
    g.influence(StateEdge::activator("watched", "absent", 1));

    let defects = g.validate();
    assert_eq!(defects.len(), 1, "got {defects:?}");
    assert_eq!(kind_of(&defects[0]), "dangling");
}

#[test]
fn a_drain_is_not_a_sink_but_a_terminal_pool_is() {
    // The distinction the Sink detector exists to make. Destroying resources
    // is a drain's job; a pool doing it is work quietly disappearing.
    let mut g = Graph::new();
    g.add(Node::pool("feeder"))
        .add(Node::drain("legit"))
        .add(Node::pool("trap"));
    g.connect(ResourceEdge::new("feeder", "legit", 1))
        .connect(ResourceEdge::new("feeder", "trap", 1));

    assert_eq!(g.validate(), vec![Defect::Sink { id: "trap".into() }]);
}

#[test]
fn a_terminal_delay_is_a_sink() {
    // A delay holds work for n steps and then releases it. With no outgoing
    // edge there is nowhere to release it to, so the work is held forever.
    // This is the case the first implementation missed: it checked only
    // pools, while its own documentation said "a defect for anything else."
    let mut g = Graph::new();
    g.add(Node::pool("feeder")).add(Node::delay("held", 3));
    g.connect(ResourceEdge::new("feeder", "held", 1));

    assert_eq!(g.validate(), vec![Defect::Sink { id: "held".into() }]);
}

#[test]
fn a_terminal_converter_is_a_sink() {
    // A converter with an inflow and no outflow eats work and emits into
    // nothing. It looks configured and it is a drain wearing a skill's name.
    let mut g = Graph::new();
    g.add(Node::pool("feeder")).add(Node::converter(
        "emits_nowhere",
        Signature::simple("M", "State"),
    ));
    g.connect(ResourceEdge::new("feeder", "emits_nowhere", 1));

    assert_eq!(
        g.validate(),
        vec![Defect::Sink {
            id: "emits_nowhere".into()
        }]
    );
}

#[test]
fn pressure_is_reported_at_the_threshold_and_not_below() {
    // The boundary case, in both directions. An off-by-one here means either
    // a healthy corpus is flagged or a drowning pool is not.
    let build = |feeders: usize| {
        let mut g = Graph::new();
        g.add(Node::pool("hot")).add(Node::drain("out"));
        for i in 0..feeders {
            let id = format!("f{i}");
            g.add(Node::pool(id.clone()));
            g.connect(ResourceEdge::new(id, "hot", 1));
        }
        g.connect(ResourceEdge::new("hot", "out", 1));
        g.validate()
    };

    assert_eq!(
        build(PRESSURE_RATIO),
        vec![Defect::UnboundedPool {
            id: "hot".into(),
            inflow: PRESSURE_RATIO,
            outflow: 1,
        }],
        "at the ratio, pressure must be reported"
    );
    assert_eq!(
        build(PRESSURE_RATIO - 1),
        vec![],
        "one below the ratio must be silent"
    );
}

#[test]
fn a_duplicated_node_is_reported_once_not_per_copy() {
    // Three copies of one id is one authoring mistake. Reporting it three
    // times, and every downstream defect on that node three times with it, is
    // how a defect report becomes something people scroll past.
    let mut g = Graph::new();
    g.add(Node::pool("twin"))
        .add(Node::pool("twin"))
        .add(Node::pool("twin"));

    assert_eq!(
        g.validate(),
        vec![Defect::DuplicateNode { id: "twin".into() }]
    );
}

#[test]
fn validation_order_is_deterministic() {
    // INV-3 reaches down here too. A defect list that reorders between runs
    // cannot be diffed across generations, and diffing it across generations
    // is the entire point of the barcode.
    let build = || {
        let mut g = clean();
        add_dangling(&mut g);
        add_duplicate(&mut g);
        add_starved(&mut g);
        add_unbounded(&mut g);
        add_orphan_source(&mut g);
        add_sink(&mut g);
        g.validate()
    };
    assert_eq!(build(), build());
}
