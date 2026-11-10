//! What the corpus does when it runs.
//!
//! Pressure, starvation, feedback loops. `prompt_economy.py` is the oracle and
//! P4.2 diffs against it.
//!
//! # Machinations, read off a category
//!
//! A type with more producers than consumers is a pool that accumulates. With
//! none it is a drain; with no producers it is a source. That reading is not a
//! metaphor laid over the category — it is the same numbers, and it is why the
//! importer builds a bipartite graph rather than keeping two models in step.
//!
//! # Refinement
//!
//! The economy counts a free coercion `M_x ⇒ M` per produced subtype (P3.6,
//! [`pact_corpus::coercions`]): one more consumer of `M_x`, one more producer
//! of `M`, and an edge `M_x → M` in the type graph. Composition already reads
//! types that way; without it a reader of `M` would look starved here while it
//! typechecks in every chain.

use pact_corpus::{economy_degrees, Category};
use std::collections::{BTreeMap, BTreeSet};

/// What a type behaves like in the economy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Machination {
    /// Produced by nothing: work enters here.
    Source,
    /// Consumed by nothing: work leaves here.
    Drain,
    /// Both. Whether it turns over is what pressure says.
    Pool,
}

/// One type, as the economy sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// The type.
    pub id: String,
    /// What it behaves like.
    pub kind: Machination,
    /// How many skills produce it.
    pub inflow: usize,
    /// How many consume it.
    pub outflow: usize,
    /// `inflow / outflow`, to two places. `None` when nothing consumes it.
    ///
    /// Above one it accumulates, below one it starves, around one it turns
    /// over. The shape of the imbalance, not its size.
    pub pressure: Option<f64>,
}

/// The dynamic picture.
#[derive(Debug, Clone, PartialEq)]
pub struct Dynamics {
    /// Every type, with its flow.
    pub nodes: Vec<Node>,
    /// Pools at or above the pressure threshold.
    pub unbounded: Vec<Node>,
    /// Cycles in the type graph, each as the types it passes through.
    pub feedback_loops: Vec<Vec<String>>,
}

impl Dynamics {
    /// One line on whether this can sustain itself.
    #[must_use]
    pub fn verdict(&self) -> String {
        if self.feedback_loops.is_empty() {
            "PIPELINE, not an economy: no feedback loop, so it runs once and drains".to_string()
        } else {
            format!(
                "{} feedback loop(s); the economy can sustain",
                self.feedback_loops.len()
            )
        }
    }
}

/// Longest cycle the walk will follow.
///
/// The oracle's bound, copied. A 1-cycle is a skill eating its own output
/// (refinement); a longer one is a real engine. Unbounded search over a dense
/// type graph does not terminate usefully, and five is where the oracle
/// stopped.
const MAX_CYCLE: usize = 5;

/// Read the dynamic structure of a corpus.
///
/// `edges` is the type graph: `a → b` when some skill reads `a` and emits `b`.
#[must_use]
pub fn analyse(category: &Category, edges: &BTreeMap<String, BTreeSet<String>>) -> Dynamics {
    let degrees = economy_degrees(category);
    let mut nodes = Vec::new();
    for ty in &category.objects {
        let degree = degrees[ty];
        let (inflow, outflow) = (degree.produced_by, degree.consumed_by);
        let kind = if inflow == 0 && outflow > 0 {
            Machination::Source
        } else if outflow == 0 && inflow > 0 {
            Machination::Drain
        } else {
            Machination::Pool
        };
        nodes.push(Node {
            id: ty.to_string(),
            kind,
            inflow,
            outflow,
            pressure: (outflow > 0).then(|| pressure(inflow, outflow)),
        });
    }

    let unbounded = nodes
        .iter()
        .filter(|n| n.kind == Machination::Pool && n.pressure.unwrap_or(0.0) >= 3.0)
        .cloned()
        .collect();

    Dynamics {
        feedback_loops: cycles(&nodes, edges),
        nodes,
        unbounded,
    }
}

/// Pressure, to two places, matching the oracle's `round(x, 2)`.
///
/// The counts are skill tallies: tens, not quadrillions. `f64` loses precision
/// above 2^53 and a corpus that large has problems this analysis will not be
/// the one to find, so the narrowing is stated once here rather than allowed
/// at each call site.
fn pressure(inflow: usize, outflow: usize) -> f64 {
    let ratio = f64::from(u32::try_from(inflow).unwrap_or(u32::MAX))
        / f64::from(u32::try_from(outflow).unwrap_or(u32::MAX));
    (ratio * 100.0).round() / 100.0
}

/// Cycles through the type graph, deduplicated by their node set.
///
/// By set, not by sequence, exactly as the oracle does it: two rotations of
/// one cycle are one loop, and reporting both would double every count in the
/// verdict.
fn cycles(nodes: &[Node], edges: &BTreeMap<String, BTreeSet<String>>) -> Vec<Vec<String>> {
    let mut found: Vec<Vec<String>> = Vec::new();
    let mut seen: BTreeSet<BTreeSet<String>> = BTreeSet::new();

    for node in nodes {
        walk(
            &node.id,
            &node.id,
            &mut vec![node.id.clone()],
            edges,
            &mut seen,
            &mut found,
        );
    }
    found
}

fn walk(
    start: &str,
    at: &str,
    path: &mut Vec<String>,
    edges: &BTreeMap<String, BTreeSet<String>>,
    seen: &mut BTreeSet<BTreeSet<String>>,
    found: &mut Vec<Vec<String>>,
) {
    let Some(next) = edges.get(at) else {
        return;
    };
    for target in next {
        if target == start {
            let key: BTreeSet<String> = path.iter().cloned().collect();
            if seen.insert(key) {
                let mut loop_path = path.clone();
                loop_path.push(start.to_string());
                found.push(loop_path);
            }
        } else if !path.contains(target) && path.len() < MAX_CYCLE {
            path.push(target.clone());
            walk(start, target, path, edges, seen, found);
            path.pop();
        }
    }
}

/// The type graph: `a → b` when some skill reads `a` and emits `b`, plus
/// `M_x → M` for each coercion.
#[must_use]
pub fn type_edges(skills: &[pact_corpus::Skill]) -> BTreeMap<String, BTreeSet<String>> {
    let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for skill in skills {
        for out in &skill.outputs {
            edges
                .entry(skill.input.to_string())
                .or_default()
                .insert(out.to_string());
        }
    }
    for sub in pact_corpus::coercions(&pact_corpus::categorise(skills)) {
        edges
            .entry(sub.to_string())
            .or_default()
            .insert("M".to_string());
    }
    edges
}
