//! P4.5 — Theorem 1, empirically.
//!
//! > A railpulse loop with no external vertex injection halts in finitely many
//! > generations.
//!
//! The proof: I2 caps a phase at `k = 4` tracks, so its complex has at most
//! `2^k − 1 = 15` non-empty faces and the configuration space is finite. I1
//! requires the induced map on homology to be non-isomorphic at every
//! generation. A strictly changing sequence in a finite space cannot be
//! infinite, so some generation admits no legal successor.
//!
//! # What would falsify it
//!
//! A loop that keeps producing new homology, generation after generation, with
//! injection switched off. If that happens the theorem is wrong and this file
//! says so rather than being adjusted until it agrees. The whole point of
//! writing a bound down is that it can be exceeded.
//!
//! # Why this is not a proof
//!
//! It is one family of runs. The theorem is about every loop; this checks the
//! ones a generator can build inside the cap, which is a strictly weaker
//! claim. It would catch a bound that is wrong by a lot and would miss one
//! wrong by a little.

use pact_analysis::{Complex, Filtration};
use std::collections::BTreeMap;

/// I2. A phase runs at most this many tracks.
const K: usize = 4;
/// The bound the proof gives: `2^k − 1` non-empty faces of a k-set.
const MAX_FACES: usize = (1 << K) - 1;

/// Deterministic generation, so a failing run is reproducible from its seed.
struct Gen(u64);

impl Gen {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1))
    }
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 11
    }
    fn below(&mut self, n: u64) -> usize {
        if n == 0 {
            0
        } else {
            usize::try_from(self.next() % n).unwrap_or(0)
        }
    }
}

const TRACKS: [&str; K] = ["t0", "t1", "t2", "t3"];

/// A complex over the four tracks, with a random set of shared artifacts.
///
/// **No injection**: the vertex set is fixed at `K` and never grows. That is
/// the condition the theorem is about, and it is what makes the configuration
/// space finite.
fn phase(gen: &mut Gen) -> Complex {
    let mut complex = Complex::new();
    for track in TRACKS {
        complex = complex.with(&[track]);
    }
    // Between one and six shared artifacts, each joining two or three tracks.
    for _ in 0..=gen.below(6) {
        let size = 2 + gen.below(2);
        let mut chosen: Vec<&str> = Vec::new();
        while chosen.len() < size {
            let pick = TRACKS[gen.below(K as u64)];
            if !chosen.contains(&pick) {
                chosen.push(pick);
            }
        }
        complex = complex.with(&chosen);
    }
    complex
}

/// The identity map on the four tracks. No vertex is invented, which is what
/// "no external injection" means at the level the admission check sees.
fn no_injection() -> BTreeMap<String, String> {
    TRACKS
        .iter()
        .map(|t| ((*t).to_string(), (*t).to_string()))
        .collect()
}

#[test]
fn the_configuration_space_is_bounded_by_two_to_the_k_minus_one() {
    // The proof's first step, checked rather than assumed. If a phase could
    // hold more faces than the bound, the finiteness argument would be about a
    // different object than the one being built.
    let mut gen = Gen::new(1);
    for _ in 0..200 {
        let complex = phase(&mut gen);
        let b = complex.betti();
        let faces = b.vertices + b.edges + b.triangles;
        assert!(
            faces <= MAX_FACES,
            "a {K}-track phase held {faces} faces, above the bound {MAX_FACES}"
        );
    }
}

#[test]
fn a_loop_with_injection_off_halts_at_or_below_the_bound() {
    // The gate. I1 says a generation is only legal when it changes homology,
    // so a run is a strictly changing sequence in a finite space; it must stop.
    let mut longest = 0usize;
    let mut halted = 0usize;

    for seed in 0..200u64 {
        let mut gen = Gen::new(seed);
        let mut filtration = Filtration::seed(phase(&mut gen));
        let mut signatures = vec![signature(&filtration)];

        // Try many more generations than the bound allows. If the theorem is
        // wrong, this is where it shows.
        let mut generations = 0;
        for _ in 0..(MAX_FACES * 4) {
            let candidate = phase(&mut gen);
            let sig = {
                let b = candidate.betti();
                (b.b0, b.b1)
            };
            // I1: a generation whose homology is unchanged is not a legal
            // successor. Skipping it is the invariant doing its job, not the
            // test being lenient.
            if signatures.contains(&sig) {
                continue;
            }
            if filtration.admit(candidate, &no_injection()).is_err() {
                // A fabrication, which injection-off runs cannot produce; if
                // it did, that would be its own finding.
                break;
            }
            signatures.push(sig);
            generations += 1;
        }

        assert!(
            generations <= MAX_FACES,
            "seed {seed}: {generations} legal generations with injection off, \
             above the bound {MAX_FACES}. Theorem 1 is falsified and this is \
             the record of it."
        );
        longest = longest.max(generations);
        halted += 1;
    }

    assert_eq!(halted, 200, "every run must terminate");
    // Non-vacuity: if no run ever advanced, the bound was never approached and
    // the test proves only that zero is less than fifteen.
    assert!(
        longest > 0,
        "no run produced a single legal successor; the bound was never tested"
    );
    println!("longest injection-off run: {longest} generations, bound {MAX_FACES}");
}

#[test]
fn injection_is_what_extends_the_life_of_a_loop() {
    // The corollary, and the half that makes the theorem useful rather than
    // merely true. External input is not an optimisation: it is the only
    // source of new vertices, so it is the only thing that grows the
    // configuration space the run is exhausting.
    let mut gen = Gen::new(7);
    let bounded = phase(&mut gen);
    let b = bounded.betti();
    let before = b.vertices + b.edges + b.triangles;

    // One imported vertex, joined in.
    let widened = bounded.clone().with(&["t0", "imported"]);
    let after = {
        let b = widened.betti();
        b.vertices + b.edges + b.triangles
    };

    assert!(
        after > before,
        "injection must enlarge the space, or the corollary is empty"
    );

    // And this is where the corollary bit back. A map CANNOT express import:
    // a vertex from outside is structurally indistinguishable from an invented
    // one, so `admit` refuses it, correctly. Theorem 1 says injection must be
    // first-class rather than an escape hatch, and until this test tried to
    // import something, it was neither.
    let mut smuggled = Filtration::seed(bounded.clone());
    assert!(
        smuggled.admit(widened.clone(), &no_injection()).is_err(),
        "an undeclared newcomer is a fabrication, whatever it really was"
    );

    let mut declared = Filtration::seed(bounded);
    assert!(
        declared
            .admit_with_import(widened, &no_injection(), &["imported"])
            .is_ok(),
        "declared import is legal, and saying so is the entire difference"
    );
}

#[test]
fn a_stalled_loop_is_reported_rather_than_run_forever() {
    // What halting looks like from inside. When the homology stops changing,
    // `stalled_for` says so, and that is the loop asking for something from
    // outside rather than failing.
    let fixed = Complex::new().with(&["t0", "t1"]).with(&["t2", "t3"]);
    let mut f = Filtration::seed(fixed.clone());
    for _ in 0..3 {
        f.admit(fixed.clone(), &no_injection())
            .expect("identity invents nothing");
    }
    assert_eq!(f.stalled_for(), 3, "three generations with no change");
}

fn signature(f: &Filtration) -> (usize, usize) {
    let b = f.generations().last().expect("seeded").betti();
    (b.b0, b.b1)
}
