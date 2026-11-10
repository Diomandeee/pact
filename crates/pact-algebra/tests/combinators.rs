//! P2.1 — the six combinators, with typed ports.
//!
//! The gate as the checklist states it is one line: `▸` rejects a type
//! mismatch at construction, with the mismatch named. That is necessary and
//! not sufficient, for the same reason P1.3's was: a `sequence` that rejected
//! *everything* would pass it. So the gate here has four sides.
//!
//!   1. `▸` rejects a mismatch, and the error names both terms and both types
//!   2. `▸` accepts what should compose, including alternative outputs
//!   3. every other combinator's type rule is checked the same way
//!   4. ports propagate correctly through composition, and the compiled graph
//!      is structurally clean and actually runs
//!
//! (4) is what makes these combinators rather than symbols. A word that
//! typechecks and compiles to a graph the engine refuses is not composing
//! anything.

use pact_algebra::{Mismatch, PortProblem, Stages, Term};
use pact_core::{Bag, Distribution, Graph, Kind, Node, ResourceEdge, Signature, Simulator, Type};

fn skill(label: &str, from: &str, to: &str) -> Term {
    Term::atom(label, Signature::simple(from, to)).expect("a one-in one-out skill is well formed")
}

fn ty(name: &str) -> Type {
    Type::new(name)
}

// ───────────────────────────────────────────────────────── ▸ the gate

#[test]
fn sequence_rejects_a_type_mismatch_and_names_it() {
    // The gate, stated as the checklist states it.
    let plan = skill("rail", "M_arch", "M_plan");
    let compact = skill("compact", "State", "M_arch");

    let error = Term::sequence(vec![plan, compact]).expect_err("M_plan is not State");

    let Mismatch::Sequence {
        left,
        right,
        produces,
        wants,
    } = &error
    else {
        panic!("wrong variant: {error:?}");
    };
    assert_eq!(left, "rail");
    assert_eq!(right, "compact");
    assert_eq!(produces, &vec![ty("M_plan")]);
    assert_eq!(wants, &ty("State"));

    // Named in the rendered message too, because that is what a person reads.
    // An error that carries the facts in fields and hides them in Display has
    // only moved the problem.
    let message = error.to_string();
    for fragment in ["rail", "compact", "M_plan", "State"] {
        assert!(
            message.contains(fragment),
            "{fragment:?} missing from {message:?}"
        );
    }
}

#[test]
fn sequence_accepts_what_composes() {
    // Side (2). Without this, a `sequence` that rejected everything would pass
    // the gate above.
    let word = Term::sequence(vec![
        skill("rail", "M_arch", "M_plan"),
        skill("pulse", "M_plan", "State"),
    ])
    .expect("M_plan meets M_plan");

    assert_eq!(word.ports().input, ty("M_arch"));
    assert!(word.ports().reaches(&ty("State")));
    assert_eq!(word.word(), "· ▸ ·");
}

#[test]
fn an_alternative_output_is_enough_to_compose() {
    // SOOP-2 writes `output_type: A | B` and the live corpus has them.
    // Composition asks whether the next input is AMONG the outputs, not
    // whether it equals the only one. Treating the set as a single type is how
    // a legal chain gets reported as broken.
    let branching = skill("triage", "M", "Report").or_produces("M_plan");
    let downstream = skill("pulse", "M_plan", "State");

    let word = Term::sequence(vec![branching, downstream]).expect("M_plan is one of the outputs");
    assert_eq!(word.ports().input, ty("M"));

    // And the mismatch still fires when the wanted type is in neither.
    let unrelated = skill("archive", "M_arch", "State");
    let error = Term::sequence(vec![
        skill("triage", "M", "Report").or_produces("M_plan"),
        unrelated,
    ])
    .expect_err("M_arch is in neither output");
    let message = error.to_string();
    assert!(message.contains("Report") && message.contains("M_plan"));
}

#[test]
fn an_empty_sequence_is_refused() {
    let error = Term::sequence(vec![]).expect_err("nothing to sequence");
    assert_eq!(error, Mismatch::Empty { combinator: "▸" });
}

// ───────────────────────────────────────────────────────── the other rules

#[test]
fn broadcast_requires_one_upstream_and_names_the_branch_that_disagreed() {
    let ok = Term::broadcast(vec![
        skill("track_a", "M_plan", "State"),
        skill("track_b", "M_plan", "Report"),
    ])
    .expect("both read M_plan");
    assert_eq!(ok.ports().input, ty("M_plan"));

    // The union of outputs, not the intersection. A fan-out's result is
    // whatever any branch may emit; intersecting would report a fan-out over
    // two differently-typed branches as producing nothing at all.
    assert!(ok.ports().reaches(&ty("State")));
    assert!(ok.ports().reaches(&ty("Report")));

    let error = Term::broadcast(vec![
        skill("track_a", "M_plan", "State"),
        skill("track_b", "M_arch", "Report"),
    ])
    .expect_err("the branches read different types");

    let Mismatch::Parallel {
        combinator,
        first,
        first_input,
        other,
        other_input,
    } = &error
    else {
        panic!("wrong variant: {error:?}");
    };
    assert_eq!(*combinator, "⊗");
    assert_eq!(first, "track_a");
    assert_eq!(first_input, &ty("M_plan"));
    assert_eq!(other, "track_b");
    assert_eq!(other_input, &ty("M_arch"));
}

#[test]
fn choice_has_the_same_type_rule_and_a_different_name() {
    // Both require one upstream; they differ in what runs, not in what
    // composes. The error has to say which one was being built, or the fix
    // is a guess.
    let error = Term::choice(vec![
        skill("fast", "M_plan", "State"),
        skill("slow", "State", "State"),
    ])
    .expect_err("the branches read different types");

    assert!(error.to_string().starts_with('+'), "{error}");
}

#[test]
fn two_skills_reading_the_same_type_are_parallel_not_sequential() {
    // The rule that decides the whole factoring: types decide, not prose. Two
    // consecutive members that consume the same input are a fan-out however
    // the documentation describes them, and `▸` refusing them is what makes
    // that mechanical rather than a matter of opinion.
    let a = skill("analyse", "M_plan", "Report");
    let b = skill("critique", "M_plan", "Report");

    assert!(
        Term::sequence(vec![a.clone(), b.clone()]).is_err(),
        "▸ must refuse two readers of one type"
    );
    assert!(
        Term::broadcast(vec![a, b]).is_ok(),
        "⊗ is what they actually are"
    );
}

#[test]
fn feedback_refuses_a_loop_that_does_not_close() {
    // This is the live corpus's central gap made mechanical. With no
    // `State -> M_arch` generator, a rail-pulse chain ends holding `State`
    // while its first stage reads `M_arch`, so the cycle runs once and stops.
    // Theorem 1 says import is the only thing that extends a loop's life;
    // this is the type-level face of the same fact.
    let open = Term::sequence(vec![
        skill("rail", "M_arch", "M_plan"),
        skill("pulse", "M_plan", "State"),
    ])
    .expect("this part composes");

    let error = open
        .feedback()
        .expect_err("nothing returns State to M_arch");
    let Mismatch::Feedback {
        wants, produces, ..
    } = &error
    else {
        panic!("wrong variant: {error:?}");
    };
    assert_eq!(wants, &ty("M_arch"));
    assert_eq!(produces, &vec![ty("State")]);

    // And it closes the moment the missing generator exists. That generator is
    // P2.5, and this is the test that will go from proving the gap to proving
    // it was filled.
    let closed = Term::sequence(vec![
        skill("rail", "M_arch", "M_plan"),
        skill("pulse", "M_plan", "State"),
        skill("compact", "State", "M_arch"),
    ])
    .expect("with compact the chain composes")
    .feedback()
    .expect("and the loop closes");

    assert_eq!(closed.ports().input, ty("M_arch"));
    assert!(closed.stages().repeat);
}

#[test]
fn iterate_is_infallible_and_that_is_the_right_asymmetry() {
    // Iteration is one effector repeating its own work, so its ports are
    // unchanged by definition and there is nothing to check. Feedback is the
    // composition returning to its own start, which is a claim about types and
    // can therefore be false. Making `*` fallible would invent a check with no
    // content; making `↺` infallible would drop the one that matters.
    let effector = skill("pulse", "M_plan", "State");
    let looping = effector.clone().iterate();

    assert_eq!(looping.ports(), effector.ports());
    assert_eq!(looping.word(), "*");
    assert!(looping.stages().execute);
}

#[test]
fn merge_is_a_fan_in_and_refuses_an_arity_below_two() {
    let m = Term::merge("select", 4, "M", "M").expect("four into one");
    assert_eq!(m.ports().input, ty("M"));
    assert!(m.ports().reaches(&ty("M")));
    assert_eq!(m.word(), "⊕");
    assert!(m.stages().select);

    // Merging one thing is the identity wearing a fan-in's name, and a word
    // containing it would report a select stage that never selected anything.
    assert!(Term::merge("noop", 1, "M", "M").is_err());
    assert!(Term::merge("noop", 0, "M", "M").is_err());
}

#[test]
fn an_atom_must_have_one_input_port_and_at_least_one_output() {
    // A signature with no single input has no upstream to name, so it cannot
    // be sequenced. Both rejected shapes are real engine nodes — a source and
    // a drain — and neither is a skill.
    let no_input = Term::atom(
        "spring",
        Signature {
            consumes: Bag::new(),
            produces: Bag::of("M", 1),
        },
    )
    .expect_err("consumes nothing");
    assert!(matches!(
        no_input,
        Mismatch::Port {
            reason: PortProblem::NoInput,
            ..
        }
    ));

    let no_output = Term::atom(
        "sink",
        Signature {
            consumes: Bag::of("M", 1),
            produces: Bag::new(),
        },
    )
    .expect_err("produces nothing");
    assert!(matches!(
        no_output,
        Mismatch::Port {
            reason: PortProblem::NoOutput,
            ..
        }
    ));

    let two_inputs = Term::atom(
        "join",
        Signature {
            consumes: {
                let mut b = Bag::of("M", 1);
                b.add(ty("State"), 1);
                b
            },
            produces: Bag::of("M_arch", 1),
        },
    )
    .expect_err("two input types");
    let Mismatch::Port {
        reason: PortProblem::ManyInputs(types),
        ..
    } = &two_inputs
    else {
        panic!("wrong variant: {two_inputs:?}");
    };
    assert_eq!(types.len(), 2, "both are named: {types:?}");
}

// ───────────────────────────────────────────────────────── ports and laws

#[test]
fn sequence_is_associative_in_its_ports() {
    // (A ▸ B) ▸ C and A ▸ (B ▸ C) must present the same interface. If they did
    // not, a word's meaning would depend on how someone happened to bracket
    // it, and the algebra would not be one.
    let a = skill("a", "M_arch", "M_plan");
    let b = skill("b", "M_plan", "State");
    let c = skill("c", "State", "Report");

    let left = Term::sequence(vec![
        Term::sequence(vec![a.clone(), b.clone()]).expect("ab"),
        c.clone(),
    ])
    .expect("(ab)c");
    let right = Term::sequence(vec![a, Term::sequence(vec![b, c]).expect("bc")]).expect("a(bc)");

    assert_eq!(left.ports(), right.ports());
}

#[test]
fn then_flattens_so_a_chain_reads_as_one_sequence() {
    let word = skill("a", "M_arch", "M_plan")
        .then(skill("b", "M_plan", "State"))
        .expect("ab")
        .then(skill("c", "State", "Report"))
        .expect("abc");

    assert_eq!(word.word(), "· ▸ · ▸ ·", "three stages, not two nested");
}

#[test]
fn the_canonical_word_renders_as_the_analysis_writes_it() {
    // `railpulse = ↺(⊗ ▸ * ▸ ⊕)`. The rendering has to match the notation the
    // analysis already uses, or the differential test at P3.2 is comparing two
    // spellings rather than two factorings.
    let expand = Term::broadcast(vec![
        skill("track_a", "M_plan", "M"),
        skill("track_b", "M_plan", "M"),
    ])
    .expect("both read M_plan");
    let execute = skill("pulse", "M", "M").iterate();
    let select = Term::merge("compact", 2, "M", "M").expect("two into one");

    let cycle = Term::sequence(vec![expand, execute, select]).expect("the stages compose");
    assert_eq!(cycle.word(), "⊗ ▸ * ▸ ⊕");

    let closed = cycle.feedback().expect_err("M_plan never returns");
    assert!(closed.to_string().contains("↺"));
}

#[test]
fn stages_report_what_is_missing_not_just_how_many() {
    // A word missing a stage is degenerate, not broken, and WHICH stage it is
    // missing is the finding. "2/4" alone sends someone back to the diagram.
    let degenerate = Term::sequence(vec![
        skill("rail", "M_arch", "M_plan"),
        skill("pulse", "M_plan", "State"),
    ])
    .expect("composes");

    let stages = degenerate.stages();
    assert_eq!(stages.present(), 0);
    assert_eq!(
        stages.missing(),
        vec!["⊗ expand", "* execute", "⊕ select", "↺ repeat"]
    );
    assert_eq!(stages.to_string(), "0/4");

    assert_eq!(Stages::default().present(), 0);
}

// ───────────────────────────────────────────────────────── compilation

#[test]
fn a_sequence_compiles_to_a_chain_the_engine_accepts() {
    // What makes these combinators rather than symbols. A word that typechecks
    // and compiles to a graph the engine refuses is not composing anything.
    let word = Term::sequence(vec![
        skill("rail", "M_arch", "M_plan"),
        skill("pulse", "M_plan", "State"),
    ])
    .expect("composes");

    let built = word.compile();
    assert_eq!(built.graph.nodes().len(), 2);
    assert_eq!(built.exits.len(), 1);
    assert_ne!(built.entry, built.exits[0], "entry and exit are distinct");

    // No dangling edges, no duplicate ids. The starved and sink defects are
    // expected: a compiled word is an open fragment with nothing attached to
    // either end yet, which is exactly what `entry` and `exits` are for.
    let structural: Vec<_> = built
        .graph
        .validate()
        .into_iter()
        .filter(|d| {
            !matches!(
                d,
                pact_core::Defect::StarvedConverter { .. } | pact_core::Defect::Sink { .. }
            )
        })
        .collect();
    assert_eq!(structural, vec![], "a compiled word must be well formed");
}

#[test]
fn broadcast_and_choice_compile_to_different_gates() {
    // The distinction the engine already draws. If both compiled to the same
    // gate, a word could not express the difference between running every
    // branch and running one, and the whole `⊗` versus `+` split would be
    // notation with no consequence.
    let branches = || {
        vec![
            skill("a", "M_plan", "State"),
            skill("b", "M_plan", "Report"),
        ]
    };

    let fan = Term::broadcast(branches()).expect("⊗").compile();
    let pick = Term::choice(branches()).expect("+").compile();

    let distribution_of = |built: &pact_algebra::Compiled, id: &str| match &built
        .graph
        .node(id)
        .expect("the gate exists")
        .kind
    {
        Kind::Gate { distribution } => distribution.clone(),
        other => panic!("entry is not a gate: {other:?}"),
    };

    assert_eq!(
        distribution_of(&fan, &fan.entry),
        Distribution::Broadcast,
        "⊗ hands every branch a full copy"
    );
    assert_eq!(
        distribution_of(&pick, &pick.entry),
        Distribution::Deterministic,
        "+ splits rather than copies"
    );
    assert_eq!(fan.exits.len(), 2, "both branches are exits");
}

#[test]
fn iterate_compiles_to_a_delay_so_a_pass_costs_a_step() {
    // An iteration is a model doing work. A loop closed with no delay would
    // let the engine run an unbounded number of passes inside one step, which
    // is a claim about compute that nothing supports.
    let built = skill("pulse", "M", "M").iterate().compile();

    let delays = built
        .graph
        .nodes()
        .iter()
        .filter(|n| matches!(n.kind, Kind::Delay { .. }))
        .count();
    assert_eq!(delays, 1, "exactly one delay in the loop");

    // And the loop is closed: something reaches the entry.
    assert!(
        built
            .graph
            .incoming(&built.entry)
            .iter()
            .any(|(_, e)| e.to == built.entry),
        "the iteration must return to its own input"
    );
}

#[test]
fn a_compiled_word_runs() {
    // The end-to-end claim. Attach a source to the entry and a drain to the
    // exits, and the graph the algebra produced is a graph the engine drives
    // without complaint.
    let word = Term::sequence(vec![
        skill("rail", "M_arch", "M_plan"),
        skill("pulse", "M_plan", "State"),
    ])
    .expect("composes");

    let built = word.compile();
    let mut graph: Graph = built.graph;
    graph.add(Node::source("subject", Bag::of("M_arch", 1)));
    graph.connect(ResourceEdge::all("subject", built.entry.clone()));
    graph.add(Node::drain("report"));
    for exit in &built.exits {
        graph.connect(ResourceEdge::all(exit.clone(), "report"));
    }

    let mut sim = Simulator::new(&graph, 1);
    let opening = sim.total_held();
    let (trace, ledger) = sim.run(&graph, 6).expect("a compiled word must run");

    assert!(
        trace
            .events()
            .any(|e| matches!(e, pact_core::Event::Converted { .. })),
        "the skills must actually fire"
    );
    assert!(
        ledger.destroyed().count(&ty("State")) > 0,
        "and work must reach the far end as State"
    );
    assert!(
        ledger.conserved(&opening, &sim.total_held()).is_ok(),
        "a compiled word must not leak"
    );
}

#[test]
fn composition_namespaces_ids_so_a_repeated_skill_is_not_a_duplicate() {
    // The same skill used twice in one word is normal. Without a path prefix
    // the two would collide into a `DuplicateNode`, which reads as an
    // authoring mistake rather than the composition it is.
    let built = Term::broadcast(vec![
        skill("pulse", "M_plan", "State"),
        skill("pulse", "M_plan", "State"),
    ])
    .expect("both read M_plan")
    .compile();

    let duplicates: Vec<_> = built
        .graph
        .validate()
        .into_iter()
        .filter(|d| matches!(d, pact_core::Defect::DuplicateNode { .. }))
        .collect();
    assert_eq!(duplicates, vec![], "ids must be namespaced by path");
    assert_eq!(built.graph.nodes().len(), 3, "a gate and two branches");
}
