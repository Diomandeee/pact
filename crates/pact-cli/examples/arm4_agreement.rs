//! Arm 4 — PACT-agreement lift (pPACT unblock plan).
//!
//! The probe predicts which turn comes next. PACT judges whether skills
//! compose. Bridge: extract member skills from the predicted turn's text with
//! the engine's own first-mention rule, and score what fraction of consecutive
//! member pairs compose (sequence or broadcast) under real skill signatures.
//! Compare probe-predicted turns against true next turns (upper reference)
//! and random turns (baseline).
//!
//! If predicted turns compose better than chance, the learned distribution
//! concentrates inside the algebraically valid region.
//!
//! Usage: `cargo run -q -p pact-analysis --example arm4_agreement --
//!   /tmp/codex_turns.jsonl /tmp/arm4_ids.jsonl`

// Experiment harness: scored counts fit f64 exactly (bounded by corpus size).
#![allow(clippy::cast_precision_loss)]
use pact_corpus::{default_root, load, members};
use std::collections::{BTreeMap, HashMap};

fn compose_rate(
    text: &str,
    known: &[String],
    in_of: &BTreeMap<String, String>,
    out_of: &BTreeMap<String, Vec<String>>,
) -> Option<f64> {
    let seq = members(text, known);
    if seq.len() < 2 {
        return None;
    }
    let mut ok = 0;
    let mut total = 0;
    for pair in seq.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let (Some(ai), Some(bi), Some(ao)) = (in_of.get(a), in_of.get(b), out_of.get(a)) else {
            continue;
        };
        total += 1;
        // Broadcast (shared input) or sequence: both non-broken.
        if ai == bi || ao.iter().any(|o| o == bi) {
            ok += 1;
        }
    }
    if total == 0 {
        return None;
    }
    Some(f64::from(ok) / f64::from(total))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let turns_path = args.get(1).expect("turns jsonl path");
    let ids_path = args.get(2).expect("arm4 ids jsonl path");

    let skills = load(&default_root()).expect("corpus must load");
    let known: Vec<String> = skills.iter().map(|s| s.name.clone()).collect();
    let in_of: BTreeMap<String, String> = skills
        .iter()
        .map(|s| (s.name.clone(), s.input.to_string()))
        .collect();
    let out_of: BTreeMap<String, Vec<String>> = skills
        .iter()
        .map(|s| {
            (
                s.name.clone(),
                s.outputs.iter().map(ToString::to_string).collect(),
            )
        })
        .collect();

    let mut texts: HashMap<String, String> = HashMap::new();
    let raw = std::fs::read_to_string(turns_path).expect("turns must read");
    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(line).expect("turn json");
        if let (Some(id), Some(text)) = (v.get("id"), v.get("text")) {
            texts.insert(
                id.as_str().unwrap_or_default().to_string(),
                text.as_str().unwrap_or_default().to_string(),
            );
        }
    }

    let mut agg: BTreeMap<&str, (f64, usize, usize)> = BTreeMap::from([
        ("true", (0.0, 0, 0)),
        ("pred", (0.0, 0, 0)),
        ("rand", (0.0, 0, 0)),
    ]);
    let ids_raw = std::fs::read_to_string(ids_path).expect("ids must read");
    for line in ids_raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(line).expect("id json");
        for key in ["true_dst", "pred_dst", "rand_dst"] {
            let label = key.split('_').next().unwrap_or(key);
            let tid = v.get(key).and_then(|x| x.as_str()).unwrap_or_default();
            let Some(text) = texts.get(tid) else { continue };
            let entry = agg.get_mut(label).expect("label");
            entry.2 += 1;
            if let Some(rate) = compose_rate(text, &known, &in_of, &out_of) {
                entry.0 += rate;
                entry.1 += 1;
            }
        }
    }

    println!(
        "arm4: {} turns loaded, {} skills",
        texts.len(),
        skills.len()
    );
    for (label, (sum, scored, total)) in &agg {
        let mean = if *scored > 0 {
            sum / *scored as f64
        } else {
            f64::NAN
        };
        println!(
            "  {label:>4}: compose_rate {mean:.4} over {scored}/{total} turns with >=2 member skills"
        );
    }
}
