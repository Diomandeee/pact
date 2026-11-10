//! A seeded graph generator, shared by the two property gates.
//!
//! Hand-rolled rather than `proptest`. INV-1 forbids `pact-core` any
//! dependency, and a dev-dependency that shapes the tests is close enough to
//! count: the generator decides which graphs the conservation gate ever sees,
//! so it is part of the argument, not scaffolding around it.
//!
//! Everything here is reproducible from an integer. A failing case has to be
//! recoverable from its index alone, or debugging it becomes archaeology.

use pact_core::{
    Activation, Bag, Demand, Distribution, Graph, Node, Rate, ResourceEdge, Signature, StateEdge,
    Type,
};

/// The type alphabet. Deliberately small, so collisions are common: a
/// generator whose types never overlap would never exercise the per-type
/// behaviour that INV-2 is about.
pub const TYPES: [&str; 3] = ["M", "M_plan", "State"];

/// Deterministic case generation.
pub struct Gen(u64);

impl Gen {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1))
    }

    pub fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 11
    }

    /// A value in `0..n`.
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next() % n
        }
    }

    /// An index into a slice of length `n`. Narrowed here rather than at each
    /// call site, so the one place that can truncate is the one place that
    /// says why it cannot: `below` returns strictly less than its argument,
    /// and every argument is a slice length that already fitted in a `usize`.
    pub fn index(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        usize::try_from(self.below(n as u64)).unwrap_or(0)
    }

    pub fn ty(&mut self) -> Type {
        Type::new(TYPES[self.index(TYPES.len())])
    }

    /// A small bag. Counts stay low on purpose: a broadcast gate inside a
    /// cycle doubles its contents every pass, and a generator that starts big
    /// tests `u64` overflow rather than conservation.
    pub fn bag(&mut self, max_kinds: u64, max_each: u64) -> Bag {
        let mut bag = Bag::new();
        for _ in 0..=self.below(max_kinds) {
            let ty = self.ty();
            bag.add(ty, 1 + self.below(max_each));
        }
        bag
    }

    pub fn rate(&mut self) -> Rate {
        if self.below(3) == 0 {
            Rate::All
        } else {
            Rate::Fixed(1 + self.below(3))
        }
    }
}

/// A random graph over three to eight nodes.
///
/// Bounded on purpose. Eight nodes and six steps keeps a broadcast cycle's
/// growth inside `u64` while still reaching every kind, every distribution and
/// both demands. A generator that can overflow tests arithmetic, not the
/// invariant.
#[must_use]
pub fn random_graph(gen: &mut Gen) -> Graph {
    let mut g = Graph::new();
    let count = 3 + gen.index(6);
    let ids: Vec<String> = (0..count).map(|i| format!("n{i}")).collect();

    for id in &ids {
        // Pools are weighted heavily. A corpus is mostly storage, and a
        // generator producing mostly converters would exercise a shape the
        // real thing never has.
        let node = match gen.below(10) {
            0..=3 => Node::pool_with(id.clone(), gen.bag(2, 3)),
            4 => Node::pool_with(id.clone(), gen.bag(1, 2)).with_activation(Activation::Automatic),
            5 => Node::source(id.clone(), gen.bag(1, 2)),
            6 => Node::drain(id.clone()),
            7 => {
                let signature = Signature {
                    consumes: gen.bag(1, 2),
                    produces: gen.bag(1, 2),
                };
                let converter = Node::converter(id.clone(), signature);
                if gen.below(2) == 0 {
                    converter.with_demand(Demand::Any)
                } else {
                    converter
                }
            }
            8 => {
                let distribution = match gen.below(3) {
                    0 => Distribution::Broadcast,
                    1 => Distribution::Deterministic,
                    _ => Distribution::Probabilistic(vec![
                        u32::try_from(gen.below(4)).unwrap_or(1),
                        u32::try_from(gen.below(4)).unwrap_or(1),
                    ]),
                };
                Node::gate(id.clone(), distribution)
            }
            _ => Node::delay(id.clone(), 1 + u32::try_from(gen.below(3)).unwrap_or(0)),
        };
        g.add(node);
    }

    // Resource edges, including cycles. Cycles are the interesting case: a
    // feedback loop is what a railpulse word compiles to, so a generator that
    // only made trees would never test the thing the algebra is for.
    let edge_attempts = count * 2;
    for _ in 0..edge_attempts {
        let from = &ids[gen.index(count)];
        let to = &ids[gen.index(count)];
        if from == to {
            continue;
        }
        let mut edge = ResourceEdge::new(from.clone(), to.clone(), 1);
        edge.rate = gen.rate();
        g.connect(edge);
    }

    // State edges. They move no resources, so they cannot break conservation
    // directly; they are here because they change WHO acts, and an activation
    // pattern nobody generated is an activation pattern nobody tested.
    for _ in 0..gen.below(3) {
        let from = &ids[gen.index(count)];
        let to = &ids[gen.index(count)];
        let level = gen.below(3);
        g.influence(match gen.below(3) {
            0 => StateEdge::activator(from.clone(), to.clone(), level),
            1 => StateEdge::inhibitor(from.clone(), to.clone(), level),
            _ => StateEdge::trigger(from.clone(), to.clone(), level),
        });
    }

    g
}
