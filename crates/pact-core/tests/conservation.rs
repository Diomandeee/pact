//! P1.6 — INV-2, conservation, per type, over generated graphs.
//!
//! # Why this is not circular
//!
//! The ledger is built from the events the scheduler emits, so checking the
//! ledger against itself would prove nothing: the same code writes both sides.
//!
//! What makes this a real check is that it compares the ledger against
//! [`Simulator::total_held`], which is read off the backend's own maps and is
//! never touched by the event stream. Any mutation without a matching event,
//! or any event without a matching mutation, shows up as an imbalance. The two
//! sides are independent, and that is the whole argument.
//!
//! # Per type, not in aggregate
//!
//! The checklist says aggregate-only checking is a fail, and it is right. Four
//! `M_plan` becoming four `State` balances perfectly in total and is exactly
//! the defect worth catching. Every assertion here is per type because
//! `Ledger::conserved` is.

mod common;

use common::{random_graph, Gen, TYPES};
use pact_core::{Bag, Simulator, Type};

const CASES: u64 = 1_000;
const STEPS: usize = 6;

/// Run one generated graph and return what the case proved, or the failure.
fn run_case(case: u64) -> Result<Outcome, String> {
    let mut gen = Gen::new(case);
    let graph = random_graph(&mut gen);
    let mut sim = Simulator::new(&graph, case | 1);

    let opening = sim.total_held();
    let (trace, ledger) = sim
        .run(&graph, STEPS)
        .map_err(|e| format!("case {case}: the run failed: {e:?}"))?;
    let closing = sim.total_held();

    ledger
        .conserved(&opening, &closing)
        .map_err(|broken| format!("case {case}: conservation broken: {broken:?}"))?;

    Ok(Outcome {
        events: trace.events().count(),
        created: ledger.created().total(),
        destroyed: ledger.destroyed().total(),
        moved_anything: closing != opening || ledger.created().total() > 0,
    })
}

struct Outcome {
    events: usize,
    created: u64,
    destroyed: u64,
    moved_anything: bool,
}

#[test]
fn conservation_holds_over_a_thousand_generated_graphs() {
    let mut failures = Vec::new();
    let mut total_events = 0usize;
    let mut total_created = 0u64;
    let mut total_destroyed = 0u64;
    let mut lively = 0u64;

    for case in 0..CASES {
        match run_case(case) {
            Ok(outcome) => {
                total_events += outcome.events;
                total_created += outcome.created;
                total_destroyed += outcome.destroyed;
                if outcome.moved_anything {
                    lively += 1;
                }
            }
            // Collect rather than panic on the first, so one run names every
            // broken case instead of one at a time.
            Err(message) => failures.push(message),
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {CASES} case(s) failed:\n{}",
        failures.len(),
        failures
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );

    // A gate that cannot fail is not a gate, and a thousand inert graphs would
    // pass this trivially. These floors say the corpus of cases actually
    // exercised creation, destruction and movement.
    assert!(
        total_events > 1_000,
        "only {total_events} events across {CASES} cases: the generator is producing dead graphs"
    );
    assert!(
        total_created > 0 && total_destroyed > 0,
        "created {total_created}, destroyed {total_destroyed}: nothing entered or left the system"
    );
    assert!(
        lively > CASES / 4,
        "only {lively} of {CASES} cases changed anything at all"
    );
}

#[test]
fn conservation_is_checked_per_type_not_in_aggregate() {
    // The substitution an aggregate check waves through. Stated here as well
    // as in the seam tests, because this is the file whose name promises it.
    let ledger = pact_core::Ledger::new();
    let opening = Bag::of("M_plan", 4);
    let closing = Bag::of("State", 4);

    assert_eq!(opening.total(), closing.total());
    let broken = ledger
        .conserved(&opening, &closing)
        .expect_err("four of one becoming four of another is not conservation");
    assert_eq!(broken.len(), 2, "both types must be named: {broken:?}");
}

#[test]
fn the_check_catches_a_leak_that_the_event_stream_hides() {
    // The negative control, and the reason the two sides have to be
    // independent. Here the ledger is told the truth about creation and the
    // holdings disagree, which is precisely the shape of a mutation that
    // forgot to emit its event.
    let ledger = pact_core::Ledger::new();
    let opening = Bag::of("M", 3);
    let closing = Bag::of("M", 4);

    let broken = ledger
        .conserved(&opening, &closing)
        .expect_err("a resource appearing with no event must be caught");
    assert_eq!(broken.len(), 1);
    assert_eq!(broken[0].expected, 3);
    assert_eq!(broken[0].found, 4);
}

#[test]
fn every_generated_type_is_reachable() {
    // If the generator only ever produced one type, the per-type check would
    // be an aggregate check wearing a costume.
    let mut seen: Vec<&str> = Vec::new();
    for case in 0..200 {
        let mut gen = Gen::new(case);
        let graph = random_graph(&mut gen);
        let sim = Simulator::new(&graph, case | 1);
        for name in TYPES {
            if sim.total_held().count(&Type::new(name)) > 0 && !seen.contains(&name) {
                seen.push(name);
            }
        }
    }
    assert_eq!(
        seen.len(),
        TYPES.len(),
        "only {seen:?} of {TYPES:?} ever appeared"
    );
}

#[test]
fn work_in_flight_counts_as_held() {
    // A delay holding work has not destroyed it. A conservation check that
    // ignored in-flight work would report a leak for exactly as long as any
    // delay held anything, and the report would be wrong every time.
    use pact_core::{Graph, Node, ResourceEdge};

    let mut g = Graph::new();
    g.add(Node::pool_with("src", Bag::of("M", 2)))
        .add(Node::delay("w", 3))
        .add(Node::pool("sink"));
    g.connect(ResourceEdge::all("src", "w"))
        .connect(ResourceEdge::all("w", "sink"));

    let mut sim = Simulator::new(&g, 1);
    let opening = sim.total_held();
    let (_, ledger) = sim.run(&g, 1).expect("the run must not fail");

    assert_eq!(
        sim.state().get("w").map(Bag::total),
        Some(0),
        "the delay's holdings are empty while it holds"
    );
    assert_eq!(
        sim.total_held().count(&Type::new("M")),
        2,
        "and the work is still counted"
    );
    assert!(ledger.conserved(&opening, &sim.total_held()).is_ok());
}
