//! One backend: synthetic tokens against an in-memory ledger.
//!
//! The simulator is deliberately the *second* thing written, after the trait.
//! Writing it first is how a trait ends up shaped like a simulator and useless
//! for the executor that has to share it, and the shape is not recoverable
//! afterwards, because by then two analysis layers depend on it.
//!
//! Everything unusual here is in service of INV-3. Same graph plus same seed
//! gives the same trace, forever. A stochastic run you cannot replay is a run
//! you cannot debug, and evidence built on an unreplayable run is not
//! evidence.
//!
//! - Holdings live in a `BTreeMap`, so iteration order is fixed.
//! - The PRNG is a seeded xorshift carried in the struct. Not `rand`, because
//!   INV-1 forbids this crate any dependency, and not a thread-local, because
//!   two runs in one process would then interfere.
//! - Delays are drained in insertion order.
//!
//! None of that is a performance choice. Every one is a determinism choice,
//! and each has a cheaper non-deterministic alternative that would be wrong.

use crate::graph::Graph;
use crate::interpret::{Interpreter, Ledger, Scheduler, Step, Trace};
use crate::node::Kind;
use crate::token::{Bag, Type};
use std::collections::BTreeMap;

/// Why a simulated operation failed.
///
/// The simulator's failures are real, not ceremonial. The scheduler plans
/// against a snapshot and the interpreter is the authority on what is actually
/// there, so a plan that over-commits is caught here rather than silently
/// producing negative stock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimError {
    /// A node was asked for more than it held.
    Insufficient {
        /// The node.
        at: String,
        /// What was asked for.
        wanted: Bag,
        /// What it actually had.
        held: Bag,
    },
}

/// Work a delay is holding.
#[derive(Debug, Clone)]
struct InFlight {
    node: String,
    what: Bag,
    remaining: u32,
}

/// An in-memory interpreter.
#[derive(Debug, Clone)]
pub struct Simulator {
    holdings: BTreeMap<String, Bag>,
    in_flight: Vec<InFlight>,
    /// Where each splitting gate's rotation has reached. `BTreeMap` for the
    /// same reason as everything else here: a trace that reorders between runs
    /// is not reproducible.
    cursors: BTreeMap<String, usize>,
    rng: u64,
}

impl Simulator {
    /// A simulator seeded from a graph's initial pool contents.
    ///
    /// `seed` is the whole source of randomness. Two simulators built from the
    /// same graph and seed produce byte-identical traces, which is what INV-3
    /// asserts and what makes a barcode comparable across generations.
    #[must_use]
    pub fn new(graph: &Graph, seed: u64) -> Self {
        let mut holdings = BTreeMap::new();
        for node in graph.nodes() {
            let initial = match &node.kind {
                Kind::Pool { initial, .. } => initial.clone(),
                _ => Bag::new(),
            };
            holdings.insert(node.id.clone(), initial);
        }
        Self {
            holdings,
            in_flight: Vec::new(),
            cursors: BTreeMap::new(),
            // Zero is the xorshift fixed point: it maps to itself forever, so
            // seed 0 would silently give a constant "random" sequence. Mapped
            // aside rather than rejected, because refusing a seed is a worse
            // surprise than quietly using a good one.
            rng: if seed == 0 {
                0x2545_F491_4F6C_DD1D
            } else {
                seed
            },
        }
    }

    /// Everything currently held, by node.
    #[must_use]
    pub fn state(&self) -> &BTreeMap<String, Bag> {
        &self.holdings
    }

    /// Everything held anywhere, including work in flight inside delays.
    ///
    /// In-flight work counts. It has not been destroyed, it is in a delay, and
    /// a conservation check that ignored it would report a leak for exactly as
    /// long as any delay held anything.
    #[must_use]
    pub fn total_held(&self) -> Bag {
        let mut all = Bag::new();
        for bag in self.holdings.values() {
            all.absorb(bag);
        }
        for held in &self.in_flight {
            all.absorb(&held.what);
        }
        all
    }

    /// Run `steps` steps, returning the trace and a ledger over it.
    ///
    /// # Errors
    ///
    /// The first backend failure. Earlier steps go with it, because a run that
    /// stopped halfway through a step is a state no invariant describes.
    pub fn run(&mut self, graph: &Graph, steps: usize) -> Result<(Trace, Ledger), SimError> {
        let mut scheduler = Scheduler::new(graph);
        let mut trace = Trace::default();
        let mut ledger = Ledger::new();

        for _ in 0..steps {
            let step: Step = scheduler.step(self)?;
            for event in &step.events {
                ledger.record(event);
            }
            trace.steps.push(step);
        }
        Ok((trace, ledger))
    }

    /// xorshift64. Deterministic and dependency-free, and good enough for
    /// weighted choice. Nothing here is cryptographic, and saying so plainly
    /// is cheaper than someone later assuming it is.
    fn next_u64(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }

    fn bag_mut(&mut self, node: &str) -> &mut Bag {
        self.holdings.entry(node.to_string()).or_default()
    }

    fn withdraw(&mut self, at: &str, what: &Bag) -> Result<Bag, SimError> {
        let held = self.holdings.get(at).cloned().unwrap_or_default();
        self.bag_mut(at).withdraw(what).ok_or_else(|| {
            // All-or-nothing, and the error carries both bags. "Insufficient
            // resources" alone sends someone back through the whole trace to
            // find out by how much.
            SimError::Insufficient {
                at: at.to_string(),
                wanted: what.clone(),
                held,
            }
        })
    }
}

impl Interpreter for Simulator {
    type Error = SimError;

    fn available(&self, node: &str, ty: &Type) -> u64 {
        self.holdings.get(node).map_or(0, |b| b.count(ty))
    }

    fn holdings(&self, node: &str) -> Bag {
        self.holdings.get(node).cloned().unwrap_or_default()
    }

    fn create(&mut self, at: &str, what: &Bag) -> Result<Bag, Self::Error> {
        self.bag_mut(at).absorb(what);
        Ok(what.clone())
    }

    fn destroy(&mut self, at: &str, what: &Bag) -> Result<Bag, Self::Error> {
        self.withdraw(at, what)
    }

    fn transfer(&mut self, from: &str, to: &str, what: &Bag) -> Result<Bag, Self::Error> {
        // Withdraw first. Absorbing first and withdrawing second would leave
        // resources duplicated on the failure path, which is the one path
        // nobody tests by hand.
        let moved = self.withdraw(from, what)?;
        self.bag_mut(to).absorb(&moved);
        Ok(moved)
    }

    fn copy_to(&mut self, from: &str, to: &str, what: &Bag) -> Result<Bag, Self::Error> {
        // The source keeps what it has. Only a broadcast gate calls this, and
        // the scheduler destroys the original afterwards, so the books show n
        // copies created and one original destroyed.
        let _ = from;
        self.bag_mut(to).absorb(what);
        Ok(what.clone())
    }

    fn hold(&mut self, at: &str, what: &Bag, duration: u32) -> Result<(), Self::Error> {
        // The work is already in the delay node; holding moves it out of
        // holdings and into flight, so nothing downstream can pull it while it
        // waits. A delay whose contents stayed visible would be a pool.
        let taken = self.withdraw(at, what)?;
        self.in_flight.push(InFlight {
            node: at.to_string(),
            what: taken,
            remaining: duration,
        });
        Ok(())
    }

    fn released(&mut self) -> Vec<(String, Bag)> {
        let mut out = Vec::new();
        let mut still = Vec::new();

        // Drained in insertion order, so two batches released on the same step
        // always appear in the same order in the trace.
        for mut held in std::mem::take(&mut self.in_flight) {
            held.remaining = held.remaining.saturating_sub(1);
            if held.remaining == 0 {
                out.push((held.node, held.what));
            } else {
                still.push(held);
            }
        }
        self.in_flight = still;

        for (node, what) in &out {
            self.holdings.entry(node.clone()).or_default().absorb(what);
        }
        out
    }

    fn rotate(&mut self, at: &str, outputs: usize) -> usize {
        if outputs == 0 {
            return 0;
        }
        let cursor = self.cursors.entry(at.to_string()).or_insert(0);
        let pick = *cursor % outputs;
        *cursor = cursor.wrapping_add(1);
        pick
    }

    fn choose(&mut self, weights: &[u32]) -> usize {
        if weights.is_empty() {
            return 0;
        }
        let total: u64 = weights.iter().map(|w| u64::from(*w)).sum();
        if total == 0 {
            // All weights zero means no preference. Uniform is the honest
            // reading; treating it as "always the first" would make a
            // misconfigured gate look like a working one.
            return usize::try_from(self.next_u64() % weights.len() as u64).unwrap_or(0);
        }
        let mut pick = self.next_u64() % total;
        for (index, weight) in weights.iter().enumerate() {
            let w = u64::from(*weight);
            if pick < w {
                return index;
            }
            pick -= w;
        }
        weights.len() - 1
    }
}
