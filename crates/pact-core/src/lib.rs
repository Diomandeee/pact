//! `pact-core` — a typed resource-flow engine.
//!
//! This crate is a Machinations engine. It knows about pools, sources, drains,
//! converters, gates, delays, and two kinds of connection. It does not know
//! what a rail is, or a pulse, or a prompt, or a skill. Those are words in a
//! composition algebra one layer up.
//!
//! # Two guarantees
//!
//! **Conservation.** Resources are created only at Sources and by Broadcast
//! gates, and destroyed only at Drains and by lossy Converters. Every other
//! movement preserves them. [`Ledger::conserved`] checks this rather than
//! trusting the implementation, because a simulation that quietly loses work
//! is worse than no simulation.
//!
//! **Reproducibility.** The only randomness is a seeded xorshift carried in
//! the run state. Same graph plus same seed gives the same trace, forever. A
//! stochastic run you cannot replay is a run you cannot debug, and evidence
//! built on an unreplayable run is not evidence.
//!
//! # Why the interpreter is a trait
//!
//! Simulation is worthless unless it predicts execution. So the scheduler that
//! decides which node acts on which step is written once, against
//! [`Interpreter`], and there are two backends: [`Simulator`] moves synthetic
//! tokens against an in-memory ledger, and an executor (a later crate) moves
//! real work by dispatching agents and observing artifacts on disk. One graph,
//! one set of invariants, two readings of what a step means.
//!
//! Determinism lives on the predicates and never on the effects. An iteration
//! is a model doing work and will never be deterministic; whether it was
//! *allowed* to run always is.

#![forbid(unsafe_code)]

// The module list tracks the unlock map. A module is declared here when its
// file opens, not before: declaring one early is the same defect as editing a
// locked file, and it is how `lib.rs` ended up referencing an `interpret`
// module that had never been written.
//
//   ./scripts/pz status    what is open
//
// Open:
pub mod edge;
pub mod graph;
pub mod interpret;
pub mod node;
pub mod sim;
pub mod token;

pub use edge::{Effect, Rate, ResourceEdge, StateEdge};
pub use graph::{Defect, Graph, PRESSURE_RATIO};
pub use interpret::{
    eligible, inhibited, Event, Imbalance, Interpreter, Ledger, Levels, Scheduler, Step, Tick,
    Trace,
};
pub use node::{Activation, Demand, Distribution, Kind, Node, Signature};
pub use sim::{SimError, Simulator};
pub use token::{Bag, Type};
