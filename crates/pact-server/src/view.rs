//! The wire shape.
//!
//! Separate types from the analysis layer's, on purpose. What a page wants is
//! flattened, sorted and pre-joined; what the analysis holds is neither. And
//! putting `serde` derives on `Statics`, `Factoring` and `Node` would spread a
//! dependency across three layers to serve one, which INV-8 exists to stop.
//!
//! # These must never disagree with the CLI
//!
//! Both read the same functions, so they cannot drift for arithmetic reasons.
//! They can drift for rendering reasons — a field dropped, a list sorted
//! differently, a number rounded on one side only — and that is the one
//! failure the 178 tests below this layer cannot see. `tests/agrees_with_cli.rs`
//! is the gate for exactly it.

use pact_analysis::{dynamics, statics};
use pact_corpus::{categorise, factor_all, Factoring, Skill};
use serde::{Deserialize, Serialize};

// The TypeScript is GENERATED from these structs, into app/src/types.ts,
// by tests/types.rs. `cfg_attr(test, ...)` keeps ts-rs a dev dependency so
// the served binary carries none of it.
//
// A field renamed here changes the generated file, and `deno check` then
// fails on whatever TypeScript referenced the old name. A mirror kept by
// hand drifts the first week nobody looks, and the drift is invisible until
// a panel renders blank with no error anywhere.

/// Everything one page load needs about the health of a corpus.
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../app/src/types.ts"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Overview {
    /// How many typed skills were read.
    pub skills: usize,
    /// How many types appear on either side of a signature.
    pub types: usize,
    /// Skills whose input nothing produces. Empty is the healthy answer.
    pub never_fires: Vec<String>,
    /// Types nothing produces: work enters here.
    pub source_only: Vec<String>,
    /// Types nothing consumes: work arrives and cannot continue.
    pub dead_ends: Vec<String>,
    /// Pools at or above the pressure threshold.
    pub pressure: Vec<PressurePoint>,
    /// How many cycles the type graph has.
    pub feedback_loops: usize,
    /// One line on whether this can sustain itself.
    pub verdict: String,
}

/// A type accumulating faster than it drains.
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../app/src/types.ts"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PressurePoint {
    /// The type.
    pub ty: String,
    /// `producers / consumers`, to two places.
    pub pressure: f64,
    /// How many skills emit it.
    pub producers: usize,
    /// How many read it.
    pub consumers: usize,
}

impl Overview {
    /// Read a corpus.
    #[must_use]
    pub fn of(skills: &[Skill]) -> Self {
        let category = categorise(skills);
        let stat = statics::analyse(skills, &category);
        let dyn_ = dynamics::analyse(&category, &dynamics::type_edges(skills));

        Self {
            skills: skills.len(),
            types: category.objects.len(),
            never_fires: stat.never_fires.clone(),
            source_only: stat.source_only.clone(),
            dead_ends: stat.dead_ends.clone(),
            pressure: dyn_
                .unbounded
                .iter()
                .map(|n| PressurePoint {
                    ty: n.id.clone(),
                    pressure: n.pressure.unwrap_or(0.0),
                    producers: n.inflow,
                    consumers: n.outflow,
                })
                .collect(),
            feedback_loops: dyn_.feedback_loops.len(),
            verdict: dyn_.verdict(),
        }
    }
}

/// One chain, as a page shows it.
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../app/src/types.ts"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Chain {
    /// Its name.
    pub name: String,
    /// The word, e.g. `⊗ ▸ * ▸ ⊕`.
    pub word: String,
    /// How many of the four canonical stages it has.
    pub present: usize,
    /// Which stages are absent. The actionable half: `2/4` alone sends
    /// someone back to the diagram.
    pub missing: Vec<String>,
    /// Whether every consecutive pair composes.
    pub typechecks: bool,
    /// Its members, in the order the factoring found them.
    pub members: Vec<String>,
    /// Why a pair is parallel, or why one does not compose.
    pub notes: Vec<String>,
}

impl From<&Factoring> for Chain {
    fn from(f: &Factoring) -> Self {
        let mut missing = Vec::new();
        for (label, present) in [
            ("⊗ expand", f.stages.expand),
            ("* execute", f.stages.execute),
            ("⊕ select", f.stages.select),
            ("↺ repeat", f.stages.repeat),
        ] {
            if !present {
                missing.push(label.to_string());
            }
        }
        Self {
            name: f.chain.clone(),
            word: f.word.clone(),
            present: f.stages.present(),
            missing,
            typechecks: f.typechecks,
            members: f.members.clone(),
            notes: f.notes.clone(),
        }
    }
}

/// Every chain, with the corpus-wide tally that says which generator is short.
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../app/src/types.ts"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Chains {
    /// One entry per chain, by name.
    pub chains: Vec<Chain>,
    /// How many chains lack each stage. The number that names a missing
    /// generator rather than the chains that suffer from it.
    pub missing_across_corpus: Vec<(String, usize)>,
}

impl Chains {
    /// Factor every chain in a corpus.
    #[must_use]
    pub fn of(skills: &[Skill]) -> Self {
        let factorings = factor_all(skills);
        let chains: Vec<Chain> = factorings.iter().map(Chain::from).collect();

        let mut tally: Vec<(String, usize)> = ["⊗ expand", "* execute", "⊕ select", "↺ repeat"]
            .iter()
            .map(|stage| {
                let count = chains
                    .iter()
                    .filter(|c| c.missing.iter().any(|m| m == stage))
                    .count();
                ((*stage).to_string(), count)
            })
            .collect();
        tally.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

        Self {
            chains,
            missing_across_corpus: tally,
        }
    }
}

/// A node in the type-flow graph.
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../app/src/types.ts"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GraphNode {
    /// The type.
    pub id: String,
    /// `source`, `drain` or `pool`.
    pub kind: String,
    /// How many skills produce it.
    pub inflow: usize,
    /// How many consume it.
    pub outflow: usize,
    /// `producers / consumers`, when anything consumes it.
    pub pressure: Option<f64>,
    /// Whether it is at or above the pressure threshold.
    pub under_pressure: bool,
}

/// A skill, drawn as the arrow it is.
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../app/src/types.ts"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GraphEdge {
    /// The type it reads.
    pub from: String,
    /// A type it may emit.
    pub to: String,
    /// The skill.
    pub skill: String,
    /// Whether it can never fire.
    pub dead: bool,
}

/// The whole diagram, as data rather than as a picture.
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../app/src/types.ts"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Graph {
    /// Types.
    pub nodes: Vec<GraphNode>,
    /// Skills.
    pub edges: Vec<GraphEdge>,
}

impl Graph {
    /// Read a corpus as a diagram.
    ///
    /// Skills are edges rather than nodes. A skill *is* the arrow from one
    /// type to another, and drawing it as a node halves the readability for
    /// no gain — the same choice `pact export` already makes for Mermaid.
    #[must_use]
    pub fn of(skills: &[Skill]) -> Self {
        let category = categorise(skills);
        let stat = statics::analyse(skills, &category);
        let dyn_ = dynamics::analyse(&category, &dynamics::type_edges(skills));

        let nodes = dyn_
            .nodes
            .iter()
            .map(|n| GraphNode {
                id: n.id.clone(),
                kind: match n.kind {
                    dynamics::Machination::Source => "source",
                    dynamics::Machination::Drain => "drain",
                    dynamics::Machination::Pool => "pool",
                }
                .to_string(),
                inflow: n.inflow,
                outflow: n.outflow,
                pressure: n.pressure,
                under_pressure: dyn_.unbounded.iter().any(|u| u.id == n.id),
            })
            .collect();

        let edges = skills
            .iter()
            .flat_map(|s| {
                let dead = stat.never_fires.contains(&s.name);
                s.outputs.iter().map(move |out| GraphEdge {
                    from: s.input.to_string(),
                    to: out.to_string(),
                    skill: s.name.clone(),
                    dead,
                })
            })
            // Refinement: each produced `M_*` subtype is read as `M` by one coercion, which the
            // economy counts as a consumer (P3.6). Drawn too, or a pool's outflow outnumbers its arrows.
            .chain(
                pact_corpus::coercions(&category)
                    .into_iter()
                    .map(|sub| GraphEdge {
                        from: sub.to_string(),
                        to: "M".to_string(),
                        skill: format!("coerce:{sub}"),
                        dead: false,
                    }),
            )
            .collect();

        Self { nodes, edges }
    }
}

/// One skill in detail, with what it can reach.
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../app/src/types.ts"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillDetail {
    /// Its name.
    pub name: String,
    /// What it reads.
    pub input: String,
    /// What it may emit.
    pub outputs: Vec<String>,
    /// SOOP-2 category, when declared.
    pub category: Option<String>,
    /// Whether `f ∘ f = f`.
    pub idempotent: bool,
    /// Skills whose output it can read.
    pub upstream: Vec<String>,
    /// Skills that can read its output.
    pub downstream: Vec<String>,
    /// Chains naming it.
    pub in_chains: Vec<String>,
    /// Whether nothing produces what it reads.
    pub can_fire: bool,
}

impl SkillDetail {
    /// Look one skill up. `None` when there is no such skill — which is a
    /// different answer from a skill with nothing to say about it.
    #[must_use]
    pub fn of(skills: &[Skill], name: &str) -> Option<Self> {
        let skill = skills.iter().find(|s| s.name == name)?;
        let category = categorise(skills);
        let stat = statics::analyse(skills, &category);

        Some(Self {
            name: skill.name.clone(),
            input: skill.input.to_string(),
            outputs: skill.outputs.iter().map(ToString::to_string).collect(),
            category: skill.category.clone(),
            idempotent: skill.idempotent,
            upstream: skills
                .iter()
                .filter(|s| s.name != skill.name && s.outputs.contains(&skill.input))
                .map(|s| s.name.clone())
                .collect(),
            downstream: skills
                .iter()
                .filter(|s| s.name != skill.name && skill.outputs.contains(&s.input))
                .map(|s| s.name.clone())
                .collect(),
            in_chains: factor_all(skills)
                .iter()
                .filter(|f| f.members.contains(&skill.name))
                .map(|f| f.chain.clone())
                .collect(),
            can_fire: !stat.never_fires.contains(&skill.name),
        })
    }
}

// ─────────────────────────────────────────────────────────── the run

/// One step of a run, as an animation needs it.
///
/// Levels and flows, nothing else. A trace carries thousands of events and a
/// canvas needs two things per frame: how full each pool is, and what moved
/// where. Sending the raw events would push the aggregation into the browser,
/// where it would be done differently from how the ledger does it.
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../app/src/types.ts"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Frame {
    /// How much each type holds after this step.
    ///
    /// `u32`, not `u64`, and that is a wire decision rather than a modelling
    /// one. The engine counts in `u64`; JavaScript's `number` is exact only to
    /// 2^53, so `ts-rs` maps `u64` to `bigint` and every arithmetic site in
    /// the page has to convert. These are skill counts — tens to hundreds —
    /// so the view narrows and says so. The conversion saturates, which shows
    /// up as an absurd number rather than a wrong small one.
    pub levels: Vec<(String, u32)>,
    /// Type-to-type movement during this step, aggregated.
    pub flows: Vec<Flow>,
    /// Skills that fired.
    pub fired: Vec<String>,
    /// Skills that were eligible and could not gather what they needed.
    pub starved: usize,
}

/// Work moving from one type to another.
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../app/src/types.ts"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Flow {
    /// The type it left.
    pub from: String,
    /// The type it became.
    pub to: String,
    /// How much. `u32` for the same reason as `Frame::levels`.
    pub n: u32,
    /// Which skill did it.
    pub via: String,
}

/// A whole run, frame by frame.
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../app/src/types.ts"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Run {
    /// Every type, in the order the frames list them.
    pub types: Vec<String>,
    /// One entry per step.
    pub frames: Vec<Frame>,
    /// The seed, so the run can be reproduced exactly. INV-3.
    pub seed: u64,
    /// Whether the books balanced.
    pub conserved: bool,
}

impl Run {
    /// Run the corpus and record what happened.
    ///
    /// Source-only types are seeded, because nothing else will seed them: a
    /// source-only type is what a person brings, and a run with none of them
    /// does nothing and reports that it conserved perfectly.
    ///
    /// # Panics
    ///
    /// If the simulator refuses a transfer it planned. That is a conservation
    /// bug in the engine rather than a condition a caller can handle, and
    /// P1.6 exists to make it impossible.
    #[must_use]
    pub fn of(skills: &[Skill], steps: usize, seed: u64) -> Self {
        use pact_core::{Bag, Event, Interpreter, Simulator, Type};

        let graph = pact_corpus::to_graph(skills);
        let category = categorise(skills);
        let mut sim = Simulator::new(&graph, seed);

        // Seeded generously on purpose. Six units drain through in twenty
        // steps and the run reaches a steady state that looks like nothing
        // happening; the accumulation this is meant to make visible needs
        // enough work in the system for the imbalance to show.
        for ty in &category.source_only {
            let _ = sim.create(&format!("type:{ty}"), &Bag::of(ty.clone(), 200));
        }
        let opening = sim.total_held();

        // What each skill reads and emits, so a `Converted` event can be read
        // as a type-to-type flow. The canvas draws types; skills are the
        // arrows between them.
        let signatures: std::collections::BTreeMap<String, (String, String)> = skills
            .iter()
            .map(|s| {
                (
                    format!("skill:{}", s.name),
                    (
                        s.input.to_string(),
                        s.outputs
                            .first()
                            .map_or_else(String::new, ToString::to_string),
                    ),
                )
            })
            .collect();

        let types: Vec<String> = category.objects.iter().map(ToString::to_string).collect();
        let mut frames = Vec::with_capacity(steps);
        let mut scheduler = pact_core::Scheduler::new(&graph);
        let mut ledger = pact_core::Ledger::new();

        for _ in 0..steps {
            let Ok(step) = scheduler.step(&mut sim) else {
                break;
            };
            for event in &step.events {
                ledger.record(event);
            }

            let mut flows: Vec<Flow> = Vec::new();
            let mut fired = Vec::new();
            let mut starved = 0;
            for event in &step.events {
                match event {
                    Event::Converted { at, consumed, .. } => {
                        if let Some((from, to)) = signatures.get(at) {
                            fired.push(at.trim_start_matches("skill:").to_string());
                            flows.push(Flow {
                                from: from.clone(),
                                to: to.clone(),
                                n: u32::try_from(consumed.total()).unwrap_or(u32::MAX),
                                via: at.trim_start_matches("skill:").to_string(),
                            });
                        }
                    }
                    Event::Starved { .. } => starved += 1,
                    _ => {}
                }
            }

            frames.push(Frame {
                levels: types
                    .iter()
                    .map(|ty| {
                        let held = sim.holdings(&format!("type:{ty}")).count(&Type::new(ty));
                        (ty.clone(), u32::try_from(held).unwrap_or(u32::MAX))
                    })
                    .collect(),
                flows,
                fired,
                starved,
            });
        }

        Self {
            types,
            frames,
            seed,
            // The engine's own check, over the whole run, per type. Reported
            // rather than asserted: a run that leaks is a finding about the
            // graph, and a canvas that showed the flow without saying the
            // books did not balance would be animating a fiction.
            conserved: ledger.conserved(&opening, &sim.total_held()).is_ok(),
        }
    }
}

#[cfg(test)]
mod export {
    //! Writing `app/src/types.ts`.
    //!
    //! A unit test rather than an integration test, and that is not a style
    //! choice: `cfg_attr(test, ...)` above applies only when *this crate* is
    //! compiled with `--test`. An integration test links the ordinary build,
    //! where the derives are not there at all.

    use super::*;
    use ts_rs::TS;

    #[test]
    fn the_typescript_is_generated_from_these_structs() {
        // Touching each type is what triggers the write.
        Overview::export_all().expect("Overview");
        PressurePoint::export_all().expect("PressurePoint");
        Chain::export_all().expect("Chain");
        Chains::export_all().expect("Chains");
        GraphNode::export_all().expect("GraphNode");
        GraphEdge::export_all().expect("GraphEdge");
        Graph::export_all().expect("Graph");
        SkillDetail::export_all().expect("SkillDetail");
        Frame::export_all().expect("Frame");
        Flow::export_all().expect("Flow");
        Run::export_all().expect("Run");

        let generated =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/src/types.ts");
        assert!(generated.exists(), "nothing written to {generated:?}");

        let text = std::fs::read_to_string(&generated).expect("readable");
        for name in [
            "Overview",
            "PressurePoint",
            "Chain",
            "Chains",
            "GraphNode",
            "GraphEdge",
            "Graph",
            "SkillDetail",
            "Frame",
            "Flow",
            "Run",
        ] {
            assert!(
                text.contains(name),
                "{name} missing from the generated types"
            );
        }

        // The wire is snake_case. If the generated file said otherwise, the
        // page would read a field that is never sent and render blank with no
        // error on either side.
        assert!(
            text.contains("never_fires"),
            "the generated types must use the names serde actually emits"
        );
    }
}
