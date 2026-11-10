//! Betti numbers over GF(2).
//!
//! `b₀` is connected components. `b₀ > 1` means the plan is a disjoint union
//! pretending to be a plan, which is Invariant 3 failing.
//!
//! `b₁` is independent cycles that do not bound. Each one is a dependency loop
//! with no deliverable closing it: everybody busy, nothing built. `b₁ → 0` is
//! the completion criterion, not a checked task list.
//!
//! # Plain Gaussian elimination, on purpose
//!
//! Complexes here are small — a phase is capped at four tracks by I2, so at
//! most fifteen faces — and the bound exists precisely so a human can check
//! the answer. A clever algorithm would be faster and would make the number
//! another assertion. `loop_harness.py` made the same choice and said so.

use std::collections::BTreeSet;

/// A simplicial complex, given by its maximal faces.
///
/// Closed downward on read: a complex listing a triangle implicitly contains
/// its edges. Requiring the author to write them out would only create a way
/// to be inconsistent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Complex {
    faces: Vec<Vec<String>>,
}

/// `b₀` and `b₁`, with the counts they were computed from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Betti {
    /// Connected components.
    pub b0: usize,
    /// Independent non-bounding cycles.
    pub b1: usize,
    /// 0-faces.
    pub vertices: usize,
    /// 1-faces.
    pub edges: usize,
    /// 2-faces.
    pub triangles: usize,
}

impl Complex {
    /// An empty complex.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a maximal face. Its sub-faces come for free.
    #[must_use]
    pub fn with(mut self, face: &[&str]) -> Self {
        let mut sorted: Vec<String> = face.iter().map(ToString::to_string).collect();
        sorted.sort();
        sorted.dedup();
        self.faces.push(sorted);
        self
    }

    /// Every `dim`-face, closed downward, sorted.
    #[must_use]
    pub fn faces(&self, dim: usize) -> Vec<Vec<String>> {
        let mut out: BTreeSet<Vec<String>> = BTreeSet::new();
        for face in &self.faces {
            if face.len() <= dim {
                continue;
            }
            combinations(face, dim + 1, &mut out);
        }
        out.into_iter().collect()
    }

    /// `b₀` and `b₁` over GF(2).
    ///
    /// # Panics
    ///
    /// If a face is missing from the closure it was computed from, which
    /// cannot happen: both come from [`Self::faces`] over the same complex.
    /// The `expect` is there so that if downward closure ever stops being
    /// closed, it fails loudly rather than returning a Betti number computed
    /// from a hole.
    #[must_use]
    pub fn betti(&self) -> Betti {
        let verts = self.faces(0);
        let edges = self.faces(1);
        let tris = self.faces(2);
        if verts.is_empty() {
            return Betti {
                b0: 0,
                b1: 0,
                vertices: 0,
                edges: 0,
                triangles: 0,
            };
        }

        let index = |set: &[Vec<String>], face: &[String]| -> usize {
            set.iter().position(|f| f == face).expect("face is present")
        };

        // d1: edges → vertices.
        let d1: Vec<u128> = edges
            .iter()
            .map(|e| {
                let a = index(&verts, std::slice::from_ref(&e[0]));
                let b = index(&verts, std::slice::from_ref(&e[1]));
                (1u128 << a) | (1u128 << b)
            })
            .collect();
        let rank_d1 = rank_gf2(d1);

        // d2: triangles → edges.
        let d2: Vec<u128> = tris
            .iter()
            .map(|t| {
                let mut mask = 0u128;
                for pair in pairs(t) {
                    mask |= 1u128 << index(&edges, &pair);
                }
                mask
            })
            .collect();
        let rank_d2 = rank_gf2(d2);

        Betti {
            b0: verts.len() - rank_d1,
            b1: (edges.len() - rank_d1).saturating_sub(rank_d2),
            vertices: verts.len(),
            edges: edges.len(),
            triangles: tris.len(),
        }
    }
}

/// Rank of a 0/1 matrix given as bitmasks, over GF(2).
fn rank_gf2(rows: Vec<u128>) -> usize {
    let mut rows: Vec<u128> = rows.into_iter().filter(|r| *r != 0).collect();
    let mut rank = 0;
    while let Some(pivot_row) = rows.pop() {
        // Lowest set bit. Any consistent pivot choice gives the same rank;
        // this one matches the oracle so a disagreement is a real one.
        let pivot = pivot_row & pivot_row.wrapping_neg();
        rank += 1;
        rows = rows
            .into_iter()
            .map(|r| if r & pivot != 0 { r ^ pivot_row } else { r })
            .filter(|r| *r != 0)
            .collect();
    }
    rank
}

fn combinations(face: &[String], size: usize, out: &mut BTreeSet<Vec<String>>) {
    let n = face.len();
    if size > n {
        return;
    }
    let mut idx: Vec<usize> = (0..size).collect();
    loop {
        out.insert(idx.iter().map(|i| face[*i].clone()).collect());
        let mut i = size;
        loop {
            if i == 0 {
                return;
            }
            i -= 1;
            if idx[i] != i + n - size {
                break;
            }
            if i == 0 {
                return;
            }
        }
        idx[i] += 1;
        for j in i + 1..size {
            idx[j] = idx[j - 1] + 1;
        }
    }
}

fn pairs(face: &[String]) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    for i in 0..face.len() {
        for j in i + 1..face.len() {
            out.push(vec![face[i].clone(), face[j].clone()]);
        }
    }
    out
}
