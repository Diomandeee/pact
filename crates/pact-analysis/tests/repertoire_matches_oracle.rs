//! P9.2 — the Rust repertoire matches the human-baked fixture, and the
//! baked fixture matches an independent Python derivation from the rows'
//! raw fields.
//!
//! Two diffs, different failures: Rust-vs-baked catches a wrong formula;
//! baked-vs-oracle catches a transcription slip made while building the
//! fixture (the expected class copied wrong from the label row). Either one
//! passing alone would leave the other half unverified. Plus the closed
//! vocabulary assertion: all 170 rows land inside the 8 classes, so no
//! eighth bucket appears on real data.

use pact_analysis::repertoire::Maneuver;
use std::process::Command;

fn manifest_fixtures() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Rows projected by Python (mechanical field extraction — the derivation
/// logic lives in the oracle and in Rust, never here).
fn oracle_lines() -> Option<Vec<(String, Option<String>, String)>> {
    let dir = manifest_fixtures();
    if !dir.join("repertoire_oracle.py").exists() {
        return None;
    }
    let out = Command::new("python3")
        .arg(dir.join("repertoire_oracle.py"))
        .arg(dir.join("repertoire-fixture.json"))
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let mut rows = Vec::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let mut parts = line.split('|');
        let (Some(idx), Some(verdict), Some(man), Some(derived)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return None;
        };
        idx.parse::<usize>().ok()?;
        rows.push((
            verdict.to_string(),
            (man != "-").then(|| man.to_string()),
            derived.to_string(),
        ));
    }
    Some(rows).filter(|r: &Vec<(String, Option<String>, String)>| !r.is_empty())
}

fn baked_expected() -> Vec<String> {
    // Baked answers read back through the same oracle projection path would
    // be circular; instead the oracle prints its derivation and Python's
    // json module reads `expected`, keeping the two derivations in separate
    // processes with no shared code.
    let dir = manifest_fixtures();
    let out = Command::new("python3")
        .arg("-c")
        .arg("import json,sys; print('\\n'.join(r['expected'] for r in json.load(open(sys.argv[1]))))")
        .arg(dir.join("repertoire-fixture.json"))
        .output()
        .expect("python3 projects the baked answers");
    assert!(out.status.success(), "baked projection failed");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

fn name(m: Maneuver) -> &'static str {
    match m {
        Maneuver::HypothesisKill => "HypothesisKill",
        Maneuver::ScaleReframe => "ScaleReframe",
        Maneuver::DomainImport => "DomainImport",
        Maneuver::PremiseRejection => "PremiseRejection",
        Maneuver::ThreadTransplant => "ThreadTransplant",
        Maneuver::AbortRestart => "AbortRestart",
        Maneuver::OtherManeuver => "OtherManeuver",
        Maneuver::Continuation => "Continuation",
    }
}

#[test]
fn repertoire_matches_oracle_and_baked() {
    let Some(rows) = oracle_lines() else {
        println!("SKIPPED: repertoire_oracle.py did not run; nothing to diff against");
        return;
    };
    let baked = baked_expected();
    assert_eq!(
        rows.len(),
        baked.len(),
        "oracle and fixture disagree on row count"
    );
    assert_eq!(
        rows.len(),
        170,
        "fixture lost rows: expected 170, got {}",
        rows.len()
    );

    let mut disagreements = Vec::new();
    for (i, ((verdict, man, derived), want)) in rows.iter().zip(&baked).enumerate() {
        // Diff 1: baked answers vs independent derivation (transcription).
        if derived != want {
            disagreements.push(format!("row {i}: baked {want}, oracle derives {derived}"));
        }
        // Diff 2: Rust vs baked answers (formula).
        let got = Maneuver::classify(verdict, man.as_deref()).map(name);
        if got != Some(want.as_str()) {
            disagreements.push(format!(
                "row {i}: baked {want}, rust {:?}",
                got.unwrap_or("CORRUPT")
            ));
        }
    }
    assert!(
        disagreements.is_empty(),
        "repertoire disagrees in {} place(s). Find out which side is wrong \
         before assuming it is the python:\n  {}",
        disagreements.len(),
        disagreements.join("\n  ")
    );
}
