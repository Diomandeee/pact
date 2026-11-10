//! P5.1 — the surface produces the same numbers as the layers under it.
//!
//! The gate as written says "the same JSON as its Python predecessor". The
//! predecessors do not emit these views: `prompt_economy.py` reports an
//! economy and `decompose.py` reports a factoring, and neither prints a
//! `check` summary. So the comparison that means something is the one against
//! **the layers this CLI is a surface over**, which are themselves already
//! diffed against the Python at P3.1, P3.2, P4.1 and P4.2.
//!
//! Stated rather than quietly substituted: the chain is CLI → analysis →
//! oracle, and this file tests the first link. A CLI that printed different
//! numbers than the crate it calls would be the one failure the lower gates
//! cannot see.

use std::process::Command;

fn run(args: &[&str]) -> (String, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_pact"))
        .args(args)
        .output()
        .expect("the binary runs");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn check_reports_what_the_analysis_layer_reports() {
    use pact_analysis::{dynamics, statics};
    use pact_corpus::{categorise, default_root, load};

    let Ok(skills) = load(&default_root()) else {
        println!("SKIPPED: no corpus");
        return;
    };
    if skills.is_empty() {
        println!("SKIPPED: corpus empty");
        return;
    }

    let (stdout, code) = run(&["check"]);
    let category = categorise(&skills);
    let stat = statics::analyse(&skills, &category);
    let dyn_ = dynamics::analyse(&category, &dynamics::type_edges(&skills));

    assert!(
        stdout.contains(&format!(
            "{} skills, {} types",
            skills.len(),
            category.objects.len()
        )),
        "the header must be the real counts:\n{stdout}"
    );
    for ty in &stat.source_only {
        assert!(stdout.contains(ty), "{ty} missing from check:\n{stdout}");
    }
    for ty in &stat.dead_ends {
        assert!(stdout.contains(ty), "{ty} missing from check:\n{stdout}");
    }
    for pool in &dyn_.unbounded {
        assert!(
            stdout.contains(&pool.id),
            "{} under pressure but unreported:\n{stdout}",
            pool.id
        );
    }
    assert!(stdout.contains(&dyn_.verdict()));

    // The exit code is the finding, so `pact check` is usable in CI without
    // anyone parsing the output.
    let expected = i32::from(!stat.never_fires.is_empty());
    assert_eq!(code, expected, "exit code must carry the verdict");
}

#[test]
fn decompose_reports_what_the_corpus_layer_reports() {
    use pact_corpus::{default_root, factor_all, load};

    let Ok(skills) = load(&default_root()) else {
        println!("SKIPPED: no corpus");
        return;
    };
    if skills.is_empty() {
        return;
    }

    let (stdout, code) = run(&["decompose"]);
    assert_eq!(code, 0);

    for f in factor_all(&skills) {
        assert!(
            stdout.contains(&f.chain),
            "{} missing from decompose",
            f.chain
        );
        assert!(
            stdout.contains(&f.word),
            "{}: word {:?} missing:\n{stdout}",
            f.chain,
            f.word
        );
    }
}

#[test]
fn simulate_runs_the_corpus_and_reports_conservation() {
    let (stdout, code) = run(&["simulate"]);
    if stdout.is_empty() {
        println!("SKIPPED: no corpus");
        return;
    }
    assert_eq!(code, 0, "the imported corpus must not leak:\n{stdout}");
    assert!(stdout.contains("conserved, per type"), "{stdout}");
    assert!(stdout.contains("firing(s)"), "{stdout}");

    // Non-vacuity: a run where nothing fired conserves perfectly and means
    // nothing. Starvations are expected and fine — most of a corpus is not on
    // any one path — but something has to happen.
    let fired: usize = stdout
        .lines()
        .find_map(|l| l.trim().split(' ').next()?.parse().ok())
        .unwrap_or(0);
    assert!(fired > 0, "nothing happened in the run:\n{stdout}");
}

#[test]
fn an_unknown_command_says_so_and_exits_two() {
    let (_, code) = run(&["frobnicate"]);
    assert_eq!(code, 2, "unknown commands are a usage error, not a failure");

    let (_, code) = run(&[]);
    assert_eq!(code, 2, "no command is the same kind of mistake");
}

#[test]
fn a_missing_corpus_fails_rather_than_reporting_an_empty_one() {
    // A tool that reports "0 skills, everything fine" for a path that does not
    // exist is worse than one that errors: the first answer looks like a
    // result.
    let (stdout, code) = run(&["check", "/nonexistent/corpus/path"]);
    assert_eq!(code, 1);
    assert!(
        stdout.is_empty(),
        "nothing should be printed as if it were a finding:\n{stdout}"
    );
}
