//! `pact-server` — the hosted surface.
//!
//! L4. One corpus, over HTTP, with a page. INV-8 makes this and `pact-cli` the
//! only crates that may take third-party dependencies; everything below
//! depends on workspace members and `std`.

#![forbid(unsafe_code)]

// The module list tracks the unlock map.
//
//   ./scripts/pz status    what is open
//
// Open:
pub mod api;
pub mod view;

pub use api::{Corpus, Shared};
pub use view::{
    Chain, Chains, Flow, Frame, Graph, GraphEdge, GraphNode, Overview, PressurePoint, Run,
    SkillDetail,
};
