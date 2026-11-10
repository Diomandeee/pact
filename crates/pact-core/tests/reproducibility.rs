//! P1.7 — INV-3, same seed, same trace, byte for byte.
//!
//! The barcode compares traces across generations. A trace that reorders
//! between runs cannot be diffed, so the comparison would be measuring the
//! iteration order of a hash map and reporting it as a change in structure.
//! That is the specific wrong answer this gate exists to make impossible.
//!
//! "Byte for byte" is taken literally: the check is on the rendered trace, not
//! only on `PartialEq`. Two traces can compare equal under a derived `Eq` and
//! still render differently if anything in them iterates a set, and the
//! rendering is what a human diffs.

mod common;

use common::{random_graph, Gen};
use pact_core::{Bag, Distribution, Graph, Node, ResourceEdge, Simulator, Trace};

const CASES: u64 = 1_000;
const STEPS: usize = 6;

fn run(case: u64, seed: u64) -> Trace {
    let mut gen = Gen::new(case);
    let graph = random_graph(&mut gen);
    let mut sim = Simulator::new(&graph, seed);
    let (trace, _) = sim.run(&graph, STEPS).expect("the run must not fail");
    trace
}

#[test]
fn the_same_seed_gives_the_same_trace() {
    let mut differing = Vec::new();
    for case in 0..CASES {
        let seed = case | 1;
        let first = run(case, seed);
        let second = run(case, seed);

        if first != second {
            differing.push(format!("case {case}: traces differ under equality"));
            continue;
        }
        if format!("{first:?}") != format!("{second:?}") {
            differing.push(format!("case {case}: traces render differently"));
        }
    }
    assert!(
        differing.is_empty(),
        "{} of {CASES} case(s) were not reproducible:\n{}",
        differing.len(),
        differing.join("\n")
    );
}

#[test]
fn the_seed_actually_does_something() {
    // Without this the gate above is satisfied by an engine that ignores its
    // seed entirely, which is reproducible in the way a constant is
    // reproducible. At least one probabilistic graph has to route differently
    // under a different seed.
    let mut g = Graph::new();
    g.add(Node::pool_with("src", Bag::of("M", 40)))
        .add(Node::gate("g", Distribution::Probabilistic(vec![1, 1])))
        .add(Node::pool("t1"))
        .add(Node::pool("t2"));
    g.connect(ResourceEdge::all("src", "g"))
        .connect(ResourceEdge::all("g", "t1"))
        .connect(ResourceEdge::all("g", "t2"));

    let render = |seed: u64| {
        let mut sim = Simulator::new(&g, seed);
        let (trace, _) = sim.run(&g, 1).expect("the run must not fail");
        format!("{trace:?}")
    };

    assert_ne!(
        render(1),
        render(0xDEAD_BEEF),
        "two seeds produced identical routing over forty draws"
    );
    assert_eq!(render(1), render(1), "and one seed is still stable");
}

#[test]
fn a_deterministic_graph_ignores_the_seed() {
    // The other half of the same claim. A graph with no probabilistic choice
    // must produce one trace regardless of seed, or something is consulting
    // the PRNG that should not be, and the run would look stochastic to
    // anyone trying to reproduce it.
    let mut g = Graph::new();
    g.add(Node::source("s", Bag::of("M", 2)))
        .add(Node::pool("p"))
        .add(Node::gate("g", Distribution::Deterministic))
        .add(Node::pool("t1"))
        .add(Node::pool("t2"))
        .add(Node::drain("d"));
    g.connect(ResourceEdge::all("s", "p"))
        .connect(ResourceEdge::all("p", "g"))
        .connect(ResourceEdge::all("g", "t1"))
        .connect(ResourceEdge::all("g", "t2"))
        .connect(ResourceEdge::all("t1", "d"));

    let render = |seed: u64| {
        let mut sim = Simulator::new(&g, seed);
        let (trace, _) = sim.run(&g, 5).expect("the run must not fail");
        format!("{trace:?}")
    };

    assert_eq!(render(1), render(9_999_999));
}

#[test]
fn holdings_iterate_in_a_fixed_order() {
    // The concrete reason `BTreeMap` is load-bearing rather than a style
    // preference. Built two different ways, the same contents must render the
    // same, or a trace's insertion history leaks into its identity.
    let forwards = {
        let mut g = Graph::new();
        for id in ["a", "b", "c", "d"] {
            g.add(Node::pool_with(id, Bag::of("M", 1)));
        }
        Simulator::new(&g, 1).total_held()
    };
    let backwards = {
        let mut g = Graph::new();
        for id in ["d", "c", "b", "a"] {
            g.add(Node::pool_with(id, Bag::of("M", 1)));
        }
        Simulator::new(&g, 1).total_held()
    };

    assert_eq!(format!("{forwards:?}"), format!("{backwards:?}"));
}

#[test]
fn a_trace_is_diffable_across_generations() {
    // What reproducibility is FOR. Two runs of a graph that differ by one
    // edge must differ in their rendering in a way a diff can localise, not
    // by being wholly reordered.
    let build = |extra: bool| {
        let mut g = Graph::new();
        g.add(Node::pool_with("src", Bag::of("M", 4)))
            .add(Node::drain("d1"))
            .add(Node::drain("d2"));
        g.connect(ResourceEdge::new("src", "d1", 1));
        if extra {
            g.connect(ResourceEdge::new("src", "d2", 1));
        }
        let mut sim = Simulator::new(&g, 1);
        let (trace, _) = sim.run(&g, 2).expect("the run must not fail");
        trace
    };

    let without = build(false);
    let with = build(true);

    assert_ne!(without, with, "adding an edge must change the trace");
    assert!(
        with.events().count() > without.events().count(),
        "and it must change it by adding, not by scrambling"
    );

    // The shared prefix survives. If it did not, every generation would look
    // like a total rewrite and the barcode would report births and deaths for
    // structures that never changed.
    let shared = without
        .events()
        .zip(with.events())
        .take_while(|(a, b)| a == b)
        .count();
    assert!(
        shared > 0,
        "the two traces share no prefix at all, so nothing can be localised"
    );
}
