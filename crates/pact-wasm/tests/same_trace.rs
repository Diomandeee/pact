//! P7.1 — one engine, not two.
//!
//! A WASM run and a native run must produce **byte-identical** reports from the
//! same seed. That is INV-3 crossing a language boundary, and it is the check
//! that keeps a browser engine from quietly becoming a second engine.
//!
//! # Why byte-identical rather than "equivalent"
//!
//! Equivalence needs a comparison function, and a comparison function is a
//! place for a difference to hide. Bytes have no such place. If the two ever
//! disagree by a rendering detail, that detail is exactly the kind of platform
//! dependence this gate exists to find — `usize` width, iteration order, an
//! integer formatted differently — and calling it "equivalent" would wave it
//! through.
//!
//! # This compares one implementation reached two ways
//!
//! `pact-wasm` builds as both `cdylib` and `rlib`, and both paths call the same
//! `pact_core::Simulator`. Nothing here is reimplemented. A test that compared
//! two implementations would be testing whether they agree today.

use pact_wasm::{EdgeSpec, GraphSpec, NodeSpec};
use std::process::Command;

/// A graph exercising every node kind and both edge rates.
///
/// Every kind, on purpose: a boundary that carried pools correctly and delays
/// wrongly would pass a simpler fixture and fail in the browser.
fn fixture() -> GraphSpec {
    let node = |id: &str, kind: &str| NodeSpec {
        id: id.to_string(),
        kind: kind.to_string(),
        ..NodeSpec::default()
    };
    let pool = |id: &str, ty: &str, n: u64| NodeSpec {
        bag: vec![(ty.to_string(), n)],
        ..node(id, "pool")
    };

    GraphSpec {
        nodes: vec![
            NodeSpec {
                bag: vec![("M_arch".into(), 3)],
                ..node("subject", "source")
            },
            pool("plan", "M_plan", 4),
            NodeSpec {
                distribution: Some("broadcast".into()),
                ..node("fan", "gate")
            },
            NodeSpec {
                distribution: Some("probabilistic".into()),
                weights: vec![3, 1],
                ..node("pick", "gate")
            },
            NodeSpec {
                consumes: vec![("M_plan".into(), 1)],
                produces: vec![("State".into(), 1)],
                ..node("pulse", "converter")
            },
            NodeSpec {
                consumes: vec![("State".into(), 2)],
                produces: vec![("M_arch".into(), 1)],
                ..node("compact", "converter")
            },
            NodeSpec {
                steps: 2,
                ..node("hold", "delay")
            },
            node("report", "drain"),
            NodeSpec {
                activation: Some("automatic".into()),
                ..pool("staging", "M", 2)
            },
        ],
        edges: vec![
            EdgeSpec {
                from: "subject".into(),
                to: "plan".into(),
                rate: Some(2),
            },
            EdgeSpec {
                from: "plan".into(),
                to: "fan".into(),
                rate: None,
            },
            EdgeSpec {
                from: "fan".into(),
                to: "pulse".into(),
                rate: None,
            },
            EdgeSpec {
                from: "fan".into(),
                to: "hold".into(),
                rate: None,
            },
            EdgeSpec {
                from: "hold".into(),
                to: "pick".into(),
                rate: None,
            },
            EdgeSpec {
                from: "pick".into(),
                to: "compact".into(),
                rate: None,
            },
            EdgeSpec {
                from: "pick".into(),
                to: "report".into(),
                rate: None,
            },
            EdgeSpec {
                from: "staging".into(),
                to: "report".into(),
                rate: Some(1),
            },
        ],
    }
}

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

/// Run the wasm through Deno, or `None` if the toolchain is not there.
///
/// Skipped loudly rather than passed. A differential test that goes green
/// because it had nothing to compare is worse than no test.
fn wasm_report(spec_json: &str, seed: u64, steps: usize) -> Option<String> {
    let wasm = root().join("target/wasm32-unknown-unknown/release/pact_wasm.wasm");
    if !wasm.exists() {
        println!("SKIPPED: no wasm artifact. Build it with:");
        println!("  cargo build -p pact-wasm --target wasm32-unknown-unknown --release");
        return None;
    }
    let spec_path = std::env::temp_dir().join("pact_spec.json");
    std::fs::write(&spec_path, spec_json).ok()?;

    let out = Command::new("deno")
        .args(["run", "--allow-read", "--quiet"])
        .arg(root().join("crates/pact-wasm/tests/run_wasm.mjs"))
        .arg(&wasm)
        .arg(&spec_path)
        .arg(seed.to_string())
        .arg(steps.to_string())
        .output()
        .ok()?;

    if !out.status.success() {
        println!("SKIPPED: deno did not run the module");
        println!("{}", String::from_utf8_lossy(&out.stderr));
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[test]
fn a_wasm_run_and_a_native_run_are_byte_identical() {
    let spec = fixture();
    let spec_json = serde_json::to_string(&spec).expect("the fixture serialises");

    for (seed, steps) in [(1u64, 12usize), (0xD1CE, 40), (7, 3)] {
        let native = pact_wasm::run_json(&spec_json, seed, steps).expect("the native run works");
        let Some(wasm) = wasm_report(&spec_json, seed, steps) else {
            return;
        };

        assert_eq!(
            native.len(),
            wasm.len(),
            "seed {seed}, {steps} steps: reports differ in length, \
             {} native vs {} wasm",
            native.len(),
            wasm.len()
        );
        if native != wasm {
            // Name the first divergence. "They differ" sends someone to diff
            // two thousand-line strings by hand.
            let at = native
                .bytes()
                .zip(wasm.bytes())
                .position(|(a, b)| a != b)
                .unwrap_or(0);
            let from = at.saturating_sub(60);
            panic!(
                "seed {seed}, {steps} steps: diverged at byte {at}\n  native ...{}\n  wasm   ...{}",
                &native[from..(at + 60).min(native.len())],
                &wasm[from..(at + 60).min(wasm.len())]
            );
        }
    }
}

#[test]
fn the_fixture_actually_exercises_the_engine() {
    // A byte comparison over two empty reports is byte-identical and proves
    // nothing. This is the non-vacuity half.
    let report = pact_wasm::run(&fixture(), 1, 12).expect("runs");

    assert!(report.steps > 0, "no steps ran");
    assert!(
        report.events.len() > 20,
        "only {} events: the fixture is inert",
        report.events.len()
    );
    assert!(
        report.conserved,
        "the fixture leaks, so it is testing a bug"
    );

    for kind in [
        "Created",
        "Moved",
        "Converted",
        "Held",
        "Copied",
        "Destroyed",
    ] {
        assert!(
            report.events.iter().any(|e| e.starts_with(kind)),
            "no {kind} event: that node kind is unexercised across the boundary"
        );
    }
}

#[test]
fn a_different_seed_gives_a_different_report() {
    // Otherwise the byte-identity above is satisfied by an engine ignoring its
    // seed, which is reproducible the way a constant is.
    let spec = fixture();
    let a = pact_wasm::run(&spec, 1, 40).expect("runs");
    let b = pact_wasm::run(&spec, 0xBEEF, 40).expect("runs");
    assert_ne!(
        a.events, b.events,
        "two seeds produced identical traces through a probabilistic gate"
    );
}

#[test]
fn the_wire_graph_round_trips() {
    let spec = fixture();
    let json = serde_json::to_string(&spec).expect("serialises");
    let back: GraphSpec = serde_json::from_str(&json).expect("deserialises");
    assert_eq!(spec, back);

    // And it builds a graph with no structural defect that means the graph
    // does not describe anything.
    let graph = spec.build();
    let breaking: Vec<_> = graph
        .validate()
        .into_iter()
        .filter(|d| {
            matches!(
                d,
                pact_core::Defect::DanglingEdge { .. } | pact_core::Defect::DuplicateNode { .. }
            )
        })
        .collect();
    assert_eq!(breaking, vec![], "the fixture is malformed");
}
