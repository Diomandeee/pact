//! P2.4 — railpulse as a word: it validates, it simulates, and it is not in
//! the engine.
//!
//! The protocol under test lives at `protocols/railpulse.rs` and is pulled in
//! by path. It is compiled into nothing that ships, which is the point: INV-7
//! keeps protocol concepts out of `pact-core`, and the same reasoning carried
//! all the way out keeps railpulse out of `pact-algebra` too. railpulse is
//! Mohamed's protocol; it is not the platform's.
//!
//! Three claims, and the third is the one that is easy to assert and hard to
//! keep:
//!
//!   1. the word compiles to a graph the protocol accepts
//!   2. that graph runs, conserves, and work goes round the loop
//!   3. the name appears in no crate's source outside a doc comment

#[path = "../../../protocols/railpulse.rs"]
mod railpulse;

use pact_algebra::{Protocol, Rule, Term, MAX_TRACKS_PER_PHASE, MIN_VIABLE_TRANSITIONS};
use pact_core::{Bag, Event, Graph, Node, ResourceEdge, Signature, Simulator, Type};

fn ty(name: &str) -> Type {
    Type::new(name)
}

// ───────────────────────────────────────────────────────── it is a word

#[test]
fn the_word_is_the_canonical_cycle() {
    let word = railpulse::word(2);
    assert_eq!(
        word.word(),
        "↺(⊗ ▸ * ▸ ⊕)",
        "the rendering must match the notation the analysis already uses"
    );
    assert_eq!(
        word.stages().present(),
        4,
        "all four stages: {}",
        word.stages()
    );
    assert!(word.stages().missing().is_empty());
}

#[test]
fn the_cycle_closes_only_because_the_select_stage_changes_type() {
    // The finding that came out of writing this. `⊕` originally forced its
    // input and output to the same type, which reads fine until you try to
    // close a loop: the select stage has to return what the fan-out reads, so
    // it is the compaction as well as the merge. A same-type fan-in cannot
    // express railpulse at all.
    let closing = Term::merge("compact", 2, railpulse::RESULT, railpulse::PLAN)
        .expect("two results into one plan");
    assert!(closing.ports().reaches(&ty(railpulse::PLAN)));

    let same_type = Term::merge("fan_in", 2, railpulse::RESULT, railpulse::RESULT)
        .expect("two results into one result");
    let open = Term::sequence(vec![
        Term::broadcast(vec![
            Term::atom("a", Signature::simple(railpulse::PLAN, railpulse::RESULT)).unwrap(),
            Term::atom("b", Signature::simple(railpulse::PLAN, railpulse::RESULT)).unwrap(),
        ])
        .unwrap(),
        same_type,
    ])
    .expect("composes");

    let refused = open
        .feedback()
        .expect_err("a same-type merge leaves the cycle open");
    assert!(
        refused.to_string().contains("↺ does not close"),
        "{refused}"
    );
}

#[test]
fn the_word_refuses_a_width_the_dimension_cap_forbids() {
    // I2 is a dimension cap, not a suggestion: k tracks implies dim K <= k-1,
    // and it is what keeps a phase hand-checkable.
    for legal in MIN_VIABLE_TRANSITIONS..=MAX_TRACKS_PER_PHASE {
        let _ = railpulse::word(legal);
    }
    assert!(std::panic::catch_unwind(|| railpulse::word(MAX_TRACKS_PER_PHASE + 1)).is_err());
    assert!(std::panic::catch_unwind(|| railpulse::word(1)).is_err());
}

// ───────────────────────────────────────────────────────── it validates

#[test]
fn the_word_compiles_to_a_graph_the_protocol_accepts() {
    // Claim 1, and the whole reason both halves of L1 exist. The word is the
    // authored side and the protocol is the checking side, and they were
    // written against the same vocabulary without either being derived from
    // the other.
    for tracks in MIN_VIABLE_TRANSITIONS..=MAX_TRACKS_PER_PHASE {
        let built = railpulse::word(tracks).compile();
        railpulse::protocol()
            .validate(&built.graph)
            .unwrap_or_else(|broken| {
                panic!(
                    "{tracks} tracks: {}",
                    broken
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join("; ")
                )
            });
    }
}

#[test]
fn a_word_missing_any_stage_fails_the_protocol() {
    // Without this the test above is satisfied by a protocol that accepts
    // whatever the compiler emits. Each degenerate word drops exactly one
    // stage and must fail exactly that rule.
    let plan = railpulse::PLAN;
    let result = railpulse::RESULT;
    let track = |i: usize| {
        Term::atom(format!("track{i}"), Signature::simple(plan, result)).expect("well formed")
    };

    // No ⊗: a single track, so nothing fans out.
    let no_expand = Term::sequence(vec![
        track(0),
        Term::atom("pulse", Signature::simple(result, result))
            .expect("well formed")
            .iterate(),
        Term::merge("compact", 2, result, plan).expect("merge"),
    ])
    .expect("composes")
    .feedback()
    .expect("closes");

    let broken = railpulse::protocol()
        .validate(&no_expand.compile().graph)
        .expect_err("no fan-out");
    let named: Vec<&str> = broken.iter().map(|v| v.rule).collect();
    assert!(named.contains(&"expands"), "{named:?}");
    // And `Repeats` goes with it, because the cycle has no fan-out to close
    // through. Those are two rules failing for one cause, which is honest:
    // they are different claims that happen to share a prerequisite.
    assert!(named.contains(&"repeats"), "{named:?}");

    // No *: nothing iterates.
    let no_execute = Term::sequence(vec![
        Term::broadcast(vec![track(0), track(1)]).expect("⊗"),
        Term::merge("compact", 2, result, plan).expect("merge"),
    ])
    .expect("composes")
    .feedback()
    .expect("closes");
    let broken = railpulse::protocol()
        .validate(&no_execute.compile().graph)
        .expect_err("nothing iterates");
    assert_eq!(
        broken.iter().map(|v| v.rule).collect::<Vec<_>>(),
        vec!["executes"],
        "exactly the missing stage: {broken:?}"
    );

    // No ↺: the loop is left open.
    let no_repeat = Term::sequence(vec![
        Term::broadcast(vec![track(0), track(1)]).expect("⊗"),
        Term::atom("pulse", Signature::simple(result, result))
            .expect("well formed")
            .iterate(),
        Term::merge("compact", 2, result, plan).expect("merge"),
    ])
    .expect("composes");
    let broken = railpulse::protocol()
        .validate(&no_repeat.compile().graph)
        .expect_err("the loop is open");
    assert_eq!(
        broken.iter().map(|v| v.rule).collect::<Vec<_>>(),
        vec!["repeats"]
    );
}

#[test]
fn the_protocols_own_witness_is_a_railpulse() {
    // INV-4 applied to the protocol that matters most. The witness is also the
    // answer to "show me the smallest thing this would accept", which is the
    // first question anyone reading the protocol has.
    let protocol = railpulse::protocol();
    let witness = protocol.emit().expect("the canonical cycle is satisfiable");
    protocol.validate(&witness).expect("and it accepts its own");
}

// ───────────────────────────────────────────────────────── it simulates

#[test]
fn the_word_runs_and_work_goes_round_the_loop() {
    // Claim 2. A protocol that validates a graph the engine cannot drive is
    // checking a diagram, not a system.
    let built = railpulse::word(2).compile();
    let mut graph: Graph = built.graph;

    // A subject enters at the fan-out. Theorem 1 says import is the only thing
    // that extends a loop's life, so the source is the only way in and it is
    // deliberately explicit.
    graph.add(Node::source("subject", Bag::of(railpulse::PLAN, 2)));
    graph.connect(ResourceEdge::all("subject", built.entry.clone()));

    let mut sim = Simulator::new(&graph, 1);
    let opening = sim.total_held();
    let (trace, ledger) = sim.run(&graph, 12).expect("the canonical cycle must run");

    let converted = trace
        .events()
        .filter(|e| matches!(e, Event::Converted { .. }))
        .count();
    assert!(converted > 0, "the tracks must actually fire");

    assert!(
        trace.events().any(|e| matches!(e, Event::Copied { .. })),
        "the fan-out must hand every track a full copy"
    );
    assert!(
        trace.events().any(|e| matches!(e, Event::Held { .. })),
        "the iterating effector must hold work"
    );
    assert!(
        trace.events().any(|e| matches!(e, Event::Released { .. })),
        "and release it, or the iteration never completes a pass"
    );
    assert!(
        ledger.conserved(&opening, &sim.total_held()).is_ok(),
        "a cycle that leaks is a cycle whose numbers mean nothing"
    );
}

#[test]
fn the_select_stage_fires_so_the_loop_actually_closes() {
    // The strongest simulation claim, and the one that would have caught a
    // cycle that is closed on paper and starved in practice. `⊕` needs two
    // results to fire; if the fan-out and the iteration never deliver two, the
    // loop is a diagram of a loop.
    let built = railpulse::word(2).compile();
    let mut graph: Graph = built.graph;
    graph.add(Node::source("subject", Bag::of(railpulse::PLAN, 2)));
    graph.connect(ResourceEdge::all("subject", built.entry.clone()));

    let mut sim = Simulator::new(&graph, 1);
    let (trace, _) = sim.run(&graph, 24).expect("must run");

    let compactions = trace
        .events()
        .filter(|e| match e {
            Event::Converted { at, .. } => at.contains("compact"),
            _ => false,
        })
        .count();
    assert!(
        compactions > 0,
        "the select stage never fired: the cycle closes on paper and starves in practice"
    );
}

#[test]
fn a_run_is_reproducible() {
    // INV-3, applied to the protocol whose traces the barcode will compare
    // across generations.
    let render = |seed: u64| {
        let built = railpulse::word(3).compile();
        let mut graph: Graph = built.graph;
        graph.add(Node::source("subject", Bag::of(railpulse::PLAN, 3)));
        graph.connect(ResourceEdge::all("subject", built.entry.clone()));
        let mut sim = Simulator::new(&graph, seed);
        let (trace, _) = sim.run(&graph, 10).expect("must run");
        format!("{trace:?}")
    };
    assert_eq!(render(7), render(7));
}

// ───────────────────────────────────────────────────────── it is not in the engine

#[test]
fn no_crate_names_this_protocol_outside_a_doc_comment() {
    // Claim 3, and the one that is easy to assert and hard to keep. INV-7's
    // canary covers `pact-core`; this covers every crate, because a railpulse
    // baked into the algebra would be privileged over every other protocol the
    // algebra could host, which is the same failure one layer up.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root")
        .join("crates");

    let mut offenders = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            // Tests are exempt: this file is one, and it exists to name the
            // thing it is keeping out.
            if path.components().any(|c| c.as_os_str() == "tests") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for (n, line) in text.lines().enumerate() {
                let code = line.trim();
                // Doc comments are exempt, because explaining WHY the algebra
                // has no railpulse belongs in the algebra.
                if code.starts_with("//") {
                    continue;
                }
                if code.to_lowercase().contains("railpulse") {
                    offenders.push(format!("{}:{}: {code}", path.display(), n + 1));
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "railpulse leaked into shipped code:\n  {}",
        offenders.join("\n  ")
    );
}

#[test]
fn the_protocol_uses_only_the_public_vocabulary() {
    // What makes "the platform hosts many protocols" a fact rather than a
    // slogan. Anything railpulse can express, someone else's protocol can
    // express, because railpulse has no access this file does not.
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../protocols/railpulse.rs"
    ))
    .expect("the protocol must be readable");

    for reaching in [
        "pact_algebra::protocol::",
        "pact_core::interpret::",
        "(crate)",
    ] {
        assert!(
            !source.contains(reaching),
            "railpulse reaches past the public API via `{reaching}`"
        );
    }

    // And it is built from Rules, like anything else would be.
    let rules = railpulse::protocol().rules().len();
    assert_eq!(rules, 8, "eight declared rules");
    assert!(
        railpulse::protocol()
            .rules()
            .iter()
            .all(|r| !r.statement().is_empty()),
        "every rule must be able to say what it wants"
    );
    let _ = Protocol::new("someone else's").require(Rule::Expands);
}
