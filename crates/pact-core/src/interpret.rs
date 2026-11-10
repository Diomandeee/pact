//! The seam between simulating a system and running one.
//!
//! Simulation is worthless unless it predicts execution. The way to get that
//! is not discipline, it is structure: the scheduler that decides which node
//! acts on which step is written **once**, here, against [`Interpreter`], and
//! there are two backends. The simulator moves synthetic tokens against an
//! in-memory ledger. An executor moves real work by dispatching agents and
//! observing artifacts on disk. One graph, one set of rules, two readings of
//! what a step means.
//!
//! # What the trait may not assume
//!
//! Every method here has to be implementable by something that dispatches an
//! agent and waits. That rules out three things a simulator would find
//! natural, and each one is checked by the P1.4 gate rather than trusted:
//!
//! 1. **No seed, no step counter, no ledger in any signature.** An executor
//!    has no seed and its step is wall-clock. A trait that mentions either is
//!    a simulator interface wearing a trait's clothes.
//! 2. **`transfer` returns `Result`.** In a simulator a transfer cannot fail.
//!    In an executor it fails routinely: the artifact is missing, the agent
//!    died, the disk is full. A signature that cannot say so cannot be
//!    implemented, and the discovery arrives after the analysis layer is
//!    already built on top of it.
//! 3. **`available` returns a count, not a borrow.** Handing back `&Bag` from
//!    an internal map is free for the simulator and impossible for anything
//!    whose state lives behind a network call.
//!
//! # Where randomness lives
//!
//! A probabilistic gate needs a choice made. The scheduler does not make it,
//! because making it would require the scheduler to own a PRNG, and a PRNG in
//! the scheduler is a seed in the trait one refactor later. Instead
//! [`Interpreter::choose`] asks the backend: the simulator answers from its
//! seeded xorshift, an executor answers from whichever worker is free.
//! Determinism lives on the predicate side, which is P6 in `SPEC.md`.

use crate::edge::{Effect, Rate, ResourceEdge};
use crate::graph::Graph;
use crate::node::{Activation, Demand, Distribution, Kind, Node, Signature};
use crate::token::{Bag, Type};
use std::collections::{BTreeMap, BTreeSet};

// ─────────────────────────────────────────────────────────── what happened

/// Something that happened during a step.
///
/// The vocabulary is deliberately narrow. Each variant is either conserving,
/// creating, or destroying, and the ledger classifies by variant rather than
/// by inspecting node kinds. A new variant therefore forces a decision about
/// conservation at the moment it is added, which is the only time anyone will
/// think about it clearly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Resources entered the system at a source.
    Created {
        /// Where they landed.
        at: String,
        /// What entered.
        what: Bag,
    },
    /// Resources left the system at a drain.
    Destroyed {
        /// Where they were destroyed.
        at: String,
        /// What left.
        what: Bag,
    },
    /// Resources moved. Conserving.
    Moved {
        /// Origin.
        from: String,
        /// Destination.
        to: String,
        /// What moved.
        what: Bag,
    },
    /// A broadcast gate handed a full copy to an output. **Creating**: the
    /// same resources are also going to every other output. This is the one
    /// place outside a source where resources appear, and it has its own
    /// variant so the ledger cannot mistake it for a move.
    Copied {
        /// The gate.
        from: String,
        /// The output that received a copy.
        to: String,
        /// What was copied.
        what: Bag,
    },
    /// A converter fired.
    Converted {
        /// The converter.
        at: String,
        /// What it ate.
        consumed: Bag,
        /// What it emitted.
        produced: Bag,
    },
    /// A delay took work in. Conserving; the work is in flight.
    Held {
        /// The delay.
        at: String,
        /// What went in.
        what: Bag,
    },
    /// A delay gave work back. Conserving.
    Released {
        /// The delay.
        at: String,
        /// What came out.
        what: Bag,
    },
    /// A node was eligible and could not gather what it needed. Not an error:
    /// this is the runtime face of a starved converter, and a run full of
    /// them is the diagnosis.
    Starved {
        /// The node that could not fire.
        at: String,
        /// What it needed.
        wanted: Bag,
    },
    /// A node was held back by an inhibitor.
    Inhibited {
        /// The node held back.
        at: String,
        /// The node holding it back.
        by: String,
    },
}

/// One step's events, in the order they happened.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Step {
    /// What happened, in order.
    pub events: Vec<Event>,
}

/// A whole run.
///
/// A step is a position in this vector, which is why no [`Event`] carries a
/// step number. A number that can be derived is a number that can disagree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Trace {
    /// Steps in order.
    pub steps: Vec<Step>,
}

impl Trace {
    /// Every event across every step, in order.
    pub fn events(&self) -> impl Iterator<Item = &Event> {
        self.steps.iter().flat_map(|s| s.events.iter())
    }
}

// ─────────────────────────────────────────────────────────── the ledger

/// One type whose books do not balance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imbalance {
    /// The type that does not add up.
    pub ty: Type,
    /// Opening stock, plus created, minus destroyed.
    pub expected: i128,
    /// What is actually held.
    pub found: i128,
}

/// The conservation ledger.
///
/// INV-2. Resources are created only at sources and by broadcast gates, and
/// destroyed only at drains and by converters. Everything else preserves them.
/// This checks it rather than trusting the implementation, because a
/// simulation that quietly loses work is worse than no simulation: it produces
/// confident numbers about an economy that never existed.
#[derive(Debug, Clone, Default)]
pub struct Ledger {
    created: Bag,
    destroyed: Bag,
}

impl Ledger {
    /// An empty ledger.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Classify one event.
    pub fn record(&mut self, event: &Event) {
        match event {
            Event::Created { what, .. } | Event::Copied { what, .. } => self.created.absorb(what),
            Event::Destroyed { what, .. } => self.destroyed.absorb(what),
            Event::Converted {
                consumed, produced, ..
            } => {
                // Read both bags. A converter is not conserving by nature and
                // not lossy by nature; its signature decides, per type. A
                // fan-in eats three `M` and emits one `M_review`, which is
                // three destroyed and one created, never "two lost".
                self.destroyed.absorb(consumed);
                self.created.absorb(produced);
            }
            Event::Moved { .. }
            | Event::Held { .. }
            | Event::Released { .. }
            | Event::Starved { .. }
            | Event::Inhibited { .. } => {}
        }
    }

    /// What has entered the system.
    #[must_use]
    pub fn created(&self) -> &Bag {
        &self.created
    }

    /// What has left it.
    #[must_use]
    pub fn destroyed(&self) -> &Bag {
        &self.destroyed
    }

    /// Do the books balance, per type?
    ///
    /// Per type, not in aggregate. An aggregate check passes a run that turned
    /// four `M_plan` into four `State`, which is the exact defect this exists
    /// to find.
    ///
    /// # Errors
    ///
    /// Every type whose closing stock differs from opening plus created minus
    /// destroyed, so one call names all of them rather than the first.
    pub fn conserved(&self, opening: &Bag, closing: &Bag) -> Result<(), Vec<Imbalance>> {
        let mut types: BTreeSet<&Type> = BTreeSet::new();
        for bag in [opening, closing, &self.created, &self.destroyed] {
            types.extend(bag.types());
        }

        let mut broken = Vec::new();
        for ty in types {
            let expected = i128::from(opening.count(ty)) + i128::from(self.created.count(ty))
                - i128::from(self.destroyed.count(ty));
            let found = i128::from(closing.count(ty));
            if expected != found {
                broken.push(Imbalance {
                    ty: ty.clone(),
                    expected,
                    found,
                });
            }
        }
        if broken.is_empty() {
            Ok(())
        } else {
            Err(broken)
        }
    }
}

// ─────────────────────────────────────────────────────────── the seam

/// What a backend must be able to do.
///
/// Read the module docs before adding a method. The three constraints there
/// are what make this implementable by something that dispatches an agent,
/// and two of them have already been violated once in draft.
pub trait Interpreter {
    /// Why an operation failed. A simulator's is effectively uninhabited; an
    /// executor's names the artifact, the agent, or the disk.
    type Error;

    /// How much of one type is held at a node.
    ///
    /// A count, never a borrow. See constraint 3 in the module docs.
    fn available(&self, node: &str, ty: &Type) -> u64;

    /// Everything held at a node, as an owned snapshot.
    fn holdings(&self, node: &str) -> Bag;

    /// Bring resources into the system.
    ///
    /// # Errors
    /// Backend-defined. A source that cannot produce is a real condition.
    fn create(&mut self, at: &str, what: &Bag) -> Result<Bag, Self::Error>;

    /// Take resources out of the system.
    ///
    /// # Errors
    /// Backend-defined.
    fn destroy(&mut self, at: &str, what: &Bag) -> Result<Bag, Self::Error>;

    /// Move resources. Conserving.
    ///
    /// # Errors
    /// Backend-defined, and routine for an executor. See constraint 2.
    fn transfer(&mut self, from: &str, to: &str, what: &Bag) -> Result<Bag, Self::Error>;

    /// Give a full copy without consuming the original. Broadcast only.
    ///
    /// # Errors
    /// Backend-defined.
    fn copy_to(&mut self, from: &str, to: &str, what: &Bag) -> Result<Bag, Self::Error>;

    /// Take work in and hold it for `duration` of the backend's own units.
    ///
    /// `duration` is a property of the graph, read off the delay node, not a
    /// counter the interpreter is expected to keep. A simulator counts steps;
    /// an executor sets a timeout.
    ///
    /// # Errors
    /// Backend-defined.
    fn hold(&mut self, at: &str, what: &Bag, duration: u32) -> Result<(), Self::Error>;

    /// Work whose hold has elapsed, by node, and now back in that node's
    /// holdings. Called once per step, before anything else acts.
    fn released(&mut self) -> Vec<(String, Bag)>;

    /// Pick an output index given relative weights.
    ///
    /// The scheduler asks rather than deciding, so no PRNG and therefore no
    /// seed ever reaches this trait.
    fn choose(&mut self, weights: &[u32]) -> usize;

    /// The next output index for a splitting gate, advancing its rotation.
    ///
    /// Round-robin needs memory, and memory lives in the backend. A cursor
    /// local to one firing looks like round-robin and is not: a gate receiving
    /// one resource per step would send every single one down the first
    /// branch, forever, and `Deterministic` would be indistinguishable from
    /// "always the first" in the commonest case there is. That defect shipped
    /// green at P1.5 and was found at P2.4 by a cycle that closed on paper and
    /// starved in practice.
    ///
    /// An executor implements this as "the next worker", which is the same
    /// idea and the reason it belongs on this side of the seam.
    fn rotate(&mut self, at: &str, outputs: usize) -> usize;
}

// ─────────────────────────────────────────────────────────── eligibility

/// What every node holds, right now.
pub type Levels = BTreeMap<String, Bag>;

/// The state a scheduling decision is made against.
///
/// Passed explicitly rather than read from an interpreter, so the decision is
/// a pure function of the graph and a snapshot. That is what lets these rules
/// be tested exhaustively at this layer, before any backend exists to run
/// them.
#[derive(Debug, Clone, Copy)]
pub struct Tick<'a> {
    /// Holdings now.
    pub levels: &'a Levels,
    /// Holdings at the end of the previous step. `None` on the first step.
    pub previous: Option<&'a Levels>,
    /// Whether this is step zero, the only step a `Starting` node acts on.
    pub is_first: bool,
}

fn level(levels: &Levels, id: &str) -> u64 {
    levels.get(id).map_or(0, Bag::total)
}

fn threshold(effect: &Effect) -> u64 {
    match effect {
        Effect::Activator { at_least }
        | Effect::Trigger { at_least }
        | Effect::Inhibitor { at_least } => *at_least,
        // A modifier scales a rate; it has no level at which it is
        // "satisfied". `u64::MAX` makes the question answer false rather than
        // accidentally true, which is the safe direction for a predicate that
        // decides whether work runs.
        Effect::Modifier { .. } => u64::MAX,
    }
}

/// Did the source cross the threshold on this step, as opposed to sitting
/// above it?
///
/// A trigger fires on the edge, not on the level. Without this a triggered
/// node fires every step forever once its condition is met, and the difference
/// between a phase gate and an automatic node disappears.
fn crossed(tick: &Tick, source: &str, at_least: u64) -> bool {
    if level(tick.levels, source) < at_least {
        return false;
    }
    // No previous step means the first, and a condition already true on step
    // zero counts as having become true.
    tick.previous
        .is_none_or(|prev| level(prev, source) < at_least)
}

/// Which nodes may act this step, in graph order.
///
/// The rules, in the order they are applied:
///
/// 1. **Activation.** Passive never acts. Starting acts on step zero only.
///    Triggered acts on the step a trigger's condition becomes true, not
///    while it stays true. Automatic always offers to act.
/// 2. **Activators.** Every activator pointing at the node must be satisfied.
///    A phase gate is inert until the previous phase fills it.
/// 3. **Inhibitors.** Any satisfied inhibitor stops the node, and it is
///    applied last so a brake beats an accelerator. That ordering is a design
///    decision, documented on the inhibitor variant, not an accident of
///    control flow.
///
/// Resource availability is deliberately **not** a rule here. A node that is
/// eligible and cannot gather what it needs emits `Starved`, which is a fact
/// worth recording. Filtering it out at eligibility would make the commonest
/// real defect invisible.
#[must_use]
pub fn eligible(graph: &Graph, tick: &Tick) -> Vec<String> {
    let mut acting = Vec::new();

    for node in graph.nodes() {
        let base = match node.activation {
            Activation::Passive => false,
            Activation::Starting => tick.is_first,
            Activation::Automatic => true,
            Activation::Triggered => graph.states.iter().any(|edge| {
                edge.to == node.id
                    && matches!(edge.effect, Effect::Trigger { .. })
                    && crossed(tick, &edge.from, threshold(&edge.effect))
            }),
        };
        if !base {
            continue;
        }

        let activated = graph
            .states
            .iter()
            .filter(|e| e.to == node.id && matches!(e.effect, Effect::Activator { .. }))
            .all(|e| level(tick.levels, &e.from) >= threshold(&e.effect));
        if !activated {
            continue;
        }

        let held_back = graph
            .states
            .iter()
            .filter(|e| e.to == node.id && matches!(e.effect, Effect::Inhibitor { .. }))
            .any(|e| level(tick.levels, &e.from) >= threshold(&e.effect));
        if held_back {
            continue;
        }

        acting.push(node.id.clone());
    }
    acting
}

/// Which nodes are being held back, and by what.
///
/// Reported rather than silently dropped: "nothing happened" and "a brake is
/// on" look identical in a trace otherwise, and they call for opposite fixes.
#[must_use]
pub fn inhibited(graph: &Graph, tick: &Tick) -> Vec<(String, String)> {
    let mut held = Vec::new();
    for node in graph.nodes() {
        if node.activation == Activation::Passive {
            continue;
        }
        for edge in &graph.states {
            if edge.to == node.id
                && matches!(edge.effect, Effect::Inhibitor { .. })
                && level(tick.levels, &edge.from) >= threshold(&edge.effect)
            {
                held.push((node.id.clone(), edge.from.clone()));
            }
        }
    }
    held
}

// ─────────────────────────────────────────────────────────── the scheduler

/// Take at most `n` resources from a bag, in type order.
///
/// Order is `BTreeMap` order, so which types travel under a fixed rate is
/// deterministic. Arbitrary but reproducible beats unspecified: INV-3 compares
/// traces byte for byte.
fn take_up_to(bag: &Bag, n: u64) -> Bag {
    let mut out = Bag::new();
    let mut budget = n;
    for ty in bag.types() {
        if budget == 0 {
            break;
        }
        let take = bag.count(ty).min(budget);
        out.add(ty.clone(), take);
        budget -= take;
    }
    out
}

/// May another node pull resources out of this one?
///
/// **Only a passive pool.** That is the whole rule, and it is the same
/// sentence as "storage is pulled from; everything active pushes" read from
/// the other side.
///
/// Every resource edge has to be driven by exactly one end. An edge whose
/// source pushes *and* whose target pulls moves its resources twice: the
/// target plans a pull against the start-of-step snapshot, the source has
/// already pushed the same resources away, and the pull fails against a node
/// the push emptied. Ordering cannot fix that, because the two mechanisms
/// disagree about who owns the edge rather than about when.
///
/// A drain is excluded even though it pushes nothing. It is not storage: it
/// destroys what it holds, so anything that planned to pull from it is
/// planning against resources that will not exist by the time it asks. That
/// was thirteen failures in a thousand generated graphs, and it is the reason
/// this predicate names what may be *pulled from* rather than what pushes.
/// The two are not complements, and writing it the other way round hid the
/// case.
fn pullable(node: &Node) -> bool {
    matches!(node.kind, Kind::Pool { .. }) && node.activation == Activation::Passive
}

/// What one edge could supply this step, given what is already promised
/// elsewhere in this firing.
fn supply(levels: &Levels, from: &str, rate: &Rate, promised: &Bag) -> Bag {
    let mut held = levels.get(from).cloned().unwrap_or_default();
    // A node feeding two edges cannot supply both in full. Without this, a
    // converter with two incoming edges from one pool double-counts, and the
    // conservation check catches it a layer too late to say why.
    for (ty, n) in promised.iter() {
        held.take(ty, n);
    }
    take_up_to(&held, rate.resolve(held.total()))
}

/// The state one step shares across every node that acts in it.
///
/// `claimed` is the one that is easy to leave out and expensive to leave out.
/// Decisions are made against `levels`, a snapshot taken at the start of the
/// step, so two converters pulling from one pool both plan to take all of it.
/// Without a running claim the second plan promises resources the first
/// already moved, and the transfer fails against a pool that was full when the
/// plan was made. That is 137 failures in a thousand generated graphs, and no
/// ordering rule fixes it: the plans are simultaneous by design.
struct Turn<'a> {
    /// Holdings at the start of the step. Every decision reads this.
    levels: &'a Levels,
    /// What each delay released this step: output on its way out, not input.
    ready: &'a Levels,
    /// What has already been claimed from each node this step.
    claimed: Levels,
}

/// Drives a graph one step at a time against any backend.
///
/// Written once. Every rule about which node acts, what it takes and what it
/// emits lives here and nowhere else, which is the entire reason
/// [`Interpreter`] exists.
pub struct Scheduler<'g> {
    graph: &'g Graph,
    previous: Option<Levels>,
    started: bool,
}

impl<'g> Scheduler<'g> {
    /// A scheduler over one graph.
    #[must_use]
    pub fn new(graph: &'g Graph) -> Self {
        Self {
            graph,
            previous: None,
            started: false,
        }
    }

    /// The graph being driven.
    #[must_use]
    pub fn graph(&self) -> &'g Graph {
        self.graph
    }

    /// Everything every node holds, as an owned snapshot.
    pub fn snapshot<I: Interpreter>(&self, interp: &I) -> Levels {
        self.graph
            .nodes()
            .iter()
            .map(|n| (n.id.clone(), interp.holdings(&n.id)))
            .collect()
    }

    /// Advance one step.
    ///
    /// # Errors
    ///
    /// Whatever the backend raised. The step is abandoned at that point and
    /// the events so far go with it, which is deliberate: a partially applied
    /// step is a state no invariant describes, and returning it would invite
    /// someone to carry on from there.
    pub fn step<I: Interpreter>(&mut self, interp: &mut I) -> Result<Step, I::Error> {
        let mut step = Step::default();

        // Delays first. Work whose hold elapsed is back in its node before
        // anything decides what to do this step, otherwise released work
        // always waits one extra step for a reason nobody could explain.
        // A delay's released work is tracked separately from its holdings for
        // the rest of the step. Both live in `holdings` as far as the backend
        // is concerned, but they mean opposite things: released work is output
        // on its way out, and anything else in a delay is input waiting to be
        // held. Conflating them makes a delay that is fed every step release
        // nothing, because it re-holds its own output forever.
        let mut ready: Levels = BTreeMap::new();
        for (at, what) in interp.released() {
            if what.is_empty() {
                continue;
            }
            ready.entry(at.clone()).or_default().absorb(&what);
            step.events.push(Event::Released { at, what });
        }

        let levels = self.snapshot(interp);
        let tick = Tick {
            levels: &levels,
            previous: self.previous.as_ref(),
            is_first: !self.started,
        };

        for (at, by) in inhibited(self.graph, &tick) {
            step.events.push(Event::Inhibited { at, by });
        }

        let mut turn = Turn {
            levels: &levels,
            ready: &ready,
            claimed: BTreeMap::new(),
        };
        for id in eligible(self.graph, &tick) {
            self.act(&id, &mut turn, interp, &mut step)?;
        }

        self.started = true;
        self.previous = Some(self.snapshot(interp));
        Ok(step)
    }

    /// Apply one node's effect.
    ///
    /// Every acting node works on **what was pushed into it** plus **what it
    /// can pull from passive storage**. `ready` is the exception: for a delay
    /// it is output on its way out, not input.
    fn act<I: Interpreter>(
        &self,
        id: &str,
        turn: &mut Turn,
        interp: &mut I,
        step: &mut Step,
    ) -> Result<(), I::Error> {
        let Some(node) = self.graph.node(id) else {
            return Ok(());
        };
        let outgoing = self.graph.outgoing(id);

        match &node.kind {
            Kind::Source { emits } => Self::act_source(emits, &outgoing, interp, step),
            Kind::Drain => self.act_drain(id, turn, interp, step),
            Kind::Converter { signature } => {
                self.act_converter(id, node.demand, signature, turn, &outgoing, interp, step)
            }
            Kind::Gate { distribution } => {
                self.act_gate(id, distribution, turn, &outgoing, interp, step)
            }
            Kind::Delay { steps } => self.act_delay(id, *steps, turn, &outgoing, interp, step),
            Kind::Pool { .. } => Self::act_pool(id, &outgoing, interp, step),
        }
    }

    /// A source creates at each target.
    ///
    /// `emits` is the total per step, SPLIT across the outgoing edges in
    /// order, not handed whole to each. Theorem 1 makes import the only thing
    /// that extends a loop's life, so how much enters has to be a property of
    /// the source and not of how many wires happen to leave it.
    ///
    /// Created straight at each target: a source holds nothing, so creating
    /// "into" it and then moving out would be two events describing one
    /// arrival.
    fn act_source<I: Interpreter>(
        emits: &Bag,
        outgoing: &[(usize, &ResourceEdge)],
        interp: &mut I,
        step: &mut Step,
    ) -> Result<(), I::Error> {
        {
            // Created straight at each target. A source holds nothing, so
            // creating "into" it and then moving out would be two events
            // describing one arrival.
            //
            // `emits` is the total per step, SPLIT across the outgoing
            // edges in order, not handed whole to each. Theorem 1 makes
            // import the only thing that extends a loop's life, so how
            // much enters has to be a property of the source and not of
            // how many wires happen to leave it.
            let mut remaining = emits.clone();
            for (_, edge) in outgoing {
                let what = take_up_to(&remaining, edge.rate.resolve(remaining.total()));
                if what.is_empty() {
                    continue;
                }
                for (ty, n) in what.iter() {
                    remaining.take(ty, n);
                }
                let made = interp.create(&edge.to, &what)?;
                step.events.push(Event::Created {
                    at: edge.to.clone(),
                    what: made,
                });
            }
        }
        Ok(())
    }

    /// A drain destroys everything it can get.
    fn act_drain<I: Interpreter>(
        &self,
        id: &str,
        turn: &mut Turn,
        interp: &mut I,
        step: &mut Step,
    ) -> Result<(), I::Error> {
        {
            let on_hand = interp.holdings(id);
            self.gather(id, turn, None, &on_hand, interp, step)?;
            let taken = interp.holdings(id);
            if !taken.is_empty() {
                let gone = interp.destroy(id, &taken)?;
                step.events.push(Event::Destroyed {
                    at: id.to_string(),
                    what: gone,
                });
            }
        }
        Ok(())
    }

    /// A converter eats its signature and emits it, split across its outputs.
    #[allow(clippy::too_many_arguments)]
    fn act_converter<I: Interpreter>(
        &self,
        id: &str,
        demand: Demand,
        signature: &Signature,
        turn: &mut Turn,
        outgoing: &[(usize, &ResourceEdge)],
        interp: &mut I,
        step: &mut Step,
    ) -> Result<(), I::Error> {
        {
            let on_hand = interp.holdings(id);
            let want = match demand {
                Demand::All => Some(signature.consumes.clone()),
                Demand::Any => None,
            };
            self.gather(id, turn, want.as_ref(), &on_hand, interp, step)?;

            // What is on hand after gathering is the whole input: pushed-in
            // work plus pulled work, indistinguishable by then and rightly
            // so.
            let available = interp.holdings(id);
            let to_eat = match demand {
                Demand::All => {
                    if !available.contains(&signature.consumes) {
                        // `gather` already recorded the starvation. Nothing
                        // moved, so there is nothing to put back.
                        return Ok(());
                    }
                    // Exactly the signature, leaving any excess on hand for
                    // the next firing. Eating the excess would make a
                    // converter's appetite depend on how much happened to
                    // arrive, which is not what a signature means.
                    signature.consumes.clone()
                }
                Demand::Any => available,
            };
            if to_eat.is_empty() {
                return Ok(());
            }

            // A converter is a boundary in the ledger, not a pipe: what it
            // eats is destroyed and what it emits is created.
            let consumed = interp.destroy(id, &to_eat)?;

            // The output is SPLIT across outgoing edges, not handed whole
            // to each. Only a broadcast gate duplicates, and it says so
            // through `Copied`. Draft one shared one `produced` bag across
            // every edge, so a converter with two outputs emitted twice
            // what its signature promised and conservation would have
            // reported the difference with no way to point at the cause.
            let mut remaining = signature.produces.clone();
            let mut emitted = Bag::new();
            for (_, edge) in outgoing {
                let share = take_up_to(&remaining, edge.rate.resolve(remaining.total()));
                if share.is_empty() {
                    continue;
                }
                for (ty, n) in share.iter() {
                    remaining.take(ty, n);
                }
                let made = interp.create(&edge.to, &share)?;
                emitted.absorb(&made);
            }

            // Report what was emitted, not what the signature promised. A
            // converter wired to nothing produces nothing, and a ledger
            // told otherwise reports a leak that is really an accounting
            // error one layer down.
            step.events.push(Event::Converted {
                at: id.to_string(),
                consumed,
                produced: emitted,
            });
        }
        Ok(())
    }

    /// A gate splits what arrives across its outputs.
    fn act_gate<I: Interpreter>(
        &self,
        id: &str,
        distribution: &Distribution,
        turn: &mut Turn,
        outgoing: &[(usize, &ResourceEdge)],
        interp: &mut I,
        step: &mut Step,
    ) -> Result<(), I::Error> {
        {
            let on_hand = interp.holdings(id);
            self.gather(id, turn, None, &on_hand, interp, step)?;
            let taken = interp.holdings(id);
            if !taken.is_empty() {
                Self::distribute(id, distribution, &taken, outgoing, interp, step)?;
            }
        }
        Ok(())
    }

    /// A delay lets released work out, then takes new work in.
    #[allow(clippy::too_many_arguments)]
    fn act_delay<I: Interpreter>(
        &self,
        id: &str,
        steps_held: u32,
        turn: &mut Turn,
        outgoing: &[(usize, &ResourceEdge)],
        interp: &mut I,
        step: &mut Step,
    ) -> Result<(), I::Error> {
        {
            // Released work leaves first, and only released work. Pushing
            // out everything on hand would push out work that arrived this
            // step and never delay it at all; re-holding what was released
            // would mean a delay fed every step releases nothing, because
            // its own output goes round again forever.
            let mut leaving = turn.ready.get(id).cloned().unwrap_or_default();
            for (_, edge) in outgoing {
                let share = take_up_to(&leaving, edge.rate.resolve(leaving.total()));
                if share.is_empty() {
                    continue;
                }
                for (ty, n) in share.iter() {
                    leaving.take(ty, n);
                }
                let moved = interp.transfer(id, &edge.to, &share)?;
                step.events.push(Event::Moved {
                    from: id.to_string(),
                    to: edge.to.clone(),
                    what: moved,
                });
            }

            // Everything still on hand is input: pushed in this step, or
            // pulled from passive storage just now.
            let on_hand = interp.holdings(id);
            self.gather(id, turn, None, &on_hand, interp, step)?;
            let taken = interp.holdings(id);
            if !taken.is_empty() {
                interp.hold(id, &taken, steps_held)?;
                step.events.push(Event::Held {
                    at: id.to_string(),
                    what: taken,
                });
            }
        }
        Ok(())
    }

    /// An active pool pushes along its outputs. A passive one never gets here.
    fn act_pool<I: Interpreter>(
        id: &str,
        outgoing: &[(usize, &ResourceEdge)],
        interp: &mut I,
        step: &mut Step,
    ) -> Result<(), I::Error> {
        {
            // A pool pushes only when its activation says so, and the
            // default is Passive, so a pool normally never reaches here.
            // Pools are pulled by whatever is downstream. That is what
            // makes a pool storage rather than a pump.
            for (_, edge) in outgoing {
                // Re-read every edge. A snapshot taken once and reused
                // lets two `Rate::All` edges each claim the whole pool,
                // and the second transfer then fails against a pool the
                // first one emptied.
                let held = interp.holdings(id);
                let share = take_up_to(&held, edge.rate.resolve(held.total()));
                if share.is_empty() {
                    continue;
                }
                let moved = interp.transfer(id, &edge.to, &share)?;
                step.events.push(Event::Moved {
                    from: id.to_string(),
                    to: edge.to.clone(),
                    what: moved,
                });
            }
        }
        Ok(())
    }

    /// Pull from passive storage into `id`.
    ///
    /// Only from **passive** storage. An incoming edge whose source pushes is
    /// driven by that source and skipped here; see [`pullable`] for why an edge
    /// driven from both ends moves its resources twice.
    ///
    /// `want` is `Some` for `Demand::All`: the pull is planned dry against
    /// what is already on hand, and if on-hand plus plan cannot meet it in
    /// full then nothing moves and the node reports starved. All-or-nothing
    /// has to be decided before the first transfer, because after the first
    /// transfer there is no "nothing" left to fall back to.
    fn gather<I: Interpreter>(
        &self,
        id: &str,
        turn: &mut Turn,
        want: Option<&Bag>,
        on_hand: &Bag,
        interp: &mut I,
        step: &mut Step,
    ) -> Result<Bag, I::Error> {
        let mut plan: Vec<(String, Bag)> = Vec::new();
        let mut total = on_hand.clone();

        for (_, edge) in self.graph.incoming(id) {
            if !self.graph.node(&edge.from).is_some_and(pullable) {
                continue;
            }

            // Claims are shared across the whole step, not just this firing.
            // The same map therefore covers two edges from one pool into this
            // node AND two different nodes pulling from that pool.
            let already = turn.claimed.get(&edge.from).cloned().unwrap_or_default();
            let mut share = supply(turn.levels, &edge.from, &edge.rate, &already);

            if let Some(want) = want {
                // Take only the shortfall, so a converter wanting two `M` and
                // already holding one does not drain a pool of nine.
                let mut trimmed = Bag::new();
                for (ty, have) in share.iter() {
                    let still = want.count(ty).saturating_sub(total.count(ty));
                    trimmed.add(ty.clone(), have.min(still));
                }
                share = trimmed;
            }
            if share.is_empty() {
                continue;
            }

            turn.claimed
                .entry(edge.from.clone())
                .or_default()
                .absorb(&share);
            total.absorb(&share);
            plan.push((edge.from.clone(), share));
        }

        if let Some(want) = want {
            if !total.contains(want) {
                // Nothing has moved yet, so the claims this plan made have to
                // go back. Leaving them would starve every later node that
                // wanted the same pool, for a firing that never happened.
                for (from, share) in &plan {
                    if let Some(held) = turn.claimed.get_mut(from) {
                        for (ty, n) in share.iter() {
                            held.take(ty, n);
                        }
                    }
                }
                step.events.push(Event::Starved {
                    at: id.to_string(),
                    wanted: want.clone(),
                });
                return Ok(Bag::new());
            }
        }

        let mut moved = Bag::new();
        for (from, share) in plan {
            let got = interp.transfer(&from, id, &share)?;
            step.events.push(Event::Moved {
                from,
                to: id.to_string(),
                what: got.clone(),
            });
            moved.absorb(&got);
        }
        Ok(moved)
    }

    /// Split what a gate took across its outputs.
    ///
    /// Associated rather than a method: distribution depends on the gate, the
    /// bag and the outgoing edges, and on nothing else in the graph. A `&self`
    /// here would be an invitation to reach for graph state that the rule
    /// deliberately does not consider.
    fn distribute<I: Interpreter>(
        id: &str,
        distribution: &Distribution,
        taken: &Bag,
        outgoing: &[(usize, &ResourceEdge)],
        interp: &mut I,
        step: &mut Step,
    ) -> Result<(), I::Error> {
        if outgoing.is_empty() {
            return Ok(());
        }

        match distribution {
            Distribution::Broadcast => {
                // Every output gets the whole thing. This is a fan-out over
                // parallel tracks: each track receives the phase context
                // entire, not a quarter of it. It creates resources, and the
                // ledger is told so through `Copied`.
                for (_, edge) in outgoing {
                    let copy = interp.copy_to(id, &edge.to, taken)?;
                    step.events.push(Event::Copied {
                        from: id.to_string(),
                        to: edge.to.clone(),
                        what: copy,
                    });
                }
                // The original is consumed by the fan-out itself, so the books
                // show n copies created and one original destroyed rather than
                // n+1 resources appearing from nowhere.
                let gone = interp.destroy(id, taken)?;
                step.events.push(Event::Destroyed {
                    at: id.to_string(),
                    what: gone,
                });
            }

            Distribution::Deterministic => {
                // Round-robin one resource at a time, so three across two
                // outputs splits 2/1 rather than landing wherever the
                // arithmetic happens to round.
                //
                // The cursor comes from the backend and persists across steps.
                // A local one would restart at zero every firing, which for a
                // gate fed one resource per step means every resource goes
                // down the first branch and the rotation never rotates.
                let mut remaining = taken.clone();
                while !remaining.is_empty() {
                    let one = take_up_to(&remaining, 1);
                    for (ty, n) in one.iter() {
                        remaining.take(ty, n);
                    }
                    let pick = interp.rotate(id, outgoing.len());
                    let (_, edge) = &outgoing[pick % outgoing.len()];
                    let moved = interp.transfer(id, &edge.to, &one)?;
                    step.events.push(Event::Moved {
                        from: id.to_string(),
                        to: edge.to.clone(),
                        what: moved,
                    });
                }
            }

            Distribution::Probabilistic(weights) => {
                let mut remaining = taken.clone();
                while !remaining.is_empty() {
                    let one = take_up_to(&remaining, 1);
                    for (ty, n) in one.iter() {
                        remaining.take(ty, n);
                    }
                    // The backend chooses. See the module docs on where
                    // randomness lives and why it is not here.
                    let pick = interp.choose(weights) % outgoing.len();
                    let (_, edge) = &outgoing[pick];
                    let moved = interp.transfer(id, &edge.to, &one)?;
                    step.events.push(Event::Moved {
                        from: id.to_string(),
                        to: edge.to.clone(),
                        what: moved,
                    });
                }
            }
        }
        Ok(())
    }
}
