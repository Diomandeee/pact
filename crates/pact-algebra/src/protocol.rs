//! A protocol is a predicate over graphs.
//!
//! # Why a graph and not a word
//!
//! A word is what *you* authored. A graph is what someone *has*. The product
//! claim is that a person can point this at their own corpus and be told which
//! rules their system breaks, and they did not build their system out of
//! [`Term`](crate::Term)s. So validation recovers structure from the graph
//! rather than reading it off a word that may not exist.
//!
//! That is strictly harder, and it is the difference between a library for
//! people who already agree with the notation and a tool for people who do
//! not.
//!
//! # Why a protocol is data
//!
//! [`Protocol`] is a struct holding a list of [`Rule`]s, not a trait someone
//! implements. Declaring a protocol is picking rules; it does not require
//! writing Rust, and a list of rules can be printed, diffed, and eventually
//! read from a file. A trait would have made every protocol a compilation
//! unit, which quietly means the only protocols that exist are the ones
//! shipped here.
//!
//! The [`Rule`] vocabulary is a closed enum, which is the trade that makes
//! that work: a fixed vocabulary is inspectable and serialisable, and a new
//! rule kind is a change to this file rather than something a user writes. For
//! v0.1 that is the right side of the trade. If real protocols keep needing
//! rules that are not here, the enum is the wrong shape, and that finding is
//! worth more than the inconvenience that produced it.
//!
//! # Validate before generate
//!
//! ADR-0002. The predicate comes first and generation is derived from it, so a
//! generator cannot drift from the thing that checks it. INV-4 (P2.3) is the
//! property that keeps that honest.
//!
//! # Where railpulse is not
//!
//! Not here. INV-7 keeps protocol concepts out of the engine, and the same
//! reasoning applies one layer up: a `railpulse` baked into the algebra would
//! be privileged over every other protocol the platform is supposed to host.
//! It lives in `protocols/`, built from this vocabulary like anyone else's.

use pact_core::{Bag, Defect, Distribution, Graph, Kind, Node, ResourceEdge, Signature, Type};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

/// Most branches one gate may have. Invariant I2.
///
/// A dimension cap: `max_tracks_per_phase = k` implies `dim K ≤ k − 1`, and it
/// is what keeps a phase hand-checkable. The same number lives in
/// `spec/invariants.toml` and `scripts/canaries.sh` checks the two agree; see
/// `pact_core::PRESSURE_RATIO` for why the duplication is deliberate.
pub const MAX_TRACKS_PER_PHASE: usize = 4;

/// Fewest branches a gate may have. Invariant I4.
///
/// A gate with one output is not a gate, it is a relabelled edge, and a phase
/// built from them has nothing to select over. Mirrors
/// `min_viable_transitions` in `spec/invariants.toml`.
pub const MIN_VIABLE_TRANSITIONS: usize = 2;

// ────────────────────────────────────────────────────────────── rules

/// One checkable requirement.
///
/// Every variant is decidable by looking at the graph. Nothing here runs a
/// simulation, which is what lets a protocol be checked on every edit rather
/// than in a batch job nobody waits for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rule {
    /// A converter reading one type and emitting another must exist.
    ///
    /// Matched by signature, not by name. Node ids are namespaced by
    /// composition path, and a rule keyed on an id would pass only for graphs
    /// this crate built.
    Stage {
        /// What this stage is called in the protocol's own vocabulary.
        role: &'static str,
        /// What it reads.
        reads: Type,
        /// What it emits.
        emits: Type,
    },
    /// `⊗` — at least one broadcast gate. The plan must fan out.
    Expands,
    /// `⊕` — at least one fan-in converter. Results must collapse to one.
    Selects,
    /// `*` — at least one delay lying on a directed cycle.
    ///
    /// On a cycle, specifically. A delay off a cycle is a buffer: it holds
    /// work once and passes it on, which is not iteration.
    Executes,
    /// `↺` — a directed cycle passing through a broadcast gate.
    ///
    /// Through the fan-out, specifically, or an inner iteration satisfies this
    /// and the outer loop never has to close. Those are different claims: one
    /// says an effector repeats, the other says the whole phase does.
    Repeats,
    /// No gate may exceed `max` outputs. I2.
    BoundedWidth {
        /// The cap.
        max: usize,
    },
    /// Every gate must have at least `min` outputs. I4.
    GatesBranch {
        /// The floor.
        min: usize,
    },
    /// One weakly-connected component. I3.
    ///
    /// Forcing is what makes edges exist, so a phase in two pieces is two
    /// phases that have not noticed.
    Connected,
    /// No dangling edge and no duplicate id.
    ///
    /// Deliberately *not* every defect the cheap tier finds. A starved
    /// converter, a sink and an orphan source are diagnoses about a system,
    /// and a fragment with nothing attached to its ends has all three by
    /// construction. A dangling edge and a duplicate id are different in kind:
    /// they mean the graph does not describe anything.
    WellFormed,
}

impl Rule {
    /// A stable identifier, so reports stay comparable across runs.
    #[must_use]
    pub fn id(&self) -> &'static str {
        match self {
            Self::Stage { .. } => "stage",
            Self::Expands => "expands",
            Self::Selects => "selects",
            Self::Executes => "executes",
            Self::Repeats => "repeats",
            Self::BoundedWidth { .. } => "I2 bounded-width",
            Self::GatesBranch { .. } => "I4 gates-branch",
            Self::Connected => "I3 connected",
            Self::WellFormed => "well-formed",
        }
    }

    /// What this rule requires, in words.
    #[must_use]
    pub fn statement(&self) -> String {
        match self {
            Self::Stage { role, reads, emits } => {
                format!("a {role} stage reading {reads} and emitting {emits}")
            }
            Self::Expands => "at least one broadcast gate".to_string(),
            Self::Selects => "at least one fan-in converter".to_string(),
            Self::Executes => "a delay on a directed cycle".to_string(),
            Self::Repeats => "a directed cycle through a broadcast gate".to_string(),
            Self::BoundedWidth { max } => format!("no gate wider than {max} branches"),
            Self::GatesBranch { min } => format!("every gate branching at least {min} ways"),
            Self::Connected => "one connected component".to_string(),
            Self::WellFormed => "no dangling edge and no duplicate id".to_string(),
        }
    }

    /// Check this rule, returning what broke it.
    ///
    /// One rule, one answer. A rule that could report two different failures
    /// would need the caller to decide which mattered, and the caller has less
    /// information than the rule does.
    #[must_use]
    pub fn check(&self, graph: &Graph) -> Option<Violation> {
        match self {
            Self::Stage { reads, emits, .. } => self.stage(graph, reads, emits),
            Self::Expands => self.expands(graph),
            Self::Selects => self.selects(graph),
            Self::Executes => self.executes(graph),
            Self::Repeats => self.repeats(graph),
            Self::BoundedWidth { max } => self.bounded_width(graph, *max),
            Self::GatesBranch { min } => self.gates_branch(graph, *min),
            Self::Connected => self.connected(graph),
            Self::WellFormed => self.well_formed(graph),
        }
    }

    /// Build the violation for this rule.
    ///
    /// Every caller wraps it in `Some`, which clippy is right to notice: the
    /// wrapping belongs at the call site, where "no violation" is a real
    /// answer, and not here, where a violation is always what is being built.
    fn broke(&self, found: String, at: Vec<String>) -> Violation {
        Violation {
            rule: self.id(),
            requires: self.statement(),
            found,
            at,
        }
    }

    fn stage(&self, graph: &Graph, reads: &Type, emits: &Type) -> Option<Violation> {
        let present = graph.nodes().iter().any(|node| match &node.kind {
            Kind::Converter { signature } => {
                signature.consumes.count(reads) > 0 && signature.produces.count(emits) > 0
            }
            _ => false,
        });
        if present {
            return None;
        }
        // Name what IS there. "No such stage" sends someone to read every
        // node; the signatures present are usually enough to see which one was
        // meant.
        Some(self.broke(format!("signatures present: {}", signatures(graph)), vec![]))
    }

    fn expands(&self, graph: &Graph) -> Option<Violation> {
        if broadcast_gates(graph).is_empty() {
            Some(self.broke(
                format!("{} gate(s), none broadcasting", gates_of(graph).len()),
                vec![],
            ))
        } else {
            None
        }
    }

    fn selects(&self, graph: &Graph) -> Option<Violation> {
        if fan_ins(graph).is_empty() {
            Some(self.broke(
                "no converter consumes more than one resource".to_string(),
                vec![],
            ))
        } else {
            None
        }
    }

    fn executes(&self, graph: &Graph) -> Option<Violation> {
        let delays: Vec<String> = graph
            .nodes()
            .iter()
            .filter(|n| matches!(n.kind, Kind::Delay { .. }))
            .map(|n| n.id.clone())
            .collect();

        if delays.iter().any(|id| on_cycle(graph, id)) {
            None
        } else if delays.is_empty() {
            Some(self.broke("no delay at all".to_string(), vec![]))
        } else {
            Some(self.broke(
                format!("{} delay(s), none on a cycle", delays.len()),
                delays,
            ))
        }
    }

    fn repeats(&self, graph: &Graph) -> Option<Violation> {
        let gates = broadcast_gates(graph);
        if gates.iter().any(|id| on_cycle(graph, id)) {
            None
        } else if gates.is_empty() {
            Some(self.broke(
                "no broadcast gate to close a cycle through".to_string(),
                vec![],
            ))
        } else {
            Some(self.broke(
                format!("{} broadcast gate(s), none on a cycle", gates.len()),
                gates,
            ))
        }
    }

    fn bounded_width(&self, graph: &Graph, max: usize) -> Option<Violation> {
        let wide: Vec<String> = gates_of(graph)
            .into_iter()
            .filter(|id| graph.outgoing(id).len() > max)
            .collect();
        if wide.is_empty() {
            return None;
        }
        let widest = wide
            .iter()
            .map(|id| graph.outgoing(id).len())
            .max()
            .unwrap_or(0);
        Some(self.broke(format!("widest gate has {widest} branches"), wide))
    }

    fn gates_branch(&self, graph: &Graph, min: usize) -> Option<Violation> {
        let narrow: Vec<String> = gates_of(graph)
            .into_iter()
            .filter(|id| graph.outgoing(id).len() < min)
            .collect();
        if narrow.is_empty() {
            return None;
        }
        let counts: Vec<String> = narrow
            .iter()
            .map(|id| format!("{id} has {}", graph.outgoing(id).len()))
            .collect();
        Some(self.broke(counts.join(", "), narrow))
    }

    fn connected(&self, graph: &Graph) -> Option<Violation> {
        let parts = components(graph);
        if parts.len() == 1 {
            return None;
        }
        // An empty graph has zero components, so it fails this rule, and
        // "found 0 component(s)" is a true statement that helps nobody. It
        // gets its own message rather than a pass: a validator that reports an
        // empty graph as connected is technically defensible and practically
        // misleading, which is the worst of both.
        if parts.is_empty() {
            return Some(self.broke("the graph has no nodes".to_string(), vec![]));
        }
        // One representative per component. A count alone says the phase is in
        // pieces; the representatives say which pieces.
        let representatives: Vec<String> =
            parts.iter().filter_map(|p| p.first().cloned()).collect();
        Some(self.broke(format!("{} component(s)", parts.len()), representatives))
    }

    fn well_formed(&self, graph: &Graph) -> Option<Violation> {
        let breaking: Vec<Defect> = graph
            .validate()
            .into_iter()
            .filter(|d| {
                matches!(
                    d,
                    Defect::DanglingEdge { .. } | Defect::DuplicateNode { .. }
                )
            })
            .collect();
        if breaking.is_empty() {
            return None;
        }
        let at = breaking
            .iter()
            .filter_map(|d| match d {
                Defect::DanglingEdge { missing, .. } => Some(missing.clone()),
                Defect::DuplicateNode { id } => Some(id.clone()),
                _ => None,
            })
            .collect();
        Some(self.broke(format!("{} defect(s)", breaking.len()), at))
    }
}

// ────────────────────────────────────────────────────────────── violations

/// A rule a graph broke.
///
/// Carries what was required and what was found, because "invalid" is not a
/// finding. Somebody has to be able to act on this without opening the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// Which rule.
    pub rule: &'static str,
    /// What it requires.
    pub requires: String,
    /// What the graph has instead.
    pub found: String,
    /// The nodes implicated, where the rule can name any.
    pub at: Vec<String>,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: requires {}, found {}",
            self.rule, self.requires, self.found
        )?;
        if !self.at.is_empty() {
            write!(f, " at {}", self.at.join(", "))?;
        }
        Ok(())
    }
}

// ────────────────────────────────────────────────────────────── protocols

/// A named set of rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Protocol {
    name: String,
    rules: Vec<Rule>,
}

impl Protocol {
    /// An empty protocol.
    ///
    /// Empty means every graph passes, which is a real and useless protocol.
    /// It is allowed rather than refused because "no rules yet" is an honest
    /// state to be in while authoring one, and a validator reporting success
    /// on it is telling the truth.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            rules: Vec::new(),
        }
    }

    /// Add a rule. Chainable.
    #[must_use]
    pub fn require(mut self, rule: Rule) -> Self {
        self.rules.push(rule);
        self
    }

    /// The protocol's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Its rules, in the order they were declared.
    #[must_use]
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// Check a graph.
    ///
    /// # Errors
    ///
    /// Every rule the graph broke, not the first. A validator that stops at
    /// one violation turns fixing a graph into a guessing game played one
    /// round-trip at a time.
    pub fn validate(&self, graph: &Graph) -> Result<(), Vec<Violation>> {
        let broken: Vec<Violation> = self.rules.iter().filter_map(|r| r.check(graph)).collect();
        if broken.is_empty() {
            Ok(())
        } else {
            Err(broken)
        }
    }

    /// The smallest graph satisfying every rule: a witness.
    ///
    /// This is the generator ADR-0002 says must be *derived from* the
    /// predicate, so it cannot drift from the thing that checks it. It is also
    /// the answer to the first question anyone authoring a protocol has, which
    /// is "show me what this would accept".
    ///
    /// # This must never consult [`Self::validate`]
    ///
    /// Emitting and then checking would make INV-4 hold by construction and
    /// prove nothing at all: the property would be testing that a filter
    /// filters. The construction below reads the rules and builds what they
    /// ask for, and it detects an unsatisfiable set *ahead of time* rather
    /// than discovering it by failing its own validator. `tests/witness.rs`
    /// greps this function for exactly that mistake.
    ///
    /// # Errors
    ///
    /// [`Unsatisfiable`] when the rules contradict each other. That outcome is
    /// the interesting half of INV-4: a generator is honest if it either
    /// produces something valid or refuses, and never produces something
    /// invalid.
    pub fn emit(&self) -> Result<Graph, Unsatisfiable> {
        let wants = Wants::read(&self.rules);
        wants.build()
    }
}

/// Why a rule set cannot be satisfied by any graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unsatisfiable {
    /// A width floor above the width cap. No gate can be both.
    Width {
        /// The floor, from `GatesBranch`.
        min: usize,
        /// The cap, from `BoundedWidth`.
        max: usize,
    },
    /// A cycle is required through a gate that is allowed no branches.
    ///
    /// `Repeats` needs the fan-out to have somewhere to send work, and a gate
    /// with no outgoing edge is on no cycle. This is the one contradiction
    /// that is not visible from the two width numbers alone.
    ClosedFanOut {
        /// The cap that leaves no room.
        max: usize,
    },
}

impl fmt::Display for Unsatisfiable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Width { min, max } => write!(
                f,
                "no gate can branch at least {min} ways and at most {max}"
            ),
            Self::ClosedFanOut { max } => write!(
                f,
                "a cycle must pass through the fan-out, but no gate may have \
                 more than {max} branches, so none is on a cycle"
            ),
        }
    }
}

/// What a rule set asks for, gathered before anything is built.
///
/// Reading the rules once and building once is what keeps the generator from
/// becoming a search. A search would satisfy INV-4 trivially and would tell an
/// author nothing about why their rules conflict.
// Four independent flags, one per stage rule. They co-occur in every
// combination, which is the case clippy's state-enum advice is not for.
#[allow(clippy::struct_excessive_bools)]
struct Wants {
    stages: Vec<(Type, Type)>,
    expand: bool,
    select: bool,
    execute: bool,
    repeat: bool,
    width_min: usize,
    width_max: usize,
}

impl Wants {
    fn read(rules: &[Rule]) -> Self {
        let mut wants = Self {
            stages: Vec::new(),
            expand: false,
            select: false,
            execute: false,
            repeat: false,
            width_min: 0,
            // No cap declared means no cap. `usize::MAX` rather than an
            // `Option` because every use is a comparison, and the comparison
            // reads the same either way.
            width_max: usize::MAX,
        };
        for rule in rules {
            match rule {
                Rule::Stage { reads, emits, .. } => {
                    wants.stages.push((reads.clone(), emits.clone()));
                }
                Rule::Expands => wants.expand = true,
                Rule::Selects => wants.select = true,
                Rule::Executes => wants.execute = true,
                Rule::Repeats => wants.repeat = true,
                // Several of the same rule is legal and the strictest wins.
                // Taking the last would make a rule set order-dependent, and a
                // protocol whose meaning depends on declaration order is not a
                // predicate.
                Rule::BoundedWidth { max } => wants.width_max = wants.width_max.min(*max),
                Rule::GatesBranch { min } => wants.width_min = wants.width_min.max(*min),
                Rule::Connected | Rule::WellFormed => {}
            }
        }
        wants
    }

    /// How wide the fan-out must be, or why it cannot exist.
    fn branches(&self) -> Result<usize, Unsatisfiable> {
        if self.width_min > self.width_max {
            return Err(Unsatisfiable::Width {
                min: self.width_min,
                max: self.width_max,
            });
        }
        // One branch by default, because a broadcast gate wired to nothing is
        // a legal answer to `Expands` and a useless witness.
        let wanted = self.width_min.max(1).min(self.width_max);
        if self.repeat && wanted == 0 {
            return Err(Unsatisfiable::ClosedFanOut {
                max: self.width_max,
            });
        }
        Ok(wanted)
    }

    fn build(&self) -> Result<Graph, Unsatisfiable> {
        // Every node hangs off one hub, which is what satisfies `Connected`
        // without a separate pass. A pool, so nothing about it can trip the
        // gate-width rules, and passive, so it is storage rather than a pump.
        let mut graph = Graph::new();
        graph.add(Node::pool("hub").with_meta("witness", "hub"));

        let carrier = self
            .stages
            .first()
            .map_or_else(|| Type::new("Witness"), |(_, emits)| emits.clone());

        for (i, (reads, emits)) in self.stages.iter().enumerate() {
            let id = format!("stage{i}");
            graph.add(
                Node::converter(
                    id.clone(),
                    Signature {
                        consumes: Bag::of(reads.clone(), 1),
                        produces: Bag::of(emits.clone(), 1),
                    },
                )
                .with_meta("witness", "stage"),
            );
            graph
                .connect(ResourceEdge::all("hub", id.clone()))
                .connect(ResourceEdge::all(id, "hub"));
        }

        if self.expand || self.repeat {
            let branches = self.branches()?;
            graph.add(Node::gate("fan", Distribution::Broadcast).with_meta("witness", "fan"));
            graph.connect(ResourceEdge::all("hub", "fan"));

            for j in 0..branches {
                let id = format!("track{j}");
                graph.add(
                    Node::converter(
                        id.clone(),
                        Signature::simple(carrier.clone(), carrier.clone()),
                    )
                    .with_meta("witness", "track"),
                );
                graph.connect(ResourceEdge::all("fan", id.clone()));
                // The first track closes the cycle, which is what makes it run
                // through the fan-out rather than around some other loop.
                if self.repeat && j == 0 {
                    graph.connect(ResourceEdge::all(id, "hub"));
                }
            }
        }

        if self.select {
            // `Selects` reads the signature, not the wiring: a fan-in is a
            // converter that eats more than one resource. Two is the smallest
            // number that is more than one.
            graph.add(
                Node::converter(
                    "select",
                    Signature::fan_in(carrier.clone(), 2, carrier.clone()),
                )
                .with_meta("witness", "select"),
            );
            graph
                .connect(ResourceEdge::all("hub", "select"))
                .connect(ResourceEdge::all("select", "hub"));
        }

        if self.execute {
            // A delay ON a cycle. A delay hanging off the hub would be a
            // buffer, and `Executes` is right to refuse it.
            graph.add(Node::delay("hold", 1).with_meta("witness", "hold"));
            graph
                .connect(ResourceEdge::all("hub", "hold"))
                .connect(ResourceEdge::all("hold", "hub"));
        }

        Ok(graph)
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "protocol {}", self.name)?;
        for rule in &self.rules {
            writeln!(f, "  {} — {}", rule.id(), rule.statement())?;
        }
        Ok(())
    }
}

// ────────────────────────────────────────────────────────────── graph facts

fn gates_of(graph: &Graph) -> Vec<String> {
    graph
        .nodes()
        .iter()
        .filter(|n| matches!(n.kind, Kind::Gate { .. }))
        .map(|n| n.id.clone())
        .collect()
}

fn broadcast_gates(graph: &Graph) -> Vec<String> {
    graph
        .nodes()
        .iter()
        .filter(|n| {
            matches!(
                &n.kind,
                Kind::Gate {
                    distribution: Distribution::Broadcast
                }
            )
        })
        .map(|n| n.id.clone())
        .collect()
}

fn fan_ins(graph: &Graph) -> Vec<String> {
    graph
        .nodes()
        .iter()
        .filter(|n| match &n.kind {
            // Two or more resources in, whatever comes out. A converter eating
            // one thing is a step; one eating several is a selection, and that
            // is the shape `⊕` names.
            Kind::Converter { signature } => signature.consumes.total() >= 2,
            _ => false,
        })
        .map(|n| n.id.clone())
        .collect()
}

fn signatures(graph: &Graph) -> String {
    let listed: Vec<String> = graph
        .nodes()
        .iter()
        .filter_map(|n| match &n.kind {
            Kind::Converter { signature } => Some(format!(
                "{} → {}",
                types_of(&signature.consumes),
                types_of(&signature.produces)
            )),
            _ => None,
        })
        .collect();
    if listed.is_empty() {
        "none".to_string()
    } else {
        listed.join("; ")
    }
}

fn types_of(bag: &Bag) -> String {
    let names: Vec<String> = bag.types().map(ToString::to_string).collect();
    if names.is_empty() {
        "nothing".to_string()
    } else {
        names.join("+")
    }
}

/// Can work leave `id` and get back to it?
///
/// Reachability from each successor rather than Tarjan. These graphs are tens
/// of nodes, the cost difference is nothing, and the correctness of this one is
/// checkable by reading it. A clever algorithm whose bugs are invisible is the
/// wrong trade at the layer that decides whether someone's loop closes.
fn on_cycle(graph: &Graph, id: &str) -> bool {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut queue: VecDeque<String> = graph
        .outgoing(id)
        .into_iter()
        .map(|(_, e)| e.to.clone())
        .collect();

    while let Some(node) = queue.pop_front() {
        if node == id {
            return true;
        }
        if !seen.insert(node.clone()) {
            continue;
        }
        for (_, edge) in graph.outgoing(&node) {
            queue.push_back(edge.to.clone());
        }
    }
    false
}

/// Weakly-connected components, each a sorted list of node ids.
///
/// Both edge kinds count. A state connection moves no resources and still
/// couples what it touches: that is what "forcing is what makes edges exist"
/// means, and a component count ignoring them would report a phase as
/// scattered precisely when its coupling is behavioural rather than material.
fn components(graph: &Graph) -> Vec<Vec<String>> {
    let mut undirected: BTreeMap<String, BTreeSet<String>> = graph
        .nodes()
        .iter()
        .map(|n| (n.id.clone(), BTreeSet::new()))
        .collect();

    let endpoints = graph
        .resources
        .iter()
        .map(|e| (e.from.clone(), e.to.clone()))
        .chain(graph.states.iter().map(|e| (e.from.clone(), e.to.clone())));

    for (from, to) in endpoints {
        // An edge naming a node that does not exist is a dangling edge, which
        // `WellFormed` reports. Skipping it here keeps one broken graph from
        // producing two unrelated complaints.
        if undirected.contains_key(&from) && undirected.contains_key(&to) {
            undirected
                .entry(from.clone())
                .or_default()
                .insert(to.clone());
            undirected.entry(to).or_default().insert(from);
        }
    }

    let mut unvisited: BTreeSet<String> = undirected.keys().cloned().collect();
    let mut found = Vec::new();
    while let Some(seed) = unvisited.iter().next().cloned() {
        let mut part = Vec::new();
        let mut queue = VecDeque::from([seed]);
        while let Some(node) = queue.pop_front() {
            if !unvisited.remove(&node) {
                continue;
            }
            if let Some(next) = undirected.get(&node) {
                for neighbour in next {
                    if unvisited.contains(neighbour) {
                        queue.push_back(neighbour.clone());
                    }
                }
            }
            part.push(node);
        }
        part.sort();
        found.push(part);
    }
    found
}
