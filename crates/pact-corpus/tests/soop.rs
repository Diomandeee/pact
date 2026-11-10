//! P3.1 — the importer matches `prompt_algebra.py` exactly.
//!
//! # Why this shells out
//!
//! A differential test comparing against numbers typed into it would be
//! testing a snapshot, and the corpus changes every time Mohamed writes a
//! skill. This runs the oracle, now, against the same corpus, and compares.
//! When the corpus moves both sides move together or the test fails, which is
//! the only arrangement that keeps meaning as the thing under test drifts.
//!
//! # When they disagree
//!
//! Do not assume the Rust is right. Twice so far the older implementation has
//! been correct. The failure message prints both sides for exactly that
//! reason: the useful question is which one is wrong, and a message saying
//! only "mismatch" makes that a fresh investigation every time.
//!
//! # If the corpus is absent
//!
//! Skipped, loudly, rather than passed. A differential test that silently
//! passes because it had nothing to compare is worse than no test: it reports
//! green for a rewrite nobody checked.

use pact_corpus::{categorise, coercions, default_root, economy_degrees, load, to_graph, Skill};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

/// Ask the oracle for a flat `key=value` view of its own JSON.
///
/// Flat rather than JSON because parsing JSON in Rust needs a dependency and
/// the shape needed here is a handful of scalars and lists. The extraction
/// runs inside the oracle's own process, so it reads the object the oracle
/// built rather than a re-serialisation of it.
fn oracle() -> Option<BTreeMap<String, String>> {
    let script = default_root().join("divergent-rail/scripts/prompt_algebra.py");
    if !script.exists() {
        return None;
    }

    let extract = r#"
import json, subprocess, sys
raw = subprocess.run([sys.executable, sys.argv[1]], capture_output=True, text=True).stdout
d = json.loads(raw)
out = ["skills=%d" % d["skills"],
       "objects=" + ",".join(d["objects"]),
       "dead_ends=" + ",".join(d["dead_end_types"]),
       "source_only=" + ",".join(d["source_only_types"])]
for ty, deg in d["object_degrees"].items():
    out.append("degree.%s=%d/%d" % (ty, deg["produced_by"], deg["consumed_by"]))
print("\n".join(out))
"#;

    let out = Command::new("python3")
        .arg("-c")
        .arg(extract)
        .arg(&script)
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

fn skip(reason: &str) {
    println!("SKIPPED: {reason}");
    println!("  A differential test with nothing to compare is not a passing test.");
}

#[test]
fn the_importer_agrees_with_the_oracle() {
    let Some(expected) = oracle() else {
        skip("prompt_algebra.py did not run; no corpus to diff against");
        return;
    };
    let skills = load(&default_root()).expect("the corpus root must be readable");
    let ours = categorise(&skills);
    let mut disagreements = Vec::new();

    let want: usize = expected["skills"].parse().expect("a number");
    if ours.morphisms != want {
        disagreements.push(format!("morphisms: python {want}, rust {}", ours.morphisms));
    }

    let want_objects: Vec<&str> = expected["objects"].split(',').collect();
    let our_objects: Vec<String> = ours.objects.iter().map(ToString::to_string).collect();
    if want_objects != our_objects {
        disagreements.push(format!(
            "objects:\n    python {want_objects:?}\n    rust   {our_objects:?}"
        ));
    }

    // Degrees, per object. The strongest of the four: a parser that found the
    // right set of types by luck still gets these wrong.
    for (ty, degree) in &ours.degrees {
        let key = format!("degree.{ty}");
        let Some(want) = expected.get(&key) else {
            disagreements.push(format!("degree.{ty}: absent from python"));
            continue;
        };
        let got = format!("{}/{}", degree.produced_by, degree.consumed_by);
        if *want != got {
            disagreements.push(format!(
                "degree of {ty}: python {want} produced/consumed, rust {got}"
            ));
        }
    }

    for (label, list, key) in [
        ("dead ends", &ours.dead_ends, "dead_ends"),
        ("source-only", &ours.source_only, "source_only"),
    ] {
        let want: Vec<&str> = expected[key].split(',').filter(|s| !s.is_empty()).collect();
        let got: Vec<String> = list.iter().map(ToString::to_string).collect();
        if want != got {
            disagreements.push(format!("{label}:\n    python {want:?}\n    rust   {got:?}"));
        }
    }

    assert!(
        disagreements.is_empty(),
        "the rewrite disagrees with the oracle in {} place(s). Find out which is \
         wrong before assuming it is the python:\n  {}",
        disagreements.len(),
        disagreements.join("\n  ")
    );

    assert!(
        ours.morphisms > 100,
        "only {} skills read: the corpus root is probably wrong",
        ours.morphisms
    );
    // Not a pinned count. Twelve was true from July to November and then
    // `M_analytical` left the economy (finding 0009), and a gate pinned to a
    // number goes stale the moment the corpus moves — the same lesson P3.1's
    // own entry records about 144. The oracle's list is the authority.
    let theirs: Vec<&str> = expected["objects"]
        .split(',')
        .filter(|s| !s.is_empty())
        .collect();
    assert_eq!(
        ours.objects.len(),
        theirs.len(),
        "object count differs from the oracle: rust {:?}, python {:?}",
        ours.objects,
        theirs
    );
    assert!(
        ours.objects.len() >= 8,
        "only {} types: the corpus root is probably wrong",
        ours.objects.len()
    );
}

#[test]
fn the_frontmatter_rules_are_the_oracles_rules() {
    // Both quirks copied deliberately. Improving either would make the Rust
    // disagree with the Python, and the differential test above would then
    // report a difference that is really a fix.
    let path = Path::new("/tmp/example/SKILL.md");

    assert!(
        Skill::parse(path, "---\ninput_type: M\noutput_type: State\n---\n").is_none(),
        "the marker is what makes a file a skill, however well typed it is"
    );

    let in_body = "---\ninput_type: M\noutput_type: State\n---\n\ntype_spec_version in prose\n";
    let parsed = Skill::parse(path, in_body).expect("the oracle would take this");
    assert_eq!(parsed.input.as_str(), "M");

    let alternatives =
        "---\ninput_type: M\noutput_type: Report | silent\ntype_spec_version: 1\n---\n";
    let parsed = Skill::parse(path, alternatives).expect("parses");
    assert_eq!(
        parsed
            .outputs
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["Report", "silent"],
        "an alternative is two objects, not one type named `A | B`"
    );

    let untyped = "---\ncategory: Effector\ntype_spec_version: 1\n---\n";
    assert_eq!(
        Skill::parse(path, untyped).expect("parses").input.as_str(),
        "?",
        "a missing type is a finding, not an error"
    );

    let injected =
        "---\ninput_type: M\noutput_type: State\ntype_spec_version: 1\n---\ninput_type: WRONG\n";
    assert_eq!(
        Skill::parse(path, injected).expect("parses").input.as_str(),
        "M",
        "the head stops at the second delimiter, so a body cannot inject a field"
    );
}

#[test]
fn the_corpus_becomes_a_graph_the_engine_accepts() {
    // What the import is for. A category is a report; a graph is something the
    // simulator can drive and a protocol can check.
    let Ok(skills) = load(&default_root()) else {
        skip("no corpus root");
        return;
    };
    if skills.is_empty() {
        skip("corpus root is empty");
        return;
    }

    let graph = to_graph(&skills);
    let category = categorise(&skills);

    assert_eq!(
        graph.nodes().len(),
        skills.len() + category.objects.len() + coercions(&category).len(),
        "one node per skill, one per type, one per coercion (P3.6)"
    );

    for edge in &graph.resources {
        assert_ne!(
            edge.from.starts_with("type:"),
            edge.to.starts_with("type:"),
            "edge {} -> {} joins two nodes of the same kind; the graph is bipartite",
            edge.from,
            edge.to
        );
    }

    let structural: Vec<_> = graph
        .validate()
        .into_iter()
        .filter(|d| {
            matches!(
                d,
                pact_core::Defect::DanglingEdge { .. } | pact_core::Defect::DuplicateNode { .. }
            )
        })
        .collect();
    assert_eq!(structural, vec![], "the imported graph must be well formed");
}

#[test]
fn the_graph_and_the_category_report_the_same_corpus() {
    // The two readings have to agree. A type with many producers and one
    // consumer is a pool under pressure in the graph and a high-degree object
    // in the category; if those disagreed, the analysis layer would have to
    // pick one and the other would quietly rot.
    let Ok(skills) = load(&default_root()) else {
        skip("no corpus root");
        return;
    };
    if skills.is_empty() {
        skip("corpus root is empty");
        return;
    }

    // Since P3.6 the graph carries the coercions, so it agrees with the
    // economy's degrees: the category's plus one converter per produced `M_*`.
    let graph = to_graph(&skills);
    for (ty, degree) in &economy_degrees(&categorise(&skills)) {
        let id = format!("type:{ty}");
        assert_eq!(
            graph.incoming(&id).len(),
            degree.produced_by,
            "{ty}: graph inflow must equal its producers, coercions included"
        );
        assert_eq!(
            graph.outgoing(&id).len(),
            degree.consumed_by,
            "{ty}: graph outflow must equal its consumers, coercions included"
        );
    }
}

fn skill(name: &str, inp: &str, out: &str) -> Skill {
    Skill {
        name: name.to_string(),
        input: pact_core::Type::new(inp),
        outputs: out
            .split('|')
            .map(|t| pact_core::Type::new(t.trim()))
            .collect(),
        category: None,
        effects: None,
        idempotent: false,
        commutes_with: vec![],
        members: vec![],
        iterates: false,
        path: std::path::PathBuf::new(),
    }
}

#[test]
fn refinement_is_one_coercion_per_produced_subtype() {
    // P3.6 on a corpus small enough to count by hand. `plan` emits M_plan,
    // `forge` reads M, `review` reads M_review that nothing emits, `report`
    // reads M_plan. Composition already lets `forge` read `plan`'s output;
    // the economy now says so too.
    let skills = vec![
        skill("plan", "Subject", "M_plan"),
        skill("forge", "M", "Report"),
        skill("review", "M_review", "Report"),
        skill("report", "M_plan", "Report"),
    ];
    let category = categorise(&skills);
    let names: Vec<String> = coercions(&category)
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        names,
        vec!["M_plan"],
        "M_review is never produced, so it coerces nothing"
    );

    let economy = economy_degrees(&category);
    let m = pact_core::Type::new("M");
    let plan = pact_core::Type::new("M_plan");
    assert_eq!(
        (category.degrees[&m].produced_by, economy[&m].produced_by),
        (0, 1)
    );
    assert_eq!(
        (
            category.degrees[&plan].consumed_by,
            economy[&plan].consumed_by
        ),
        (1, 2)
    );
    assert_eq!(
        category.dead_ends,
        vec![pact_core::Type::new("Report")],
        "the category stays nominal"
    );

    let graph = to_graph(&skills);
    assert_eq!(
        graph.outgoing("type:M_plan").len(),
        2,
        "report and coerce:M_plan"
    );
    assert_eq!(graph.incoming("type:M").len(), 1, "coerce:M_plan");
    assert!(graph.nodes().iter().any(|n| n.id == "coerce:M_plan"));

    // Without an `M` object there is nothing to coerce into.
    let no_general = vec![
        skill("plan", "Subject", "M_plan"),
        skill("report", "M_plan", "Report"),
    ];
    assert!(coercions(&categorise(&no_general)).is_empty());
}
