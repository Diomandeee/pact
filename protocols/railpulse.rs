//! railpulse — the canonical cycle, as a protocol and as a word.
//!
//! ```text
//!   railpulse = ↺( ⊗ ▸ * ▸ ⊕ )
//!                  expand execute select repeat
//! ```
//!
//! # Why this file is not in a crate
//!
//! It is `#[path]`-included by the gate test and compiled into nothing that
//! ships. That is deliberate and it is INV-7's reasoning carried all the way
//! out: a protocol living inside `pact-core` would be privileged over every
//! other one the engine could host, and a protocol living inside
//! `pact-algebra` would be privileged over every other one the *algebra* could
//! host. railpulse is Mohamed's protocol. It is not the platform's.
//!
//! The test is what proves the separation is real rather than stated: the
//! canary greps every crate's source for this name and finds nothing outside
//! doc comments.
//!
//! # This is one protocol, and the platform hosts many
//!
//! Everything below is built from the public [`Rule`] vocabulary. Nothing here
//! reaches into the algebra's internals, which means someone else's protocol
//! has exactly the same tools available and no fewer.
//!
//! # What it does not claim
//!
//! The rules are structural. They say a system fans out, iterates, selects and
//! returns to its own start. They say nothing about whether any of that
//! produces something good. Determinism buys the predicates and never the
//! judgment, and that gap is permanent.

use pact_algebra::{Protocol, Rule, Term, MAX_TRACKS_PER_PHASE, MIN_VIABLE_TRANSITIONS};
use pact_core::Signature;

/// What a phase carries between stages. A plan fans out as `M_plan`, each
/// track returns an `M`, and the select stage collapses those back to a plan.
pub const PLAN: &str = "M_plan";
/// What one track produces.
pub const RESULT: &str = "M";

/// The protocol: four stages and the invariants a phase carries.
///
/// Deliberately no `Rule::Stage`. The canonical cycle is a claim about
/// *shape*, and pinning a stage to `M_arch → M_plan` would make railpulse a
/// claim about one vocabulary as well. A typed refinement is something a user
/// adds on top, and the fact that they can is the point.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol::new("railpulse")
        .require(Rule::WellFormed)
        .require(Rule::Expands)
        .require(Rule::Executes)
        .require(Rule::Selects)
        .require(Rule::Repeats)
        .require(Rule::BoundedWidth {
            max: MAX_TRACKS_PER_PHASE,
        })
        .require(Rule::GatesBranch {
            min: MIN_VIABLE_TRANSITIONS,
        })
        .require(Rule::Connected)
}

/// The word, over `tracks` parallel tracks.
///
/// # Panics
///
/// Never, for `2 ..= MAX_TRACKS_PER_PHASE`. The cycle is closed by
/// construction and every `expect` below names the type rule that makes it so,
/// which is more useful than a `Result` nobody can act on: a caller cannot
/// repair the canonical cycle, they can only pick a legal width.
#[must_use]
pub fn word(tracks: usize) -> Term {
    assert!(
        (MIN_VIABLE_TRANSITIONS..=MAX_TRACKS_PER_PHASE).contains(&tracks),
        "a phase runs {MIN_VIABLE_TRANSITIONS} to {MAX_TRACKS_PER_PHASE} tracks; \
         I2 is a dimension cap, not a suggestion"
    );

    // ⊗ — every track reads the same plan, and each returns a result.
    let expand = Term::broadcast(
        (0..tracks)
            .map(|i| {
                Term::atom(format!("track{i}"), Signature::simple(PLAN, RESULT))
                    .expect("one type in, one out")
            })
            .collect(),
    )
    .expect("every track reads the plan, which is what makes them parallel");

    // * — the effector repeats its own work. Its ports are unchanged by
    // definition, which is why `iterate` is the one combinator that cannot
    // fail.
    let execute = Term::atom("pulse", Signature::simple(RESULT, RESULT))
        .expect("one type in, one out")
        .iterate();

    // ⊕ — and this is where the loop is won or lost. The select stage returns
    // the type the fan-out reads, so it is the compaction as well as the
    // merge. A same-type fan-in would leave the cycle open, and `↺` below
    // would refuse to build.
    let select = Term::merge("compact", tracks as u64, RESULT, PLAN)
        .expect("two or more tracks to collapse");

    Term::sequence(vec![expand, execute, select])
        .expect("M_plan → M → M → M_plan composes")
        .feedback()
        .expect("and the compaction returns what the fan-out reads, so it closes")
}
