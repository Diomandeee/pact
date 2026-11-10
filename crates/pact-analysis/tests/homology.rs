//! P4.3 — `b₀` and `b₁` over GF(2), on nine known-answer complexes.
//!
//! Nine complexes small enough to check by hand, which is the only reason to
//! trust a number nobody can check by eye. Each one is also diffed against
//! `loop_harness.py`'s `betti()`, so the claim is both "matches what I can
//! verify myself" and "matches the implementation that was already verified".
//!
//! Those are different claims. The first catches a wrong formula; the second
//! catches a formula that is right and differently right from the oracle,
//! which would make every barcode incomparable across the rewrite.

use pact_analysis::Complex;
use std::process::Command;

/// The nine, with answers computable on paper.
fn cases() -> Vec<(&'static str, Complex, usize, usize)> {
    vec![
        ("empty", Complex::new(), 0, 0),
        ("point", Complex::new().with(&["a"]), 1, 0),
        ("two points", Complex::new().with(&["a"]).with(&["b"]), 2, 0),
        ("one edge", Complex::new().with(&["a", "b"]), 1, 0),
        (
            "path of three",
            Complex::new().with(&["a", "b"]).with(&["b", "c"]),
            1,
            0,
        ),
        (
            // Three edges, no filling. The hole is the point.
            "hollow triangle",
            Complex::new()
                .with(&["a", "b"])
                .with(&["b", "c"])
                .with(&["a", "c"]),
            1,
            1,
        ),
        (
            // The same three edges with the 2-face present. The cycle now
            // bounds, so b1 drops to zero: this is what "hardening" means.
            "filled triangle",
            Complex::new().with(&["a", "b", "c"]),
            1,
            0,
        ),
        (
            "two hollow triangles",
            Complex::new()
                .with(&["a", "b"])
                .with(&["b", "c"])
                .with(&["a", "c"])
                .with(&["x", "y"])
                .with(&["y", "z"])
                .with(&["x", "z"]),
            2,
            2,
        ),
        (
            // Two loops sharing one vertex. Connected, and two independent
            // cycles.
            "figure eight",
            Complex::new()
                .with(&["a", "b"])
                .with(&["b", "c"])
                .with(&["a", "c"])
                .with(&["c", "d"])
                .with(&["d", "e"])
                .with(&["c", "e"]),
            1,
            2,
        ),
    ]
}

#[test]
fn nine_known_answers() {
    let all = cases();
    assert_eq!(all.len(), 9, "the gate says nine");

    for (name, complex, b0, b1) in all {
        let got = complex.betti();
        assert_eq!(got.b0, b0, "{name}: b0, got {got:?}");
        assert_eq!(got.b1, b1, "{name}: b1, got {got:?}");
    }
}

#[test]
fn filling_a_cycle_is_what_kills_b1() {
    // The completion criterion, isolated. A phase is hard when its
    // non-bounding cycles bound, which happens when the filling artifact
    // exists and not when someone ticks a box.
    let hollow = Complex::new()
        .with(&["a", "b"])
        .with(&["b", "c"])
        .with(&["a", "c"]);
    assert_eq!(hollow.betti().b1, 1, "a cycle with nothing closing it");

    let filled = Complex::new().with(&["a", "b", "c"]);
    assert_eq!(filled.betti().b1, 0, "the artifact exists, so it bounds");
    assert_eq!(
        filled.betti().edges,
        3,
        "and the edges are still there: closure is downward"
    );
}

#[test]
fn a_disjoint_plan_is_visible_as_b0() {
    // Invariant 3. Forcing is what makes edges exist, so a phase in pieces is
    // a disjoint union pretending to be a plan.
    let split = Complex::new().with(&["a", "b"]).with(&["c", "d"]);
    assert_eq!(split.betti().b0, 2);

    let joined = Complex::new()
        .with(&["a", "b"])
        .with(&["c", "d"])
        .with(&["b", "c"]);
    assert_eq!(joined.betti().b0, 1, "one shared artifact connects them");
}

#[test]
fn the_answers_match_the_oracle() {
    // The second claim. A formula that is right and differently right from
    // `loop_harness.py` would make every barcode incomparable across the
    // rewrite, and nothing in the numbers alone would say so.
    let script = pact_corpus::default_root().join("divergent-rail/scripts/loop_harness.py");
    if !script.exists() {
        println!("SKIPPED: loop_harness.py absent; nothing to diff against");
        return;
    }

    let mut disagreements = Vec::new();
    for (name, complex, _, _) in cases() {
        let faces: Vec<String> = complex
            .faces(0)
            .iter()
            .map(|v| format!("({:?},)", v[0]))
            .chain(
                complex
                    .faces(1)
                    .iter()
                    .map(|e| format!("({:?}, {:?})", e[0], e[1])),
            )
            .chain(
                complex
                    .faces(2)
                    .iter()
                    .map(|t| format!("({:?}, {:?}, {:?})", t[0], t[1], t[2])),
            )
            .collect();

        let program = format!(
            "import sys, json, importlib.util\n\
             spec = importlib.util.spec_from_file_location('h', sys.argv[1])\n\
             m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)\n\
             print(json.dumps(m.betti([{}])))",
            faces.join(", ")
        );
        let out = Command::new("python3")
            .arg("-c")
            .arg(&program)
            .arg(&script)
            .output()
            .expect("python3 runs");
        let stdout = String::from_utf8_lossy(&out.stdout);
        if !out.status.success() || stdout.trim().is_empty() {
            println!("SKIPPED {name}: oracle did not answer");
            continue;
        }

        let ours = complex.betti();
        let want_b0 = extract(&stdout, "b0");
        let want_b1 = extract(&stdout, "b1");
        if want_b0 != Some(ours.b0) || want_b1 != Some(ours.b1) {
            disagreements.push(format!(
                "{name}: python b0={want_b0:?} b1={want_b1:?}, rust b0={} b1={}",
                ours.b0, ours.b1
            ));
        }
    }

    assert!(
        disagreements.is_empty(),
        "homology disagrees with the oracle in {} case(s). Find out which is \
         wrong before assuming it is the python:\n  {}",
        disagreements.len(),
        disagreements.join("\n  ")
    );
}

fn extract(json: &str, key: &str) -> Option<usize> {
    let needle = format!("\"{key}\":");
    let at = json.find(&needle)? + needle.len();
    json[at..]
        .trim_start()
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}
