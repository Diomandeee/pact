//! `pact-analysis` — what a corpus is, and what it does when it runs.
//!
//! L2, and it sits above L3 on purpose. Analysis with no real corpus produces
//! beautiful metrics over synthetic graphs and never meets a fact that can
//! contradict it, so the importer came first and every gate here reads
//! "matches the Python" against a live corpus.

#![forbid(unsafe_code)]

// The module list tracks the unlock map.
//
//   ./scripts/pz status    what is open
//
// Open:
pub mod dynamics;
pub mod homology;
pub mod persistence;
pub mod policy;
pub mod repertoire;
pub mod statics;
pub mod stuck;

pub use dynamics::{type_edges, Dynamics, Machination};
pub use homology::{Betti, Complex};
pub use persistence::{Bar, Fabrication, Filtration};
pub use policy::{select, Rate, MIN_N, TIEBAND};
pub use repertoire::Maneuver;
pub use statics::Statics;
pub use stuck::{StuckDetector, TurnMove};
