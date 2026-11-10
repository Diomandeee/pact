//! `pact-algebra` — the six combinators, and protocols over what they build.
//!
//! L1. It composes engine graphs and knows nothing about how one runs; that is
//! `pact-core`'s business, one layer down.
//!
//! # Why this is a separate layer
//!
//! `railpulse` is a word in this algebra, not a primitive in the engine. INV-7
//! enforces that in the other direction with a canary: `pact-core` names no
//! protocol concept, no `Rail`, no `Pulse`, no `Phase`. A protocol baked into
//! the core would be privileged over every other one the platform is supposed
//! to host, and the whole product claim is that someone else can author theirs
//! against the same machinery.
//!
//! # Ill-typed words are unrepresentable
//!
//! Composition is checked at construction, so a word that does not typecheck
//! cannot be held. Everything a caller asks of a [`Term`] is therefore a
//! question about a word that already composes, and the analysis layer never
//! has to carry a "but is this one real" flag through its reports.

#![forbid(unsafe_code)]

// The module list tracks the unlock map. A module is declared here when its
// file opens, not before.
//
//   ./scripts/pz status    what is open
//
// Open:
pub mod combinator;
pub mod protocol;

pub use combinator::{Compiled, Mismatch, PortProblem, Ports, Stages, Term};
pub use protocol::{
    Protocol, Rule, Unsatisfiable, Violation, MAX_TRACKS_PER_PHASE, MIN_VIABLE_TRANSITIONS,
};
