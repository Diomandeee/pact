//! `pact-corpus` — read a real corpus as a graph.
//!
//! L3. This is the layer with filesystem access, which is why the engine can
//! have none: INV-1 keeps `pact-core` pure by putting the reading here.
//!
//! Import comes before analysis on purpose. Analysis with no real corpus
//! produces beautiful metrics over synthetic graphs and never meets a fact
//! that can contradict it.

#![forbid(unsafe_code)]

// The module list tracks the unlock map.
//
//   ./scripts/pz status    what is open
//
// Open:
pub mod chains;
pub mod soop;

pub use chains::{
    compatible, factor, factor_all, factor_declared, iterates, members, rebuild, refactor, stages,
    Factoring, Stage, Step, ITERATORS,
};
pub use soop::{
    categorise, coercions, default_root, economy_degrees, load, to_graph, Category, Degree, Skill,
};
