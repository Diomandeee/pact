//! The two connection kinds.
//!
//! Machinations' deepest idea is that a diagram has two orthogonal wirings.
//! Resources flow along one. Behaviour is modified along the other. Collapsing
//! them into a single "arrow" is what makes most systems diagrams useless: you
//! cannot tell whether A feeds B or merely influences B.
//!
//! In a rail/pulse system the split lands exactly on the invariants:
//!
//! - A resource connection is a type match: `out(A) == in(B)`. Work moves.
//! - A state connection is Invariant 3, cross-layer forcing. One track's state
//!   changes another track's behaviour without handing it any work. That is
//!   precisely what "a parallel track that discovers a blocker must signal it
//!   immediately" means, and it is why forcing is what makes the phase
//!   connected rather than four isolated vertices.

/// How much moves, per firing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rate {
    /// A fixed number of resources.
    Fixed(u64),
    /// Everything available. Used for gates and drains.
    All,
}

impl Rate {
    /// Resolve against what is actually there.
    #[must_use]
    pub fn resolve(&self, available: u64) -> u64 {
        match self {
            Self::Fixed(n) => (*n).min(available),
            Self::All => available,
        }
    }

    /// What this rate WANTS, ignoring availability. The distinction matters
    /// for `Demand::All`: a converter must know its full appetite to decide
    /// whether it can fire at all.
    #[must_use]
    pub fn appetite(&self) -> u64 {
        match self {
            Self::Fixed(n) => *n,
            Self::All => 1,
        }
    }
}

/// Resources move along this edge.
#[derive(Debug, Clone)]
pub struct ResourceEdge {
    /// Source node id.
    pub from: String,
    /// Target node id.
    pub to: String,
    /// How much moves per firing.
    pub rate: Rate,
    /// Optional provenance, e.g. the type being carried: "`M_plan`".
    pub label: Option<String>,
}

impl ResourceEdge {
    /// A resource connection carrying `n` per firing.
    #[must_use]
    pub fn new(from: impl Into<String>, to: impl Into<String>, n: u64) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            rate: Rate::Fixed(n),
            label: None,
        }
    }

    /// A resource connection carrying everything available.
    #[must_use]
    pub fn all(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            rate: Rate::All,
            label: None,
        }
    }

    /// Tag the edge with the type it carries. Chainable.
    #[must_use]
    pub fn carrying(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// What a state connection does to its target.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Enable the target while the condition holds. This is a phase gate:
    /// the next phase is inert until the previous one satisfies it.
    Activator {
        /// Level the source must hold for the target to be enabled.
        at_least: u64,
    },
    /// Fire the target once, on the step the condition becomes true.
    /// Cross-layer forcing: a track that finds a blocker fires the signal
    /// immediately rather than waiting for the gate.
    Trigger {
        /// Level at which the target fires once.
        at_least: u64,
    },
    /// Scale a resource edge's rate by the source's level.
    /// How a track that is going well accelerates the critical path.
    Modifier {
        /// Which resource edge's rate is scaled.
        edge_index: usize,
        /// Change to that rate per unit held by the source.
        per_unit: i64,
    },
    /// Disable the target while the condition holds. A brake.
    Inhibitor {
        /// Level at which the target is disabled. An inhibitor beats an
        /// activator: a brake must win against an accelerator.
        at_least: u64,
    },
}

/// Behaviour is modified along this edge. No resources move.
#[derive(Debug, Clone)]
pub struct StateEdge {
    /// Source node id.
    pub from: String,
    /// Target node id.
    pub to: String,
    /// What this connection does to its target. No resources move along it.
    pub effect: Effect,
}

impl StateEdge {
    /// Target is inert until `from` holds at least `n`.
    #[must_use]
    pub fn activator(from: impl Into<String>, to: impl Into<String>, n: u64) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            effect: Effect::Activator { at_least: n },
        }
    }

    /// Target fires once when `from` reaches `n`.
    #[must_use]
    pub fn trigger(from: impl Into<String>, to: impl Into<String>, n: u64) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            effect: Effect::Trigger { at_least: n },
        }
    }

    /// Target is disabled while `from` holds at least `n`.
    #[must_use]
    pub fn inhibitor(from: impl Into<String>, to: impl Into<String>, n: u64) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            effect: Effect::Inhibitor { at_least: n },
        }
    }
}
