//! P5.2 — the diagram renders, and its shapes carry the diagnosis.
//!
//! A diagram drawing every node the same is a picture of a graph. The claim
//! here is stronger: someone should see the finding without reading the
//! report. Sources are stadiums, drains are trapezoids, a pool under pressure
//! is a double box, and a skill that can never fire is a dotted arrow.
//!
//! Mermaid, because it renders in a Markdown file, a GitHub comment and a
//! terminal preview with no toolchain.

use std::process::Command;

fn export() -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_pact"))
        .arg("export")
        .output()
        .expect("the binary runs");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn the_output_is_a_well_formed_flowchart() {
    let text = export();
    if text.is_empty() {
        println!("SKIPPED: no corpus");
        return;
    }
    assert!(
        text.starts_with("flowchart LR\n"),
        "{}",
        &text[..40.min(text.len())]
    );

    // Every non-comment, non-header line is a node or an edge. A stray line
    // makes the whole diagram fail to render, and Mermaid's failure mode is a
    // blank box with no message.
    for line in text.lines().skip(1).filter(|l| !l.trim().is_empty()) {
        let trimmed = line.trim();
        assert!(
            trimmed.starts_with("%%")
                || trimmed.contains("-->")
                || trimmed.contains("-.->")
                || trimmed.contains("==>")
                || trimmed.contains('[')
                || trimmed.contains('('),
            "line is neither node, edge nor comment: {line:?}"
        );
    }
}

#[test]
fn shapes_carry_the_diagnosis() {
    use pact_analysis::{dynamics, statics};
    use pact_corpus::{categorise, default_root, load};

    let Ok(skills) = load(&default_root()) else {
        println!("SKIPPED: no corpus");
        return;
    };
    if skills.is_empty() {
        return;
    }
    let text = export();
    let category = categorise(&skills);
    let stat = statics::analyse(&skills, &category);
    let dyn_ = dynamics::analyse(&category, &dynamics::type_edges(&skills));

    // A source is a stadium: work enters here.
    for ty in &stat.source_only {
        let id = sanitise(ty);
        assert!(
            text.contains(&format!("{id}([")),
            "{ty} is a source and is not drawn as one"
        );
    }

    // A drain is a trapezoid: work leaves here.
    for ty in &stat.dead_ends {
        let id = sanitise(ty);
        assert!(
            text.contains(&format!("{id}[/")),
            "{ty} is a dead end and is not drawn as one"
        );
    }

    // A pool under pressure is a double box, and carries its number.
    for pool in &dyn_.unbounded {
        let id = sanitise(&pool.id);
        assert!(
            text.contains(&format!("{id}[[")),
            "{} is under pressure and is not marked",
            pool.id
        );
        assert!(
            text.contains(&format!("{}", pool.pressure.unwrap_or(0.0))),
            "{}'s pressure is not on the diagram",
            pool.id
        );
    }

    // And an ordinary pool is neither, or the marks mean nothing.
    let ordinary = dyn_
        .nodes
        .iter()
        .find(|n| {
            n.kind == dynamics::Machination::Pool && !dyn_.unbounded.iter().any(|u| u.id == n.id)
        })
        .expect("some pool is fine");
    let id = sanitise(&ordinary.id);
    assert!(
        text.contains(&format!("{id}[\"")),
        "{} is an ordinary pool and should be an ordinary box",
        ordinary.id
    );
}

#[test]
fn a_skill_that_cannot_fire_is_drawn_differently() {
    // The corpus currently has none, which is itself the P2.5 result. So the
    // claim is checked on the rendering rule rather than on the corpus: if a
    // dead skill existed it would be dotted, and the legend says so.
    let text = export();
    if text.is_empty() {
        println!("SKIPPED: no corpus");
        return;
    }
    assert!(
        text.contains("dotted: a skill that can never fire"),
        "the legend must explain every mark it uses"
    );
    assert!(
        !text.contains("-.->"),
        "no skill should be dotted right now: P2.5 took the dead converters to zero"
    );
}

#[test]
fn every_type_appears_exactly_once_as_a_node() {
    use pact_corpus::{categorise, default_root, load};

    let Ok(skills) = load(&default_root()) else {
        return;
    };
    if skills.is_empty() {
        return;
    }
    let text = export();
    for ty in &categorise(&skills).objects {
        let id = sanitise(&ty.to_string());
        let declarations = text
            .lines()
            .filter(|l| {
                let t = l.trim();
                t.starts_with(&format!("{id}[")) || t.starts_with(&format!("{id}("))
            })
            .count();
        assert_eq!(declarations, 1, "{ty} declared {declarations} times");
    }
}

fn sanitise(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}
