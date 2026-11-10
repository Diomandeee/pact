//! P3.3 — graph → word → graph, and the rebuild is isomorphic.
//!
//! This is the "deconstruct them, then mechanically build them into the new
//! program" requirement. The factoring says what a chain *is*; the rebuild
//! turns that back into something the algebra holds. Only doing both tells you
//! whether the decomposition captured the chain or merely described it.
//!
//! # What isomorphic means here
//!
//! Not graph equality: the imported corpus graph is bipartite (types are
//! pools, skills are converters) and a compiled word is skill-only, so they
//! are different objects on purpose. The claim is that **the skill layer
//! survives the round trip exactly**:
//!
//!   1. the same skills, as a multiset of signatures
//!   2. the same canonical stages
//!   3. re-factoring the rebuilt word gives the word we started from
//!
//! (3) is the closing of the loop. Without it, (1) and (2) are satisfied by a
//! rebuild that keeps the pieces and loses the structure.
//!
//! # Ten of thirteen chains cannot be rebuilt, and that is the correct answer
//!
//! The algebra refuses to hold an ill-typed word, so a chain that does not
//! typecheck has no word to hold. That is not a limitation of the rebuild; it
//! is the same fact the factoring already reported, arriving a second time
//! through an independent path. A rebuild that succeeded on a broken chain
//! would mean one of the two implementations is lying.

use pact_corpus::{default_root, factor, factor_all, load, rebuild, refactor, Skill};
use std::collections::BTreeMap;

fn corpus() -> Option<(Vec<Skill>, BTreeMap<String, Skill>)> {
    let skills = load(&default_root()).ok()?;
    if skills.is_empty() {
        return None;
    }
    let by_name = skills.iter().map(|s| (s.name.clone(), s.clone())).collect();
    Some((skills, by_name))
}

#[test]
fn every_chain_that_typechecks_rebuilds() {
    let Some((skills, by_name)) = corpus() else {
        println!("SKIPPED: no corpus");
        return;
    };
    let factorings = factor_all(&skills);
    let legal: Vec<_> = factorings.iter().filter(|f| f.typechecks).collect();

    assert!(
        !legal.is_empty(),
        "no chain typechecks, so the round trip has nothing to test"
    );

    for f in &legal {
        rebuild(f, &by_name).unwrap_or_else(|e| {
            panic!(
                "{} typechecks as a factoring but the algebra refuses it: {e}\n  \
                 the two implementations disagree, and that is the finding",
                f.chain
            )
        });
    }
    println!("{} of {} chains rebuilt", legal.len(), factorings.len());
}

#[test]
fn a_chain_that_does_not_typecheck_cannot_be_rebuilt() {
    // The other half, and the one that makes the first mean something. If a
    // broken chain rebuilt, the factoring and the algebra would disagree about
    // what composes, and one of them would be wrong.
    let Some((skills, by_name)) = corpus() else {
        println!("SKIPPED: no corpus");
        return;
    };
    let factorings = factor_all(&skills);
    let broken: Vec<_> = factorings.iter().filter(|f| !f.typechecks).collect();
    if broken.is_empty() {
        // Since 2026-11-03 every chain in the live corpus typechecks (finding
        // 0009), so this half has no corpus example. The refusal itself is
        // still proven, in isolation, by
        // `the_rebuild_refuses_a_chain_whose_types_do_not_meet` below — which
        // is why this is a loud skip and not a weakened assertion.
        println!("SKIPPED: every chain typechecks; the refusal is proven on a synthetic chain");
        return;
    }

    for f in &broken {
        // A broken factoring may still rebuild when the break is between two
        // members the grouping puts in the same fan-out, because a fan-out
        // does not require its branches to compose with each other. Anything
        // else must be refused.
        if let Ok(term) = rebuild(f, &by_name) {
            let regrouped = term.word();
            assert!(
                regrouped.contains('⊗'),
                "{}: rebuilt despite not typechecking, and not via a fan-out: {regrouped}\n  \
                 notes: {:?}",
                f.chain,
                f.notes
            );
        }
    }
}

#[test]
fn the_round_trip_preserves_the_skills_exactly() {
    // Claim 1. A rebuild that dropped a member, duplicated one, or changed a
    // signature would still compile and still look right in a diagram.
    let Some((skills, by_name)) = corpus() else {
        println!("SKIPPED: no corpus");
        return;
    };

    for f in factor_all(&skills).iter().filter(|f| f.typechecks) {
        let term = rebuild(f, &by_name).expect("typechecks, so it rebuilds");
        let built = term.compile();

        let mut want: Vec<String> = f
            .members
            .iter()
            .filter_map(|m| by_name.get(m))
            .map(|s| format!("{:?}", s.signature()))
            .collect();
        let mut got: Vec<String> = built
            .graph
            .nodes()
            .iter()
            .filter_map(|n| match &n.kind {
                pact_core::Kind::Converter { signature } => Some(format!("{signature:?}")),
                _ => None,
            })
            .collect();
        want.sort();
        got.sort();

        assert_eq!(
            want, got,
            "{}: the round trip changed the skill layer",
            f.chain
        );
    }
}

#[test]
fn the_round_trip_preserves_the_canonical_stages() {
    // Claim 2. The stages are what the whole analysis reports, so a rebuild
    // that lost one would make every downstream number about a different
    // chain than the one on disk.
    let Some((skills, by_name)) = corpus() else {
        println!("SKIPPED: no corpus");
        return;
    };

    for f in factor_all(&skills).iter().filter(|f| f.typechecks) {
        let term = rebuild(f, &by_name).expect("rebuilds");
        let stages = term.stages();

        assert_eq!(
            stages.expand, f.stages.expand,
            "{}: fan-out lost or invented in the rebuild",
            f.chain
        );
        assert_eq!(
            stages.execute, f.stages.execute,
            "{}: iteration lost or invented in the rebuild",
            f.chain
        );
    }
}

#[test]
fn re_factoring_the_rebuild_gives_the_same_word() {
    // Claim 3, and the one that closes the loop. Without it the first two are
    // satisfied by a rebuild that keeps every piece and loses the structure
    // holding them together.
    let Some((skills, by_name)) = corpus() else {
        println!("SKIPPED: no corpus");
        return;
    };

    for f in factor_all(&skills).iter().filter(|f| f.typechecks) {
        // Re-factor from what the algebra holds — the declared shape when the
        // chain has one, the member order otherwise — rather than from the
        // prose. The prose is where the oracle's weakness lives; the algebra
        // holds the structure, so this asks whether the structure survived.
        let again = refactor(f, &by_name);
        assert_eq!(
            again.word, f.word,
            "{}: the word changed on a second factoring",
            f.chain
        );
        assert_eq!(again.stages, f.stages, "{}: stages changed", f.chain);
        assert_eq!(again.typechecks, f.typechecks);
    }
}

#[test]
fn the_rebuild_refuses_a_chain_whose_types_do_not_meet() {
    // Isolated, so the corpus agreeing cannot hide a rebuild that never
    // refuses anything.
    use pact_core::Type;
    use std::path::PathBuf;

    let skill = |name: &str, inp: &str, out: &str| Skill {
        name: name.to_string(),
        input: Type::new(inp),
        outputs: vec![Type::new(out)],
        category: None,
        effects: None,
        idempotent: false,
        commutes_with: vec![],
        members: vec![],
        iterates: false,
        path: PathBuf::new(),
    };
    let by_name: BTreeMap<String, Skill> = [
        ("a".to_string(), skill("a", "M_arch", "M_plan")),
        ("b".to_string(), skill("b", "State", "M")),
    ]
    .into_iter()
    .collect();

    let names = ["a".to_string(), "b".to_string()];
    let f = factor("broken", &names, &by_name);
    assert!(!f.typechecks);

    let refused = rebuild(&f, &by_name).expect_err("M_plan is not State");
    let message = refused.to_string();
    assert!(
        message.contains("M_plan") && message.contains("State"),
        "{message}"
    );
}
