//! The engine, across a language boundary.
//!
//! # One engine, two callers
//!
//! This crate builds as both `cdylib` and `rlib`. The browser calls the wasm;
//! the differential test calls the rlib. **Both reach the same
//! `pact_core::Simulator`**, so P7.1 compares one implementation via two paths
//! rather than two implementations that happen to agree.
//!
//! That distinction is the whole point. A TypeScript reimplementation of the
//! simulator would be a second engine, and two engines drift — which is the
//! failure this project has spent every gate preventing.
//!
//! # No wasm-bindgen
//!
//! Its CLI is a build step, and the boundary here is a flat buffer that needs
//! none: JSON in, JSON out, over linear memory. A dependency whose job is
//! marshalling two strings would be the largest thing in this crate.
//!
//! # Where the unsafe is, and why it is only here
//!
//! `unsafe_code` is `forbid` across the workspace. An FFI boundary cannot be
//! written without it: something has to turn a pointer and a length from the
//! host into a slice. So it is allowed in this file, confined to three
//! functions, and nowhere else — `pact-core` and everything above it stay
//! forbidden, and a canary checks the allowance did not spread.

#![allow(unsafe_code)]

use pact_core::{
    Activation, Bag, Distribution, Graph, Node, ResourceEdge, Signature, Simulator, Type,
};
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────── the wire graph

/// A graph, as something outside Rust can describe one.
///
/// `pact_core::Graph` carries no `serde` derives and must not: INV-1 keeps the
/// engine free of dependencies. So the wire form lives here, at L4, and
/// converts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphSpec {
    /// Every node.
    pub nodes: Vec<NodeSpec>,
    /// Every resource connection.
    pub edges: Vec<EdgeSpec>,
}

/// One node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct NodeSpec {
    /// Stable identity.
    pub id: String,
    /// `pool`, `source`, `drain`, `converter`, `gate` or `delay`.
    pub kind: String,
    /// What a pool starts with, or what a source emits.
    #[serde(default)]
    pub bag: Vec<(String, u64)>,
    /// A converter's input.
    #[serde(default)]
    pub consumes: Vec<(String, u64)>,
    /// A converter's output.
    #[serde(default)]
    pub produces: Vec<(String, u64)>,
    /// A gate's distribution: `broadcast`, `probabilistic`, else deterministic.
    #[serde(default)]
    pub distribution: Option<String>,
    /// Weights, when the distribution is weighted.
    #[serde(default)]
    pub weights: Vec<u32>,
    /// A delay's hold.
    #[serde(default)]
    pub steps: u32,
    /// `automatic`, `triggered`, `starting` or `passive`. Defaults per kind.
    #[serde(default)]
    pub activation: Option<String>,
}

/// One resource connection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EdgeSpec {
    /// Origin.
    pub from: String,
    /// Destination.
    pub to: String,
    /// How much per firing. Absent means everything available.
    #[serde(default)]
    pub rate: Option<u64>,
}

fn bag_of(pairs: &[(String, u64)]) -> Bag {
    let mut bag = Bag::new();
    for (ty, n) in pairs {
        bag.add(Type::new(ty.clone()), *n);
    }
    bag
}

impl GraphSpec {
    /// Build the engine's graph.
    ///
    /// An unrecognised kind becomes a passive pool rather than an error: a
    /// spec from outside is data, and refusing a whole graph over one node
    /// tells the caller less than showing them the rest of it.
    #[must_use]
    pub fn build(&self) -> Graph {
        let mut graph = Graph::new();
        for spec in &self.nodes {
            let node = match spec.kind.as_str() {
                "source" => Node::source(spec.id.clone(), bag_of(&spec.bag)),
                "drain" => Node::drain(spec.id.clone()),
                "converter" => Node::converter(
                    spec.id.clone(),
                    Signature {
                        consumes: bag_of(&spec.consumes),
                        produces: bag_of(&spec.produces),
                    },
                ),
                "gate" => Node::gate(
                    spec.id.clone(),
                    match spec.distribution.as_deref() {
                        Some("broadcast") => Distribution::Broadcast,
                        Some("probabilistic") => Distribution::Probabilistic(spec.weights.clone()),
                        _ => Distribution::Deterministic,
                    },
                ),
                "delay" => Node::delay(spec.id.clone(), spec.steps),
                _ => Node::pool_with(spec.id.clone(), bag_of(&spec.bag)),
            };
            let node = match spec.activation.as_deref() {
                Some("automatic") => node.with_activation(Activation::Automatic),
                Some("triggered") => node.with_activation(Activation::Triggered),
                Some("starting") => node.with_activation(Activation::Starting),
                Some("passive") => node.with_activation(Activation::Passive),
                _ => node,
            };
            graph.add(node);
        }
        for edge in &self.edges {
            graph.connect(match edge.rate {
                Some(n) => ResourceEdge::new(edge.from.clone(), edge.to.clone(), n),
                None => ResourceEdge::all(edge.from.clone(), edge.to.clone()),
            });
        }
        graph
    }
}

// ─────────────────────────────────────────────────────────── the run

/// What a run reports.
///
/// Rendered as JSON and compared **byte for byte** between the wasm and the
/// native caller. INV-3 says the same seed gives the same trace; this is that
/// claim crossing a language boundary, which is where it would break first if
/// anything in the engine depended on the platform.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunReport {
    /// The seed it ran with.
    pub seed: u64,
    /// How many steps completed.
    pub steps: usize,
    /// Every event, rendered. Debug rather than a schema on purpose: the point
    /// is byte equality, and a formatting difference between platforms is
    /// exactly the divergence this is meant to catch.
    pub events: Vec<String>,
    /// What entered the system, per type.
    pub created: Vec<(String, u64)>,
    /// What left it.
    pub destroyed: Vec<(String, u64)>,
    /// Whether the books balanced.
    pub conserved: bool,
}

/// Run a graph and report what happened.
///
/// The single entry point both callers reach. There is no wasm-specific path
/// and no native-specific path, because the moment there are two the gate is
/// comparing two things.
///
/// # Errors
///
/// The message from a backend refusal, which for the simulator means a
/// transfer it planned could not be made.
pub fn run(spec: &GraphSpec, seed: u64, steps: usize) -> Result<RunReport, String> {
    let graph = spec.build();
    let mut sim = Simulator::new(&graph, seed);
    let opening = sim.total_held();

    let (trace, ledger) = sim
        .run(&graph, steps)
        .map_err(|e| format!("the run failed: {e:?}"))?;

    let pairs = |bag: &Bag| -> Vec<(String, u64)> {
        bag.types().map(|t| (t.to_string(), bag.count(t))).collect()
    };

    Ok(RunReport {
        seed,
        steps: trace.steps.len(),
        events: trace.events().map(|e| format!("{e:?}")).collect(),
        created: pairs(ledger.created()),
        destroyed: pairs(ledger.destroyed()),
        conserved: ledger.conserved(&opening, &sim.total_held()).is_ok(),
    })
}

/// Run from JSON and return JSON. What the boundary actually calls.
///
/// # Errors
///
/// A parse failure, or whatever [`run`] returned.
pub fn run_json(spec_json: &str, seed: u64, steps: usize) -> Result<String, String> {
    let spec: GraphSpec = serde_json::from_str(spec_json).map_err(|e| e.to_string())?;
    let report = run(&spec, seed, steps)?;
    serde_json::to_string(&report).map_err(|e| e.to_string())
}

// ─────────────────────────────────────────────────────────── the boundary
//
// Three functions and about a dozen lines of unsafe. Something has to turn a
// pointer and a length from the host into a slice, and no amount of
// architecture removes that. It is here, it is small, and it is the only place
// in the workspace where `unsafe_code` is allowed.

/// Reserve `len` bytes the host can write into.
///
/// The host must call [`dealloc`] with the same length. A leaked buffer in a
/// wasm module leaks for the module's lifetime, which for a page is the page.
#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut buf: Vec<u8> = Vec::with_capacity(len);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// Give back what [`alloc`] handed out.
///
/// # Safety
///
/// `ptr` must have come from [`alloc`] with this exact `len`, and must not be
/// used afterwards.
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len));
}

/// Run from a buffer, and write the answer into a fresh one.
///
/// Returns a pointer to `[u32 little-endian length][utf-8 bytes]`, because a
/// wasm export returns one value and the host needs both. The host reads the
/// length, reads the bytes, then calls [`dealloc`].
///
/// # Safety
///
/// `ptr` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn run_buffer(
    ptr: *const u8,
    len: usize,
    seed: u64,
    steps: usize,
) -> *mut u8 {
    let input = std::slice::from_raw_parts(ptr, len);
    let answer = match std::str::from_utf8(input) {
        Ok(json) => run_json(json, seed, steps).unwrap_or_else(|e| format!("{{\"error\":{e:?}}}")),
        Err(e) => format!("{{\"error\":\"{e}\"}}"),
    };

    let bytes = answer.into_bytes();
    let mut out = Vec::with_capacity(4 + bytes.len());
    out.extend_from_slice(&u32::try_from(bytes.len()).unwrap_or(0).to_le_bytes());
    out.extend_from_slice(&bytes);
    let out_ptr = out.as_mut_ptr();
    std::mem::forget(out);
    out_ptr
}
