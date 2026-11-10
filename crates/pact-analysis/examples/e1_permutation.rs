//! E1 — the permutation test (THEORY.md section 6).
//!
//! Randomly permute the type assignments among the skills and recompute the
//! analysis, to get a null distribution for: chains that typecheck, dead
//! converters, composable pairs, and pressure. If the real corpus is a sharp
//! outlier, the typing carries information. If not, the vocabulary is too
//! coarse to catch anything and the thesis fails cheaply, as designed.
//!
//! b1 is excluded: no corpus-to-Complex constructor exists yet, and inventing
//! one inside the experiment would test the constructor, not the corpus.
//!
//! Usage: `cargo run -p pact-analysis --example e1_permutation -- [reps] [seed] [mode]`
//! Defaults: 2000 reps, seed 7, mode `perm`. Deterministic for a fixed seed
//! and corpus.
//!
//! Modes:
//!
//! - `perm`: shuffle inputs and outputs independently among skills. Preserves
//!   both marginals, so every degree statistic (dead ends, sources, pressure,
//!   never-fires) is invariant by construction and only chain-level metrics
//!   vary. This is the literal reading of THEORY E1.
//! - `boot`: resample each skill's input and output-vector with replacement
//!   from the empirical marginals. Expected degrees match the corpus but
//!   realised degrees vary, so the null speaks to the degree statistics too.
//!   This is the reading under which E1's listed metrics are all testable.

// Experiment harness, not library code: counts are bounded by corpus size
// (single-digit thousands, far below 2^52) and the RNG modulus by `n`,
// so the pedantic numeric-cast lints are noise here.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_lossless
)]
use pact_corpus::{categorise, default_root, factor, load, members, Skill};
use std::collections::BTreeMap;

/// `SplitMix64`. No `rand` dependency for one shuffle; the seed is reported.
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

#[derive(Debug, Clone, Copy, Default)]
struct Metrics {
    typecheck: usize,
    chain_pairs_ok: usize,
    chain_pairs_total: usize,
    never_fires: usize,
    composable_pairs: usize,
    max_pressure: f64,
    dead_ends: usize,
    source_only: usize,
}

fn metrics(skills: &[Skill], chains: &[(String, Vec<String>)]) -> Metrics {
    let category = categorise(skills);
    let statics = pact_analysis::statics::analyse(skills, &category);
    let by_name: BTreeMap<String, Skill> =
        skills.iter().map(|s| (s.name.clone(), s.clone())).collect();
    let factorings: Vec<_> = chains
        .iter()
        .map(|(name, seq)| factor(name, seq, &by_name))
        .collect();
    let typecheck = factorings.iter().filter(|f| f.typechecks).count();
    // Pair-level: every chain is one Bernoulli trial per consecutive pair,
    // which sharpens the n=13 chain-level test without changing its meaning.
    let (mut pairs_ok, mut pairs_total) = (0, 0);
    for f in &factorings {
        let broken = f.word.split_whitespace().filter(|s| *s == "✗").count();
        // Word symbols beyond pairs (⊕, *, ↺) are stage markers, not pairs.
        let pair_marks = f
            .word
            .split_whitespace()
            .filter(|s| matches!(*s, "⊗" | "▸" | "✗"))
            .count();
        pairs_total += pair_marks;
        pairs_ok += pair_marks - broken;
    }
    let max_pressure = category
        .degrees
        .values()
        .filter(|d| d.consumed_by > 0)
        .map(|d| d.produced_by as f64 / d.consumed_by as f64)
        .fold(0.0_f64, f64::max);
    Metrics {
        typecheck,
        chain_pairs_ok: pairs_ok,
        chain_pairs_total: pairs_total,
        never_fires: statics.never_fires.len(),
        composable_pairs: statics.composable_pairs,
        max_pressure,
        dead_ends: statics.dead_ends.len(),
        source_only: statics.source_only.len(),
    }
}

fn report(name: &str, base: f64, null: &[f64], good_when_high: bool) {
    let n = null.len() as f64;
    let mean = null.iter().sum::<f64>() / n;
    let min = null.iter().copied().fold(f64::INFINITY, f64::min);
    let max = null.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let extreme = if good_when_high {
        null.iter().filter(|&&v| v >= base).count() as f64 / n
    } else {
        null.iter().filter(|&&v| v <= base).count() as f64 / n
    };
    println!(
        "{name:>16}: baseline {base:>8.2}  null mean {mean:>8.2}  range [{min:.2}, {max:.2}]  p={extreme:.4}"
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let reps: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(2000);
    let seed: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(7);
    let mode: String = args.get(3).cloned().unwrap_or_else(|| "perm".to_string());
    assert!(
        mode == "perm" || mode == "boot",
        "mode must be perm or boot"
    );

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

    println!(
        "E1 ({}): {} skills, {} chains, {} reps, seed {}",
        mode,
        skills.len(),
        chains.len(),
        reps,
        seed
    );
    let base = metrics(&skills, &chains);

    // Shuffle inputs and outputs INDEPENDENTLY. Permuting whole signatures as
    // units would preserve the signature multiset, making every degree
    // statistic invariant by construction and the test vacuous. The question
    // is how much power comes from the vocabulary (the per-side marginals,
    // which independent shuffling preserves) versus the corpus's specific
    // skill-to-signature links (which it destroys).
    let mut inputs: Vec<pact_core::Type> = skills.iter().map(|s| s.input.clone()).collect();
    let mut outputs: Vec<Vec<pact_core::Type>> = skills.iter().map(|s| s.outputs.clone()).collect();
    let mut rng = Rng(seed);
    let mut null_tc = Vec::with_capacity(reps);
    let mut null_pairs = Vec::with_capacity(reps);
    let mut null_nf = Vec::with_capacity(reps);
    let mut null_cp = Vec::with_capacity(reps);
    let mut null_press = Vec::with_capacity(reps);
    let mut null_de = Vec::with_capacity(reps);
    let mut null_src = Vec::with_capacity(reps);

    for _ in 0..reps {
        if mode == "perm" {
            for i in (1..inputs.len()).rev() {
                inputs.swap(i, rng.below(i + 1));
            }
            for i in (1..outputs.len()).rev() {
                outputs.swap(i, rng.below(i + 1));
            }
        }
        let permuted: Vec<Skill> = skills
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let (input, outs) = if mode == "perm" {
                    (&inputs[i], &outputs[i])
                } else {
                    (
                        &inputs[rng.below(inputs.len())],
                        &outputs[rng.below(outputs.len())],
                    )
                };
                Skill {
                    input: input.clone(),
                    outputs: outs.clone(),
                    ..s.clone()
                }
            })
            .collect();
        let m = metrics(&permuted, &chains);
        null_tc.push(m.typecheck as f64);
        null_pairs.push(m.chain_pairs_ok as f64);
        null_nf.push(m.never_fires as f64);
        null_cp.push(m.composable_pairs as f64);
        null_press.push(m.max_pressure);
        null_de.push(m.dead_ends as f64);
        null_src.push(m.source_only as f64);
    }

    report("typecheck", base.typecheck as f64, &null_tc, true);
    println!(
        "   (chain consecutive pairs: {} total)",
        base.chain_pairs_total
    );
    report("pairs_ok", base.chain_pairs_ok as f64, &null_pairs, true);
    report("composable", base.composable_pairs as f64, &null_cp, true);
    report("never_fires", base.never_fires as f64, &null_nf, false);
    report("max_pressure", base.max_pressure, &null_press, false);
    report("dead_ends", base.dead_ends as f64, &null_de, false);
    report("source_only", base.source_only as f64, &null_src, false);
}
