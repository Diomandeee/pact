//! The barcode, and the door it comes through.
//!
//! Generations form a persistence module `K₁ → K₂ → K₃ → …`. A *bar* is a
//! homology class with a birth and a death. The barcode is the output; the
//! complexes are scaffolding.
//!
//! # Admission, not reporting
//!
//! A face in `K_{n+1}` with no preimage in `K_n` was **invented** by the
//! compaction. That is P3 in `SPEC.md`, and it is the only structural detector
//! of a hallucinated connection there is.
//!
//! It is checked at the door. A fabricated face admitted and flagged later has
//! already entered the barcode, and every bar computed after it is a bar about
//! a complex that never existed. Rejecting at admission means the module only
//! ever contains generations that came from somewhere.
//!
//! # What this barcode identifies, and what it only counts
//!
//! `b₀` bars are **identified**: a connected component is a set of vertices,
//! the simplicial map says where those vertices go, so a component can be
//! followed generation to generation and its birth and death are exact.
//!
//! `b₁` bars are **counted**, not identified. Following a specific cycle
//! through a filtration needs the persistence algorithm proper, and this is
//! not it. What is reported is how many independent cycles each generation has
//! and where that number changes, which is enough to say "a hole opened at
//! generation 3 and closed at 5" and not enough to say which hole.
//!
//! Stated here rather than implied, because a barcode that quietly counted
//! would look exactly like one that tracked.

use crate::homology::Complex;
use std::collections::{BTreeMap, BTreeSet};

/// A face that appeared with nothing behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fabrication {
    /// The face in the target with no preimage.
    pub face: Vec<String>,
    /// Which generation was being admitted.
    pub generation: usize,
    /// The vertices of that face the map cannot account for.
    pub unmapped: Vec<String>,
}

impl std::fmt::Display for Fabrication {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "generation {}: the face {:?} has no preimage; {:?} came from nowhere",
            self.generation, self.face, self.unmapped
        )
    }
}

/// One bar: a class, when it appeared, when it went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bar {
    /// What the class is. For `b₀`, the component's vertices at birth.
    pub label: String,
    /// Generation it appeared in.
    pub birth: usize,
    /// Generation it disappeared in, or `None` if it survives.
    pub death: Option<usize>,
}

impl Bar {
    /// How many generations it lasted, so far.
    #[must_use]
    pub fn length(&self, latest: usize) -> usize {
        self.death.unwrap_or(latest + 1).saturating_sub(self.birth)
    }
}

/// A sequence of generations, and the maps between them.
#[derive(Debug, Clone, Default)]
pub struct Filtration {
    generations: Vec<Complex>,
}

impl Filtration {
    /// An empty filtration.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed with the first generation.
    ///
    /// Nothing to check: there is no source for it to fabricate against.
    #[must_use]
    pub fn seed(complex: Complex) -> Self {
        Self {
            generations: vec![complex],
        }
    }

    /// How many generations so far.
    #[must_use]
    pub fn len(&self) -> usize {
        self.generations.len()
    }

    /// Whether any generation exists.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.generations.is_empty()
    }

    /// Every generation.
    #[must_use]
    pub fn generations(&self) -> &[Complex] {
        &self.generations
    }

    /// Admit the next generation, under a simplicial map from the last.
    ///
    /// `map` sends each vertex of the previous generation to a vertex of the
    /// next. It **may collapse** — two vertices may share an image, which is
    /// what compaction does — and it **may not invent**: every vertex of the
    /// next generation must be the image of something.
    ///
    /// # Errors
    ///
    /// The first fabricated face, naming which vertices came from nowhere. At
    /// the door, before it can reach a barcode.
    pub fn admit(
        &mut self,
        next: Complex,
        map: &BTreeMap<String, String>,
    ) -> Result<(), Fabrication> {
        self.admit_with_import(next, map, &[])
    }

    /// Admit the next generation, declaring vertices imported from outside.
    ///
    /// # Why this exists, and why `admit` alone was not enough
    ///
    /// `admit` allows a generation's vertices to be images of the previous
    /// one's, and nothing else. That is exactly right for catching
    /// fabrication, and it means **a map cannot express import at all**: a
    /// vertex from outside is indistinguishable from an invented one, because
    /// structurally that is what it is.
    ///
    /// Theorem 1's corollary says external input is not an optimisation, it is
    /// the only source of new vertices and therefore the only thing that
    /// extends a loop's life — so any implementation must make injection a
    /// first-class operation rather than an escape hatch. It did not, until a
    /// test written for the corollary tried to import something and could not.
    ///
    /// So import is declared, in the call, by name. An undeclared newcomer is
    /// still a fabrication. The difference between import and hallucination is
    /// not structural and never will be; it is whether someone said so.
    ///
    /// # Errors
    ///
    /// The first face containing a vertex that is neither an image nor a
    /// declared import.
    pub fn admit_with_import(
        &mut self,
        next: Complex,
        map: &BTreeMap<String, String>,
        imported: &[&str],
    ) -> Result<(), Fabrication> {
        let generation = self.generations.len();
        if let Some(previous) = self.generations.last() {
            let images: BTreeSet<String> = previous
                .faces(0)
                .iter()
                .filter_map(|v| map.get(&v[0]).cloned())
                .collect();
            let declared: BTreeSet<String> = imported.iter().map(|s| (*s).to_string()).collect();
            let reachable: BTreeSet<&String> = images.union(&declared).collect();

            for dim in 0..3 {
                for face in next.faces(dim) {
                    let unmapped: Vec<String> = face
                        .iter()
                        .filter(|v| !reachable.contains(v))
                        .cloned()
                        .collect();
                    if !unmapped.is_empty() {
                        return Err(Fabrication {
                            face,
                            generation,
                            unmapped,
                        });
                    }
                }
            }
        }
        self.generations.push(next);
        Ok(())
    }

    /// The `b₀` barcode: components followed generation to generation.
    ///
    /// Identified, not counted. A component is a set of vertices, and a
    /// component in generation `n+1` continues one from generation `n` when
    /// they share a vertex.
    #[must_use]
    pub fn components_barcode(&self) -> Vec<Bar> {
        let mut bars: Vec<Bar> = Vec::new();
        let mut live: Vec<(usize, BTreeSet<String>)> = Vec::new();

        for (n, complex) in self.generations.iter().enumerate() {
            let mut carried: Vec<(usize, BTreeSet<String>)> = Vec::new();

            for part in components(complex) {
                if let Some(i) = live.iter().position(|(_, was)| !was.is_disjoint(&part)) {
                    carried.push((live[i].0, part));
                } else {
                    bars.push(Bar {
                        label: label_of(&part),
                        birth: n,
                        death: None,
                    });
                    carried.push((bars.len() - 1, part));
                }
            }

            // Anything live that nothing continued died entering this
            // generation.
            for (index, _) in &live {
                if !carried.iter().any(|(i, _)| i == index) {
                    bars[*index].death = Some(n);
                }
            }
            live = carried;
        }
        bars
    }

    /// How many independent cycles each generation has.
    ///
    /// Counted, not identified. See the module docs for why that distinction
    /// is stated rather than left for someone to discover.
    #[must_use]
    pub fn cycle_counts(&self) -> Vec<usize> {
        self.generations.iter().map(|k| k.betti().b1).collect()
    }

    /// How many consecutive generations have had identical homology.
    ///
    /// Invariant 1: the induced map on homology must not be an isomorphism at
    /// every generation. A run of generations with identical `(b₀, b₁)` is the
    /// loop telling you it needs something from outside, which is Theorem 1
    /// arriving empirically rather than as a proof sketch.
    #[must_use]
    pub fn stalled_for(&self) -> usize {
        let signature = |k: &Complex| {
            let b = k.betti();
            (b.b0, b.b1)
        };
        let mut run = 0;
        for pair in self.generations.windows(2) {
            if signature(&pair[0]) == signature(&pair[1]) {
                run += 1;
            } else {
                run = 0;
            }
        }
        run
    }
}

fn label_of(part: &BTreeSet<String>) -> String {
    part.iter().cloned().collect::<Vec<_>>().join("+")
}

/// Connected components, as vertex sets.
fn components(complex: &Complex) -> Vec<BTreeSet<String>> {
    let mut neighbours: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for v in complex.faces(0) {
        neighbours.entry(v[0].clone()).or_default();
    }
    for e in complex.faces(1) {
        neighbours
            .entry(e[0].clone())
            .or_default()
            .insert(e[1].clone());
        neighbours
            .entry(e[1].clone())
            .or_default()
            .insert(e[0].clone());
    }

    let mut unvisited: BTreeSet<String> = neighbours.keys().cloned().collect();
    let mut out = Vec::new();
    while let Some(seed) = unvisited.iter().next().cloned() {
        let mut part = BTreeSet::new();
        let mut queue = vec![seed];
        while let Some(v) = queue.pop() {
            if !unvisited.remove(&v) {
                continue;
            }
            part.insert(v.clone());
            if let Some(next) = neighbours.get(&v) {
                queue.extend(next.iter().cloned());
            }
        }
        out.push(part);
    }
    out
}
