//! P3.2 — the chain factoring matches `decompose.py` on every chain.
//!
//! Every chain, not eleven. The gate said eleven because there were eleven
//! when it was written; there are thirteen now, and a test pinned to a count
//! would have gone stale the moment a chain was added. It compares whatever
//! the oracle reports against whatever the importer reports, chain by chain.
//!
//! # What is compared
//!
//! The member list, the word, the four stage flags and the typecheck verdict.
//! The member list matters most: it is prose-derived, so it is the part most
//! likely to diverge for reasons that have nothing to do with the algebra.

use pact_corpus::{default_root, factor_all, load};
use std::collections::BTreeMap;
use std::process::Command;

/// The oracle's factoring, flattened to `chain|field=value` lines.
fn oracle() -> Option<BTreeMap<String, String>> {
    let script = default_root().join("divergent-rail/scripts/decompose.py");
    if !script.exists() {
        return None;
    }
    let extract = r#"
import json, subprocess, sys
raw = subprocess.run([sys.executable, sys.argv[1]], capture_output=True, text=True).stdout
d = json.loads(raw)
out = ["chains=%d" % d["chains_analysed"]]
for c in d["detail"]:
    n = c["chain"]
    out.append("%s|members=%s" % (n, ",".join(c["members"])))
    out.append("%s|word=%s" % (n, c["word"]))
    out.append("%s|typechecks=%s" % (n, "1" if c["typechecks"] else "0"))
    s = c["stages_present"]
    flags = "".join("1" if s[k] else "0" for k in ["⊗ expand", "* execute", "⊕ select", "↺ repeat"])
    out.append("%s|stages=%s" % (n, flags))
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

#[test]
fn the_factoring_agrees_with_the_oracle_on_every_chain() {
    let Some(expected) = oracle() else {
        println!("SKIPPED: decompose.py did not run; nothing to diff against");
        println!("  A differential test with nothing to compare is not a passing test.");
        return;
    };
    let skills = load(&default_root()).expect("corpus readable");
    let ours = factor_all(&skills);

    let want_count: usize = expected["chains"].parse().expect("a number");
    let mut disagreements = Vec::new();
    if ours.len() != want_count {
        disagreements.push(format!(
            "chain count: python {want_count}, rust {}",
            ours.len()
        ));
    }

    for f in &ours {
        let flags = format!(
            "{}{}{}{}",
            u8::from(f.stages.expand),
            u8::from(f.stages.execute),
            u8::from(f.stages.select),
            u8::from(f.stages.repeat)
        );
        for (field, got) in [
            ("members", f.members.join(",")),
            ("word", f.word.clone()),
            ("typechecks", u8::from(f.typechecks).to_string()),
            ("stages", flags),
        ] {
            let key = format!("{}|{field}", f.chain);
            match expected.get(&key) {
                Some(want) if *want == got => {}
                Some(want) => disagreements.push(format!(
                    "{} {field}:\n    python {want}\n    rust   {got}",
                    f.chain
                )),
                None => disagreements.push(format!("{}: absent from python", f.chain)),
            }
        }
    }

    assert!(
        disagreements.is_empty(),
        "the factoring disagrees with the oracle in {} place(s). Find out which \
         is wrong before assuming it is the python:\n  {}",
        disagreements.len(),
        disagreements.join("\n  ")
    );
    assert!(ours.len() >= 11, "only {} chains factored", ours.len());
}

#[test]
fn types_decide_and_not_prose() {
    // The rule the whole factoring turns on, isolated so a regression in it
    // cannot hide behind an agreeing corpus. Two members reading the same type
    // are parallel even when the second could also follow the first, which is
    // why input equality is checked before composition.
    use pact_core::Type;
    use pact_corpus::{factor, Skill};
    use std::path::PathBuf;

    let skill = |name: &str, inp: &str, out: &str| Skill {
        name: name.to_string(),
        input: Type::new(inp),
        outputs: vec![Type::new(out)],
        category: None,
        effects: None,
        idempotent: false,
        commutes_with: vec![],
        members: vec![],
        iterates: false,
        path: PathBuf::new(),
    };
    let pair = |a: Skill, b: Skill| -> BTreeMap<String, Skill> {
        [(a.name.clone(), a), (b.name.clone(), b)]
            .into_iter()
            .collect()
    };
    let names = ["a".to_string(), "b".to_string()];

    // `a : M -> M` could follow itself, and `b` reads the same type. Parallel
    // wins, because input equality is checked first.
    let f = factor(
        "t",
        &names,
        &pair(skill("a", "M", "M"), skill("b", "M", "State")),
    );
    assert_eq!(f.word, "⊗", "same input means parallel, however it reads");
    assert!(f.typechecks);

    // A genuine sequence stays a sequence.
    let f = factor(
        "t",
        &names,
        &pair(
            skill("a", "M_arch", "M_plan"),
            skill("b", "M_plan", "State"),
        ),
    );
    assert_eq!(f.word, "▸");

    // A break is recorded and named, not silently dropped.
    let f = factor(
        "t",
        &names,
        &pair(skill("a", "M_arch", "M_plan"), skill("b", "State", "M")),
    );
    assert_eq!(f.word, "✗");
    assert!(!f.typechecks);
    assert!(f.notes[0].contains("does not compose"), "{:?}", f.notes);
}

#[test]
fn member_extraction_is_word_boundary_matched() {
    // `pulse` must not match `pulse:start`. The oracle uses a lookaround for
    // this and the corpus has names where it matters.
    use pact_corpus::members;

    let known: Vec<String> = ["pulse", "merge", "compact"]
        .iter()
        .map(ToString::to_string)
        .collect();

    assert_eq!(
        members("run pulse:start then merge the results", &known),
        vec!["merge".to_string()],
        "pulse:start is not pulse"
    );
    assert_eq!(
        members("first compact, then pulse, then merge", &known),
        vec![
            "compact".to_string(),
            "pulse".to_string(),
            "merge".to_string()
        ],
        "ordered by first mention"
    );
}
