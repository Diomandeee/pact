//! P4.1 and P4.2 — statics and dynamics match the Python, on the live corpus.
//!
//! Both gates in one file because they read one corpus and diff against two
//! oracles that were written against the same data. Splitting them would mean
//! loading it twice and, worse, leaving open the possibility that the two
//! halves agree with their oracles and not with each other.

use pact_analysis::{dynamics, statics};
use pact_corpus::{categorise, default_root, load};
use std::collections::BTreeMap;
use std::process::Command;

fn oracle(script: &str, extract: &str) -> Option<BTreeMap<String, String>> {
    let path = default_root().join("divergent-rail/scripts").join(script);
    if !path.exists() {
        return None;
    }
    let out = Command::new("python3")
        .arg("-c")
        .arg(extract)
        .arg(&path)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let map: BTreeMap<String, String> = text
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    if map.is_empty() {
        None
    } else {
        Some(map)
    }
}

const HEAD: &str = r"
import json, subprocess, sys
raw = subprocess.run([sys.executable, sys.argv[1]], capture_output=True, text=True).stdout
d = json.loads(raw)
out = []
";

#[test]
fn statics_match_prompt_algebra() {
    let extract = format!(
        "{HEAD}{}",
        r#"
out.append("dead_ends=" + ",".join(d["dead_end_types"]))
out.append("source_only=" + ",".join(d["source_only_types"]))
for sig, names in d["signature_classes"].items():
    out.append("class.%s=%s" % (sig, ",".join(sorted(names))))
print("\n".join(out))
"#
    );
    let Some(expected) = oracle("prompt_algebra.py", &extract) else {
        println!("SKIPPED: prompt_algebra.py did not run; nothing to diff against");
        return;
    };

    let skills = load(&default_root()).expect("corpus readable");
    let category = categorise(&skills);
    let ours = statics::analyse(&skills, &category);

    let mut disagreements = Vec::new();
    for (label, got, key) in [
        ("dead ends", &ours.dead_ends, "dead_ends"),
        ("source-only", &ours.source_only, "source_only"),
    ] {
        let want: Vec<&str> = expected[key].split(',').filter(|s| !s.is_empty()).collect();
        if want != *got {
            disagreements.push(format!("{label}:\n    python {want:?}\n    rust   {got:?}"));
        }
    }

    // Signature classes: the shortlist of skills that might be the same
    // morphism. The strongest comparison here, because it is keyed on the
    // exact rendering of a signature and a parser that got any type wrong
    // lands in a different class.
    for (sig, names) in &ours.signature_classes {
        let key = format!("class.{sig}");
        match expected.get(&key) {
            Some(want) if *want == names.join(",") => {}
            Some(want) => disagreements.push(format!(
                "class {sig}:\n    python {want}\n    rust   {}",
                names.join(",")
            )),
            None => disagreements.push(format!("class {sig}: absent from python")),
        }
    }

    assert!(
        disagreements.is_empty(),
        "statics disagree with the oracle in {} place(s). Find out which is \
         wrong before assuming it is the python:\n  {}",
        disagreements.len(),
        disagreements.join("\n  ")
    );
    // A floor against pointing at the wrong directory, not a claim about the
    // corpus. Fifteen classes of two or more is what 150 skills over 12 types
    // actually produces; the first draft guessed twenty and was measuring
    // nothing but my guess.
    assert!(
        ours.signature_classes.len() >= 10,
        "only {} signature classes: the corpus root is probably wrong",
        ours.signature_classes.len()
    );
}

#[test]
fn dynamics_match_prompt_economy() {
    let extract = format!(
        "{HEAD}{}",
        r#"
out.append("sources=" + ",".join(d["sources"]))
out.append("drains=" + ",".join(d["drains"]))
out.append("never_fires=" + ",".join(sorted(d["converters_that_never_fire"])))
out.append("loops=%d" % len(d["feedback_loops"]))
out.append("verdict=" + d["verdict"])
for n in d["nodes"]:
    out.append("node.%s=%s/%d/%d/%s" % (n["id"], n["machinations"], n["inflow"], n["outflow"], n["pressure"]))
for u in d["unbounded_pools"]:
    out.append("unbounded.%s=%s" % (u["pool"], u["pressure"]))
print("\n".join(out))
"#
    );
    let Some(expected) = oracle("prompt_economy.py", &extract) else {
        println!("SKIPPED: prompt_economy.py did not run; nothing to diff against");
        return;
    };

    let skills = load(&default_root()).expect("corpus readable");
    let category = categorise(&skills);
    let edges = dynamics::type_edges(&skills);
    let ours = dynamics::analyse(&category, &edges);
    let stat = statics::analyse(&skills, &category);

    let mut disagreements = Vec::new();

    // Per-type flow. Kind, inflow, outflow and pressure at once, because a
    // reading that got the kind right by accident still gets the numbers
    // wrong.
    for node in &ours.nodes {
        let kind = match node.kind {
            dynamics::Machination::Source => "source",
            dynamics::Machination::Drain => "drain",
            dynamics::Machination::Pool => "pool",
        };
        let pressure = node
            .pressure
            .map_or_else(|| "None".to_string(), format_ratio);
        let got = format!("{kind}/{}/{}/{pressure}", node.inflow, node.outflow);
        let key = format!("node.{}", node.id);
        match expected.get(&key) {
            Some(want) if *want == got => {}
            Some(want) => disagreements.push(format!(
                "{}: python {want}, rust {got}  (kind/in/out/pressure)",
                node.id
            )),
            None => disagreements.push(format!("{}: absent from python", node.id)),
        }
    }

    let mut never = stat.never_fires.clone();
    never.sort();
    let want_never: Vec<&str> = expected["never_fires"]
        .split(',')
        .filter(|s| !s.is_empty())
        .collect();
    if want_never != never {
        disagreements.push(format!(
            "never fires:\n    python {want_never:?}\n    rust   {never:?}"
        ));
    }

    let want_loops: usize = expected["loops"].parse().expect("a number");
    if ours.feedback_loops.len() != want_loops {
        disagreements.push(format!(
            "feedback loops: python {want_loops}, rust {}",
            ours.feedback_loops.len()
        ));
    }
    if expected["verdict"] != ours.verdict() {
        disagreements.push(format!(
            "verdict:\n    python {}\n    rust   {}",
            expected["verdict"],
            ours.verdict()
        ));
    }
    for u in &ours.unbounded {
        let key = format!("unbounded.{}", u.id);
        let got = u.pressure.map_or_else(|| "None".to_string(), format_ratio);
        match expected.get(&key) {
            Some(want) if *want == got => {}
            Some(want) => {
                disagreements.push(format!("unbounded {}: python {want}, rust {got}", u.id));
            }
            None => disagreements.push(format!("unbounded {}: absent from python", u.id)),
        }
    }

    assert!(
        disagreements.is_empty(),
        "dynamics disagree with the oracle in {} place(s). Find out which is \
         wrong before assuming it is the python:\n  {}",
        disagreements.len(),
        disagreements.join("\n  ")
    );
}

/// Render a ratio the way Python's `round(x, 2)` prints it.
///
/// `16.5` not `16.50`, and `3.0` not `3`. Matching a number's *rendering*
/// rather than its value is usually a smell; here the comparison is against
/// another language's formatter, and the alternative is parsing floats out of
/// both sides and picking an epsilon nobody can justify.
fn format_ratio(p: f64) -> String {
    let rounded = (p * 100.0).round() / 100.0;
    let mut s = format!("{rounded}");
    if !s.contains('.') {
        s.push_str(".0");
    }
    s
}

#[test]
fn the_two_halves_agree_with_each_other() {
    // Both gates diff against their own oracle, and the oracles were written
    // separately. If statics and dynamics disagreed about the same corpus,
    // both could still pass and the analysis layer would be two tools that
    // happen to share a crate.
    let Ok(skills) = load(&default_root()) else {
        println!("SKIPPED: no corpus");
        return;
    };
    if skills.is_empty() {
        println!("SKIPPED: corpus empty");
        return;
    }

    let category = categorise(&skills);
    let stat = statics::analyse(&skills, &category);
    let dyn_ = dynamics::analyse(&category, &dynamics::type_edges(&skills));

    let dynamic_sources: Vec<String> = dyn_
        .nodes
        .iter()
        .filter(|n| n.kind == dynamics::Machination::Source)
        .map(|n| n.id.clone())
        .collect();
    assert_eq!(
        stat.source_only, dynamic_sources,
        "a source-only type and an economic source are the same fact"
    );

    let dynamic_drains: Vec<String> = dyn_
        .nodes
        .iter()
        .filter(|n| n.kind == dynamics::Machination::Drain)
        .map(|n| n.id.clone())
        .collect();
    assert_eq!(
        stat.dead_ends, dynamic_drains,
        "a dead-end type and an economic drain are the same fact"
    );
}
