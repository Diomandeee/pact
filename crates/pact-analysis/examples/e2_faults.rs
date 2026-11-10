//! E2' — fault injection: the filter's precision and recall (THEORY E2 lite).
//!
//! Literal E2 needs P8.2/P8.3 (a model reader); no generative model is
//! available for \$0, so this experiment seeds synthetic signature faults
//! instead and measures whether the engine's detectors catch them. Two fault
//! classes: (a) dead inputs (input pointed at an unproduced ghost type) and
//! (b) chain breaks (a chain member's outputs emptied of the shared type).
//! Ground truth is known by construction.
//!
//! Usage: `cargo run -q -p pact-analysis --example e2_faults -- [trials] [seed]`
//! Defaults: 200 trials, seed 7.

// Experiment harness: trial counts fit f64 exactly (bounded, small).
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::too_many_lines
)]

use pact_corpus::{categorise, default_root, factor, load, members, Skill};
use std::collections::BTreeMap;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let trials: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(200);
    let seed: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(7);
    let mut rng = Rng(seed);

    let skills = load(&default_root()).expect("corpus must load");
    let known: Vec<String> = skills.iter().map(|s| s.name.clone()).collect();
    let chains: Vec<(String, Vec<String>)> = skills
        .iter()
        .filter(|s| s.name.starts_with("chain:"))
        .filter_map(|s| {
            let text = std::fs::read_to_string(&s.path).ok()?;
            let seq = members(&text, &known);
            if seq.is_empty() {
                return None;
            }
            Some((s.name.clone(), seq))
        })
        .collect();

    // Baseline: which chains typecheck before faulting (10/13 fail already;
    // only NEWLY broken chains count as detections).
    let base_by_name: BTreeMap<String, Skill> =
        skills.iter().map(|s| (s.name.clone(), s.clone())).collect();
    let base_ok: Vec<bool> = chains
        .iter()
        .map(|(name, seq)| factor(name, seq, &base_by_name).typechecks)
        .collect();

    let mut dead = [0u32; 3]; // [tp, fp, fn]
    let mut brk = [0u32; 3]; // [tp, fp, fn]

    for t in 0..trials {
        let mut faulty = skills.clone();
        // Fault (a): 3 random non-chain skills get dead inputs.
        let mut a_victims = Vec::new();
        while a_victims.len() < 3 {
            let i = rng.below(faulty.len());
            if faulty[i].name.starts_with("chain:") || a_victims.contains(&i) {
                continue;
            }
            a_victims.push(i);
            faulty[i].input = pact_core::Type::new(format!("M_ghost_{t}_{i}"));
        }
        // Fault (b): break one member link in up to 2 passing chains.
        let mut b_victims: Vec<usize> = Vec::new();
        let passing: Vec<usize> = base_ok
            .iter()
            .enumerate()
            .filter(|(_, ok)| **ok)
            .map(|(i, _)| i)
            .collect();
        for _ in 0..2.min(passing.len()) {
            let ci = passing[rng.below(passing.len())];
            if b_victims.contains(&ci) || chains[ci].1.len() < 2 {
                continue;
            }
            b_victims.push(ci);
            // Empty the first member's outputs: the opening pair must break.
            let member = &chains[ci].1[0];
            if let Some(s) = faulty.iter_mut().find(|s| &s.name == member) {
                s.outputs = Vec::new();
            }
        }

        let category = categorise(&faulty);
        let statics = pact_analysis::statics::analyse(&faulty, &category);
        // (a) detection: victims must appear in never_fires; anything else
        // there beyond the (empty) baseline is a false alarm.
        let flagged: std::collections::HashSet<&str> =
            statics.never_fires.iter().map(String::as_str).collect();
        for &i in &a_victims {
            if flagged.contains(faulty[i].name.as_str()) {
                dead[0] += 1;
            } else {
                dead[2] += 1;
            }
        }
        for f in &flagged {
            if !a_victims.iter().any(|&i| faulty[i].name.as_str() == *f) {
                dead[1] += 1;
            }
        }
        // (b) detection: victim chains must newly fail; other newly failing
        // chains are false alarms (localization error).
        let by_name: BTreeMap<String, Skill> =
            faulty.iter().map(|s| (s.name.clone(), s.clone())).collect();
        for (ci, (name, seq)) in chains.iter().enumerate() {
            let now_ok = factor(name, seq, &by_name).typechecks;
            let newly_broken = base_ok[ci] && !now_ok;
            if b_victims.contains(&ci) {
                if newly_broken {
                    brk[0] += 1;
                } else {
                    brk[2] += 1;
                }
            } else if newly_broken {
                brk[1] += 1;
            }
        }
    }

    let pr = |tp: u32, fp: u32, f: u32| {
        (
            f64::from(tp) / f64::from((tp + fp).max(1)),
            f64::from(tp) / f64::from((tp + f).max(1)),
        )
    };
    let (a_p, a_r) = pr(dead[0], dead[1], dead[2]);
    let (b_p, b_r) = pr(brk[0], brk[1], brk[2]);
    println!(
        "E2': {trials} trials, seed {seed}, {} skills, {} chains",
        skills.len(),
        chains.len()
    );
    println!(
        "  dead-input : precision {a_p:.4} recall {a_r:.4} (tp={} fp={} fn={})",
        dead[0], dead[1], dead[2]
    );
    println!(
        "  chain-break: precision {b_p:.4} recall {b_r:.4} (tp={} fp={} fn={})",
        brk[0], brk[1], brk[2]
    );
}
