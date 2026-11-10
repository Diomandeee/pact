//! The node vocabulary, adapted from Machinations.
//!
//! Small on purpose. Every type here expresses something the others cannot,
//! and nothing here knows what a rail or a pulse is. Those are words in a
//! composition algebra one layer up; making them primitives would privilege
//! one protocol over every other you might author, which is the opposite of
//! the point.
//!
//! | node      | in a prompt system                                        |
//! |-----------|-----------------------------------------------------------|
//! | Pool      | a type reservoir: `M_plan`, `State`, work in flight        |
//! | Source    | where work enters: a Subject, an external import           |
//! | Drain     | where work leaves: a `Report` nobody consumes              |
//! | Converter | a skill. Its signature IS the SOOP-2 block                 |
//! | Gate      | fan-out: one arrival, distributed across outputs           |
//! | Delay     | an iteration in flight; work held for n steps              |

use crate::token::{Bag, Type};
use std::collections::BTreeMap;

/// How a node decides to act on a given step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activation {
    /// Acts every step it can. The default for a running system.
    Automatic,
    /// Acts only when a state connection triggers it. A phase gate does not
    /// fire on a clock; it fires when its condition is met.
    Triggered,
    /// Acts once, on the first step. Seeds the system.
    Starting,
    /// Never acts on its own. Pure storage.
    Passive,
}

/// Whether a node fires partially or not at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Demand {
    /// Fires only if every input can be satisfied in full.
    All,
    /// Takes whatever is available.
    Any,
}

/// How a gate splits what arrives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Distribution {
    /// Round-robin. Deterministic and reproducible.
    Deterministic,
    /// Weighted random, using the run's seeded PRNG.
    Probabilistic(Vec<u32>),
    /// Every output gets a full copy.
    ///
    /// This is how a fan-out over parallel tracks behaves: each track receives
    /// the phase context entire, it does not receive a quarter of it. Which
    /// means broadcast CREATES resources, and the conservation ledger has to
    /// account for that rather than pretend otherwise.
    Broadcast,
}

/// A converter's signature: what it eats and what it emits.
///
/// This is the same shape as a skill's `input_type` and `output_type`, which
/// is why importing the corpus is translation rather than interpretation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Signature {
    /// What one firing eats. This is a skill's `input_type`, as a multiset.
    pub consumes: Bag,
    /// What one firing emits. This is a skill's `output_type`.
    pub produces: Bag,
}

impl Signature {
    /// One type in, one type out. The shape of most skills.
    #[must_use]
    pub fn simple(input: impl Into<Type>, output: impl Into<Type>) -> Self {
        Self {
            consumes: Bag::of(input.into(), 1),
            produces: Bag::of(output.into(), 1),
        }
    }

    /// `n` of one type in, one out. The shape of a merge.
    #[must_use]
    pub fn fan_in(input: impl Into<Type>, n: u64, output: impl Into<Type>) -> Self {
        Self {
            consumes: Bag::of(input.into(), n),
            produces: Bag::of(output.into(), 1),
        }
    }

    /// Does this signature destroy resources on net? Legal, and worth being
    /// able to ask, because a corpus full of net-destroying converters is a
    /// corpus that drains.
    #[must_use]
    pub fn is_lossy(&self) -> bool {
        self.produces.total() < self.consumes.total()
    }
}

/// What a node is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// Holds typed resources.
    Pool {
        /// Contents at step zero.
        initial: Bag,
        /// Upper bound, if this pool can overflow. `None` means unbounded.
        capacity: Option<u64>,
    },
    /// Creates resources from nothing.
    ///
    /// The only legitimate way new work enters, which makes external import
    /// explicit rather than incidental. Theorem 1 says import is the only
    /// thing that extends a loop's life, so it should be impossible to add
    /// without noticing.
    Source {
        /// Emitted every step this source acts.
        emits: Bag,
    },
    /// Destroys resources.
    Drain,
    /// A skill.
    Converter {
        /// What it eats and emits. A skill's SOOP-2 block.
        signature: Signature,
    },
    /// Distributes arrivals. Holds nothing.
    Gate {
        /// How arrivals are split across outputs.
        distribution: Distribution,
    },
    /// Holds resources for a fixed number of steps, then releases them.
    Delay {
        /// Steps work is held before release.
        steps: u32,
    },
}

/// A node in the diagram.
#[derive(Debug, Clone)]
pub struct Node {
    /// Stable identity. Distinct from a `Type`; both are strings and they are not the same kind of thing.
    pub id: String,
    /// What this node is. The only field that changes how it behaves.
    pub kind: Kind,
    /// When it acts. A gate that fires on a clock is not a gate.
    pub activation: Activation,
    /// Whether it fires partially. `All` is the default for converters: half-run is worse than not-run.
    pub demand: Demand,
    /// Provenance carried through simulation, so a trace names real things:
    /// which skill, which track, which artifact.
    pub meta: BTreeMap<String, String>,
}

impl Node {
    fn base(id: impl Into<String>, kind: Kind, activation: Activation) -> Self {
        Self {
            id: id.into(),
            kind,
            activation,
            demand: Demand::All,
            meta: BTreeMap::new(),
        }
    }

    /// A pool, empty.
    #[must_use]
    pub fn pool(id: impl Into<String>) -> Self {
        Self::base(
            id,
            Kind::Pool {
                initial: Bag::new(),
                capacity: None,
            },
            Activation::Passive,
        )
    }

    /// A pool seeded with contents.
    #[must_use]
    pub fn pool_with(id: impl Into<String>, initial: Bag) -> Self {
        Self::base(
            id,
            Kind::Pool {
                initial,
                capacity: None,
            },
            Activation::Passive,
        )
    }

    /// A source emitting `bag` every step.
    #[must_use]
    pub fn source(id: impl Into<String>, emits: Bag) -> Self {
        Self::base(id, Kind::Source { emits }, Activation::Automatic)
    }

    /// A drain.
    #[must_use]
    pub fn drain(id: impl Into<String>) -> Self {
        let mut node = Self::base(id, Kind::Drain, Activation::Automatic);
        node.demand = Demand::Any;
        node
    }

    /// A converter with a signature.
    #[must_use]
    pub fn converter(id: impl Into<String>, signature: Signature) -> Self {
        Self::base(id, Kind::Converter { signature }, Activation::Automatic)
    }

    /// A gate.
    #[must_use]
    pub fn gate(id: impl Into<String>, distribution: Distribution) -> Self {
        let mut node = Self::base(id, Kind::Gate { distribution }, Activation::Automatic);
        node.demand = Demand::Any;
        node
    }

    /// A delay of `steps`.
    #[must_use]
    pub fn delay(id: impl Into<String>, steps: u32) -> Self {
        let mut node = Self::base(id, Kind::Delay { steps }, Activation::Automatic);
        node.demand = Demand::Any;
        node
    }

    /// Attach provenance. Chainable.
    #[must_use]
    pub fn with_meta(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.meta.insert(key.into(), value.into());
        self
    }

    /// Set activation. Chainable.
    #[must_use]
    pub fn with_activation(mut self, activation: Activation) -> Self {
        self.activation = activation;
        self
    }

    /// Set demand. Chainable.
    #[must_use]
    pub fn with_demand(mut self, demand: Demand) -> Self {
        self.demand = demand;
        self
    }

    /// Does this node store resources?
    #[must_use]
    pub fn is_storage(&self) -> bool {
        matches!(self.kind, Kind::Pool { .. } | Kind::Delay { .. })
    }

    /// What this node consumes per firing, if it is a converter.
    #[must_use]
    pub fn appetite(&self) -> Option<&Bag> {
        match &self.kind {
            Kind::Converter { signature } => Some(&signature.consumes),
            _ => None,
        }
    }
}
