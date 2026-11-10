//! P9.3 — the trigger A/B harness proves its own scoring.
//!
//! The gate test runs the harness in `--smoke` mode only: canned arms with
//! known scores (0.85 vs 0.45), asserting the math, that the winner
//! declaration flips in both directions, and that a sub-margin gap falls
//! back to the spec's tiebreak. No API, no corpus paths, deterministic.
//! The scored `--offline` round is run by hand (it needs the prototype
//! corpus and, once, the embedding endpoint for the ranker cache); its
//! results file is the evidence the checklist item cites.

use std::process::Command;

#[test]
fn ab_harness_smoke_proves_scoring() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    assert!(
        dir.join("live_gate_ab.md").exists(),
        "the spec must travel with the harness"
    );
    let out = Command::new("python3")
        .arg(dir.join("live_gate_ab.py"))
        .arg("--smoke")
        .output()
        .expect("harness runs");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "smoke failed:\nstdout: {text}\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("smoke ok"),
        "smoke ran but did not certify itself: {text}"
    );
}
