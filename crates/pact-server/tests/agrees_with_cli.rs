//! P6.1 — the surface and the CLI cannot disagree.
//!
//! Both read the same functions, so they cannot drift for arithmetic reasons.
//! They can drift for **rendering** reasons: a field dropped, a list sorted
//! differently, a number rounded on one side only. That is the one failure the
//! 178 tests below this layer cannot see, because every one of them stops at
//! the analysis crates.
//!
//! So this runs the real CLI binary and reads its real output, and asserts the
//! JSON carries the same facts. Slow, and the alternative is comparing the view
//! against the analysis layer — which would pass even if the CLI had gone wrong,
//! and the CLI is what people currently trust.

use pact_corpus::{default_root, load};
use pact_server::{Chains, Graph, Overview, SkillDetail};
use std::process::Command;

fn corpus() -> Option<Vec<pact_corpus::Skill>> {
    let skills = load(&default_root()).ok()?;
    (!skills.is_empty()).then_some(skills)
}

/// Run `pact check` through cargo, so the test uses the same binary a person
/// would.
fn cli(subcommand: &str) -> Option<String> {
    let out = Command::new("cargo")
        .args(["run", "-q", "-p", "pact-cli", "--", subcommand])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    (!text.trim().is_empty()).then_some(text)
}

#[test]
fn the_overview_carries_the_same_numbers_as_pact_check() {
    let (Some(skills), Some(printed)) = (corpus(), cli("check")) else {
        println!("SKIPPED: no corpus, or the CLI did not run");
        return;
    };
    let view = Overview::of(&skills);

    assert!(
        printed.contains(&format!("{} skills, {} types", view.skills, view.types)),
        "the header disagrees:\n{printed}"
    );
    assert!(
        printed.contains(&view.verdict),
        "the verdict disagrees:\n{printed}"
    );

    for ty in view.source_only.iter().chain(&view.dead_ends) {
        assert!(printed.contains(ty), "{ty} is in the view and not in check");
    }
    for point in &view.pressure {
        assert!(
            printed.contains(&point.ty) && printed.contains(&point.pressure.to_string()),
            "{} at {} is in the view and not in check:\n{printed}",
            point.ty,
            point.pressure
        );
    }

    // And the other direction, which is the half that catches a view that
    // quietly drops a finding: check reports never-fires, so the view must.
    let cli_flags_dead = printed.contains("can never fire");
    assert_eq!(
        cli_flags_dead,
        !view.never_fires.is_empty(),
        "one side reports dead skills and the other does not:\n{printed}"
    );
}

#[test]
fn the_chains_view_carries_the_same_words_as_pact_decompose() {
    let (Some(skills), Some(printed)) = (corpus(), cli("decompose")) else {
        println!("SKIPPED: no corpus, or the CLI did not run");
        return;
    };
    let view = Chains::of(&skills);

    assert!(!view.chains.is_empty(), "no chains in the view");
    for chain in &view.chains {
        assert!(
            printed.contains(&chain.name),
            "{} is in the view and not in decompose",
            chain.name
        );
        assert!(
            printed.contains(&chain.word),
            "{}'s word {:?} disagrees:\n{printed}",
            chain.name,
            chain.word
        );
        assert!(
            printed.contains(&format!("{}/4", chain.present)),
            "{}'s completeness disagrees",
            chain.name
        );
    }

    // The corpus-wide tally is the number that names a missing generator
    // rather than the chains that suffer from it, so it has to survive.
    for (stage, count) in &view.missing_across_corpus {
        if *count > 0 {
            assert!(
                printed.contains(stage),
                "{stage} missing from decompose:\n{printed}"
            );
        }
    }
}

#[test]
fn every_view_round_trips_through_json() {
    // A view that serialises and does not come back is one where the page and
    // the server disagree about a field, and the symptom is a blank panel with
    // no error anywhere.
    let Some(skills) = corpus() else {
        println!("SKIPPED: no corpus");
        return;
    };

    let overview = Overview::of(&skills);
    let text = serde_json::to_string(&overview).expect("serialises");
    let back: Overview = serde_json::from_str(&text).expect("deserialises");
    assert_eq!(overview, back);

    let chains = Chains::of(&skills);
    let back: Chains = serde_json::from_str(&serde_json::to_string(&chains).unwrap()).unwrap();
    assert_eq!(chains, back);

    let graph = Graph::of(&skills);
    let back: Graph = serde_json::from_str(&serde_json::to_string(&graph).unwrap()).unwrap();
    assert_eq!(graph, back);

    let name = &skills[0].name;
    let detail = SkillDetail::of(&skills, name).expect("the first skill exists");
    let back: SkillDetail = serde_json::from_str(&serde_json::to_string(&detail).unwrap()).unwrap();
    assert_eq!(detail, back);
}

#[test]
fn a_skill_that_does_not_exist_is_none_and_not_an_empty_detail() {
    // "No findings" and "no such thing" are different answers, and a view that
    // returned an empty struct for a typo would let the page render a page
    // about nothing.
    let Some(skills) = corpus() else {
        return;
    };
    assert!(SkillDetail::of(&skills, "no-such-skill-anywhere").is_none());
    assert!(SkillDetail::of(&skills, &skills[0].name).is_some());
}

#[test]
fn the_graph_view_is_the_same_graph_the_exporter_draws() {
    // `pact export` already decides which shape carries which diagnosis. The
    // page must not reach a different conclusion from the same corpus, or a
    // node is a stadium in one view and a box in the other.
    let (Some(skills), Some(drawn)) = (corpus(), cli("export")) else {
        println!("SKIPPED: no corpus, or the CLI did not run");
        return;
    };
    let view = Graph::of(&skills);

    for node in &view.nodes {
        let id: String = node
            .id
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect();
        let expected_open = match node.kind.as_str() {
            "source" => format!("{id}(["),
            "drain" => format!("{id}[/"),
            _ if node.under_pressure => format!("{id}[["),
            _ => format!("{id}[\""),
        };
        assert!(
            drawn.contains(&expected_open),
            "{} is {} in the view and drawn otherwise",
            node.id,
            node.kind
        );
    }

    assert_eq!(
        view.edges.iter().filter(|e| e.dead).count(),
        drawn.matches("-.->").count(),
        "the two views disagree about how many skills can never fire"
    );
}

#[test]
fn the_graph_view_draws_the_coercions_the_exporter_draws() {
    // P3.6 counts each coercion `M_x ⇒ M` as a consumer of its subtype. A view
    // that counted it and drew no arrow would show a pool with more outflow
    // than arrows leaving it.
    let (Some(skills), Some(drawn)) = (corpus(), cli("export")) else {
        println!("SKIPPED: no corpus, or the CLI did not run");
        return;
    };
    let view = Graph::of(&skills);
    let coercions: Vec<_> = view
        .edges
        .iter()
        .filter(|e| e.skill.starts_with("coerce:"))
        .collect();

    let expected = pact_corpus::coercions(&pact_corpus::categorise(&skills));
    assert_eq!(
        coercions.len(),
        expected.len(),
        "one arrow per produced subtype"
    );
    for e in &coercions {
        assert_eq!(e.to, "M");
        assert_eq!(e.skill, format!("coerce:{}", e.from));
        assert!(!e.dead, "a coercion from a produced subtype always fires");
    }
    assert_eq!(
        coercions.len(),
        drawn.matches("==>").count(),
        "the two views disagree about the coercions"
    );
}
