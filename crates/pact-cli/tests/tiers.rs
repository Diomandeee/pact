//! P5.3 — most checks are structural, and the ones that are not say why.
//!
//! The gate: measure the share the engine answered alone, and log every
//! escalation with its reason. **An evaluator that silently calls a model for
//! everything is the failure this tier exists to prevent** — it works, it is
//! slow, and it stops being reproducible, because the answers are no longer
//! derivable from the corpus.

use pact_corpus::{default_root, factor_all, load};
use std::process::Command;

fn corpus() -> Option<Vec<pact_corpus::Skill>> {
    let skills = load(&default_root()).ok()?;
    (!skills.is_empty()).then_some(skills)
}

fn run() -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_pact"))
        .arg("tiers")
        .output()
        .expect("the binary runs");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn the_engine_answers_the_overwhelming_majority() {
    // The number the gate is about. Not a target to tune toward: a floor, so
    // that a release where the engine quietly stopped answering shows up as a
    // failure rather than as a slower run.
    let text = run();
    if text.is_empty() {
        println!("SKIPPED: no corpus");
        return;
    }

    let share: u32 = text
        .lines()
        .find_map(|l| l.trim().split('%').next()?.trim().parse().ok())
        .expect("the share is reported");

    assert!(
        share >= 80,
        "only {share}% answered from structure. Either the corpus changed \
         shape or the tier is collapsing, and the second is what this floor \
         is for:\n{text}"
    );
    println!("engine answered {share}% alone");
}

#[test]
fn every_escalation_carries_a_reason() {
    // An escalation with no reason is indistinguishable from a lazy default,
    // and the point of the tier is being able to audit what left.
    use pact_cli_tiers::{interrogate, tally, Tier};

    let Some(skills) = corpus() else {
        println!("SKIPPED: no corpus");
        return;
    };
    let questions = interrogate(&skills, &factor_all(&skills));

    for q in &questions {
        match q.tier {
            Tier::Engine => assert!(
                q.answer.is_some(),
                "{}: the engine claimed it and answered nothing",
                q.about
            ),
            Tier::Model | Tier::Agent => {
                assert!(
                    q.reason.as_ref().is_some_and(|r| !r.is_empty()),
                    "{}: escalated with no reason",
                    q.about
                );
                assert!(
                    q.answer.is_none(),
                    "{}: escalated and answered anyway",
                    q.about
                );
            }
        }
    }

    let counted = tally(&questions);
    assert_eq!(
        counted.total(),
        questions.len(),
        "the tally must be complete"
    );
}

#[test]
fn a_structural_question_never_escalates() {
    // The specific claim. Typechecking and stage presence are decidable from
    // signatures; if either ever escalated, a reproducible fact would have
    // become an opinion.
    use pact_cli_tiers::{interrogate, Tier};

    let Some(skills) = corpus() else {
        return;
    };
    for q in interrogate(&skills, &factor_all(&skills)) {
        if q.about.starts_with("does ") || q.about.starts_with("which stages") {
            assert_eq!(
                q.tier,
                Tier::Engine,
                "{} is structural and escalated anyway",
                q.about
            );
        }
    }
}

#[test]
fn a_question_about_quality_always_escalates() {
    // The other half, and the one that keeps the tier honest. If the engine
    // ever claimed to answer "is this worth running", it would be reporting a
    // judgment as a fact. Determinism buys the predicates and never the
    // judgment; that gap is permanent.
    use pact_cli_tiers::{interrogate, Tier};

    let Some(skills) = corpus() else {
        return;
    };
    let questions = interrogate(&skills, &factor_all(&skills));
    let quality: Vec<_> = questions
        .iter()
        .filter(|q| q.about.contains("worth running"))
        .collect();

    assert!(!quality.is_empty(), "the suite must ask at least one");
    for q in quality {
        assert_ne!(q.tier, Tier::Engine, "{}", q.about);
        assert!(q.reason.as_ref().is_some_and(|r| r.contains("judgment")));
    }
}

#[test]
fn the_report_names_every_reason_anything_left() {
    let text = run();
    if text.is_empty() {
        return;
    }
    assert!(text.contains("engine"), "{text}");
    assert!(text.contains("answered from structure alone"), "{text}");
    if text.contains("model      0") && text.contains("agent      0") {
        return;
    }
    assert!(
        text.contains("why anything left the engine"),
        "escalations happened and the report did not say why:\n{text}"
    );
}
