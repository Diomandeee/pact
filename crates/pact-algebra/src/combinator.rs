//! The six combinators.
//!
//! A *word* in this algebra composes engine graphs. That is the whole point of
//! the layer: `railpulse` is not a special case baked into the core, it is a
//! word like any other, and so is whatever someone else authors.
//!
//! ```text
//!  ▸  sequence    out(A) ∋ in(B)          work flows onward
//!  ⊗  broadcast   in(A) == in(B)          every branch reads the same upstream
//!  ⊕  merge       consumes M^n            many collapse to one
//!  ↺  feedback    out(A) ∋ in(A)          the last output reaches the first input
//!  *  iterate     no type constraint      repeat until a condition
//!  +  choice      in(A) == in(B)          one branch runs
//! ```
//!
//! # Types decide, not prose
//!
//! Two consecutive skills that consume the **same** input type are parallel,
//! not sequential, however the documentation describes them. That one rule is
//! what caught a chain in the live corpus describing a fan-out as a sequence,
//! and it is why composition is checked at construction rather than at
//! validation: a word that does not typecheck should be impossible to hold,
//! not merely reportable.
//!
//! The rules here are the same rules as
//! `~/.claude/skills/divergent-rail/scripts/decompose.py`, which is the oracle
//! P3.2 will diff against. Where they disagree, find out which one is wrong;
//! twice so far the older implementation has been correct.
//!
//! # ⊗ and + are not the same shape
//!
//! Both require their branches to read the same upstream, and they differ in
//! what happens next: broadcast runs every branch, choice runs one. The engine
//! already draws exactly this distinction between `Distribution::Broadcast`
//! and a splitting gate, which is why the compilation below is a translation
//! rather than an interpretation.

use pact_core::{Distribution, Graph, Node, ResourceEdge, Signature, Type};
use std::collections::BTreeSet;
use std::fmt;

// ─────────────────────────────────────────────────────────────── ports

/// The declared interface of a term: what it reads, and what it may emit.
///
/// `outputs` is a set because a SOOP-2 skill may declare `output_type: A | B`
/// and emit one of them. Composition asks whether the next term's input is
/// **among** those, not whether it equals the only one. Treating a set as a
/// single type is how a legal chain gets reported as broken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ports {
    /// What one firing reads.
    pub input: Type,
    /// What one firing may emit. Never empty.
    pub outputs: BTreeSet<Type>,
}

impl Ports {
    /// Can a term with these ports be followed by one reading `wanted`?
    #[must_use]
    pub fn reaches(&self, wanted: &Type) -> bool {
        self.outputs.contains(wanted)
    }

    fn outputs_listed(&self) -> Vec<Type> {
        self.outputs.iter().cloned().collect()
    }
}

// ─────────────────────────────────────────────────────────────── errors

/// Why a term could not be built.
///
/// Every variant names both sides and both types. "Does not compose" sends
/// someone back to read two SOOP-2 blocks by hand, and the whole argument for
/// doing this mechanically is that they should not have to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mismatch {
    /// `▸`: the left term cannot produce what the right term reads.
    Sequence {
        /// The upstream term.
        left: String,
        /// The downstream term.
        right: String,
        /// Everything the upstream may emit.
        produces: Vec<Type>,
        /// What the downstream reads.
        wants: Type,
    },
    /// `⊗` or `+`: branches must read the same upstream.
    Parallel {
        /// Which combinator: `⊗` or `+`.
        combinator: &'static str,
        /// The branch that set the expectation.
        first: String,
        /// What that branch reads.
        first_input: Type,
        /// The branch that disagreed.
        other: String,
        /// What it reads instead.
        other_input: Type,
    },
    /// `↺`: the loop does not close, so the cycle runs once and stops.
    Feedback {
        /// The term the loop was drawn around.
        term: String,
        /// What it reads, and therefore what the loop must return to it.
        wants: Type,
        /// What it actually emits.
        produces: Vec<Type>,
    },
    /// A combinator with nothing to combine.
    Empty {
        /// Which combinator.
        combinator: &'static str,
    },
    /// A signature with no single input port, or no output at all.
    Port {
        /// The term.
        term: String,
        /// What is wrong with it.
        reason: PortProblem,
    },
}

/// Why a signature does not present a usable port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortProblem {
    /// Consumes nothing. That is a source, not a skill.
    NoInput,
    /// Consumes more than one type. There is no single upstream to name, so
    /// the term cannot be sequenced. A skill that genuinely takes two kinds is
    /// a join, and the algebra spells a join `⊕` over one type.
    ManyInputs(Vec<Type>),
    /// Produces nothing. That is a drain, not a skill.
    NoOutput,
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn list(types: &[Type]) -> String {
            if types.is_empty() {
                "nothing".to_string()
            } else {
                types
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" | ")
            }
        }

        match self {
            Self::Sequence {
                left,
                right,
                produces,
                wants,
            } => write!(
                f,
                "▸ does not compose: {left} yields {}, {right} wants {wants}",
                list(produces)
            ),
            Self::Parallel {
                combinator,
                first,
                first_input,
                other,
                other_input,
            } => write!(
                f,
                "{combinator} needs one upstream: {first} reads {first_input}, \
                 {other} reads {other_input}"
            ),
            Self::Feedback {
                term,
                wants,
                produces,
            } => write!(
                f,
                "↺ does not close: {term} reads {wants} but yields {}",
                list(produces)
            ),
            Self::Empty { combinator } => write!(f, "{combinator} has no members"),
            Self::Port { term, reason } => match reason {
                PortProblem::NoInput => write!(f, "{term} consumes nothing: that is a source"),
                PortProblem::ManyInputs(types) => write!(
                    f,
                    "{term} consumes {}: no single input port to sequence on",
                    list(types)
                ),
                PortProblem::NoOutput => write!(f, "{term} produces nothing: that is a drain"),
            },
        }
    }
}

// ─────────────────────────────────────────────────────────────── terms

/// What a term is made of.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Shape {
    /// One skill.
    Atom(Signature),
    /// `⊕` — `n` of one type collapsed into one of another.
    ///
    /// This is `merge : M^n → M`, one of the two generators the corpus
    /// analysis found missing. It is atom-shaped rather than a wrapper because
    /// that is what it is: a converter with a fan-in signature.
    ///
    /// The output type is separate from the input on purpose. The first
    /// version forced them equal, which reads fine until you try to write the
    /// canonical cycle: `↺(⊗ ▸ * ▸ ⊕)` only closes because the select stage
    /// returns the type the fan-out reads. A same-type-only merge cannot
    /// express railpulse at all, which is how P2.4 found the gap.
    Merge {
        arity: u64,
        of: Type,
        into: Type,
    },
    Sequence(Vec<Term>),
    Broadcast(Vec<Term>),
    Choice(Vec<Term>),
    Feedback(Box<Term>),
    Iterate(Box<Term>),
}

/// A word in the algebra.
///
/// Construction is fallible wherever a type rule applies, so an ill-typed word
/// cannot be held. Everything a caller can ask afterwards is therefore a
/// question about a word that typechecks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Term {
    label: String,
    ports: Ports,
    shape: Shape,
}

impl Term {
    /// One skill, from its SOOP-2 signature.
    ///
    /// # Errors
    ///
    /// [`Mismatch::Port`] when the signature has no single input type or emits
    /// nothing. Both are real shapes in the engine — a source and a drain —
    /// and neither is a skill, so neither belongs in a word.
    pub fn atom(label: impl Into<String>, signature: Signature) -> Result<Self, Mismatch> {
        let label = label.into();
        let inputs: Vec<Type> = signature.consumes.types().cloned().collect();
        let outputs: BTreeSet<Type> = signature.produces.types().cloned().collect();

        let input = match inputs.len() {
            0 => {
                return Err(Mismatch::Port {
                    term: label,
                    reason: PortProblem::NoInput,
                })
            }
            1 => inputs[0].clone(),
            _ => {
                return Err(Mismatch::Port {
                    term: label,
                    reason: PortProblem::ManyInputs(inputs),
                })
            }
        };
        if outputs.is_empty() {
            return Err(Mismatch::Port {
                term: label,
                reason: PortProblem::NoOutput,
            });
        }

        Ok(Self {
            label,
            ports: Ports { input, outputs },
            shape: Shape::Atom(signature),
        })
    }

    /// Declare another type this term may emit.
    ///
    /// SOOP-2 writes it `output_type: A | B`, and the live corpus has them.
    /// Widening the port set is a declaration about the interface, not about
    /// the signature: what the term emits on any one firing is unchanged.
    #[must_use]
    pub fn or_produces(mut self, ty: impl Into<Type>) -> Self {
        self.ports.outputs.insert(ty.into());
        self
    }

    /// `⊕` — collapse `arity` resources of one type into one of another.
    ///
    /// `of == into` is the plain fan-in. They differ when the select stage is
    /// also the compaction, which is the case that makes a cycle close.
    ///
    /// # Errors
    ///
    /// [`Mismatch::Empty`] for an arity below two. Merging one thing is the
    /// identity wearing a fan-in's name, and a word containing it would report
    /// a select stage that never selected anything.
    pub fn merge(
        label: impl Into<String>,
        arity: u64,
        of: impl Into<Type>,
        into: impl Into<Type>,
    ) -> Result<Self, Mismatch> {
        let label = label.into();
        let of = of.into();
        let into = into.into();
        if arity < 2 {
            return Err(Mismatch::Empty { combinator: "⊕" });
        }
        let mut outputs = BTreeSet::new();
        outputs.insert(into.clone());
        Ok(Self {
            label,
            ports: Ports {
                input: of.clone(),
                outputs,
            },
            shape: Shape::Merge { arity, of, into },
        })
    }

    /// `▸` — run terms one after another.
    ///
    /// # Errors
    ///
    /// [`Mismatch::Sequence`] naming both terms, everything the upstream may
    /// emit, and what the downstream reads. [`Mismatch::Empty`] for no members.
    pub fn sequence(terms: Vec<Self>) -> Result<Self, Mismatch> {
        if terms.is_empty() {
            return Err(Mismatch::Empty { combinator: "▸" });
        }
        for pair in terms.windows(2) {
            let (left, right) = (&pair[0], &pair[1]);
            if !left.ports.reaches(&right.ports.input) {
                return Err(Mismatch::Sequence {
                    left: left.label.clone(),
                    right: right.label.clone(),
                    produces: left.ports.outputs_listed(),
                    wants: right.ports.input.clone(),
                });
            }
        }
        let ports = Ports {
            input: terms[0].ports.input.clone(),
            outputs: terms[terms.len() - 1].ports.outputs.clone(),
        };
        Ok(Self {
            label: terms
                .iter()
                .map(|t| t.label.as_str())
                .collect::<Vec<_>>()
                .join("▸"),
            ports,
            shape: Shape::Sequence(terms),
        })
    }

    /// `▸`, appending. Flattens, so `a.then(b).then(c)` is one three-stage
    /// sequence rather than two nested ones.
    ///
    /// # Errors
    ///
    /// As [`Self::sequence`].
    pub fn then(self, next: Self) -> Result<Self, Mismatch> {
        match self.shape {
            Shape::Sequence(mut terms) => {
                terms.push(next);
                Self::sequence(terms)
            }
            _ => Self::sequence(vec![self, next]),
        }
    }

    /// `⊗` — every branch reads the same upstream, and every branch runs.
    ///
    /// # Errors
    ///
    /// [`Mismatch::Parallel`] naming the branch that disagreed and both input
    /// types. [`Mismatch::Empty`] for no branches.
    pub fn broadcast(branches: Vec<Self>) -> Result<Self, Mismatch> {
        Self::parallel("⊗", branches, Shape::Broadcast)
    }

    /// `+` — every branch reads the same upstream, and one branch runs.
    ///
    /// # Errors
    ///
    /// As [`Self::broadcast`], with `+` named instead.
    pub fn choice(branches: Vec<Self>) -> Result<Self, Mismatch> {
        Self::parallel("+", branches, Shape::Choice)
    }

    fn parallel(
        combinator: &'static str,
        branches: Vec<Self>,
        wrap: fn(Vec<Self>) -> Shape,
    ) -> Result<Self, Mismatch> {
        let Some(first) = branches.first() else {
            return Err(Mismatch::Empty { combinator });
        };
        let input = first.ports.input.clone();
        let first_label = first.label.clone();

        for branch in &branches[1..] {
            if branch.ports.input != input {
                return Err(Mismatch::Parallel {
                    combinator,
                    first: first_label,
                    first_input: input,
                    other: branch.label.clone(),
                    other_input: branch.ports.input.clone(),
                });
            }
        }

        // The union, not the intersection. A fan-out's result is whatever any
        // branch may emit, and intersecting would report a fan-out over two
        // differently-typed branches as producing nothing.
        let outputs = branches
            .iter()
            .flat_map(|b| b.ports.outputs.iter().cloned())
            .collect();
        let label = branches
            .iter()
            .map(|t| t.label.as_str())
            .collect::<Vec<_>>()
            .join(combinator);

        Ok(Self {
            label,
            ports: Ports { input, outputs },
            shape: wrap(branches),
        })
    }

    /// `↺` — the last output returns to the first input.
    ///
    /// # Errors
    ///
    /// [`Mismatch::Feedback`] when the loop does not close, naming what the
    /// term reads and what it actually emits. This is the check that reports
    /// the live corpus's central gap: with no `State → M_arch` generator,
    /// nothing closes, so `↺` cannot be built over any real chain and the loop
    /// runs once.
    pub fn feedback(self) -> Result<Self, Mismatch> {
        if !self.ports.reaches(&self.ports.input) {
            return Err(Mismatch::Feedback {
                term: self.label,
                wants: self.ports.input,
                produces: self.ports.outputs.into_iter().collect(),
            });
        }
        Ok(Self {
            label: format!("↺{}", self.label),
            ports: self.ports.clone(),
            shape: Shape::Feedback(Box::new(self)),
        })
    }

    /// `*` — repeat until a condition.
    ///
    /// Infallible, and that is the correct asymmetry rather than an oversight.
    /// Iteration is a property of one effector repeating its own work, so its
    /// input and output types are unchanged by definition. `↺` is the
    /// composition returning to its own start, which is a claim about types
    /// and can therefore be false.
    #[must_use]
    pub fn iterate(self) -> Self {
        Self {
            label: format!("*{}", self.label),
            ports: self.ports.clone(),
            shape: Shape::Iterate(Box::new(self)),
        }
    }

    /// What this term reads and may emit.
    #[must_use]
    pub fn ports(&self) -> &Ports {
        &self.ports
    }

    /// The term's name.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// The canonical symbol word, e.g. `↺(⊗ ▸ * ▸ ⊕)`.
    ///
    /// Stage symbols only, matching `decompose.py`, so the two can be diffed
    /// directly at P3.2. An atom renders as `·`: it is a stage, and which
    /// stage it is belongs in [`Self::label`].
    #[must_use]
    pub fn word(&self) -> String {
        match &self.shape {
            Shape::Atom(_) => "·".to_string(),
            Shape::Merge { .. } => "⊕".to_string(),
            Shape::Broadcast(_) => "⊗".to_string(),
            Shape::Choice(_) => "+".to_string(),
            Shape::Iterate(_) => "*".to_string(),
            Shape::Feedback(inner) => format!("↺({})", inner.word()),
            Shape::Sequence(terms) => terms.iter().map(Self::word).collect::<Vec<_>>().join(" ▸ "),
        }
    }

    /// Which of the four canonical stages this word contains.
    #[must_use]
    pub fn stages(&self) -> Stages {
        let mut found = Stages::default();
        self.mark(&mut found);
        found
    }

    fn mark(&self, found: &mut Stages) {
        match &self.shape {
            Shape::Atom(_) => {}
            Shape::Merge { .. } => found.select = true,
            Shape::Broadcast(branches) => {
                found.expand = true;
                for branch in branches {
                    branch.mark(found);
                }
            }
            Shape::Choice(branches) => {
                for branch in branches {
                    branch.mark(found);
                }
            }
            Shape::Iterate(inner) => {
                found.execute = true;
                inner.mark(found);
            }
            Shape::Feedback(inner) => {
                found.repeat = true;
                inner.mark(found);
            }
            Shape::Sequence(terms) => {
                for term in terms {
                    term.mark(found);
                }
            }
        }
    }

    /// Compile to an engine graph.
    #[must_use]
    pub fn compile(&self) -> Compiled {
        let mut graph = Graph::new();
        let (entry, exits) = self.build(&mut graph, "0");
        Compiled {
            graph,
            entry,
            exits,
        }
    }

    /// Emit nodes and edges, returning where input arrives and where output
    /// leaves.
    ///
    /// `path` namespaces ids. Two atoms with the same label compose into one
    /// graph, and without a path they would collide into a `DuplicateNode`
    /// that looks like an authoring mistake rather than the composition it is.
    fn build(&self, graph: &mut Graph, path: &str) -> (String, Vec<String>) {
        match &self.shape {
            Shape::Atom(signature) => {
                let id = format!("{path}:{}", self.label);
                graph.add(
                    Node::converter(id.clone(), signature.clone()).with_meta("term", &self.label),
                );
                (id.clone(), vec![id])
            }

            Shape::Merge { arity, of, into } => {
                let id = format!("{path}:{}", self.label);
                graph.add(
                    Node::converter(
                        id.clone(),
                        Signature::fan_in(of.clone(), *arity, into.clone()),
                    )
                    .with_meta("term", &self.label)
                    .with_meta("combinator", "⊕"),
                );
                (id.clone(), vec![id])
            }

            Shape::Sequence(terms) => {
                let mut entry = None;
                let mut previous: Vec<String> = Vec::new();
                for (i, term) in terms.iter().enumerate() {
                    let (term_entry, term_exits) = term.build(graph, &format!("{path}.{i}"));
                    if entry.is_none() {
                        entry = Some(term_entry.clone());
                    }
                    for exit in &previous {
                        graph.connect(ResourceEdge::all(exit.clone(), term_entry.clone()));
                    }
                    previous = term_exits;
                }
                (entry.unwrap_or_default(), previous)
            }

            Shape::Broadcast(branches) => {
                Self::fan(graph, path, branches, Distribution::Broadcast, "⊗")
            }

            // Compiled as a splitting gate, not as a condition. The algebra
            // carries no predicate to branch on, so round-robin is the honest
            // structural stand-in: it routes, and it does not pretend to know
            // why. A protocol that needs a real condition attaches a state
            // edge to the gate, which is what state edges are for.
            Shape::Choice(branches) => {
                Self::fan(graph, path, branches, Distribution::Deterministic, "+")
            }

            Shape::Feedback(inner) => {
                let (entry, exits) = inner.build(graph, &format!("{path}.0"));
                for exit in &exits {
                    graph.connect(ResourceEdge::all(exit.clone(), entry.clone()));
                }
                (entry, exits)
            }

            Shape::Iterate(inner) => {
                let (entry, exits) = inner.build(graph, &format!("{path}.0"));

                // "Repeat until a condition", and the algebra carries no
                // condition, so the split is a deterministic gate for the same
                // reason `+` is: it routes and does not pretend to know why.
                //
                // The first version wired the effector straight into the delay
                // and let its own exits carry on downstream. That looks right
                // and starves the next stage dead: a converter SPLITS its
                // output across outgoing edges, so the loop arm took all of it
                // and nothing ever came out. The cycle closed on paper and
                // starved in practice, which is exactly the failure this
                // layer is supposed to detect in other people's systems.
                let split = format!("{path}:*split");
                let held = format!("{path}:*");
                graph.add(
                    Node::gate(split.clone(), Distribution::Deterministic)
                        .with_meta("combinator", "*"),
                );
                // A delay, so a pass costs a step. An iteration is a model
                // doing work; a loop closing with no delay would let the
                // engine run unbounded passes inside one step, which is a
                // claim about compute that nothing supports.
                graph.add(Node::delay(held.clone(), 1).with_meta("combinator", "*"));

                for exit in &exits {
                    graph.connect(ResourceEdge::all(exit.clone(), split.clone()));
                }
                graph.connect(ResourceEdge::all(split.clone(), held.clone()));
                graph.connect(ResourceEdge::all(held, entry.clone()));

                // The gate is the exit. Whatever comes next attaches to its
                // other arm, which is what gives the loop somewhere to let go.
                (entry, vec![split])
            }
        }
    }
}

impl Term {
    fn fan(
        graph: &mut Graph,
        path: &str,
        branches: &[Self],
        distribution: Distribution,
        symbol: &'static str,
    ) -> (String, Vec<String>) {
        let gate = format!("{path}:{symbol}");
        graph.add(Node::gate(gate.clone(), distribution).with_meta("combinator", symbol));

        let mut exits = Vec::new();
        for (i, branch) in branches.iter().enumerate() {
            let (branch_entry, branch_exits) = branch.build(graph, &format!("{path}.{i}"));
            graph.connect(ResourceEdge::all(gate.clone(), branch_entry));
            exits.extend(branch_exits);
        }
        (gate, exits)
    }
}

impl fmt::Display for Term {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} : {}", self.word(), self.label)
    }
}

/// Which canonical stages a word contains.
///
/// The four that `railpulse = ↺(⊗ ▸ * ▸ ⊕)` names. A word missing one is not
/// broken; it is degenerate, and which one it is missing is the finding.
// Four independent booleans, which is what this is. Clippy suggests a state
// enum for structs with several bools, and that advice is for mutually
// exclusive states; these four co-occur in every combination, and 2^4 named
// variants would be strictly worse to read and to extend.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stages {
    /// `⊗` — the plan fans out.
    pub expand: bool,
    /// `*` — an effector iterates.
    pub execute: bool,
    /// `⊕` — results collapse to one.
    pub select: bool,
    /// `↺` — the cycle returns to its own start.
    pub repeat: bool,
}

impl Stages {
    /// How many of the four are present.
    #[must_use]
    pub fn present(self) -> usize {
        usize::from(self.expand)
            + usize::from(self.execute)
            + usize::from(self.select)
            + usize::from(self.repeat)
    }

    /// The ones that are absent, named as they are in the analysis.
    #[must_use]
    pub fn missing(self) -> Vec<&'static str> {
        let mut absent = Vec::new();
        if !self.expand {
            absent.push("⊗ expand");
        }
        if !self.execute {
            absent.push("* execute");
        }
        if !self.select {
            absent.push("⊕ select");
        }
        if !self.repeat {
            absent.push("↺ repeat");
        }
        absent
    }
}

impl fmt::Display for Stages {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/4", self.present())
    }
}

/// A word, compiled.
#[derive(Debug, Clone)]
pub struct Compiled {
    /// The engine graph.
    pub graph: Graph,
    /// Where input arrives.
    pub entry: String,
    /// Where output leaves.
    pub exits: Vec<String>,
}
